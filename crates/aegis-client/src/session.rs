//! The client's brain: one task that owns the open project.
//!
//! [`Session`] is a handle; the work happens in one actor task (rule C1)
//! that owns the [`Workspace`], the lease, and the selection. A UI sends
//! [`Command`]s through a bounded channel and reads immutable
//! [`ViewModel`] snapshots from a `watch` channel, so the UI thread never
//! waits for the network and the actor never waits for the UI.
//!
//! # Rule: the actor never awaits the network inline
//!
//! Every network call runs in its own short task and reports back through an
//! internal channel as an `Internal` event. So while a save is in flight,
//! the actor still applies the user's next edit in the same millisecond
//! (optimistic editing) and still answers commands.
//!
//! Events carry an **epoch**. Opening, closing, or resyncing a project starts
//! a new epoch, and events from an older one are dropped, so a late answer
//! can never touch a project it was not meant for.
//!
//! # Modes
//!
//! ```text
//!             BeginEdit: acquire lease          lease lost / released
//!  ReadOnly ─────────────────────────► Editing ───────────────────────► ReadOnly
//!     ▲  watching: remote ops applied    │ watching: lock events only     │
//!     └──────────────────────────────────┴──── resync (reload) on any doubt
//! ```
//!
//! While editing, only this client can change the project, so the watch
//! stream's op events are ignored (they are this client's own ops, which can
//! arrive before the save's answer) and only lock events are used.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use aegis_auth::layer::CredentialProvider;
use aegis_model::{Methodology, OpCodec, Project};
use aegis_proto as pb;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tokio::time::Instant;

use crate::commands::Command;
use crate::connection::{Connection, ServerInfo};
use crate::error::{ClientError, ServerCode};
use crate::sample::sample_ops;
use crate::state::{
    ConnectionView, LicenseView, Mode, Notice, ProjectEntry, ProjectView, Severity, SyncView,
    ViewModel,
};
use crate::view::{UiState, inspector, risks_differ, threat_rows, tree_view, use_server_risks};
use crate::workspace::{Origin, Workspace, WorkspaceError};

/// Most notices kept.
const MAX_NOTICES: usize = 5;
/// Commands queued from the UI before it must wait (R4).
const COMMAND_QUEUE: usize = 256;
/// Events queued from tasks before they must wait (R4).
const EVENT_QUEUE: usize = 256;

/// Tunables.
#[derive(Clone, Debug)]
pub struct SessionConfig {
    /// Who the user is, for provenance and display.
    pub user: String,
    /// Wall clock in milliseconds since the epoch.
    pub clock_ms: fn() -> u64,
    /// Wait before reconnecting or restarting a stream.
    pub retry_delay: Duration,
}

impl SessionConfig {
    /// Defaults for `user`: system clock, 2 s retry delay.
    #[must_use]
    pub fn new(user: &str) -> Self {
        SessionConfig {
            user: user.to_owned(),
            clock_ms: system_clock_ms,
            retry_delay: Duration::from_secs(2),
        }
    }
}

fn unix_secs() -> u64 {
    system_clock_ms() / 1000
}

fn system_clock_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

/// A handle to a running session.
#[derive(Clone)]
pub struct Session {
    commands: mpsc::Sender<Command>,
    view: watch::Receiver<Arc<ViewModel>>,
}

impl Session {
    /// Starts a session against `url`. Returns at once; connecting happens
    /// in the background and shows up in the view model.
    ///
    /// Must be called inside a tokio runtime.
    pub fn start<P: CredentialProvider>(
        url: &str,
        provider: Arc<P>,
        config: SessionConfig,
    ) -> (Session, JoinHandle<()>) {
        let (commands, command_rx) = mpsc::channel(COMMAND_QUEUE);
        let (events, event_rx) = mpsc::channel(EVENT_QUEUE);
        let initial = Arc::new(ViewModel {
            revision: 0,
            connection: ConnectionView::Connecting,
            projects: Vec::new(),
            project: None,
            notices: Vec::new(),
            user: config.user.clone(),
        });
        let (view_tx, view) = watch::channel(initial);
        let actor = Actor {
            url: url.to_owned(),
            provider,
            config,
            conn: None,
            info: None,
            server_risks: None,
            verified: None,
            distrusted: None,
            commands: command_rx,
            events,
            event_rx,
            view_tx,
            revision: 0,
            connection: ConnectionView::Connecting,
            projects: Vec::new(),
            open: None,
            notices: VecDeque::new(),
            next_notice: 1,
            epoch: 0,
            renew: None,
        };
        let task = tokio::spawn(actor.run());
        (Session { commands, view }, task)
    }

    /// Sends a command. Never blocks: if the queue is full (the actor is far
    /// behind) the command is refused and the UI may try again.
    ///
    /// # Errors
    ///
    /// The command back, if the queue is full or the session has stopped.
    pub fn send(&self, command: Command) -> Result<(), Command> {
        self.commands.try_send(command).map_err(|e| match e {
            mpsc::error::TrySendError::Full(c) | mpsc::error::TrySendError::Closed(c) => c,
        })
    }

