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
//!     ▲  every `version_check`: is there    │ the lease is checked by the
//!     │  a newer version? (notice only)     │ renewal and by every save
//!     └──────────────────────────────────┴──── resync (reload) on any doubt
//! ```
//!
//! There is no live stream (decision D23). A read-only user sees the project
//! as last loaded; now and then the client asks the server for the newest
//! sequence number and, if it is ahead, raises a "newer version" flag that
//! the UI turns into a reload offer. The editor is the only writer, so it
//! needs no stream: a lost lease shows in the answer to a renewal or a save.
//! After the project opens and after every confirmed batch the client asks
//! the server for its own risk numbers (`GetRisks`) to check against its
//! engine.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use aegis_auth::layer::CredentialProvider;
use aegis_model::{
    AssetId, ClaimId, ComponentId, ControlId, DamageId, DataFlowId, GoalId, InterfaceId,
    Methodology, OpCodec, Project, ThreatId, TreeId, TrustBoundaryId,
};
use aegis_proto as pb;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tokio::time::Instant;

use crate::commands::{Command, Edit};
use crate::connection::{Connection, ServerInfo};
use crate::error::{ClientError, ServerCode};
use crate::pages;
use crate::report::{
    ExportKind, FigureAttachment, ReportContext, ReportOptions, ReportProfile,
    report_html_with_options, report_pdf_with_options, report_workbook_xlsx_with_figures,
    risk_register_csv, tree_svg,
};
use crate::sample::sample_ops;
use crate::state::{
    BaselineRow, ConnectionView, HistoryRow, LicenseView, Me, MembershipRow, Mode, Notice,
    NoticeAction, NumbersCheck, PageData, ProjectEntry, ProjectView, ServerSettingsView, Severity,
    SyncView, UserRow, ViewModel,
};
use crate::time::format_ms;
use crate::undo_label::Label;
use crate::updates::{UpdateChecker, UpdateError, UpdateOffer, UpdateStatus};
use crate::view::{
    AdminAction, DraftKind, Page, UiState, inspector, page_descriptions, risks_differ, shown_tree,
    threat_rows, tree_view_cached, use_server_risks,
};
use crate::workspace::{Origin, Workspace, WorkspaceError};

/// Most notices kept.
const MAX_NOTICES: usize = 5;
/// Commands queued from the UI before it must wait (R4).
const COMMAND_QUEUE: usize = 256;
/// Events queued from tasks before they must wait (R4).
const EVENT_QUEUE: usize = 256;

/// Called after a logout, so the UI can forget the credentials (the session
/// does not know what kind they are).
#[derive(Clone)]
pub struct LogOutHook(pub Arc<dyn Fn() + Send + Sync>);

impl std::fmt::Debug for LogOutHook {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("LogOutHook")
    }
}

/// Tunables.
#[derive(Clone, Debug)]
pub struct SessionConfig {
    /// Who the user is, for provenance and display. Replaced by the
    /// server's answer to `WhoAmI` once connected.
    pub user: String,
    /// Wall clock in milliseconds since the epoch.
    pub clock_ms: fn() -> u64,
    /// Wait before reconnecting or retrying a save.
    pub retry_delay: Duration,
    /// Which protocol to speak.
    pub transport: crate::transport::TransportKind,
    /// Called after [`Command::LogOut`].
    pub on_log_out: Option<LogOutHook>,
    /// How often a read-only client asks whether the project has a newer
    /// version on the server.
    pub version_check: Duration,
    /// Where the signed update manifest is. Empty: no address is set.
    pub update_url: String,
    /// Checks the manifest against the update keys of this build.
    pub update_checker: Arc<UpdateChecker>,
    /// Where a downloaded installer is saved (job 054).
    pub update_dir: crate::updates::DownloadDir,
    /// The HTTP client for update checks and downloads. Tests give it a
    /// client that trusts a test certificate authority.
    pub update_http: Arc<aegis_auth::http::HyperClient>,
}

impl SessionConfig {
    /// Defaults for `user`: system clock, 2 s retry delay, 30 s version
    /// check, the default transport for the server address (decided at
    /// start).
    #[must_use]
    pub fn new(user: &str) -> Self {
        SessionConfig {
            user: user.to_owned(),
            clock_ms: system_clock_ms,
            retry_delay: Duration::from_secs(2),
            transport: crate::transport::TransportKind::Grpc,
            on_log_out: None,
            version_check: Duration::from_secs(30),
            update_url: String::new(),
            update_checker: Arc::new(UpdateChecker::for_this_build()),
            update_dir: crate::updates::DownloadDir::for_this_user(),
            update_http: Arc::new(aegis_auth::http::HyperClient::new()),
        }
    }

    /// The same, with a chosen transport.
    #[must_use]
    pub fn with_transport(mut self, kind: crate::transport::TransportKind) -> Self {
        self.transport = kind;
        self
    }
}

/// How many events the administration page loads.
const EVENT_ROWS: u32 = 200;

/// How many recent changes the history page shows.
const HISTORY_ROWS: u32 = 200;

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
            me: None,
            users: Vec::new(),
            admin_text: None,
            settings: None,
            events: Vec::new(),
            events_error: None,
            sessions: Vec::new(),
            memberships: Vec::new(),
            memberships_error: None,
            membership_error: None,
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
            check: None,
            me: None,
            users: Vec::new(),
            admin_text: None,
            settings: None,
            audit_rows: Vec::new(),
            events_error: None,
            session_rows: Vec::new(),
            memberships: Vec::new(),
            memberships_error: None,
            membership_error: None,
            offered: None,
            downloading: false,
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
    /// The server's newest sequence number, when it is ahead of ours.
    newer_seq: Option<u64>,
    /// A status call is in flight.
    status_pending: bool,
    /// A `GetRisks` call is in flight.
    risks_pending: bool,
    /// Another batch was confirmed while a `GetRisks` call was in flight.
    risks_again: bool,
    /// The methodology's labels.
    vocab: Arc<pages::Vocabulary>,
    /// Baselines, oldest first.
    baselines: Vec<BaselineRow>,
    /// The baseline being compared with: its name and project.
    compare: Option<(String, Project)>,
    /// Recent changes.
    history: Vec<HistoryRow>,
    /// The last export written.
    last_export: Option<String>,
    /// Cached laid-out scene for `(tree_id, show_residual)`, invalidated on
    /// workspace edits/undo/redo.
    cached_scene: Option<(aegis_model::TreeId, bool, Arc<aegis_canvas::Scene>)>,
    /// Cached problem count for the sidebar badge, invalidated on workspace
    /// edits/undo/redo.
    cached_lint_count: Option<usize>,
    /// Figures attached to the project or its elements (job 073).
    figures: Vec<FigureAttachment>,
    /// Current dry-run preview of an Excel TARA import (job 080).
    excel_preview: Option<crate::excel::ExcelPreview>,
}

/// The result of loading a project: everything the actor needs to start.
struct Loaded {
    summary: pb::ProjectSummary,
    last_seq: u64,
    lock_holder: String,
    methodology: Arc<Methodology>,
    project: Project,
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
    Status {
        epoch: u64,
        result: Result<pb::ProjectStatus, ClientError>,
    },
    Risks {
        epoch: u64,
        result: Result<pb::GetRisksResponse, ClientError>,
    },
    RetrySubmit {
        epoch: u64,
    },
    Reconnect,
    Me(Result<pb::WhoAmIResponse, ClientError>),
    Baselines {
        epoch: u64,
        result: Result<Vec<pb::BaselineInfo>, ClientError>,
    },
    BaselineCreated {
        epoch: u64,
        result: Result<pb::BaselineInfo, ClientError>,
    },
    Compared {
        epoch: u64,
        name: String,
        result: Box<Result<Project, ClientError>>,
    },
    History {
        epoch: u64,
        result: Result<Vec<pb::HistoryEntry>, ClientError>,
    },
    Figures {
        epoch: u64,
        result: Result<Vec<FigureAttachment>, ClientError>,
    },
    FigureSaved {
        epoch: u64,
        result: Result<FigureAttachment, ClientError>,
    },
    FigureDeleted {
        epoch: u64,
        figure_id: String,
        result: Result<(), ClientError>,
    },
    Exported {
        epoch: u64,
        result: Result<String, String>,
    },
    Renamed {
        epoch: u64,
        result: Result<pb::ProjectSummary, ClientError>,
    },
    Deleted {
        id: String,
        result: Result<(), ClientError>,
    },
    Users(Result<Vec<pb::UserInfo>, ClientError>),
    Updates(Result<UpdateStatus, UpdateError>),
    Downloaded(Result<std::path::PathBuf, UpdateError>),
    Events(Result<Vec<pb::EventInfo>, ClientError>),
    Sessions(Result<Vec<pb::SessionInfo>, ClientError>),
    Memberships(Result<Vec<pb::MembershipInfo>, ClientError>),
    MembershipDone {
        what: String,
        result: Result<(), ClientError>,
    },
    Settings(Result<pb::ServerSettings, ClientError>),
    SettingsSet(Result<pb::ServerSettings, ClientError>),
    AdminDone {
        what: String,
        result: Result<(), ClientError>,
        reload_users: bool,
    },
    Audit(Result<pb::VerifyAuditResponse, ClientError>),
    PasswordChanged(Result<(), ClientError>),
    LoggedOut,
}