    /// The latest view model.
    #[must_use]
    pub fn view(&self) -> Arc<ViewModel> {
        Arc::clone(&self.view.borrow())
    }

    /// A receiver that wakes on every change.
    #[must_use]
    pub fn subscribe(&self) -> watch::Receiver<Arc<ViewModel>> {
        self.view.clone()
    }
}

// ---------------------------------------------------------------------------
// Internals.
// ---------------------------------------------------------------------------

struct Lease {
    token: String,
    last_ok: Instant,
    length: Duration,
}

struct Open {
    epoch: u64,
    id: String,
    entry: ProjectEntry,
    codec: OpCodec,
    ws: Workspace,
    ui: UiState,
    mode: Mode,
    lease: Option<Lease>,
    lock_holder: Option<String>,
    /// An acquire call is in flight.
    acquiring: bool,
    /// The project is being reloaded.
    resyncing: bool,
    /// The user asked to stop editing; release once everything is saved.
    ending: bool,
    /// After a reload, continue into edit mode with this lease.
    resume_edit: Option<Lease>,
    watch: Option<JoinHandle<()>>,
}

impl Open {
    fn stop_tasks(&mut self) {
        if let Some(w) = self.watch.take() {
            w.abort();
        }
    }
}

/// The result of loading a project: everything the actor needs to start.
struct Loaded {
    summary: pb::ProjectSummary,
    last_seq: u64,
    lock_holder: String,
    methodology: Arc<Methodology>,
    project: Project,
}

enum WatchItem {
    Event(pb::WatchEvent),
    /// The stream ended, with the error if it failed.
    Ended(Option<ClientError>),
}

enum Internal<P> {
    Connected(Box<Result<(Connection<P>, ServerInfo), ClientError>>),
    Projects(Result<Vec<pb::ProjectSummary>, ClientError>),
    Created(Result<pb::ProjectSummary, ClientError>),
    Opened {
        epoch: u64,
        result: Box<Result<Loaded, ClientError>>,
    },
    Acquired {
        epoch: u64,
        result: Result<pb::Lease, ClientError>,
    },
    Renewed {
        epoch: u64,
        result: Result<pb::Lease, ClientError>,
    },
    Released {
        epoch: u64,
    },
    Submitted {
        epoch: u64,
        result: Result<u64, ClientError>,
    },
    Watch {
        epoch: u64,
        item: WatchItem,
    },
    RestartWatch {
        epoch: u64,
    },
    RetrySubmit {
        epoch: u64,
    },
    Reconnect,
}

struct Actor<P: CredentialProvider> {
    url: String,
    provider: Arc<P>,
    config: SessionConfig,
    conn: Option<Connection<P>>,
    info: Option<ServerInfo>,
    /// The latest recomputed risks the server sent, and for which project.
    server_risks: Option<(String, pb::RisksEvent)>,
    /// The project and the sequence number at which the local numbers last
    /// matched the server's.
    verified: Option<(String, u64)>,
    /// A project whose local numbers disagreed with the server's: its
    /// numbers come from the server from then on.
    distrusted: Option<String>,
    commands: mpsc::Receiver<Command>,
    events: mpsc::Sender<Internal<P>>,
    event_rx: mpsc::Receiver<Internal<P>>,
    view_tx: watch::Sender<Arc<ViewModel>>,
    revision: u64,
    connection: ConnectionView,
    projects: Vec<ProjectEntry>,
    open: Option<Open>,
    notices: VecDeque<Notice>,
    next_notice: u64,
    epoch: u64,
    renew: Option<tokio::time::Interval>,
}

impl<P: CredentialProvider> Actor<P> {
    async fn run(mut self) {
        self.connect();
        self.publish();
        loop {
            let renew_tick = async {
                match self.renew.as_mut() {
                    Some(interval) => {
                        interval.tick().await;
                    }
                    None => std::future::pending().await,
                }
            };
            tokio::select! {
                command = self.commands.recv() => {
                    match command {
                        Some(Command::Shutdown) | None => break,
                        Some(command) => self.on_command(command),
                    }
                }
                Some(event) = self.event_rx.recv() => self.on_event(event),
                () = renew_tick => self.renew_lease(),
            }
            self.publish();
        }
        self.close_project(false);
    }

    // ---- plumbing --------------------------------------------------------

    fn notice(&mut self, severity: Severity, text: impl Into<String>) {
        if self.notices.len() == MAX_NOTICES {
            self.notices.pop_front();
        }
        let id = self.next_notice;
        self.next_notice += 1;
        self.notices.push_back(Notice {
            id,
            severity,
            text: text.into(),
        });
    }

    /// Runs `work` in a task and delivers its result as an event.
    fn spawn<F, Fut>(&self, work: F)
    where
        F: FnOnce(Connection<P>) -> Fut,
        Fut: std::future::Future<Output = Internal<P>> + Send + 'static,
        P: 'static,
    {
        let Some(conn) = self.conn.clone() else {
            return;
        };
        let events = self.events.clone();
        let fut = work(conn);
        tokio::spawn(async move {
            let event = fut.await;
            // The actor may be gone; then nobody wants the result.
            let _ = events.send(event).await;
        });
    }