struct Actor<P: CredentialProvider> {
    url: String,
    provider: Arc<P>,
    config: SessionConfig,
    conn: Option<Connection<P>>,
    info: Option<ServerInfo>,
    /// The latest recomputed risks the server sent, and for which project.
    server_risks: Option<(String, pb::GetRisksResponse)>,
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
    /// Asks a read-only client's server for a newer version.
    check: Option<tokio::time::Interval>,
    me: Option<Me>,
    users: Vec<UserRow>,
    admin_text: Option<String>,
    settings: Option<ServerSettingsView>,
    audit_rows: Vec<crate::audit::EventRow>,
    events_error: Option<String>,
    session_rows: Vec<crate::audit::SessionRow>,
    /// The open project's membership rows (project access, job 058).
    memberships: Vec<MembershipRow>,
    /// Why the membership list is empty when it could not be read.
    memberships_error: Option<String>,
    /// The last membership add/change/remove error, shown inline.
    membership_error: Option<String>,
    /// The release the last update check offered, for "Download update".
    offered: Option<UpdateOffer>,
    /// An installer download is running.
    downloading: bool,
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
            let check_tick = async {
                match self.check.as_mut() {
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
                () = check_tick => self.request_status(),
            }
            self.publish();
        }
        self.close_project(false);
    }

    // ---- plumbing --------------------------------------------------------

    fn notice(&mut self, severity: Severity, text: impl Into<String>) {
        self.notice_with(severity, text, None);
    }

    fn notice_with(
        &mut self,
        severity: Severity,
        text: impl Into<String>,
        action: Option<NoticeAction>,
    ) {
        if self.notices.len() == MAX_NOTICES {
            self.notices.pop_front();
        }
        if action.is_some() {
            // UI-10.9: a toast with a button replaces the previous one.
            self.notices.retain(|n| n.action.is_none());
        }
        let id = self.next_notice;
        self.next_notice += 1;
        self.notices.push_back(Notice {
            id,
            severity,
            text: text.into(),
            action,
        });
    }

    /// UI-10.7: says so when the undo history was dropped (a rejected edit,
    /// a reload, a lost lock).
    fn history_cleared(&mut self, why: &str) {
        let steps = self.open.as_mut().map_or(0, |o| o.ws.take_cleared());
        if steps > 0 {
            self.notice(Severity::Info, format!("Undo history cleared: {why}."));
        }
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

    // ---- connection ------------------------------------------------------

    fn connect(&self)
    where
        P: 'static,
    {
        let (url, provider, events, kind) = (
            self.url.clone(),
            Arc::clone(&self.provider),
            self.events.clone(),
            self.config.transport,
        );
        tokio::spawn(async move {
            let result = async {
                let conn = Connection::connect_with(&url, kind, provider).await?;
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
                self.spawn(|conn| async move { Internal::Me(conn.who_am_i().await) });
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
            Internal::Status { epoch, result } => self.on_status(epoch, result),
            Internal::Risks { epoch, result } => self.on_risks(epoch, result),
            Internal::RetrySubmit { epoch } => {
                if self.open.as_ref().is_some_and(|o| o.epoch == epoch) {
                    self.resend();
                }
            }
            other => self.on_tool_event(other),
        }
    }

    /// Answers of the calls around the editing core: account, versions,
    /// history, exports, administration.
    #[allow(clippy::too_many_lines)]
    fn on_tool_event(&mut self, event: Internal<P>) {
        let current =
            |open: &Option<Open>, epoch: u64| open.as_ref().is_some_and(|o| o.epoch == epoch);
        match event {
            Internal::Me(Ok(me)) => {
                // The lease holder and the audit actor are the server's
                // subject: use exactly that, or our own lease looks foreign.
                if !me.subject.is_empty() {
                    self.config.user.clone_from(&me.subject);
                }
                self.me = Some(Me {
                    subject: me.subject,
                    display_name: me.display_name,
                    admin: me.admin,
                    method: me.method,
                });
            }
            // An older server without WhoAmI: keep the configured name.
            Internal::Me(Err(_)) => {}
            Internal::Baselines { epoch, result } => match result {
                Ok(list) if current(&self.open, epoch) => {
                    if let Some(open) = self.open.as_mut() {
                        open.baselines = list
                            .into_iter()
                            .map(|b| BaselineRow {
                                id: b.baseline_id,
                                name: b.name,
                                seq: b.seq,
                                created_by: b.created_by,
                                created: format_ms(b.created_ms),
                            })
                            .collect();
                    }
                }
                Ok(_) => {}
                Err(e) => self.on_failure("Could not load the baselines", &e),
            },
            Internal::BaselineCreated { epoch, result } => match result {
                Ok(b) => {
                    self.notice(
                        Severity::Info,
                        format!("Baseline \"{}\" saved at change {}.", b.name, b.seq),
                    );
                    if current(&self.open, epoch) {
                        self.load_baselines();
                    }
                }
                Err(e) => self.on_failure("Could not create the baseline", &e),
            },
            Internal::Compared {
                epoch,
                name,
                result,
            } => match *result {
                Ok(project) if current(&self.open, epoch) => {
                    if let Some(open) = self.open.as_mut() {
                        open.compare = Some((name, project));
                    }
                }
                Ok(_) => {}
                Err(e) => self.on_failure("Could not load the baseline", &e),
            },
            Internal::History { epoch, result } => match result {
                Ok(entries) if current(&self.open, epoch) => {
                    if let Some(open) = self.open.as_mut() {
                        open.history = entries
                            .into_iter()
                            .map(|h| HistoryRow {
                                seq: h.seq,
                                actor: h.actor,
                                when: format_ms(h.timestamp_ms),
                                kind: kind_words(&h.kind),
                            })
                            .collect();
                    }
                }
                Ok(_) => {}
                Err(e) => self.on_failure("Could not load the history", &e),
            },
            Internal::Figures { epoch, result } => match result {
                Ok(figs) if current(&self.open, epoch) => {
                    if let Some(open) = self.open.as_mut() {
                        open.figures = figs;
                    }
                }
                Ok(_) => {}
                Err(e) => tracing::debug!(error = %e, "could not load figures"),
            },
            Internal::FigureSaved { epoch, result } => match result {
                Ok(fig) => {
                    self.notice(
                        Severity::Info,
                        format!(
                            "Saved figure \"{}\".",
                            if fig.caption.is_empty() {
                                &fig.figure_id
                            } else {
                                &fig.caption
                            }
                        ),
                    );
                    if current(&self.open, epoch)
                        && let Some(open) = self.open.as_mut()
                    {
                        if let Some(existing) = open
                            .figures
                            .iter_mut()
                            .find(|f| f.figure_id == fig.figure_id)
                        {
                            existing.caption = fig.caption;
                            existing.target = fig.target;
                            existing.updated_by = fig.updated_by;
                            existing.updated_at_ms = fig.updated_at_ms;
                            if !fig.data.is_empty() {
                                existing.data = fig.data;
                            }
                        } else {
                            open.figures.push(fig);
                        }
                    }
                }
                Err(e) => self.on_failure("Could not save the figure", &e),
            },
            Internal::FigureDeleted {
                epoch,
                figure_id,
                result,
            } => match result {
                Ok(()) => {
                    self.notice(Severity::Info, "The figure was deleted.");
                    if current(&self.open, epoch)
                        && let Some(open) = self.open.as_mut()
                    {
                        open.figures.retain(|f| f.figure_id != figure_id);
                    }
                }
                Err(e) => self.on_failure("Could not delete the figure", &e),
            },
            Internal::Exported { epoch, result } => match result {
                Ok(path) => {
                    self.notice(Severity::Info, format!("Saved {path}"));
                    if current(&self.open, epoch)
                        && let Some(open) = self.open.as_mut()
                    {
                        open.last_export = Some(path);
                    }
                }
                Err(why) => self.notice(Severity::Error, format!("The export failed: {why}")),
            },
            Internal::Renamed { epoch, result } => match result {
                Ok(summary) => {
                    for p in &mut self.projects {
                        if p.id == summary.project_id {
                            p.name.clone_from(&summary.name);
                        }
                    }
                    if current(&self.open, epoch)
                        && let Some(open) = self.open.as_mut()
                    {
                        open.entry.name = summary.name;
                    }
                }
                Err(e) => self.on_failure("Could not rename the project", &e),
            },
            Internal::Deleted { id, result } => match result {
                Ok(()) => {
                    self.notice(Severity::Info, "The project was deleted.");
                    if self.open.as_ref().is_some_and(|o| o.id == id) {
                        self.close_project(false);
                    }
                    self.projects.retain(|p| p.id != id);
                }
                Err(e) => self.on_failure("Could not delete the project", &e),
            },
            Internal::Users(result) => match result {
                Ok(users) => {
                    self.users = users
                        .into_iter()
                        .map(|u| UserRow {
                            username: u.username,
                            display_name: u.display_name,
                            admin: u.admin,
                            disabled: u.disabled,
                            last_login: if u.last_login_ms == 0 {
                                "never".to_owned()
                            } else {
                                format_ms(u.last_login_ms)
                            },
                            sessions: u.sessions,
                        })
                        .collect();
                }
                Err(e) => self.on_failure("Could not load the users", &e),
            },
            Internal::Events(result) => match result {
                Ok(events) => {
                    self.audit_rows = events.iter().map(crate::audit::event_row).collect();
                    self.events_error = None;
                }
                Err(e) => {
                    self.audit_rows.clear();
                    self.events_error = Some(match e.code() {
                        Some(ServerCode::Other(c)) if c == "no_event_log" => {
                            "This server keeps no event log.".to_owned()
                        }
                        _ => format!("The event log could not be read: {e}"),
                    });
                }
            },
            Internal::Sessions(result) => match result {
                Ok(list) => {
                    self.session_rows = list.iter().map(crate::audit::session_row).collect()
                }
                Err(e) => {
                    self.session_rows.clear();
                    self.on_failure("Could not load the sessions", &e);
                }
            },
            Internal::Memberships(result) => match result {
                Ok(list) => {
                    self.memberships = list
                        .into_iter()
                        .map(|m| MembershipRow {
                            name: m
                                .member
                                .split_once(':')
                                .map_or(m.member.as_str(), |(_, n)| n)
                                .to_owned(),
                            member: m.member,
                            role: m.role,
                        })
                        .collect();
                    self.memberships_error = None;
                    self.membership_error = None;
                }
                Err(e) => {
                    self.memberships.clear();
                    self.memberships_error =
                        Some(format!("The membership list could not be read: {e}"));
                }
            },
            Internal::MembershipDone { what, result } => match result {
                Ok(()) => {
                    self.notice(Severity::Info, format!("{what}: done."));
                    self.membership_error = None;
                    self.load_memberships();
                }
                Err(e) => {
                    self.membership_error = Some(e.to_string());
                    self.notice(Severity::Error, format!("{what}: {e}"));
                }
            },
            Internal::Updates(result) => match result {
                Ok(UpdateStatus::UpToDate) => {
                    self.notice(Severity::Info, "AegisTARA is up to date.");
                }
                Ok(UpdateStatus::NoBuildForThisPlatform { release }) => self.notice(
                    Severity::Warning,
                    format!("Release {release} exists, but not for this computer."),
                ),
                Ok(UpdateStatus::Available(offer)) => {
                    let notes: String = offer.notes.chars().take(300).collect();
                    self.notice(
                        Severity::Info,
                        format!(
                            "AegisTARA {} is available ({} MB). Choose Help, Download update, or fetch it from {}. {notes}",
                            offer.release,
                            offer.bytes().div_ceil(1_000_000),
                            offer.url(),
                        ),
                    );
                    self.offered = Some(offer);
                }
                Err(e) => self.notice(Severity::Error, format!("Could not check for updates: {e}")),
            },
            Internal::Downloaded(result) => {
                self.downloading = false;
                match result {
                    Ok(path) => self.notice(
                        Severity::Info,
                        format!(
                            "The update was downloaded and checked against the signed manifest: {}. Close AegisTARA, then open that file to install it.",
                            path.display()
                        ),
                    ),
                    Err(e) => self.notice(Severity::Error, format!("Could not download the update: {e}")),
                }
            }
            Internal::Settings(result) => match result {
                Ok(s) => self.settings = Some(settings_view(&s)),
                Err(e) => self.on_failure("Could not load the server settings", &e),
            },
            Internal::SettingsSet(result) => match result {
                Ok(s) => {
                    let text = if s.password_login_enabled {
                        "Password login is on."
                    } else {
                        "Password login is off. Everyone logs in with the company account."
                    };
                    self.admin_text = Some(text.to_owned());
                    self.notice(Severity::Info, text);
                    self.settings = Some(settings_view(&s));
                    // The server ended every password session, this one too.
                    // Say so now instead of at the next failed request.
                    if !s.password_login_enabled
                        && self.me.as_ref().is_some_and(|m| m.method == "password")
                    {
                        self.on_event(Internal::LoggedOut);
                    }
                }
                Err(e) => {
                    let text = match e.code() {
                        Some(ServerCode::NoCompanyLogin) => {
                            "Password login stays on: this server has no company login to use instead."
                                .to_owned()
                        }
                        _ => format!("The login setting was not changed: {e}"),
                    };
                    self.admin_text = Some(text.clone());
                    self.notice(Severity::Error, text);
                }
            },
            Internal::AdminDone {
                what,
                result,
                reload_users,
            } => {
                match result {
                    Ok(()) => {
                        self.admin_text = Some(format!("{what}: done."));
                        self.notice(Severity::Info, format!("{what}: done."));
                    }
                    Err(e) => {
                        self.admin_text = Some(format!("{what}: {e}"));
                        self.on_failure(&what, &e);
                    }
                }
                if reload_users {
                    self.load_users();
                    self.load_sessions();
                }
            }
            Internal::Audit(result) => {
                let text = match result {
                    Ok(r) if r.intact => format!(
                        "The audit chain is intact: {} change(s) checked.",
                        r.verified_ops
                    ),
                    Ok(r) => format!(
                        "The audit chain is broken at change {}. The stored history was changed outside AegisTARA.",
                        r.first_bad_seq
                    ),
                    Err(e) => format!("The audit check failed: {e}"),
                };
                let severity = if text.contains("intact") {
                    Severity::Info
                } else {
                    Severity::Error
                };
                self.notice(severity, text.clone());
                self.admin_text = Some(text);
            }
            Internal::PasswordChanged(result) => match result {
                Ok(()) => self.notice(
                    Severity::Info,
                    "Your password was changed. Your other sessions were logged out.",
                ),
                Err(e) => self.notice(
                    Severity::Error,
                    format!("The password was not changed: {e}"),
                ),
            },
            Internal::LoggedOut => {
                self.close_project(false);
                self.conn = None;
                self.me = None;
                self.users.clear();
                self.settings = None;
                self.audit_rows.clear();
                self.events_error = None;
                self.session_rows.clear();
                self.memberships.clear();
                self.memberships_error = None;
                self.membership_error = None;
                self.projects.clear();
                if let Some(hook) = &self.config.on_log_out {
                    (hook.0)();
                }
                self.connection = ConnectionView::LoginRequired;
            }
            _ => {}
        }
    }

    fn load_baselines(&self) {
        let Some(open) = self.open.as_ref() else {
            return;
        };
        let (epoch, id) = (open.epoch, open.id.clone());
        self.spawn(move |conn| async move {
            Internal::Baselines {
                epoch,
                result: conn.baselines(&id).await,
            }
        });
    }

    fn load_history(&self) {
        let Some(open) = self.open.as_ref() else {
            return;
        };
        let (epoch, id) = (open.epoch, open.id.clone());
        self.spawn(move |conn| async move {
            Internal::History {
                epoch,
                result: conn.history(&id, HISTORY_ROWS).await,
            }
        });
    }

    fn load_figures(&self) {
        let Some(open) = self.open.as_ref() else {
            return;
        };
        let (epoch, id) = (open.epoch, open.id.clone());
        self.spawn(move |conn| async move {
            let result = async {
                let infos = conn.list_figures(&id).await?;
                let mut out = Vec::with_capacity(infos.len());
                for info in infos {
                    let data = match conn.get_figure(&id, &info.figure_id).await {
                        Ok((_, bytes)) => bytes,
                        Err(_) => Vec::new(),
                    };
                    out.push(figure_from_pb(info, data));
                }
                Ok(out)
            }
            .await;
            Internal::Figures { epoch, result }
        });
    }

    fn load_users(&self) {
        if self
            .me
            .as_ref()
            .is_some_and(|m| m.admin && m.method == "password")
        {
            self.spawn(|conn| async move { Internal::Users(conn.users().await) });
        }
    }

    /// Fetches the signed manifest in the background. Needs no server
    /// connection: an air gapped site simply sets no address.
    fn check_for_updates(&mut self) {
        if self.config.update_url.is_empty() {
            self.notice(
                Severity::Info,
                "No update address is set. Your administrator can set update_url in the settings file.",
            );
            return;
        }
        let (url, checker, http) = (
            self.config.update_url.clone(),
            Arc::clone(&self.config.update_checker),
            Arc::clone(&self.config.update_http),
        );
        let events = self.events.clone();
        tokio::spawn(async move {
            let result = checker.check_url(&*http, &url).await;
            let _ = events.send(Internal::Updates(result)).await;
        });
    }

    /// Downloads the installer of the release the last check offered, in the
    /// background. Nothing is run or opened.
    fn download_update(&mut self) {
        let Some(offer) = self.offered.clone() else {
            self.notice(
                Severity::Info,
                "There is no update to download. Choose Help, Check for updates first.",
            );
            return;
        };
        if self.downloading {
            self.notice(Severity::Info, "The update is already being downloaded.");
            return;
        }
        self.downloading = true;
        self.notice(
            Severity::Info,
            format!(
                "Downloading AegisTARA {} ({} MB)...",
                offer.release,
                offer.bytes().div_ceil(1_000_000)
            ),
        );
        let (target, events, http) = (
            self.config.update_dir.clone(),
            self.events.clone(),
            Arc::clone(&self.config.update_http),
        );
        tokio::spawn(async move {
            let result = offer.download(&*http, &target, |_| {}).await;
            let _ = events.send(Internal::Downloaded(result)).await;
        });
    }

    fn load_sessions(&self) {
        if self
            .me
            .as_ref()
            .is_some_and(|m| m.admin && m.method == "password")
        {
            self.spawn(|conn| async move { Internal::Sessions(conn.sessions().await) });
        }
    }

    /// Loads the open project's membership rows. Needs a project; the
    /// server checks the caller's `read` permission (job 058).
    fn load_memberships(&self) {
        let Some(id) = self.open.as_ref().map(|o| o.id.clone()) else {
            return;
        };
        self.spawn(move |conn| async move { Internal::Memberships(conn.memberships(&id).await) });
    }

    fn load_events(&self) {
        if self.me.as_ref().is_some_and(|m| m.admin) {
            self.spawn(|conn| async move { Internal::Events(conn.events(EVENT_ROWS).await) });
        }
    }

    fn load_settings(&self) {
        if self.me.as_ref().is_some_and(|m| m.admin) {
            self.spawn(|conn| async move { Internal::Settings(conn.server_settings().await) });
        }
    }

    fn admin(&self, action: AdminAction) {
        if let AdminAction::SetPasswordLogin { enabled } = action {
            self.spawn(move |conn| async move {
                Internal::SettingsSet(conn.set_password_login(enabled).await)
            });
            return;
        }
        let (what, reload) = match &action {
            AdminAction::CreateUser { username, .. } => (format!("Create user {username}"), true),
            AdminAction::SetAdmin { username, admin } => (
                format!(
                    "{} administrator rights for {username}",
                    if *admin { "Grant" } else { "Remove" }
                ),
                true,
            ),
            AdminAction::SetDisabled { username, disabled } => (
                format!(
                    "{} {username}",
                    if *disabled { "Disable" } else { "Enable" }
                ),
                true,
            ),
            AdminAction::ResetPassword { username, .. } => {
                (format!("Set a new password for {username}"), true)
            }
            AdminAction::DeleteUser { username } => (format!("Delete user {username}"), true),
            AdminAction::RevokeSession { username, .. } => {
                (format!("End the session of {username}"), true)
            }
            AdminAction::SetPasswordLogin { .. } => return,
        };
        self.spawn(move |conn| async move {
            let result = match action {
                AdminAction::CreateUser {
                    username,
                    display_name,
                    password,
                    admin,
                } => conn
                    .create_user(pb::CreateUserRequest {
                        username,
                        display_name,
                        password,
                        admin,
                    })
                    .await
                    .map(|_| ()),
                AdminAction::SetAdmin { username, admin } => conn
                    .update_user(pb::UpdateUserRequest {
                        username,
                        admin: Some(admin),
                        ..pb::UpdateUserRequest::default()
                    })
                    .await
                    .map(|_| ()),
                AdminAction::SetDisabled { username, disabled } => conn
                    .update_user(pb::UpdateUserRequest {
                        username,
                        disabled: Some(disabled),
                        ..pb::UpdateUserRequest::default()
                    })
                    .await
                    .map(|_| ()),
                AdminAction::ResetPassword { username, password } => conn
                    .update_user(pb::UpdateUserRequest {
                        username,
                        password: Some(password),
                        ..pb::UpdateUserRequest::default()
                    })
                    .await
                    .map(|_| ()),
                AdminAction::DeleteUser { username } => conn.delete_user(&username).await,
                AdminAction::RevokeSession { id, .. } => conn.revoke_session(&id).await,
                // Sent as `Internal::SettingsSet` above; never reaches here.
                AdminAction::SetPasswordLogin { .. } => Ok(()),
            };
            Internal::AdminDone {
                what,
                result,
                reload_users: reload,
            }
        });
    }

    /// Builds the export on the blocking pool from a copy of the project
    /// (O(project size), R5), writes it, and records it on the server with
    /// the report profile in the audit event (RPT-03).
    fn export(&mut self, kind: ExportKind, path: std::path::PathBuf, mut profile: ReportProfile) {
        let Some(open) = self.open.as_ref() else {
            return;
        };
        if profile.figures.is_empty() {
            profile.figures = open.figures.clone();
        }
        let ctx = ReportContext {
            project: open.entry.name.clone(),
            user: self
                .me
                .as_ref()
                .map_or_else(|| self.config.user.clone(), |m| m.display_name.clone()),
            now_ms: (self.config.clock_ms)(),
            seq: open.ws.confirmed_seq(),
            unsaved: open.ws.pending_batches() > 0,
        };
        let tree = shown_tree(&open.ws, &open.ui);
        let residual = open.ui.show_residual;
        if kind == ExportKind::TreeSvg && tree.is_none() {
            self.notice(Severity::Warning, "Open an attack tree first.");
            return;
        }
        let project = open.ws.project().clone();
        let seq = open.ws.confirmed_seq();
        let (epoch, id) = (open.epoch, open.id.clone());
        let conn = self.conn.clone();
        let events = self.events.clone();
        let detail = profile.audit_summary();
        tokio::spawn(async move {
            // Building and writing are both blocking work (R5).
            let written = tokio::task::spawn_blocking(move || {
                let ws = Workspace::new(project, seq);
                let opts = ReportOptions {
                    profile,
                    baseline_project: None,
                };
                let bytes: Vec<u8> = match kind {
                    ExportKind::Report => {
                        Some(report_html_with_options(&ws, &ctx, &opts).into_bytes())
                    }
                    ExportKind::ReportPdf => {
                        Some(report_pdf_with_options(&ws, &ctx, &opts).map_err(|e| e.to_string())?)
                    }
                    ExportKind::ReportXlsx => Some(report_workbook_xlsx_with_figures(
                        &ws,
                        &opts.profile.figures,
                    )),
                    ExportKind::RiskRegister => Some(risk_register_csv(&ws, &ctx).into_bytes()),
                    ExportKind::TreeSvg => tree
                        .and_then(|t| tree_svg(&ws, t, residual))
                        .map(String::into_bytes),
                }
                .ok_or_else(|| "there is nothing to export".to_owned())?;
                crate::settings::write_private_file(&path, &bytes)
                    .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
                Ok::<_, String>(path.display().to_string())
            })
            .await
            .unwrap_or_else(|_| Err("the export was interrupted".to_owned()));
            if written.is_ok()
                && let Some(conn) = conn
                && let Err(e) = conn.record_export(&id, kind.audit_name(), &detail).await
            {
                // The file is written either way; a failed audit call is
                // logged, not shown.
                tracing::warn!(error = %e, "the export was not recorded on the server");
            }
            let result = written;
            let _ = events.send(Internal::Exported { epoch, result }).await;
        });
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
                let methodology = match [
                    Methodology::iso21434_r155(),
                    Methodology::eu_cra(),
                    Methodology::stride_services_cloud(),
                ]
                .into_iter()
                .flatten()
                .find(|m| m.key() == key && m.version() == version)
                {
                    Some(builtin) => Arc::new(builtin),
                    None => conn.methodology(&key).await?,
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
                open.newer_seq = None;
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
                let vocab = Arc::new(pages::vocabulary(&loaded.methodology));
                self.open = Some(Open {
                    epoch,
                    id: entry.id.clone(),
                    vocab,
                    baselines: Vec::new(),
                    compare: None,
                    history: Vec::new(),
                    last_export: None,
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
                    newer_seq: None,
                    status_pending: false,
                    risks_pending: false,
                    risks_again: false,
                    cached_scene: None,
                    cached_lint_count: None,
                    figures: Vec::new(),
                    excel_preview: None,
                });
            }
        }
        let period = self.config.version_check.max(Duration::from_millis(50));
        let mut interval = tokio::time::interval_at(Instant::now() + period, period);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        self.check = Some(interval);
        self.request_risks();
        self.load_figures();
        if self.open.as_ref().is_some_and(|o| o.ui.page == Page::Admin) {
            self.load_memberships();
        }
    }

    fn close_project(&mut self, release: bool) {
        self.renew = None;
        self.check = None;
        let Some(mut open) = self.open.take() else {
            return;
        };
        self.memberships.clear();
        self.memberships_error = None;
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
        // The old workspace goes away with its undo history (UI-10.7).
        let steps = open.ws.take_cleared();
        open.resyncing = true;
        open.status_pending = false;
        open.risks_pending = false;
        open.risks_again = false;
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
        if steps > 0 {
            self.notice(
                Severity::Info,
                "Undo history cleared: the project was reloaded from the server.",
            );
        }
        self.start_open(epoch, id);
    }

    // ---- version check and server numbers --------------------------------

    /// Asks the server for the project's newest sequence number and lock
    /// holder. Only a read-only client needs the answer: an editor is the
    /// only writer.
    fn request_status(&mut self) {
        let Some(open) = self.open.as_mut() else {
            return;
        };
        if open.mode != Mode::ReadOnly || open.resyncing || open.status_pending {
            return;
        }
        open.status_pending = true;
        let (epoch, id) = (open.epoch, open.id.clone());
        self.spawn(move |conn| async move {
            Internal::Status {
                epoch,
                result: conn.status(&id).await,
            }
        });
    }

    fn on_status(&mut self, epoch: u64, result: Result<pb::ProjectStatus, ClientError>) {
        let Some(open) = self.open.as_mut().filter(|o| o.epoch == epoch) else {
            return;
        };
        open.status_pending = false;
        match result {
            // Only a read-only view follows what the server says; an editor
            // holds the lease itself.
            Ok(status) if open.mode == Mode::ReadOnly && !open.resyncing => {
                open.lock_holder = Some(status.lock_holder).filter(|h| !h.is_empty());
                open.newer_seq = Some(status.last_seq).filter(|seq| *seq > open.ws.confirmed_seq());
            }
            Ok(_) => {}
            Err(ClientError::LoginRequired) => {
                self.on_failure(
                    "Could not check for a newer version",
                    &ClientError::LoginRequired,
                );
            }
            // A missed check is not worth a message; the next one tries again.
            Err(e) => tracing::debug!(error = %e, "version check failed"),
        }
    }

    /// Asks the server for its own risk numbers, to check this engine's
    /// (accuracy: two engines must agree) or to show them when the engines
    /// differ. One call at a time; a batch confirmed meanwhile asks again.
    fn request_risks(&mut self) {
        let Some(open) = self.open.as_mut() else {
            return;
        };
        if open.resyncing {
            return;
        }
        if open.risks_pending {
            open.risks_again = true;
            return;
        }
        open.risks_pending = true;
        let (epoch, id) = (open.epoch, open.id.clone());
        self.spawn(move |conn| async move {
            Internal::Risks {
                epoch,
                result: conn.risks(&id).await,
            }
        });
    }

    fn on_risks(&mut self, epoch: u64, result: Result<pb::GetRisksResponse, ClientError>) {
        let Some(open) = self.open.as_mut().filter(|o| o.epoch == epoch) else {
            return;
        };
        open.risks_pending = false;
        let again = std::mem::take(&mut open.risks_again);
        match result {
            Ok(risks) => {
                self.server_risks = Some((open.id.clone(), risks));
                self.compare_server_risks();
            }
            // The numbers stay "being checked"; the next batch asks again.
            Err(e) => tracing::debug!(error = %e, "server numbers not fetched"),
        }
        if again {
            self.request_risks();
        }
    }

    /// Compares the newest server numbers with this computer's, if they
    /// describe exactly the state shown. Runs when the server's numbers
    /// arrive and again when one of our own batches is confirmed (the
    /// numbers may have been fetched a moment before the answer to our
    /// `SubmitOps` came). O(threats).
    fn compare_server_risks(&mut self) {
        let engine_ok = self.engine_ok();
        let Some(open) = self.open.as_ref() else {
            return;
        };
        let Some((project, risks)) = self.server_risks.as_ref() else {
            return;
        };
        let comparable = *project == open.id
            && !open.resyncing
            && open.ws.pending_batches() == 0
            && risks.as_of_seq == open.ws.confirmed_seq();
        if !engine_ok || !comparable {
            return;
        }
        let id = open.id.clone();
        let verified_seq = risks.as_of_seq;
        if !risks_differ(&open.ws, risks) {
            self.verified = Some((id, verified_seq));
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

    // ---- editing mode ----------------------------------------------------

    /// Reloads the open project from the server (the "newer version" offer).
    /// An editor is always current, so only a read-only view reloads.
    fn reload(&mut self) {
        if self.open.as_ref().is_some_and(|o| o.mode == Mode::ReadOnly) {
            self.resync("the user asked to reload");
        }
    }

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
                open.lock_holder = Some(lease.holder.clone()).filter(|h| !h.is_empty());
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
            // We held it; the next version check says who has it now.
            open.lock_holder = None;
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

    /// The user is done editing: save what is pending, then release.
    ///
    /// Editing stops when the release goes out, not when its answer comes
    /// back: the release itself makes the server announce a free lock, and a
    /// renew that was in flight when we asked may still be refused. Neither
    /// says anything was lost, and read-only is what the user asked for.
    fn finish_ending(&mut self) {
        let (epoch, id, token) = {
            let Some(open) = self.open.as_mut() else {
                return;
            };
            if !open.ending || open.ws.pending_batches() > 0 {
                return;
            }
            let Some(lease) = open.lease.take() else {
                return;
            };
            (open.epoch, open.id.clone(), lease.token.clone())
        };
        self.set_read_only();
        self.spawn(move |conn| async move {
            // Whether or not the release call works, we stop editing: the
            // lease expires on its own if the call never arrives.
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
        // The lease is already gone: we released it, or lost it and went
        // read-only. A refusal now says nothing, so do not report one.
        if open.mode != Mode::Editing {
            return;
        }
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
        self.history_cleared("the edit lock was lost");
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
                self.compare_server_risks();
                if self
                    .open
                    .as_ref()
                    .is_some_and(|o| o.ws.pending_batches() == 0)
                {
                    self.request_risks();
                }
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
                self.history_cleared("the project was reloaded");
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
                self.history_cleared("the server refused a change");
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
                self.history_cleared("a change could not be saved");
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
            Command::Reload => self.reload(),
            Command::BeginEdit => self.begin_edit(),
            Command::EndEdit => self.end_edit(),
            Command::Edit(edit) => {
                self.edit(&edit);
            }
            Command::BeginDraft(kind) => self.begin_draft(kind),
            Command::CommitDraft { name, next } => self.commit_draft(&name, next),
            Command::CancelDraft => self.set_draft(None),
            Command::SetDraftAsset(asset) => {
                if let Some(open) = self.open.as_mut() {
                    open.ui.draft_asset = Some(asset);
                }
                self.pump();
            }
            Command::SetDraftCategory(category) => {
                if let Some(open) = self.open.as_mut() {
                    open.ui.draft_category = Some(category);
                }
                self.pump();
            }
            Command::SetDraftComponent(component) => {
                if let Some(open) = self.open.as_mut() {
                    open.ui.draft_component = Some(component);
                }
                self.pump();
            }
            Command::SetDraftTo(component) => {
                if let Some(open) = self.open.as_mut() {
                    open.ui.draft_to = Some(component);
                }
                self.pump();
            }
            Command::SetDraftInterfaceKind(kind) => {
                if let Some(open) = self.open.as_mut() {
                    open.ui.draft_interface_kind = Some(kind);
                }
                self.pump();
            }
            Command::SuggestTreeNodes => self.suggest_tree_nodes(),
            Command::AcceptSuggestion(index) => self.accept_suggestion(index),
            Command::AcceptAllSuggestions => self.accept_all_suggestions(),
            Command::RejectSuggestion(index) => self.reject_suggestion(index),
            Command::RejectAllSuggestions => self.reject_all_suggestions(),
            Command::ShowPage(page) => self.show_page(page),
            Command::Select {
                selection,
                navigate,
            } => {
                if let Some(open) = self.open.as_mut() {
                    open.ui.draft = None;
                    pages::select(&mut open.ui, selection);
                    if navigate {
                        let page = pages::page_of(selection);
                        self.show_page(page);
                    }
                }
            }
            Command::RenameProject(name) => {
                let Some(open) = self.open.as_ref() else {
                    return;
                };
                let Some(lease) = open.lease.as_ref() else {
                    self.notice(Severity::Info, "Press Edit to take the edit lock first.");
                    return;
                };
                let (epoch, id, token) = (open.epoch, open.id.clone(), lease.token.clone());
                self.spawn(move |conn| async move {
                    Internal::Renamed {
                        epoch,
                        result: conn.rename_project(&id, &token, &name).await,
                    }
                });
            }
            Command::DeleteProject(id) => {
                self.spawn(move |conn| async move {
                    let result = conn.delete_project(&id).await;
                    Internal::Deleted { id, result }
                });
            }
            Command::LoadBaselines => self.load_baselines(),
            Command::CreateBaseline(name) => {
                let Some(open) = self.open.as_ref() else {
                    return;
                };
                if open.ws.pending_batches() > 0 {
                    self.notice(
                        Severity::Info,
                        "Some changes are not saved yet. Try again in a moment, so the baseline includes them.",
                    );
                    return;
                }
                let (epoch, id) = (open.epoch, open.id.clone());
                self.spawn(move |conn| async move {
                    Internal::BaselineCreated {
                        epoch,
                        result: conn.create_baseline(&id, &name).await,
                    }
                });
            }
            Command::CompareWith(None) => {
                if let Some(open) = self.open.as_mut() {
                    open.compare = None;
                }
            }
            Command::CompareWith(Some(baseline)) => self.compare_with(baseline),
            Command::LoadHistory => self.load_history(),
            Command::LoadFigures => self.load_figures(),
            Command::PutFigure {
                caption,
                target,
                data,
            } => {
                let Some(open) = self.open.as_ref() else {
                    return;
                };
                let Some(lease) = open.lease.as_ref() else {
                    self.notice(Severity::Info, "Press Edit to take the edit lock first.");
                    return;
                };
                let (epoch, id, token) = (open.epoch, open.id.clone(), lease.token.clone());
                self.spawn(move |conn| async move {
                    let result = conn
                        .put_figure(&id, &token, &caption, &target, data.clone())
                        .await
                        .map(|info| figure_from_pb(info, data));
                    Internal::FigureSaved { epoch, result }
                });
            }
            Command::UpdateFigure {
                figure_id,
                caption,
                target,
            } => {
                let Some(open) = self.open.as_ref() else {
                    return;
                };
                let Some(lease) = open.lease.as_ref() else {
                    self.notice(Severity::Info, "Press Edit to take the edit lock first.");
                    return;
                };
                let (epoch, id, token) = (open.epoch, open.id.clone(), lease.token.clone());
                self.spawn(move |conn| async move {
                    let result = conn
                        .update_figure(&id, &token, &figure_id, &caption, &target)
                        .await
                        .map(|info| figure_from_pb(info, Vec::new()));
                    Internal::FigureSaved { epoch, result }
                });
            }
            Command::DeleteFigure { figure_id } => {
                let Some(open) = self.open.as_ref() else {
                    return;
                };
                let Some(lease) = open.lease.as_ref() else {
                    self.notice(Severity::Info, "Press Edit to take the edit lock first.");
                    return;
                };
                let (epoch, id, token) = (open.epoch, open.id.clone(), lease.token.clone());
                self.spawn(move |conn| async move {
                    let result = conn.delete_figure(&id, &token, &figure_id).await;
                    Internal::FigureDeleted {
                        epoch,
                        figure_id,
                        result,
                    }
                });
            }
            Command::PreviewExcelImport { bytes, mapping } => {
                self.preview_excel_import(&bytes, &mapping);
            }
            Command::ApplyExcelImport => self.apply_excel_import(),
            Command::ClearExcelPreview => {
                if let Some(open) = self.open.as_mut() {
                    open.excel_preview = None;
                }
            }
            Command::Export {
                kind,
                path,
                profile,
            } => self.export(kind, path, *profile),
            Command::CheckForUpdates => self.check_for_updates(),
            Command::DownloadUpdate => self.download_update(),
            Command::LoadEvents => self.load_events(),
            Command::LoadSessions => self.load_sessions(),
            Command::LoadMemberships => self.load_memberships(),
            Command::SetMembership { member, role } => {
                let Some(id) = self.open.as_ref().map(|o| o.id.clone()) else {
                    return;
                };
                let what = format!("Set {member} to {role}");
                self.spawn(move |conn| async move {
                    let result = conn.set_membership(&id, &member, &role).await.map(|_| ());
                    Internal::MembershipDone { what, result }
                });
            }
            Command::RemoveMembership { member } => {
                let Some(id) = self.open.as_ref().map(|o| o.id.clone()) else {
                    return;
                };
                let what = format!("Remove {member}");
                self.spawn(move |conn| async move {
                    let result = conn.remove_membership(&id, &member).await;
                    Internal::MembershipDone { what, result }
                });
            }
            Command::LoadUsers => {
                self.load_users();
                self.load_settings();
            }
            Command::Admin(action) => self.admin(action),
            Command::BreakLock => {
                let Some(open) = self.open.as_ref() else {
                    return;
                };
                let id = open.id.clone();
                self.spawn(move |conn| async move {
                    Internal::AdminDone {
                        what: "Break the edit lock".to_owned(),
                        result: conn.break_lock(&id).await,
                        reload_users: false,
                    }
                });
            }
            Command::VerifyAudit => {
                let Some(open) = self.open.as_ref() else {
                    return;
                };
                let id = open.id.clone();
                self.spawn(
                    move |conn| async move { Internal::Audit(conn.verify_audit(&id).await) },
                );
            }
            Command::ChangePassword { current, new } => {
                self.spawn(move |conn| async move {
                    Internal::PasswordChanged(conn.change_password(&current, &new).await)
                });
            }
            Command::LogOut => {
                self.close_project(true);
                let events = self.events.clone();
                match self.conn.clone() {
                    Some(conn) => {
                        tokio::spawn(async move {
                            // Best effort: the session ends on the server at
                            // its expiry anyway.
                            let _ = conn.log_out().await;
                            let _ = events.send(Internal::LoggedOut).await;
                        });
                    }
                    None => {
                        tokio::spawn(async move {
                            let _ = events.send(Internal::LoggedOut).await;
                        });
                    }
                }
            }
            Command::Undo => self.history(true),
            Command::Redo => self.history(false),
            Command::CreateSample => {
                let (user, now) = (self.config.user.clone(), (self.config.clock_ms)());
                let _ = self.apply_edit(
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
        let Some(view) = tree_view_cached(&open.ws, &open.ui, open.cached_scene.as_ref()) else {
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
            .and_then(|t| t.id_of(idx))
            .or_else(|| {
                open.ws
                    .suggestions()
                    .and_then(|b| b.preview_tree(project, view.tree))
                    .and_then(|(pt, _)| pt.id_of(idx))
            });
        open.ui.selected_node = node;
    }

    fn suggest_tree_nodes(&mut self) {
        if !self.can_edit() {
            return;
        }
        let Some(open) = self.open.as_mut() else {
            return;
        };
        let branch = match crate::suggest::suggest_tree_nodes(&open.ws, &open.ui) {
            Ok(branch) => branch,
            Err(e) => {
                self.notice(
                    Severity::Warning,
                    format!("Could not synthesize ghost steps: {e}."),
                );
                return;
            }
        };
        let count = branch.len();
        if let Err(e) = open.ws.set_suggestions(Some(branch)) {
            self.notice(
                Severity::Warning,
                format!("Could not stage suggestions: {e}."),
            );
            return;
        }
        open.cached_scene = None;
        self.notice(
            Severity::Info,
            format!("Proposed {count} ghost step(s) with Aegis-Core Gear 1."),
        );
    }

    fn accept_suggestion(&mut self, index: usize) {
        if !self.can_edit() {
            return;
        }
        let (user, now) = (self.config.user.clone(), (self.config.clock_ms)());
        let Some(open) = self.open.as_mut() else {
            return;
        };
        let Some(id) = crate::suggest::suggestion_rows(&open.ws)
            .get(index)
            .map(|r| r.id)
        else {
            return;
        };
        match open.ws.accept_suggestion(id, &user, now) {
            Ok(()) => {
                open.cached_scene = None;
                open.cached_lint_count = None;
                self.pump();
            }
            Err(e) => {
                self.notice(
                    Severity::Warning,
                    format!("That suggestion was refused: {e}."),
                );
            }
        }
    }

    fn accept_all_suggestions(&mut self) {
        if !self.can_edit() {
            return;
        }
        let (user, now) = (self.config.user.clone(), (self.config.clock_ms)());
        let Some(open) = self.open.as_mut() else {
            return;
        };
        match open.ws.accept_all_suggestions(&user, now) {
            Ok(count) if count > 0 => {
                open.cached_scene = None;
                open.cached_lint_count = None;
                self.pump();
            }
            Ok(_) => {}
            Err(e) => {
                self.notice(Severity::Warning, format!("Suggestions were refused: {e}."));
            }
        }
    }

    fn reject_suggestion(&mut self, index: usize) {
        let Some(open) = self.open.as_mut() else {
            return;
        };
        let Some(id) = crate::suggest::suggestion_rows(&open.ws)
            .get(index)
            .map(|r| r.id)
        else {
            return;
        };
        if open.ws.reject_suggestion(id).is_ok() {
            open.cached_scene = None;
        }
    }

    fn reject_all_suggestions(&mut self) {
        let Some(open) = self.open.as_mut() else {
            return;
        };
        if open.ws.reject_all_suggestions() > 0 {
            open.cached_scene = None;
        }
    }

    fn preview_excel_import(&mut self, bytes: &[u8], mapping: &crate::excel::ExcelMapping) {
        let (user, now) = (self.config.user.clone(), (self.config.clock_ms)());
        let Some(open) = self.open.as_mut() else {
            return;
        };
        let wb = match crate::excel::read_workbook(bytes) {
            Ok(wb) => wb,
            Err(e) => {
                self.notice(
                    Severity::Warning,
                    format!("Could not read the Excel workbook: {e}."),
                );
                return;
            }
        };
        match crate::excel::preview_import(open.ws.project(), &wb, mapping, &user, now) {
            Ok(preview) => {
                let summary = format!(
                    "Excel dry-run preview ({}): {}",
                    preview.active_sheet,
                    preview.summary(),
                );
                open.excel_preview = Some(preview);
                self.notice(Severity::Info, summary);
            }
            Err(e) => {
                self.notice(
                    Severity::Warning,
                    format!("Excel import preview failed: {e}."),
                );
            }
        }
    }

    fn apply_excel_import(&mut self) {
        if !self.can_edit() {
            return;
        }
        let ops = {
            let Some(open) = self.open.as_mut() else {
                return;
            };
            let Some(preview) = open.excel_preview.take() else {
                self.notice(
                    Severity::Info,
                    "Preview an Excel workbook first before applying the import.",
                );
                return;
            };
            preview.ops
        };
        if ops.is_empty() {
            self.notice(
                Severity::Info,
                "The Excel preview has no new elements to import.",
            );
            return;
        }
        let count = ops.len();
        if self.apply_edit(|_| Ok(ops), Origin::Edit) {
            self.notice(
                Severity::Info,
                format!("Imported {count} change(s) from the Excel workbook."),
            );
        }
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

    /// Applies an edit locally and queues it. Returns whether it was applied.
    fn apply_edit(
        &mut self,
        build: impl FnOnce(&Project) -> Result<Vec<aegis_model::Op>, String>,
        origin: Origin,
    ) -> bool {
        if !self.can_edit() {
            return false;
        }
        let Some(open) = self.open.as_mut() else {
            return false;
        };
        let ops = match build(open.ws.project()) {
            Ok(ops) => ops,
            Err(why) => {
                // UI-10.11: an action error is a sticky error toast.
                self.notice(
                    Severity::Error,
                    format!("That change is not possible: {why}."),
                );
                return false;
            }
        };
        if ops.is_empty() {
            return true;
        }
        match open.ws.apply(ops, origin) {
            Ok(()) => {
                open.cached_scene = None;
                open.cached_lint_count = None;
                // UI-10.9: a removal happens at once; the toast says what was
                // removed and offers the way back.
                let removed = (origin == Origin::Edit)
                    .then(|| {
                        open.ws
                            .undo_label()
                            .filter(|l| l.is_removal())
                            .map(Label::past)
                    })
                    .flatten();
                if let Some(text) = removed {
                    self.notice_with(Severity::Info, text, Some(NoticeAction::Undo));
                }
                self.pump();
                true
            }
            Err(e) => {
                self.notice(Severity::Error, format!("That change was refused: {e}."));
                false
            }
        }
    }

    /// Applies one edit as the user; true if it was applied.
    fn edit(&mut self, edit: &Edit) -> bool {
        let (user, now) = (self.config.user.clone(), (self.config.clock_ms)());
        let created = edit.created();
        let applied = self.apply_edit(
            |project| edit.to_ops(project, &user, now).map_err(|e| e.to_string()),
            Origin::Edit,
        );
        if applied
            && let Some(selection) = created
            && let Some(open) = self.open.as_mut()
        {
            pages::select(&mut open.ui, selection);
        }
        applied
    }

    /// Sets or clears the draft; a new draft gets a new sequence number.
    fn set_draft(&mut self, draft: Option<DraftKind>) {
        if let Some(open) = self.open.as_mut() {
            open.ui.draft = draft;
            open.ui.draft_error = None;
            if draft.is_some() {
                open.ui.draft_seq += 1;
            }
            self.pump();
        }
    }

    fn begin_draft(&mut self, kind: DraftKind) {
        if !self.can_edit() {
            return;
        }
        // The page opens first: leaving a page drops its draft.
        self.show_page(kind.page());
        if let Some(open) = self.open.as_mut() {
            // UI-08.2: fill what the context gives: the selected asset, or the
            // only asset there is.
            let assets = pages::asset_rows(&open.ws);
            open.ui.draft_category = None;
            open.ui.draft_to = None;
            open.ui.draft_interface_kind = None;
            open.ui.draft_damage = None;
            open.ui.draft_threat = None;
            open.ui.draft_control = None;
            // Row-action context (UI-09): the selected element of the page the
            // user came from becomes the parent of the new element.
            open.ui.draft_damage = open.ui.selected_damage;
            open.ui.draft_threat = open.ui.selected_threat;
            open.ui.draft_control = open.ui.selected_control;
            let components = pages::component_rows(&open.ws);
            let only_component = match components.as_slice() {
                [only] => Some(only.id),
                _ => None,
            };
            open.ui.draft_component = open
                .ui
                .selected_component
                .filter(|id| components.iter().any(|c| c.id == *id))
                .or(only_component);
            let only = match assets.as_slice() {
                [only] => Some(only.id),
                _ => None,
            };
            open.ui.draft_asset = open
                .ui
                .selected_asset
                .filter(|id| assets.iter().any(|a| a.id == *id))
                .or(only);
        }
        self.set_draft(Some(kind));
    }

    /// UI-08: the draft becomes one element, or nothing. An empty name drops
    /// the draft without an op; a refused edit keeps it (and says why).
    fn commit_draft(&mut self, name: &str, next: bool) {
        let Some(kind) = self.open.as_ref().and_then(|o| o.ui.draft) else {
            return;
        };
        let name = name.trim();
        if name.is_empty() {
            self.set_draft(None);
            return;
        }
        // UI-08.2: nothing is created until every required field has a value.
        let Some(open) = self.open.as_ref() else {
            return;
        };
        if open.ui.draft_incomplete() {
            return;
        }
        let (asset, category) = (open.ui.draft_asset, open.ui.draft_category);
        let (component, to, interface_kind) = (
            open.ui.draft_component,
            open.ui.draft_to,
            open.ui.draft_interface_kind,
        );
        let (damage, threat, control) = (
            open.ui.draft_damage,
            open.ui.draft_threat,
            open.ui.draft_control,
        );
        let name = name.to_owned();
        let edit = match kind {
            DraftKind::Asset => {
                let count = self.open.as_ref().map_or(0, |o| {
                    pages::vocabulary(o.ws.project().methodology())
                        .properties
                        .len()
                });
                let properties = (0..count).collect();
                match component {
                    Some(component) => Edit::AddAssetBound {
                        id: AssetId::new_v7(),
                        name,
                        properties,
                        component,
                    },
                    None => Edit::AddAsset {
                        id: AssetId::new_v7(),
                        name,
                        properties,
                    },
                }
            }
            DraftKind::Damage => Edit::AddDamage {
                id: DamageId::new_v7(),
                name,
            },
            DraftKind::Control => Edit::AddControl {
                id: ControlId::new_v7(),
                name,
            },
            DraftKind::Threat => {
                let (Some(asset), Some(category)) = (asset, category) else {
                    return;
                };
                match damage {
                    Some(damage) => Edit::AddThreatLeading {
                        id: ThreatId::new_v7(),
                        name,
                        asset,
                        category,
                        damage,
                    },
                    None => Edit::AddThreat {
                        id: ThreatId::new_v7(),
                        name,
                        asset,
                        category,
                    },
                }
            }
            DraftKind::Goal => match control {
                Some(control) => Edit::AddGoalFor {
                    id: GoalId::new_v7(),
                    name,
                    control,
                },
                None => Edit::AddGoal {
                    id: GoalId::new_v7(),
                    name,
                },
            },
            DraftKind::Claim => Edit::AddClaim {
                id: ClaimId::new_v7(),
                name,
            },
            DraftKind::Tree => Edit::NewTree {
                id: TreeId::new_v7(),
                root: name,
                threat,
            },
            DraftKind::Component => Edit::AddComponent {
                id: ComponentId::new_v7(),
                name,
                parent: None,
            },
            DraftKind::Interface => {
                let (Some(component), Some(kind)) = (component, interface_kind) else {
                    return;
                };
                Edit::AddInterface {
                    id: InterfaceId::new_v7(),
                    name,
                    component,
                    kind,
                }
            }
            DraftKind::DataFlow => {
                let (Some(from), Some(to)) = (component, to) else {
                    return;
                };
                Edit::AddDataFlow {
                    id: DataFlowId::new_v7(),
                    name,
                    from,
                    to,
                    interface: None,
                }
            }
            DraftKind::TrustBoundary => Edit::AddTrustBoundary {
                id: TrustBoundaryId::new_v7(),
                name,
            },
        };
        let notice_before = self.next_notice;
        if self.edit(&edit) {
            self.set_draft(next.then_some(kind));
        } else {
            // UI-08.6, UI-10.11: the reason is shown in the draft, which keeps the
            // user's text, instead of as a toast.
            let reason = if self.next_notice > notice_before {
                self.notices.pop_back().map(|n| n.text)
            } else {
                None
            };
            if let Some(open) = self.open.as_mut() {
                open.ui.draft_error =
                    Some(reason.unwrap_or_else(|| "That could not be created.".to_owned()));
            }
            self.pump();
        }
    }

    fn show_page(&mut self, page: Page) {
        let Some(open) = self.open.as_mut() else {
            return;
        };
        if open.ui.page != page {
            open.ui.draft = None;
        }
        open.ui.page = page;
        match page {
            Page::Versions => self.load_baselines(),
            Page::History => self.load_history(),
            Page::Admin => {
                self.load_events();
                self.load_users();
                self.load_settings();
            }
            _ => {}
        }
    }

    /// Downloads a baseline and builds its project for the comparison.
    fn compare_with(&self, baseline: String) {
        let Some(open) = self.open.as_ref() else {
            return;
        };
        let name = open
            .baselines
            .iter()
            .find(|b| b.id == baseline)
            .map_or_else(|| "baseline".to_owned(), |b| b.name.clone());
        let methodology = Arc::clone(open.ws.project().methodology());
        let (epoch, id) = (open.epoch, open.id.clone());
        self.spawn(move |conn| async move {
            let result = async {
                let (_, bytes) = conn.baseline(&id, &baseline).await?;
                tokio::task::spawn_blocking(move || Project::load(bytes.as_slice(), methodology))
                    .await
                    .map_err(|_| ClientError::Protocol("loading was interrupted".into()))?
                    .map_err(|e| {
                        ClientError::Protocol(format!("the baseline could not be read: {e}"))
                    })
            }
            .await;
            Internal::Compared {
                epoch,
                name,
                result: Box::new(result),
            }
        });
    }

    fn history(&mut self, undo: bool) {
        if !self.can_edit() {
            return;
        }
        let Some(open) = self.open.as_mut() else {
            return;
        };
        let label = if undo {
            open.ws.undo_label()
        } else {
            open.ws.redo_label()
        }
        .cloned();
        let result = if undo { open.ws.undo() } else { open.ws.redo() };
        match result {
            Ok(()) => {
                open.cached_scene = None;
                open.cached_lint_count = None;
                // UI-10.5: say what changed and offer the way back.
                let (done, action) = if undo {
                    ("Undone", NoticeAction::Redo)
                } else {
                    ("Redone", NoticeAction::Undo)
                };
                let shown = label.as_ref().and_then(|l| l.selection(open.ws.project()));
                let text = label.map_or_else(
                    || format!("{done}."),
                    |l| format!("{done}: {}", l.imperative()),
                );
                self.notice_with(Severity::Info, text, Some(action));
                // Open the page of the element and select it.
                if let Some(sel) = shown
                    && let Some(open) = self.open.as_mut()
                {
                    pages::select(&mut open.ui, sel);
                    self.show_page(pages::page_of(sel));
                }
                self.pump();
            }
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
        let Some(open) = self.open.as_mut() else {
            self.view_tx.send_replace(Arc::new(ViewModel {
                revision: self.revision,
                connection: self.connection.clone(),
                projects: self.projects.clone(),
                project: None,
                notices: self.notices.iter().cloned().collect(),
                user: self
                    .me
                    .as_ref()
                    .map_or_else(|| self.config.user.clone(), |m| m.display_name.clone()),
                me: self.me.clone(),
                users: self.users.clone(),
                admin_text: self.admin_text.clone(),
                settings: self.settings,
                events: self.audit_rows.clone(),
                events_error: self.events_error.clone(),
                sessions: self.session_rows.clone(),
                memberships: self.memberships.clone(),
                memberships_error: self.memberships_error.clone(),
                membership_error: self.membership_error.clone(),
            }));
            return;
        };
        let project = {
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
            let numbers = if !engine_ok {
                NumbersCheck::NotComparable
            } else if self.distrusted.as_deref() == Some(open.id.as_str()) {
                NumbersCheck::ServerNumbersShown
            } else if open.ws.pending_batches() == 0
                && !open.resyncing
                && self
                    .verified
                    .as_ref()
                    .is_some_and(|(p, seq)| *p == open.id && *seq == open.ws.confirmed_seq())
            {
                NumbersCheck::Checked
            } else {
                NumbersCheck::Checking
            };
            let selected_threat = open
                .ui
                .selected_threat
                .and_then(|id| threats.iter().position(|t| t.id == id));
            let tree = tree_view_cached(&open.ws, &open.ui, open.cached_scene.as_ref());
            if let Some(tv) = &tree {
                open.cached_scene = Some((tv.tree, tv.residual, Arc::clone(&tv.scene)));
            }
            if open.cached_lint_count.is_none() {
                open.cached_lint_count = Some(open.ws.project().lint().len());
            }
            let selected_node = tree.as_ref().and_then(|t| t.selected);
            let suggestions = crate::suggest::suggestion_rows(&open.ws);
            let selected_suggestion = open.ui.selected_node.and_then(|nid| {
                let branch = open.ws.suggestions()?;
                let t = shown_tree(&open.ws, &open.ui)?;
                let (_, ghosts) = branch.preview_tree(open.ws.project(), t)?;
                let sid = ghosts.iter().find(|g| g.node_id == nid)?.suggestion_id;
                suggestions.iter().position(|r| r.id == sid)
            });
            let data = page_data(open);
            let comparison = (open.ui.page == Page::Versions)
                .then(|| {
                    open.compare.as_ref().and_then(|(name, base)| {
                        pages::comparison(base, name, open.ws.project()).ok()
                    })
                })
                .flatten();
            Some(ProjectView {
                page: open.ui.page,
                vocab: Arc::clone(&open.vocab),
                data,
                baselines: open.baselines.clone(),
                comparison,
                history: open.history.clone(),
                last_export: open.last_export.clone(),
                nav_counts: nav_counts(open),
                suggestions,
                selected_suggestion,
                figures: open.figures.clone(),
                excel_preview: open.excel_preview.clone(),
                palette_elements: pages::palette_elements(&open.ws),
                entry: open.entry.clone(),
                mode: open.mode,
                lock_holder: open.lock_holder.clone(),
                newer_version: open.newer_seq.is_some() && open.mode == Mode::ReadOnly,
                sync: if open.resyncing {
                    SyncView::Resyncing
                } else if open.ws.pending_batches() > 0 {
                    SyncView::Syncing {
                        pending: open.ws.pending_batches(),
                    }
                } else {
                    SyncView::Synced
                },
                descriptions: page_descriptions(&open.ws, &open.ui),
                is_empty: open.ws.project().asset_count() == 0,
                inspector: inspector(&open.ws, &open.ui),
                can_undo: open.ws.can_undo(),
                can_redo: open.ws.can_redo(),
                undo_label: open.ws.undo_label().map(Label::imperative),
                redo_label: open.ws.redo_label().map(Label::imperative),
                draft: open.ui.draft,
                draft_seq: open.ui.draft_seq,
                draft_error: open.ui.draft_error.clone(),
                draft_asset: open.ui.draft_asset,
                draft_category: open.ui.draft_category,
                draft_component: open.ui.draft_component,
                draft_to: open.ui.draft_to,
                draft_interface_kind: open.ui.draft_interface_kind,
                draft_incomplete: open.ui.draft_incomplete(),
                show_residual: open.ui.show_residual,
                confirmed_seq: open.ws.confirmed_seq(),
                numbers,
                risks_verified_seq: self
                    .verified
                    .as_ref()
                    .filter(|(project, _)| *project == open.id)
                    .map(|(_, seq)| *seq),
                threats,
                selected_threat,
                tree,
                selected_node,
            })
        };
        self.view_tx.send_replace(Arc::new(ViewModel {
            revision: self.revision,
            connection: self.connection.clone(),
            projects: self.projects.clone(),
            project,
            notices: self.notices.iter().cloned().collect(),
            user: self
                .me
                .as_ref()
                .map_or_else(|| self.config.user.clone(), |m| m.display_name.clone()),
            me: self.me.clone(),
            users: self.users.clone(),
            admin_text: self.admin_text.clone(),
            settings: self.settings,
            events: self.audit_rows.clone(),
            events_error: self.events_error.clone(),
            sessions: self.session_rows.clone(),
            memberships: self.memberships.clone(),
            memberships_error: self.memberships_error.clone(),
            membership_error: self.membership_error.clone(),
        }));
    }
}

/// The rows of the page on screen. Time O(entities of that page).
fn page_data(open: &Open) -> PageData {
    let ws = &open.ws;
    let ui = &open.ui;
    let mut d = PageData::default();
    match ui.page {
        Page::Overview => d.overview = Some(pages::overview(ws)),
        Page::Item => {
            d.components = pages::component_rows(ws);
            d.component = ui
                .selected_component
                .and_then(|id| pages::component_detail(ws, id));
            d.interfaces = pages::interface_rows(ws);
            d.interface = ui
                .selected_interface
                .and_then(|id| pages::interface_detail(ws, id));
            d.data_flows = pages::data_flow_rows(ws);
            d.data_flow = ui
                .selected_data_flow
                .and_then(|id| pages::data_flow_detail(ws, id));
            d.trust_boundaries = pages::trust_boundary_rows(ws);
            d.trust_boundary = ui
                .selected_trust_boundary
                .and_then(|id| pages::trust_boundary_detail(ws, id));
        }
        Page::Assets => {
            d.assets = pages::asset_rows(ws);
            d.asset = ui.selected_asset.and_then(|id| pages::asset_detail(ws, id));
        }
        Page::Damage => {
            d.damages = pages::damage_rows(ws);
            d.damage = ui
                .selected_damage
                .and_then(|id| pages::damage_detail(ws, id));
        }
        Page::Threats | Page::Risks => {
            if ui.page == Page::Threats {
                d.assets = pages::asset_rows(ws);
            } else {
                d.risks = pages::risk_rows(ws);
            }
            d.threat = ui
                .selected_threat
                .and_then(|id| pages::threat_detail(ws, id));
        }
        Page::Trees => {
            d.trees = pages::tree_rows(ws);
            let project = ws.project();
            d.outline = shown_tree(ws, ui)
                .and_then(|t| project.attack_tree_idx(&t))
                .and_then(|i| project.attack_tree(i))
                .map(crate::outline::to_outline);
            d.controls = pages::control_rows(ws);
        }
        Page::Controls => {
            d.controls = pages::control_rows(ws);
            d.control = ui
                .selected_control
                .and_then(|id| pages::control_detail(ws, id));
        }
        Page::Goals => {
            d.goals = pages::goal_rows(ws);
            d.goal = ui.selected_goal.and_then(|id| pages::goal_detail(ws, id));
            d.claims = pages::claim_rows(ws);
            d.claim = ui.selected_claim.and_then(|id| pages::claim_detail(ws, id));
        }
        Page::Problems => d.problems = pages::problems(ws),
        Page::Versions | Page::History | Page::Reports | Page::Admin => {}
    }
    d
}

/// The sidebar counts. Time O(project) for the lint; the rest O(1).
fn nav_counts(open: &Open) -> Vec<Option<usize>> {
    let p = open.ws.project();
    Page::ALL
        .iter()
        .map(|page| match page {
            Page::Item => Some(
                p.component_count()
                    + p.interface_count()
                    + p.data_flow_count()
                    + p.trust_boundary_count(),
            )
            .filter(|n| *n > 0),
            Page::Assets => Some(p.asset_count()),
            Page::Damage => Some(p.damage_scenario_count()),
            Page::Threats | Page::Risks => Some(p.threat_scenario_count()),
            Page::Trees => Some(p.attack_tree_count()),
            Page::Controls => Some(p.security_control_count()),
            Page::Goals => Some(p.cybersecurity_goal_count() + p.cybersecurity_claim_count()),
            Page::Problems => Some(open.cached_lint_count.unwrap_or_else(|| p.lint().len())),
            Page::Versions => Some(open.baselines.len()).filter(|n| *n > 0),
            _ => None,
        })
        .collect()
}

fn figure_from_pb(info: pb::FigureInfo, data: Vec<u8>) -> FigureAttachment {
    FigureAttachment {
        figure_id: info.figure_id,
        media_type: info.media_type,
        caption: info.caption,
        target: info.target,
        width_px: info.width,
        height_px: info.height,
        size_bytes: info.byte_len,
        data,
        updated_by: info.created_by,
        updated_at_ms: i64::try_from(info.created_ms).unwrap_or(0),
    }
}

/// `AddAsset` as "Add asset".
fn kind_words(kind: &str) -> String {
    let mut out = String::with_capacity(kind.len() + 4);
    for (i, c) in kind.chars().enumerate() {
        if c.is_ascii_uppercase() && i > 0 {
            out.push(' ');
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

fn entry_of(summary: pb::ProjectSummary) -> ProjectEntry {
    ProjectEntry {
        id: summary.project_id,
        name: summary.name,
        methodology: summary.methodology_key,
    }
}

fn settings_view(s: &pb::ServerSettings) -> ServerSettingsView {
    ServerSettingsView {
        password_login_enabled: s.password_login_enabled,
        company_login_available: s.company_login_available,
    }
}