    fn after(&self, delay: Duration, event: Internal<P>)
    where
        P: 'static,
    {
        let events = self.events.clone();
        tokio::spawn(async move {
            tokio::time::sleep(delay).await;
            let _ = events.send(event).await;
        });
    }

    fn me(&self) -> &str {
        &self.config.user
    }

    // ---- connection ------------------------------------------------------

    fn connect(&self)
    where
        P: 'static,
    {
        let (url, provider, events) = (
            self.url.clone(),
            Arc::clone(&self.provider),
            self.events.clone(),
        );
        tokio::spawn(async move {
            let result = async {
                let conn = Connection::connect(&url, provider).await?;
                let info = conn.info().await?;
                Ok((conn, info))
            }
            .await;
            let _ = events.send(Internal::Connected(Box::new(result))).await;
        });
    }

    fn on_connected(&mut self, result: Result<(Connection<P>, ServerInfo), ClientError>) {
        match result {
            Ok((conn, info)) => {
                let engine_ok = info.engine_version == aegis_model::ENGINE_VERSION;
                if !engine_ok {
                    self.notice(
                        Severity::Warning,
                        format!(
                            "The server runs engine {} and this app runs {}. Numbers may differ and editing is off. Please update AegisTARA.",
                            info.engine_version,
                            aegis_model::ENGINE_VERSION
                        ),
                    );
                }
                let license = info
                    .license
                    .as_ref()
                    .and_then(|l| LicenseView::from_info(l, unix_secs()));
                if let Some((severity, text)) = license.as_ref().and_then(LicenseView::reminder) {
                    self.notice(severity, text);
                }
                self.connection = ConnectionView::Online {
                    server_version: info.server_version.clone(),
                    engine_ok,
                    license,
                };
                self.conn = Some(conn);
                self.info = Some(info);
                self.refresh_projects();
            }
            Err(ClientError::LoginRequired) => self.connection = ConnectionView::LoginRequired,
            Err(e) => {
                self.connection = ConnectionView::Offline {
                    reason: e.to_string(),
                };
                let delay = self.config.retry_delay;
                self.after(delay, Internal::Reconnect);
            }
        }
    }

    fn engine_ok(&self) -> bool {
        self.info
            .as_ref()
            .is_some_and(|i| i.engine_version == aegis_model::ENGINE_VERSION)
    }

    fn refresh_projects(&self) {
        self.spawn(|conn| async move { Internal::Projects(conn.list_projects().await) });
    }

    /// A failed call tells us if the login is the problem.
    fn on_failure(&mut self, what: &str, error: &ClientError) {
        match error {
            ClientError::LoginRequired => {
                self.connection = ConnectionView::LoginRequired;
                self.notice(
                    Severity::Warning,
                    "Your login has expired. Please log in again.",
                );
            }
            ClientError::Transport(_) => {
                self.notice(
                    Severity::Error,
                    format!("{what}: the server cannot be reached."),
                );
            }
            other => self.notice(Severity::Error, format!("{what}: {other}")),
        }
    }

    // ---- events ----------------------------------------------------------

    fn on_event(&mut self, event: Internal<P>) {
        match event {
            Internal::Connected(result) => self.on_connected(*result),
            Internal::Reconnect => self.connect(),
            Internal::Projects(Ok(list)) => {
                self.projects = list.into_iter().map(entry_of).collect();
            }
            Internal::Projects(Err(e)) => self.on_failure("Could not load the project list", &e),
            Internal::Created(Ok(summary)) => {
                let id = summary.project_id.clone();
                self.projects.push(entry_of(summary));
                self.open_project(&id);
            }
            Internal::Created(Err(e)) => self.on_failure("Could not create the project", &e),
            Internal::Opened { epoch, result } => self.on_opened(epoch, *result),
            Internal::Acquired { epoch, result } => self.on_acquired(epoch, result),
            Internal::Renewed { epoch, result } => self.on_renewed(epoch, result),
            Internal::Released { epoch } => {
                if self.open.as_ref().is_some_and(|o| o.epoch == epoch) {
                    self.set_read_only();
                }
            }
            Internal::Submitted { epoch, result } => self.on_submitted(epoch, result),
            Internal::Watch { epoch, item } => self.on_watch(epoch, item),
            Internal::RestartWatch { epoch } => self.start_watch(epoch),
            Internal::RetrySubmit { epoch } => {
                if self.open.as_ref().is_some_and(|o| o.epoch == epoch) {
                    self.resend();
                }
            }
        }
    }

    // ---- opening and closing --------------------------------------------

    fn open_project(&mut self, id: &str) {
        self.close_project(true);
        self.epoch += 1;
        let epoch = self.epoch;
        self.start_open(epoch, id.to_owned());
    }

    /// Downloads the project and its methodology and builds the model.
    fn start_open(&self, epoch: u64, id: String) {
        self.spawn(move |conn| async move {
            let result = async {
                let downloaded = conn.open_project(&id).await?;
                let key = downloaded.summary.methodology_key.clone();
                let version = downloaded.summary.methodology_version.clone();
                let methodology = match Methodology::iso21434_r155() {
                    Ok(builtin) if builtin.key() == key && builtin.version() == version => {
                        Arc::new(builtin)
                    }
                    _ => conn.methodology(&key).await?,
                };
                let m = Arc::clone(&methodology);
                let snapshot = downloaded.snapshot;
                let project =
                    tokio::task::spawn_blocking(move || Project::load(snapshot.as_slice(), m))
                        .await
                        .map_err(|_| ClientError::Protocol("loading was interrupted".into()))?
                        .map_err(|e| {
                            ClientError::Protocol(format!("the project could not be read: {e}"))
                        })?;
                Ok(Loaded {
                    summary: downloaded.summary,
                    last_seq: downloaded.last_seq,
                    lock_holder: downloaded.lock_holder,
                    methodology,
                    project,
                })
            }
            .await;
            Internal::Opened {
                epoch,
                result: Box::new(result),
            }
        });
    }

    fn on_opened(&mut self, epoch: u64, result: Result<Loaded, ClientError>) {
        if epoch != self.epoch {
            return;
        }
        let loaded = match result {
            Ok(l) => l,
            Err(e) => {
                self.on_failure("Could not open the project", &e);
                if let Some(open) = self.open.as_mut() {
                    open.resyncing = false;
                }
                return;
            }
        };
        let holder = Some(loaded.lock_holder).filter(|h| !h.is_empty());
        let ws = Workspace::new(loaded.project, loaded.last_seq);
        let entry = entry_of(loaded.summary);
        match self.open.as_mut().filter(|o| o.epoch == epoch) {
            // A resync: keep the selection, replace the data.
            Some(open) => {
                open.ws = ws;
                open.lock_holder = holder;
                open.resyncing = false;
                open.mode = Mode::ReadOnly;
                open.lease = None;
                self.renew = None;
                if let Some(lease) = self.open.as_mut().and_then(|o| o.resume_edit.take()) {
                    self.enter_edit(lease);
                }
            }
            None => {
                let ui = UiState {
                    selected_threat: threat_rows(&ws).first().map(|t| t.id),
                    ..UiState::default()
                };
                self.open = Some(Open {
                    epoch,
                    id: entry.id.clone(),
                    codec: OpCodec::new(loaded.methodology),
                    entry,
                    ws,
                    ui,
                    mode: Mode::ReadOnly,
                    lease: None,
                    lock_holder: holder,
                    acquiring: false,
                    resyncing: false,
                    ending: false,
                    resume_edit: None,
                    watch: None,
                });
            }
        }
        self.start_watch(epoch);
    }

    fn close_project(&mut self, release: bool) {
        self.renew = None;
        let Some(mut open) = self.open.take() else {
            return;
        };
        open.stop_tasks();
        if release
            && let Some(lease) = open.lease.take()
            && let Some(conn) = self.conn.clone()
        {
            // Best effort: the lease expires by itself if this fails.
            let id = open.id.clone();
            tokio::spawn(async move {
                let _ = conn.release(&id, &lease.token).await;
            });
        }
        self.epoch += 1;
    }

    /// Reloads the open project from the server, in read-only mode.
    fn resync(&mut self, reason: &str) {
        let Some(open) = self.open.as_mut() else {
            return;
        };
        if open.resyncing {
            return;
        }
        tracing::info!(reason, "resyncing project");
        open.stop_tasks();
        open.resyncing = true;
        open.mode = Mode::ReadOnly;
        open.ending = false;
        self.renew = None;
        // The old lease, if any, is dropped; the server frees it on expiry.
        if let Some(lease) = open.lease.take()
            && let Some(conn) = self.conn.clone()
        {
            let id = open.id.clone();
            tokio::spawn(async move {
                let _ = conn.release(&id, &lease.token).await;
            });
        }
        self.epoch += 1;
        open.epoch = self.epoch;
        let (epoch, id) = (self.epoch, open.id.clone());
        self.start_open(epoch, id);
    }

    // ---- watching --------------------------------------------------------

    fn start_watch(&mut self, epoch: u64) {
        let Some(conn) = self.conn.clone() else {
            return;
        };
        let Some(open) = self.open.as_mut().filter(|o| o.epoch == epoch) else {
            return;
        };
        if let Some(old) = open.watch.take() {
            old.abort();
        }
        let (id, after) = (open.id.clone(), open.ws.confirmed_seq());
        let events = self.events.clone();
        open.watch = Some(tokio::spawn(async move {
            let mut stream = match conn.watch(&id, after).await {
                Ok(s) => s,
                Err(e) => {
                    let _ = events
                        .send(Internal::Watch {
                            epoch,
                            item: WatchItem::Ended(Some(e)),
                        })
                        .await;
                    return;
                }
            };
            loop {
                let item = match stream.message().await {
                    Ok(Some(event)) => WatchItem::Event(event),
                    Ok(None) => WatchItem::Ended(None),
                    Err(status) => WatchItem::Ended(Some(status.into())),
                };
                let ended = matches!(item, WatchItem::Ended(_));
                if events.send(Internal::Watch { epoch, item }).await.is_err() || ended {
                    return;
                }
            }
        }));
    }

    fn on_watch(&mut self, epoch: u64, item: WatchItem) {
        if self.open.as_ref().is_none_or(|o| o.epoch != epoch) {
            return;
        }
        match item {
            WatchItem::Ended(error) => {
                // A server that no longer has our history, or a gap: reload.
                if matches!(
                    error.as_ref().and_then(ClientError::code),
                    Some(ServerCode::OutOfRange)
                ) {
                    self.resync("the server no longer holds the history after our position");
                    return;
                }
                if let Some(e) = &error
                    && matches!(e, ClientError::LoginRequired)
                {
                    self.on_failure("Live updates stopped", e);
                    return;
                }
                let delay = self.config.retry_delay;
                self.after(delay, Internal::RestartWatch { epoch });
            }
            WatchItem::Event(event) => {
                use pb::watch_event::Event;
                match event.event {
                    Some(Event::Lock(lock)) => self.on_lock_event(&lock.holder),
                    Some(Event::Op(op)) => self.on_remote_op(op),
                    Some(Event::Risks(risks)) => self.on_server_risks(risks),
                    Some(Event::Heartbeat(_)) | None => {}
                }
            }
        }
    }

    fn on_lock_event(&mut self, holder: &str) {
        let me = self.me().to_owned();
        let Some(open) = self.open.as_mut() else {
            return;
        };
        open.lock_holder = Some(holder.to_owned()).filter(|h| !h.is_empty());
        // Our lease was taken away (admin break) or expired.
        if open.mode == Mode::Editing && holder != me {
            self.lose_lease("An administrator took the edit lock, or it expired.");
        }
    }

    /// The server's recomputed risks. When they describe exactly the state
    /// shown here, they are checked against this engine's numbers (accuracy:
    /// two engines must agree). A difference means one of them is wrong; the
    /// server's is the official one, so the project is reloaded once and, if
    /// it persists, the server's numbers are shown.
    fn on_server_risks(&mut self, risks: pb::RisksEvent) {
        let engine_ok = self.engine_ok();
        let Some(open) = self.open.as_ref() else {
            return;
        };
        let id = open.id.clone();
        let comparable = !open.resyncing
            && open.ws.pending_batches() == 0
            && risks.as_of_seq == open.ws.confirmed_seq();
        let differs = engine_ok && comparable && risks_differ(&open.ws, &risks);
        let verified_seq = risks.as_of_seq;
        self.server_risks = Some((id.clone(), risks));
        if !differs {
            if engine_ok && comparable {
                self.verified = Some((id, verified_seq));
            }
            return;
        }
        if self.distrusted.as_deref() == Some(id.as_str()) {
            return;
        }
        self.distrusted = Some(id);
        tracing::error!("the local risk numbers differ from the server's");
        self.notice(
            Severity::Error,
            "The risk numbers on this computer differ from the server's. The project was reloaded and the server's numbers are shown. Please report this.",
        );
        self.resync("the local risks differ from the server's");
    }

    fn on_remote_op(&mut self, op: pb::AcceptedOp) {
        let Some(open) = self.open.as_mut() else {
            return;
        };
        // While editing, ops can only be our own; read-only applies them.
        if open.mode == Mode::Editing || open.resyncing || op.seq <= open.ws.confirmed_seq() {
            return;
        }
        let applied = open
            .codec
            .decode(&op.op)
            .map_err(|_| ())
            .and_then(|decoded| open.ws.apply_remote(op.seq, decoded).map_err(|_| ()));
        if applied.is_err() {
            self.resync("a remote op could not be applied");
        }
    }

    // ---- editing mode ----------------------------------------------------

    fn begin_edit(&mut self) {
        if !self.engine_ok() {
            self.notice(
                Severity::Warning,
                "Editing is off because this app is older or newer than the server.",
            );
            return;
        }
        let Some(open) = self.open.as_mut() else {
            return;
        };
        if open.mode == Mode::Editing || open.acquiring || open.resyncing {
            return;
        }
        open.acquiring = true;
        let (epoch, id) = (open.epoch, open.id.clone());
        self.spawn(move |conn| async move {
            Internal::Acquired {
                epoch,
                result: conn.acquire(&id).await,
            }
        });
    }

    fn on_acquired(&mut self, epoch: u64, result: Result<pb::Lease, ClientError>) {
        let Some(open) = self.open.as_mut().filter(|o| o.epoch == epoch) else {
            return;
        };
        open.acquiring = false;
        match result {
            Err(e) if matches!(e.code(), Some(ServerCode::LicenseExpired)) => {
                self.notice(
                    Severity::Warning,
                    "The license has expired, so editing is off. You can still read. Ask your administrator to renew it.",
                );
            }
            Err(e) if matches!(e.code(), Some(ServerCode::LicenseSeatsFull)) => {
                self.notice(
                    Severity::Warning,
                    "All licensed seats are in use by other people, so you can look but not change anything. Ask your administrator for more seats.",
                );
            }
            Err(e) => {
                let held = matches!(e.code(), Some(ServerCode::LockHeld));
                if held {
                    self.notice(
                        Severity::Warning,
                        format!("{e}. You can look, but not change anything."),
                    );
                } else {
                    self.on_failure("Could not take the edit lock", &e);
                }
            }
            Ok(lease) => {
                let length = Duration::from_millis(lease.expires_in_ms.max(1000));
                let mine = Lease {
                    token: lease.token,
                    last_ok: Instant::now(),
                    length,
                };
                if lease.last_seq == open.ws.confirmed_seq() {
                    self.enter_edit(mine);
                } else {
                    // Someone edited since we looked: reload, then continue.
                    open.resume_edit = Some(mine);
                    self.resync("the project changed before the lock was granted");
                }
            }
        }
    }

    fn enter_edit(&mut self, lease: Lease) {
        let Some(open) = self.open.as_mut() else {
            return;
        };
        let length = lease.length;
        open.lease = Some(lease);
        open.mode = Mode::Editing;
        open.ending = false;
        // Renew at a third of the lease length: two misses are survivable.
        let period = (length / 3).max(Duration::from_millis(500));
        let mut interval = tokio::time::interval_at(Instant::now() + period, period);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        self.renew = Some(interval);
    }

    fn set_read_only(&mut self) {
        self.renew = None;
        if let Some(open) = self.open.as_mut() {
            open.lease = None;
            open.mode = Mode::ReadOnly;
            open.ending = false;
        }
    }

    /// The user is done editing: save what is pending, then release.
    fn end_edit(&mut self) {
        let Some(open) = self.open.as_mut() else {
            return;
        };
        if open.mode != Mode::Editing {
            return;
        }
        open.ending = true;
        self.finish_ending();
    }

    fn finish_ending(&mut self) {
        let Some(open) = self.open.as_mut() else {
            return;
        };
        if !open.ending || open.ws.pending_batches() > 0 {
            return;
        }
        let Some(lease) = open.lease.as_ref() else {
            return;
        };
        let (epoch, id, token) = (open.epoch, open.id.clone(), lease.token.clone());
        open.ending = false;
        self.spawn(move |conn| async move {
            // Whether or not the release call works, we stop editing.
            let _ = conn.release(&id, &token).await;
            Internal::Released { epoch }
        });
    }

    fn renew_lease(&mut self) {
        let Some(open) = self.open.as_ref() else {
            return;
        };
        let Some(lease) = open.lease.as_ref() else {
            return;
        };
        let (epoch, id, token) = (open.epoch, open.id.clone(), lease.token.clone());
        self.spawn(move |conn| async move {
            Internal::Renewed {
                epoch,
                result: conn.renew(&id, &token).await,
            }
        });
    }

    fn on_renewed(&mut self, epoch: u64, result: Result<pb::Lease, ClientError>) {
        let Some(open) = self.open.as_mut().filter(|o| o.epoch == epoch) else {
            return;
        };
        match result {
            Ok(_) => {
                if let Some(lease) = open.lease.as_mut() {
                    lease.last_ok = Instant::now();
                }
            }
            Err(e) => {
                let gone = matches!(
                    e.code(),
                    Some(ServerCode::LockInvalid | ServerCode::NotLocked | ServerCode::LockHeld)
                );
                let expired = open
                    .lease
                    .as_ref()
                    .is_none_or(|l| l.last_ok.elapsed() >= l.length);
                if gone || expired {
                    self.lose_lease("The edit lock was lost.");
                }
            }
        }
    }

    /// The lease is gone: unsaved edits cannot be saved. Roll them back,
    /// go read-only, and reload so the user sees the server's truth.
    fn lose_lease(&mut self, why: &str) {
        let Some(open) = self.open.as_mut() else {
            return;
        };
        let lost = open.ws.on_rejected();
        let text = if lost > 0 {
            format!("{why} {lost} unsaved change(s) were discarded.")
        } else {
            format!("{why} You are now in read-only mode.")
        };
        self.notice(Severity::Warning, text);
        self.resync("lease lost");
    }

    // ---- saving ----------------------------------------------------------

    /// Sends the next batch of edits if nothing is in flight.
    fn pump(&mut self) {
        let Some(open) = self.open.as_mut() else {
            return;
        };
        if open.lease.is_none() || open.resyncing {
            return;
        }
        let Some(submission) = open.ws.next_submission() else {
            return;
        };
        self.send_submission(&submission);
    }

    fn resend(&mut self) {
        let Some(open) = self.open.as_mut() else {
            return;
        };
        if let Some(submission) = open.ws.in_flight_submission() {
            self.send_submission(&submission);
        }
    }

    fn send_submission(&mut self, submission: &crate::workspace::Submission) {
        let Some(open) = self.open.as_mut() else {
            return;
        };
        let Some(lease) = open.lease.as_ref() else {
            return;
        };
        let encoded: Result<Vec<_>, _> = submission
            .ops
            .iter()
            .map(|op| open.codec.encode(op).map(bytes::Bytes::from))
            .collect();
        let ops = match encoded {
            Ok(ops) => ops,
            Err(_) => {
                open.ws.on_rejected();
                self.notice(
                    Severity::Error,
                    "That change cannot be saved: it is too large.",
                );
                return;
            }
        };
        let (epoch, id, token, base) = (
            open.epoch,
            open.id.clone(),
            lease.token.clone(),
            submission.base_seq,
        );
        self.spawn(move |conn| async move {
            Internal::Submitted {
                epoch,
                result: conn.submit(&id, &token, base, ops).await,
            }
        });
    }

    fn on_submitted(&mut self, epoch: u64, result: Result<u64, ClientError>) {
        if self.open.as_ref().is_none_or(|o| o.epoch != epoch) {
            return;
        }
        match result {
            Ok(last_seq) => {
                let accepted = self.open.as_mut().map(|o| o.ws.on_accepted(last_seq));
                if matches!(accepted, Some(Err(WorkspaceError::Desync { .. }))) {
                    self.resync("the server's sequence does not match");
                    return;
                }
                self.pump();
                self.finish_ending();
            }
            Err(e) => self.on_submit_failed(epoch, &e),
        }
    }

    fn on_submit_failed(&mut self, epoch: u64, error: &ClientError) {
        match error.code() {
            Some(ServerCode::StaleBase { .. }) => {
                if let Some(open) = self.open.as_mut() {
                    open.ws.on_rejected();
                }
                self.notice(Severity::Warning, "The project changed on the server. It was reloaded; your last change was not saved.");
                self.resync("stale base");
            }
            Some(ServerCode::LockInvalid | ServerCode::NotLocked | ServerCode::LockHeld) => {
                self.lose_lease("The edit lock was lost.");
            }
            Some(ServerCode::MalformedOp | ServerCode::RejectedOp) => {
                if let Some(open) = self.open.as_mut() {
                    open.ws.on_rejected();
                }
                self.notice(
                    Severity::Error,
                    format!("The server refused the change: {error}. It was undone."),
                );
            }
            _ if error.is_transient() => {
                // Keep the edits; try the same call again shortly.
                let delay = self.config.retry_delay;
                self.after(delay, Internal::RetrySubmit { epoch });
                self.notice(
                    Severity::Warning,
                    "Saving is delayed: the server cannot be reached. Retrying.",
                );
            }
            _ => {
                if let Some(open) = self.open.as_mut() {
                    open.ws.on_rejected();
                }
                self.on_failure("Could not save the change", error);
            }
        }
    }

    // ---- commands --------------------------------------------------------

    fn on_command(&mut self, command: Command) {
        match command {
            Command::Shutdown => {}
            Command::Reconnect => {
                self.connection = ConnectionView::Connecting;
                self.connect();
            }
            Command::Refresh => self.refresh_projects(),
            Command::CreateProject {
                name,
                methodology_key,
            } => {
                self.spawn(move |conn| async move {
                    Internal::Created(conn.create_project(&name, &methodology_key).await)
                });
            }
            Command::OpenProject { id } => self.open_project(&id),
            Command::CloseProject => self.close_project(true),
            Command::SelectThreat(position) => {
                if let Some(open) = self.open.as_mut() {
                    open.ui.selected_threat = threat_rows(&open.ws).get(position).map(|t| t.id);
                    open.ui.selected_node = None;
                }
            }
            Command::SelectNode(position) => self.select_node(position),
            Command::ShowResidual(on) => {
                if let Some(open) = self.open.as_mut() {
                    open.ui.show_residual = on;
                }
            }
            Command::BeginEdit => self.begin_edit(),
            Command::EndEdit => self.end_edit(),
            Command::Edit(edit) => {
                let (user, now) = (self.config.user.clone(), (self.config.clock_ms)());
                self.apply_edit(
                    |project| edit.to_ops(project, &user, now).map_err(|e| e.to_string()),
                    Origin::Edit,
                );
            }
            Command::Undo => self.history(true),
            Command::Redo => self.history(false),
            Command::CreateSample => {
                let (user, now) = (self.config.user.clone(), (self.config.clock_ms)());
                self.apply_edit(
                    |project| {
                        if project.asset_count() > 0 {
                            return Err("the project already has content".to_owned());
                        }
                        sample_ops(project.methodology(), &user, now)
                            .ok_or_else(|| "this methodology has no sample".to_owned())
                    },
                    Origin::Edit,
                );
            }
            Command::DismissNotice(id) => self.notices.retain(|n| n.id != id),
        }
    }

    fn select_node(&mut self, position: Option<usize>) {
        let Some(open) = self.open.as_mut() else {
            return;
        };
        let Some(position) = position else {
            open.ui.selected_node = None;
            return;
        };
        let Some(view) = tree_view(&open.ws, &open.ui) else {
            return;
        };
        let Some(idx) = u32::try_from(position)
            .ok()
            .and_then(|p| view.scene.node_idx(p))
        else {
            return;
        };
        let project = open.ws.project();
        let node = project
            .attack_tree_idx(&view.tree)
            .and_then(|t| project.attack_tree(t))
            .and_then(|t| t.id_of(idx));
        open.ui.selected_node = node;
    }

    /// Whether edits are allowed right now; tells the user why not.
    fn can_edit(&mut self) -> bool {
        let state = self.open.as_ref().map(|o| (o.mode, o.resyncing));
        match state {
            Some((Mode::Editing, false)) => true,
            Some((_, true)) => {
                self.notice(
                    Severity::Info,
                    "The project is reloading. Try again in a moment.",
                );
                false
            }
            Some((Mode::ReadOnly, false)) => {
                self.notice(Severity::Info, "Press Edit to take the edit lock first.");
                false
            }
            None => false,
        }
    }

    fn apply_edit(
        &mut self,
        build: impl FnOnce(&Project) -> Result<Vec<aegis_model::Op>, String>,
        origin: Origin,
    ) {
        if !self.can_edit() {
            return;
        }
        let Some(open) = self.open.as_mut() else {
            return;
        };
        let ops = match build(open.ws.project()) {
            Ok(ops) => ops,
            Err(why) => {
                self.notice(
                    Severity::Warning,
                    format!("That change is not possible: {why}."),
                );
                return;
            }
        };
        match open.ws.apply(ops, origin) {
            Ok(()) => self.pump(),
            Err(e) => self.notice(Severity::Warning, format!("That change was refused: {e}.")),
        }
    }

    fn history(&mut self, undo: bool) {
        if !self.can_edit() {
            return;
        }
        let Some(open) = self.open.as_mut() else {
            return;
        };
        let result = if undo { open.ws.undo() } else { open.ws.redo() };
        match result {
            Ok(()) => self.pump(),
            Err(e @ (WorkspaceError::NothingToUndo | WorkspaceError::NothingToRedo)) => {
                self.notice(Severity::Info, format!("{e}."));
            }
            Err(e) => self.notice(Severity::Warning, format!("That change was refused: {e}.")),
        }
    }

    // ---- view ------------------------------------------------------------

    /// Keeps the selection pointing at something that exists: the first threat
    /// when nothing is selected (a project that just got its first threats)
    /// or when the selected one is gone (an edit was rolled back).
    fn ensure_selection(&mut self) {
        let Some(open) = self.open.as_mut() else {
            return;
        };
        let threats = threat_rows(&open.ws);
        let still_there = open
            .ui
            .selected_threat
            .is_some_and(|id| threats.iter().any(|t| t.id == id));
        if !still_there {
            open.ui.selected_threat = threats.first().map(|t| t.id);
            open.ui.selected_node = None;
        }
    }

    fn publish(&mut self) {
        self.ensure_selection();
        let engine_ok = self.engine_ok();
        self.revision += 1;
        let project = self.open.as_ref().map(|open| {
            let mut threats = threat_rows(&open.ws);
            // This engine's numbers are shown, except when it cannot be
            // trusted: it differs from the server's, or disagreed with it.
            let distrust = !engine_ok || self.distrusted.as_deref() == Some(open.id.as_str());
            if distrust
                && let Some((project, risks)) = &self.server_risks
                && *project == open.id
                && risks.as_of_seq == open.ws.confirmed_seq()
                && open.ws.pending_batches() == 0
            {
                use_server_risks(&open.ws, &mut threats, risks);
            }
            let selected_threat = open
                .ui
                .selected_threat
                .and_then(|id| threats.iter().position(|t| t.id == id));
            let tree = tree_view(&open.ws, &open.ui);
            let selected_node = tree.as_ref().and_then(|t| t.selected);
            ProjectView {
                entry: open.entry.clone(),
                mode: open.mode,
                lock_holder: open.lock_holder.clone(),
                sync: if open.resyncing {
                    SyncView::Resyncing
                } else if open.ws.pending_batches() > 0 {
                    SyncView::Syncing {
                        pending: open.ws.pending_batches(),
                    }
                } else {
                    SyncView::Synced
                },
                is_empty: open.ws.project().asset_count() == 0,
                inspector: inspector(&open.ws, &open.ui),
                can_undo: open.ws.can_undo(),
                can_redo: open.ws.can_redo(),
                show_residual: open.ui.show_residual,
                confirmed_seq: open.ws.confirmed_seq(),
                risks_verified_seq: self
                    .verified
                    .as_ref()
                    .filter(|(project, _)| *project == open.id)
                    .map(|(_, seq)| *seq),
                threats,
                selected_threat,
                tree,
                selected_node,
            }
        });
        self.view_tx.send_replace(Arc::new(ViewModel {
            revision: self.revision,
            connection: self.connection.clone(),
            projects: self.projects.clone(),
            project,
            notices: self.notices.iter().cloned().collect(),
            user: self.config.user.clone(),
        }));
    }
}

fn entry_of(summary: pb::ProjectSummary) -> ProjectEntry {
    ProjectEntry {
        id: summary.project_id,
        name: summary.name,
        methodology: summary.methodology_key,
    }
}
