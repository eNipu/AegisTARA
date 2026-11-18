//! The session against a real server: optimistic editing, saving, the
//! read-only view and its "newer version" offer, lock loss, and reloading,
//! over loopback gRPC.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use aegis_client::commands::{Command, Edit};
use aegis_client::connection::DevLogin;
use aegis_client::session::{Session, SessionConfig};
use aegis_client::state::{
    ConnectionView, Mode, NoticeAction, NumbersCheck, Severity, SyncView, ViewModel,
};
use aegis_server::auth_layer::{AuthLayer, DevAuthn};
use aegis_server::config::ServerConfig;
use aegis_server::methodologies::Methodologies;
use aegis_server::{AegisService, MemoryStore, Registry, server};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio_stream::wrappers::TcpListenerStream;
use tower::{Layer, Service};

/// Ops in the sample TARA: 11 entities and links, plus 5 attack potential factors.
const SAMPLE_OPS: u64 = 16;

// ---------------------------------------------------------------------------
// Harness.
// ---------------------------------------------------------------------------

/// Delays every request whose path ends in `SubmitOps`, to model a slow
/// link, and counts `GetProjectStatus` calls (the version check).
#[derive(Clone)]
struct SlowSubmit(Duration, Arc<AtomicUsize>);

impl<S> Layer<S> for SlowSubmit {
    type Service = SlowSubmitService<S>;
    fn layer(&self, inner: S) -> SlowSubmitService<S> {
        SlowSubmitService {
            inner,
            delay: self.0,
            status_calls: Arc::clone(&self.1),
        }
    }
}

#[derive(Clone)]
struct SlowSubmitService<S> {
    inner: S,
    delay: Duration,
    status_calls: Arc<AtomicUsize>,
}

impl<S, B> Service<http::Request<B>> for SlowSubmitService<S>
where
    S: Service<http::Request<B>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    B: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<S::Response, S::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), S::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: http::Request<B>) -> Self::Future {
        let clone = self.inner.clone();
        let mut inner = std::mem::replace(&mut self.inner, clone);
        let slow = request.uri().path().ends_with("SubmitOps");
        if request.uri().path().ends_with("/GetProjectStatus") {
            self.status_calls.fetch_add(1, Ordering::Relaxed);
        }
        let delay = self.delay;
        Box::pin(async move {
            if slow {
                tokio::time::sleep(delay).await;
            }
            inner.call(request).await
        })
    }
}

/// A session that stops itself when the test is done, so no client keeps a
/// stream open against a server that is trying to shut down.
struct Client(Session);

impl std::ops::Deref for Client {
    type Target = Session;
    fn deref(&self) -> &Session {
        &self.0
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.0.send(Command::Shutdown);
    }
}

struct TestServer {
    url: String,
    /// `GetProjectStatus` calls the server received.
    status_calls: Arc<AtomicUsize>,
    stop: Option<oneshot::Sender<()>>,
    done: Option<tokio::task::JoinHandle<()>>,
}

impl TestServer {
    async fn start(submit_delay: Duration) -> Self {
        Self::start_with(submit_delay, None).await
    }

    async fn start_with(
        submit_delay: Duration,
        gate: Option<Arc<aegis_server::LicenseGate>>,
    ) -> Self {
        Self::start_config(
            submit_delay,
            gate,
            ServerConfig {
                lease: Duration::from_secs(30),
                ..ServerConfig::default()
            },
        )
        .await
    }

    async fn start_config(
        submit_delay: Duration,
        gate: Option<Arc<aegis_server::LicenseGate>>,
        config: ServerConfig,
    ) -> Self {
        let registry = Registry::new(
            Arc::new(MemoryStore::new()),
            Arc::new(Methodologies::builtin().expect("builtin")),
            Arc::new(config),
        );
        let registry = Arc::new(match gate {
            Some(gate) => registry.with_license(gate),
            None => registry,
        });
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let url = format!("http://{}", listener.local_addr().expect("addr"));
        let (stop, stopped) = oneshot::channel::<()>();
        let service = AegisService::new(Arc::clone(&registry)).into_server();
        let layer = AuthLayer::new(Arc::new(DevAuthn), &url);
        let status_calls = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&status_calls);
        let done = tokio::spawn(async move {
            server::builder()
                .layer(layer)
                .layer(SlowSubmit(submit_delay, counter))
                .add_service(service)
                .serve_with_incoming_shutdown(TcpListenerStream::new(listener), async {
                    let _ = stopped.await;
                    registry.shutdown().await;
                })
                .await
                .expect("serve");
        });
        TestServer {
            url,
            status_calls,
            stop: Some(stop),
            done: Some(done),
        }
    }

    fn session(&self, user: &str, admin: bool) -> Client {
        self.session_with(user, admin, |_| {})
    }

    fn session_with(&self, user: &str, admin: bool, tune: impl Fn(&mut SessionConfig)) -> Client {
        let mut config = SessionConfig::new(user);
        config.retry_delay = Duration::from_millis(100);
        tune(&mut config);
        Client(Session::start(&self.url, Arc::new(DevLogin::new(user, admin)), config).0)
    }

    async fn stop(mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(done) = self.done.take() {
            tokio::time::timeout(Duration::from_secs(5), done)
                .await
                .expect("the server did not shut down in 5 seconds")
                .expect("server task");
        }
    }
}

/// Waits until the view satisfies `ready`, or fails after five seconds.
async fn until(
    session: &Session,
    what: &str,
    ready: impl Fn(&ViewModel) -> bool,
) -> Arc<ViewModel> {
    let mut rx = session.subscribe();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        {
            let view = rx.borrow_and_update().clone();
            if ready(&view) {
                return view;
            }
        }
        let left = deadline.saturating_duration_since(Instant::now());
        assert!(!left.is_zero(), "timed out waiting for: {what}");
        let _ = tokio::time::timeout(left, rx.changed()).await;
    }
}

fn online(vm: &ViewModel) -> bool {
    matches!(vm.connection, ConnectionView::Online { .. })
}

fn project_ready(vm: &ViewModel) -> bool {
    vm.project
        .as_ref()
        .is_some_and(|p| p.sync == SyncView::Synced)
}

/// Creates a project and opens it; returns its ID.
async fn create_and_open(session: &Session, name: &str) -> String {
    until(session, "online", online).await;
    session
        .send(Command::CreateProject {
            name: name.into(),
            methodology_key: "iso21434-r155".into(),
        })
        .expect("sent");
    let view = until(session, "project open", |vm| vm.project.is_some()).await;
    view.project.as_ref().expect("project").entry.id.clone()
}

async fn start_editing(session: &Session) {
    session.send(Command::BeginEdit).expect("sent");
    until(session, "editing", |vm| {
        vm.project.as_ref().is_some_and(|p| p.mode == Mode::Editing)
    })
    .await;
}

// ---------------------------------------------------------------------------
// Tests.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn connecting_shows_the_server_and_lists_projects() {
    let server = TestServer::start(Duration::ZERO).await;
    let alice = server.session("alice", false);
    assert!(matches!(
        alice.view().connection,
        ConnectionView::Connecting | ConnectionView::Online { .. }
    ));
    let view = until(&alice, "online", online).await;
    assert!(matches!(
        &view.connection,
        ConnectionView::Online {
            engine_ok: true,
            ..
        }
    ));
    assert_eq!(view.user, "alice");
    drop(alice);
    server.stop().await;
}

/// A one-seat license that ended `days_ago` days ago with `grace` days of
/// grace, signed by a throwaway key.
fn gate(seats: u32, ends_in_days: i64, grace: u32) -> Arc<aegis_server::LicenseGate> {
    use aegis_license::{DAY, Issuer, License, TrustedKeys};
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_secs();
    let ends = now
        .checked_add_signed(ends_in_days * DAY as i64)
        .expect("date");
    let (pkcs8, public) = Issuer::generate().expect("key");
    let issuer = Issuer::from_pkcs8(&pkcs8).expect("issuer");
    let file = issuer
        .sign(&License {
            version: 1,
            id: "t".into(),
            kid: "t".into(),
            customer: "Example Motors".into(),
            seats,
            issued_at: now - 400 * DAY,
            expires_at: ends,
            grace_days: grace,
            features: vec![],
        })
        .expect("signs");
    let keys = TrustedKeys::single("t", public).expect("keys");
    Arc::new(
        aegis_server::LicenseGate::load(&file, &keys, aegis_server::license::system_clock_arc())
            .expect("loads"),
    )
}

#[tokio::test]
async fn the_license_shows_in_the_view_and_a_lapsed_one_raises_a_notice() {
    use aegis_client::state::LicenseKind;
    let server = TestServer::start_with(Duration::ZERO, Some(gate(3, -5, 20))).await;
    let alice = server.session("alice", false);
    let view = until(&alice, "online", online).await;
    let ConnectionView::Online {
        license: Some(license),
        ..
    } = &view.connection
    else {
        panic!("the server reports its license");
    };
    assert_eq!(license.kind, LicenseKind::Grace);
    assert_eq!((license.seats, license.seats_used), (3, 0));
    // 15 days of grace are left; the clock may have ticked past the second.
    let days = license.days_left.expect("grace has an end");
    assert!((14..=15).contains(&days), "{days}");
    assert_eq!(license.customer, "Example Motors");
    assert!(
        view.notices
            .iter()
            .any(|n| n.severity == Severity::Warning
                && n.text.contains(&format!("{days} more days"))),
        "{:?}",
        view.notices
    );
    drop(alice);
    server.stop().await;
}

#[tokio::test]
async fn a_user_without_a_free_seat_can_look_but_is_told_why_they_cannot_edit() {
    let server = TestServer::start_with(Duration::ZERO, Some(gate(1, 200, 0))).await;
    let alice = server.session("alice", false);
    let bob = server.session("bob", false);
    let id = create_and_open(&alice, "Seats").await;
    start_editing(&alice).await;
    alice.send(Command::EndEdit).expect("sent");
    until(&alice, "released", |vm| {
        vm.project
            .as_ref()
            .is_some_and(|p| p.mode == Mode::ReadOnly && p.lock_holder.is_none())
    })
    .await;

    until(&bob, "online", online).await;
    bob.send(Command::OpenProject { id }).expect("sent");
    until(&bob, "bob sees the project", |vm| vm.project.is_some()).await;
    bob.send(Command::BeginEdit).expect("sent");
    let view = until(&bob, "seats notice", |vm| {
        vm.notices
            .iter()
            .any(|n| n.text.contains("seats are in use"))
    })
    .await;
    assert_eq!(view.project.as_ref().expect("project").mode, Mode::ReadOnly);
    drop(alice);
    drop(bob);
    server.stop().await;
}

#[tokio::test]
async fn an_unreachable_server_is_reported_and_retried() {
    let mut config = SessionConfig::new("alice");
    config.retry_delay = Duration::from_millis(50);
    let (session, _task) = Session::start(
        "http://127.0.0.1:1",
        Arc::new(DevLogin::new("alice", false)),
        config,
    );
    let view = until(&session, "offline", |vm| {
        matches!(vm.connection, ConnectionView::Offline { .. })
    })
    .await;
    assert!(matches!(view.connection, ConnectionView::Offline { .. }));
}

#[tokio::test]
async fn a_new_project_opens_read_only_and_editing_needs_the_lock() {
    let server = TestServer::start(Duration::ZERO).await;
    let alice = server.session("alice", false);
    create_and_open(&alice, "Brake").await;
    let view = alice.view();
    let project = view.project.as_ref().expect("project");
    assert_eq!(project.mode, Mode::ReadOnly);
    assert!(project.is_empty && project.threats.is_empty());

    // Editing without the lock is refused with an explanation.
    alice.send(Command::CreateSample).expect("sent");
    let view = until(&alice, "notice", |vm| !vm.notices.is_empty()).await;
    assert!(view.notices[0].text.contains("Edit"));
    assert!(view.project.as_ref().expect("project").is_empty);

    start_editing(&alice).await;
    let view = until(&alice, "lock shown", |vm| {
        vm.project
            .as_ref()
            .is_some_and(|p| p.lock_holder.as_deref() == Some("alice"))
    })
    .await;
    assert_eq!(view.project.as_ref().expect("project").mode, Mode::Editing);
    drop(alice);
    server.stop().await;
}

#[tokio::test]
async fn an_edit_shows_at_once_and_is_saved_behind_the_scenes() {
    // A slow link: every save takes 300 ms. The user must not wait for it.
    let server = TestServer::start(Duration::from_millis(300)).await;
    let alice = server.session("alice", false);
    create_and_open(&alice, "Brake").await;
    start_editing(&alice).await;

    let sent = Instant::now();
    alice.send(Command::CreateSample).expect("sent");
    // The sample is on screen within a few milliseconds, saved or not.
    let view = until(&alice, "sample visible", |vm| {
        vm.project.as_ref().is_some_and(|p| p.threats.len() == 2)
    })
    .await;
    let visible_after = sent.elapsed();
    assert!(
        visible_after < Duration::from_millis(200),
        "took {visible_after:?}"
    );
    let project = view.project.as_ref().expect("project");
    assert!(
        matches!(project.sync, SyncView::Syncing { pending: 1 }),
        "{:?}",
        project.sync
    );
    assert_eq!(project.confirmed_seq, 0);
    assert!(project.can_undo);

    // Then the server confirms every op of the sample.
    let view = until(&alice, "saved", project_ready).await;
    let project = view.project.as_ref().expect("project");
    assert_eq!(project.confirmed_seq, SAMPLE_OPS);
    assert!(
        sent.elapsed() >= Duration::from_millis(300),
        "the save really was slow"
    );
    drop(alice);
    server.stop().await;
}

#[tokio::test]
async fn ending_an_edit_is_not_reported_as_losing_the_lock() {
    // Releasing the lease makes the server announce a free lock. That is the
    // event the user asked for, so it must not be shown as a lock loss (nor
    // reload the project).
    let server = TestServer::start(Duration::ZERO).await;
    let alice = server.session("alice", false);
    create_and_open(&alice, "Brake").await;
    start_editing(&alice).await;
    alice.send(Command::EndEdit).expect("sent");
    let view = until(&alice, "released", |vm| {
        vm.project
            .as_ref()
            .is_some_and(|p| p.mode == Mode::ReadOnly && p.lock_holder.is_none())
    })
    .await;
    assert!(
        !view.notices.iter().any(|n| n.text.contains("edit lock")),
        "{:?}",
        view.notices
    );
    drop(alice);
    server.stop().await;
}

#[tokio::test]
async fn rating_a_step_updates_the_risks_immediately_and_undo_restores_them() {
    let server = TestServer::start(Duration::ZERO).await;
    let alice = server.session("alice", false);
    create_and_open(&alice, "Brake").await;
    start_editing(&alice).await;
    alice.send(Command::CreateSample).expect("sent");
    until(&alice, "saved", |vm| {
        project_ready(vm) && vm.project.as_ref().is_some_and(|p| p.threats.len() == 2)
    })
    .await;

    // Select the first threat's tree and the debug-port step.
    alice.send(Command::SelectThreat(0)).expect("sent");
    let view = until(&alice, "tree", |vm| {
        vm.project.as_ref().is_some_and(|p| p.tree.is_some())
    })
    .await;
    let project = view.project.as_ref().expect("project");
    let scene = project.scene().expect("scene").clone();
    let debug = (0..scene.len())
        .find(|i| scene.label(*i as u32).starts_with("Exploit"))
        .expect("step");
    alice.send(Command::SelectNode(Some(debug))).expect("sent");
    let view = until(&alice, "inspector", |vm| {
        vm.project.as_ref().is_some_and(|p| p.inspector.is_some())
    })
    .await;
    let inspector = view
        .project
        .as_ref()
        .expect("project")
        .inspector
        .clone()
        .expect("inspector");
    assert_eq!(inspector.title, "Exploit the debug port on the TCU board");
    let before = view.project.as_ref().expect("project").threats[0]
        .inherent
        .clone();
    assert!(before.is_some());

    // Make that step very hard (level 0): the inherent risk drops at once.
    let edit = Edit::SetStepFeasibility {
        tree: inspector.tree,
        node: inspector.node,
        level: Some(0),
    };
    let sent = Instant::now();
    alice.send(Command::Edit(edit)).expect("sent");
    let view = until(&alice, "risk changed", |vm| {
        vm.project
            .as_ref()
            .is_some_and(|p| p.threats[0].inherent != before)
    })
    .await;
    assert!(sent.elapsed() < Duration::from_millis(500));
    assert_eq!(
        view.project
            .as_ref()
            .expect("project")
            .inspector
            .as_ref()
            .expect("inspector")
            .rating,
        Some(0)
    );

    // Saved, then undone: the risk comes back and the undo is saved too.
    until(&alice, "saved", project_ready).await;
    // The server recomputes after the edit, and the mark follows the
    // confirmed change: it never says "checked" for a state the server has
    // not agreed on (G15).
    until(&alice, "numbers checked after the edit", |vm| {
        vm.project.as_ref().is_some_and(|p| {
            assert!(
                p.numbers != NumbersCheck::Checked || p.risks_verified_seq == Some(p.confirmed_seq),
                "a stale check must not show as checked"
            );
            p.numbers == NumbersCheck::Checked && p.confirmed_seq == SAMPLE_OPS + 1
        })
    })
    .await;
    alice.send(Command::Undo).expect("sent");
    let view = until(&alice, "restored", |vm| {
        vm.project
            .as_ref()
            .is_some_and(|p| p.threats[0].inherent == before)
    })
    .await;
    assert!(view.project.as_ref().expect("project").can_redo);
    let view = until(&alice, "saved", project_ready).await;
    let seq = view.project.as_ref().expect("project").confirmed_seq;
    assert_eq!(
        seq,
        SAMPLE_OPS + 2,
        "the edit and its undo are both in the server's log"
    );
    drop(alice);
    server.stop().await;
}

/// UI-10.5, UI-10.9: a removal happens at once and a toast offers Undo; Undo
/// says what it undid and offers Redo; a new toast replaces the old one.
#[tokio::test]
async fn a_removal_toasts_with_undo_and_undo_toasts_with_redo() {
    let server = TestServer::start(Duration::ZERO).await;
    let alice = server.session("alice", false);
    create_and_open(&alice, "Brake").await;
    start_editing(&alice).await;
    alice.send(Command::CreateSample).expect("sent");
    let view = until(&alice, "saved", |vm| {
        project_ready(vm) && vm.project.as_ref().is_some_and(|p| p.threats.len() == 2)
    })
    .await;
    let threat = view.project.as_ref().expect("project").threats[0].clone();
    let toast = |vm: &ViewModel| {
        vm.notices
            .iter()
            .filter(|n| n.action.is_some())
            .map(|n| (n.text.clone(), n.action))
            .collect::<Vec<_>>()
    };
    assert!(toast(&view).is_empty());

    alice
        .send(Command::Edit(Edit::RemoveThreat { threat: threat.id }))
        .expect("sent");
    let view = until(&alice, "removal toast", |vm| !toast(vm).is_empty()).await;
    let toasts = toast(&view);
    assert_eq!(toasts.len(), 1);
    assert_eq!(toasts[0].1, Some(NoticeAction::Undo));
    // UI-10.10: the toast also says what else the removal changed.
    assert!(
        toasts[0]
            .0
            .starts_with(&format!("Threat scenario '{}' removed", threat.name)),
        "{}",
        toasts[0].0
    );
    let project = view.project.as_ref().expect("project");
    assert_eq!(project.threats.len(), 1);
    assert!(
        project
            .undo_label
            .as_deref()
            .is_some_and(|l| l.starts_with(&format!("remove threat scenario '{}'", threat.name)))
    );

    alice.send(Command::Undo).expect("sent");
    let view = until(&alice, "undo toast", |vm| {
        toast(vm)
            .iter()
            .any(|(_, a)| *a == Some(NoticeAction::Redo))
    })
    .await;
    let toasts = toast(&view);
    assert_eq!(toasts.len(), 1, "the new toast replaced the removal toast");
    assert!(
        toasts[0]
            .0
            .starts_with(&format!("Undone: remove threat scenario '{}'", threat.name)),
        "{}",
        toasts[0].0
    );
    let project = view.project.as_ref().expect("project");
    assert_eq!(project.threats.len(), 2, "the threat is back");
    // UI-10.5: the page of the restored element is open.
    assert_eq!(project.page, aegis_client::view::Page::Threats);
    assert!(
        project
            .redo_label
            .as_deref()
            .is_some_and(|l| l.starts_with(&format!("remove threat scenario '{}'", threat.name)))
    );
    drop(alice);
    server.stop().await;
}

/// UI-08: a draft is client-only. Cancelling or leaving it empty sends nothing;
/// committing creates the element as one edit that one undo takes back;
/// create-and-next starts the next draft.
#[tokio::test]
async fn a_draft_sends_nothing_until_it_is_created_and_undo_takes_it_back() {
    use aegis_client::view::DraftKind;
    let server = TestServer::start(Duration::ZERO).await;
    let alice = server.session("alice", false);
    create_and_open(&alice, "Brake").await;
    start_editing(&alice).await;
    let seq = |vm: &ViewModel| vm.project.as_ref().expect("project").confirmed_seq;
    let assets = |vm: &ViewModel| vm.project.as_ref().expect("project").data.assets.len();
    let draft = |vm: &ViewModel| vm.project.as_ref().expect("project").draft;
    let view = until(&alice, "ready", project_ready).await;
    let start = seq(&view);

    // Begin, then cancel: no op, no history.
    alice
        .send(Command::BeginDraft(DraftKind::Asset))
        .expect("sent");
    let view = until(&alice, "draft", |vm| draft(vm) == Some(DraftKind::Asset)).await;
    assert_eq!(
        view.project.as_ref().expect("p").page,
        aegis_client::view::Page::Assets
    );
    alice.send(Command::CancelDraft).expect("sent");
    let view = until(&alice, "cancelled", |vm| draft(vm).is_none()).await;
    assert_eq!((seq(&view), assets(&view)), (start, 0));
    assert!(!view.project.as_ref().expect("p").can_undo);

    // An empty name discards the draft the same way.
    alice
        .send(Command::BeginDraft(DraftKind::Asset))
        .expect("sent");
    until(&alice, "draft", |vm| draft(vm).is_some()).await;
    alice
        .send(Command::CommitDraft {
            name: "   ".into(),
            next: false,
        })
        .expect("sent");
    let view = until(&alice, "discarded", |vm| draft(vm).is_none()).await;
    assert_eq!((seq(&view), assets(&view)), (start, 0));

    // Create and next: the asset exists and a new draft is open.
    alice
        .send(Command::BeginDraft(DraftKind::Asset))
        .expect("sent");
    let first = until(&alice, "draft", |vm| draft(vm).is_some()).await;
    let first_seq = first.project.as_ref().expect("p").draft_seq;
    alice
        .send(Command::CommitDraft {
            name: "Brake ECU".into(),
            next: true,
        })
        .expect("sent");
    let view = until(&alice, "created", |vm| assets(vm) == 1).await;
    let p = view.project.as_ref().expect("p");
    assert_eq!(p.draft, Some(DraftKind::Asset), "the next draft started");
    assert!(p.draft_seq > first_seq, "the window clears its field");

    // Create and finish: one undo removes it.
    alice
        .send(Command::CommitDraft {
            name: "Gateway".into(),
            next: false,
        })
        .expect("sent");
    let view = until(&alice, "two assets", |vm| {
        assets(vm) == 2 && draft(vm).is_none()
    })
    .await;
    assert!(view.project.as_ref().expect("p").can_undo);
    alice.send(Command::Undo).expect("sent");
    until(&alice, "undone", |vm| assets(vm) == 1).await;
    drop(alice);
    server.stop().await;
}

/// UI-08.2: a threat draft needs an asset and a category. The asset comes from
/// the context when there is one; nothing is created until both have a value.
#[tokio::test]
async fn a_threat_draft_is_created_only_when_its_required_fields_are_set() {
    use aegis_client::view::DraftKind;
    use aegis_model::AssetId;
    let server = TestServer::start(Duration::ZERO).await;
    let alice = server.session("alice", false);
    create_and_open(&alice, "Brake").await;
    start_editing(&alice).await;
    fn p(vm: &ViewModel) -> &aegis_client::state::ProjectView {
        vm.project.as_ref().expect("project")
    }

    // No asset yet: the draft is incomplete and committing creates nothing.
    alice
        .send(Command::BeginDraft(DraftKind::Threat))
        .expect("sent");
    let view = until(&alice, "draft", |vm| p(vm).draft == Some(DraftKind::Threat)).await;
    assert!(p(&view).draft_incomplete);
    alice
        .send(Command::CommitDraft {
            name: "Spoofed frame".into(),
            next: false,
        })
        .expect("sent");
    let view = until(&alice, "still a draft", |vm| p(vm).draft.is_some()).await;
    assert_eq!(
        p(&view).threats.len(),
        0,
        "an incomplete draft creates nothing"
    );

    // With exactly one asset, the asset comes from the context.
    let asset = AssetId::new_v7();
    alice
        .send(Command::Edit(Edit::AddAsset {
            id: asset,
            name: "ECU".into(),
            properties: (0..6).collect(),
        }))
        .expect("sent");
    until(&alice, "asset", |vm| p(vm).data.assets.len() == 1).await;
    alice.send(Command::CancelDraft).expect("sent");
    until(&alice, "no draft", |vm| p(vm).draft.is_none()).await;
    alice
        .send(Command::BeginDraft(DraftKind::Threat))
        .expect("sent");
    let view = until(&alice, "draft 2", |vm| p(vm).draft.is_some()).await;
    assert_eq!(
        p(&view).draft_asset,
        Some(asset),
        "the only asset is the context"
    );
    assert_eq!(p(&view).draft_category, None);
    assert!(p(&view).draft_incomplete, "the category is missing");

    // Commit without a category does nothing; with one it creates the threat.
    alice
        .send(Command::CommitDraft {
            name: "Spoofed frame".into(),
            next: false,
        })
        .expect("sent");
    let view = until(&alice, "still a draft 2", |vm| p(vm).draft.is_some()).await;
    assert_eq!(p(&view).threats.len(), 0);
    alice.send(Command::SetDraftCategory(0)).expect("sent");
    let view = until(&alice, "complete", |vm| !p(vm).draft_incomplete).await;
    assert_eq!(p(&view).draft_category, Some(0));
    alice
        .send(Command::CommitDraft {
            name: "Spoofed frame".into(),
            next: false,
        })
        .expect("sent");
    let view = until(&alice, "created", |vm| p(vm).threats.len() == 1).await;
    assert_eq!(p(&view).threats[0].name, "Spoofed frame");
    assert!(p(&view).draft.is_none());
    alice.send(Command::Undo).expect("sent");
    until(&alice, "undone", |vm| p(vm).threats.is_empty()).await;
    drop(alice);
    server.stop().await;
}

/// UI-08.1: goals, claims and trees are created from a name alone.
#[tokio::test]
async fn goal_claim_and_tree_drafts_are_created_from_a_name() {
    use aegis_client::view::DraftKind;
    let server = TestServer::start(Duration::ZERO).await;
    let alice = server.session("alice", false);
    create_and_open(&alice, "Brake").await;
    start_editing(&alice).await;
    fn p(vm: &ViewModel) -> &aegis_client::state::ProjectView {
        vm.project.as_ref().expect("project")
    }
    for (kind, name) in [
        (DraftKind::Goal, "No remote braking"),
        (DraftKind::Claim, "Debug port is closed"),
        (DraftKind::Tree, "Reflash the ECU"),
    ] {
        alice.send(Command::BeginDraft(kind)).expect("sent");
        let view = until(&alice, "draft", |vm| p(vm).draft == Some(kind)).await;
        assert!(!p(&view).draft_incomplete, "{kind:?} needs only a name");
        alice
            .send(Command::CommitDraft {
                name: name.into(),
                next: false,
            })
            .expect("sent");
        // Each page fills only its own data, so check on the page of the draft.
        let view = until(&alice, name, |vm| {
            let p = p(vm);
            p.draft.is_none()
                && match kind {
                    DraftKind::Goal => p.data.goals.len() == 1,
                    DraftKind::Claim => p.data.claims.len() == 1,
                    _ => p.data.trees.len() == 1,
                }
        })
        .await;
        if kind == DraftKind::Goal {
            assert_eq!(p(&view).data.goals[0].name, name);
        }
    }
    drop(alice);
    server.stop().await;
}

/// UI-08.1/08.2 on the item definition page: components and trust boundaries
/// need a name; an interface needs a component and a kind; a data flow needs
/// two different components.
#[tokio::test]
async fn item_definition_drafts_wait_for_their_required_fields() {
    use aegis_client::view::DraftKind;
    use aegis_model::ComponentId;
    let server = TestServer::start(Duration::ZERO).await;
    let alice = server.session("alice", false);
    create_and_open(&alice, "Brake").await;
    start_editing(&alice).await;
    fn p(vm: &ViewModel) -> &aegis_client::state::ProjectView {
        vm.project.as_ref().expect("project")
    }
    let commit = |name: &str| Command::CommitDraft {
        name: name.into(),
        next: false,
    };

    // Component and trust boundary: a name is enough.
    for (kind, name) in [
        (DraftKind::Component, "Gateway"),
        (DraftKind::Component, "Brake ECU"),
        (DraftKind::TrustBoundary, "Safety domain"),
    ] {
        alice.send(Command::BeginDraft(kind)).expect("sent");
        let view = until(&alice, "draft", |vm| p(vm).draft == Some(kind)).await;
        assert!(!p(&view).draft_incomplete, "{kind:?} needs only a name");
        assert_eq!(p(&view).page, aegis_client::view::Page::Item);
        alice.send(commit(name)).expect("sent");
        until(&alice, name, |vm| p(vm).draft.is_none()).await;
    }
    let view = until(&alice, "two components", |vm| {
        p(vm).data.components.len() == 2 && p(vm).data.trust_boundaries.len() == 1
    })
    .await;
    let ids: Vec<ComponentId> = p(&view).data.components.iter().map(|c| c.id).collect();

    // Interface: the component comes from the context (the last one created),
    // the kind is missing until chosen.
    alice
        .send(Command::BeginDraft(DraftKind::Interface))
        .expect("sent");
    let view = until(&alice, "interface draft", |vm| {
        p(vm).draft == Some(DraftKind::Interface)
    })
    .await;
    assert!(
        p(&view).draft_component.is_some(),
        "context fills the component"
    );
    assert!(p(&view).draft_incomplete, "the kind is missing");
    alice.send(commit("CAN-2 port")).expect("sent");
    let view = until(&alice, "still a draft", |vm| p(vm).draft.is_some()).await;
    assert_eq!(p(&view).data.interfaces.len(), 0);
    alice.send(Command::SetDraftInterfaceKind(0)).expect("sent");
    until(&alice, "complete", |vm| !p(vm).draft_incomplete).await;
    alice.send(commit("CAN-2 port")).expect("sent");
    until(&alice, "interface created", |vm| {
        p(vm).draft.is_none() && p(vm).data.interfaces.len() == 1
    })
    .await;

    // Data flow: from comes from the context, to is missing; the same component
    // for both is still incomplete.
    alice
        .send(Command::BeginDraft(DraftKind::DataFlow))
        .expect("sent");
    let view = until(&alice, "flow draft", |vm| {
        p(vm).draft == Some(DraftKind::DataFlow)
    })
    .await;
    let from = p(&view).draft_component.expect("from from the context");
    assert!(p(&view).draft_incomplete);
    alice.send(Command::SetDraftTo(from)).expect("sent");
    let view = until(&alice, "same end", |vm| p(vm).draft_to == Some(from)).await;
    assert!(p(&view).draft_incomplete, "from and to must differ");
    let other = *ids
        .iter()
        .find(|id| **id != from)
        .expect("another component");
    alice.send(Command::SetDraftTo(other)).expect("sent");
    until(&alice, "complete", |vm| !p(vm).draft_incomplete).await;
    alice.send(commit("Brake frame")).expect("sent");
    until(&alice, "flow created", |vm| {
        p(vm).draft.is_none() && p(vm).data.data_flows.len() == 1
    })
    .await;
    alice.send(Command::Undo).expect("sent");
    until(&alice, "flow undone", |vm| p(vm).data.data_flows.is_empty()).await;
    drop(alice);
    server.stop().await;
}

/// A system is created from a draft (job 119) and the creation is undone.
#[tokio::test]
async fn a_system_is_created_from_a_draft_and_undone() {
    use aegis_client::view::{DraftKind, Page};
    let server = TestServer::start(Duration::ZERO).await;
    let alice = server.session("alice", false);
    create_and_open(&alice, "Brake").await;
    start_editing(&alice).await;
    fn p(vm: &ViewModel) -> &aegis_client::state::ProjectView {
        vm.project.as_ref().expect("project")
    }
    alice.send(Command::ShowPage(Page::Item)).expect("sent");
    alice
        .send(Command::BeginDraft(DraftKind::System))
        .expect("sent");
    until(&alice, "draft", |vm| p(vm).draft == Some(DraftKind::System)).await;
    alice
        .send(Command::CommitDraft {
            name: "Brake".into(),
            next: false,
        })
        .expect("sent");
    until(&alice, "one system", |vm| p(vm).data.systems.len() == 1).await;
    alice.send(Command::Undo).expect("sent");
    until(&alice, "no system", |vm| p(vm).data.systems.is_empty()).await;
    drop(alice);
    server.stop().await;
}

/// UI-08.6: when an edit is refused, the draft stays with the user's text and
/// the reason shows in the draft, not as a toast.
#[tokio::test]
async fn a_refused_draft_stays_and_says_why_in_the_draft() {
    use aegis_client::view::DraftKind;
    let server = TestServer::start(Duration::ZERO).await;
    let alice = server.session("alice", false);
    create_and_open(&alice, "Brake").await;
    start_editing(&alice).await;
    fn p(vm: &ViewModel) -> &aegis_client::state::ProjectView {
        vm.project.as_ref().expect("project")
    }
    alice
        .send(Command::BeginDraft(DraftKind::Control))
        .expect("sent");
    until(&alice, "draft", |vm| p(vm).draft.is_some()).await;
    // A name longer than the model allows is refused.
    alice
        .send(Command::CommitDraft {
            name: "x".repeat(400),
            next: false,
        })
        .expect("sent");
    let view = until(&alice, "refused", |vm| p(vm).draft_error.is_some()).await;
    assert!(p(&view).draft.is_some(), "the draft stays");
    assert!(
        !view.notices.iter().any(|n| n.severity == Severity::Error),
        "no error toast: the reason is in the draft"
    );
    // A good name creates it and clears the error.
    alice
        .send(Command::CommitDraft {
            name: "Secure boot".into(),
            next: false,
        })
        .expect("sent");
    let view = until(&alice, "created", |vm| p(vm).draft.is_none()).await;
    assert!(p(&view).draft_error.is_none());
    drop(alice);
    server.stop().await;
}

#[tokio::test]
async fn a_second_user_reads_a_saved_version_and_is_offered_the_newer_one() {
    let server = TestServer::start(Duration::ZERO).await;
    let alice = server.session("alice", false);
    let bob = server.session_with("bob", false, |c| {
        c.version_check = Duration::from_millis(100);
    });
    let id = create_and_open(&alice, "Brake").await;
    start_editing(&alice).await;

    until(&bob, "online", online).await;
    bob.send(Command::OpenProject { id }).expect("sent");
    let view = until(&bob, "bob sees the project", |vm| vm.project.is_some()).await;
    let project = view.project.as_ref().expect("project");
    assert_eq!(project.lock_holder.as_deref(), Some("alice"));
    assert!(!project.newer_version);

    // Alice builds the sample. Bob's screen does not change by itself: he is
    // told that a newer version exists and keeps reading the one he has.
    alice.send(Command::CreateSample).expect("sent");
    let view = until(&bob, "bob is told a newer version exists", |vm| {
        vm.project.as_ref().is_some_and(|p| p.newer_version)
    })
    .await;
    let project = view.project.as_ref().expect("project");
    assert_eq!(project.mode, Mode::ReadOnly);
    assert_eq!(project.confirmed_seq, 0, "still the version he opened");
    assert!(project.threats.is_empty());
    let view = until(&alice, "alice is current", project_ready).await;
    assert!(
        !view.project.as_ref().expect("project").newer_version,
        "an editor is never offered a reload"
    );

    // Bob reloads and reads Alice's saved work. His numbers are checked
    // against the server's after the reload, with no complaint.
    bob.send(Command::Reload).expect("sent");
    let view = until(&bob, "bob sees the threats", |vm| {
        vm.project.as_ref().is_some_and(|p| p.threats.len() == 2)
    })
    .await;
    let project = view.project.as_ref().expect("project");
    assert_eq!(project.mode, Mode::ReadOnly);
    assert_eq!(project.confirmed_seq, SAMPLE_OPS);
    assert!(!project.newer_version);
    let view = until(
        &bob,
        "bob's numbers are checked against the server's",
        |vm| {
            vm.project
                .as_ref()
                .is_some_and(|p| p.risks_verified_seq == Some(SAMPLE_OPS))
        },
    )
    .await;
    assert_eq!(
        view.project.as_ref().expect("project").numbers,
        NumbersCheck::Checked
    );
    assert!(
        !view.notices.iter().any(|n| n.text.contains("differ")),
        "{:?}",
        view.notices
    );

    // Bob asks for the lock and is told no.
    bob.send(Command::BeginEdit).expect("sent");
    let view = until(&bob, "lock refused", |vm| {
        vm.notices.iter().any(|n| n.text.contains("alice"))
    })
    .await;
    assert_eq!(view.project.as_ref().expect("project").mode, Mode::ReadOnly);

    // Alice finishes; the next version check shows the lock is free and Bob
    // can take it.
    alice.send(Command::EndEdit).expect("sent");
    until(&bob, "lock free", |vm| {
        vm.project.as_ref().is_some_and(|p| p.lock_holder.is_none())
    })
    .await;
    start_editing(&bob).await;
    let view = until(&bob, "bob edits", |vm| {
        vm.project.as_ref().is_some_and(|p| p.mode == Mode::Editing)
    })
    .await;
    assert_eq!(
        view.project.as_ref().expect("project").confirmed_seq,
        SAMPLE_OPS,
        "Bob's copy was current"
    );
    drop(alice);
    drop(bob);
    server.stop().await;
}

#[tokio::test]
async fn taking_the_lock_on_an_old_copy_reloads_it_first() {
    // Bob never presses Reload; asking for the lock still gives him the
    // newest version before he edits, so he cannot save onto an old base.
    let server = TestServer::start(Duration::ZERO).await;
    let alice = server.session("alice", false);
    let bob = server.session_with("bob", false, |c| {
        c.version_check = Duration::from_secs(3600);
    });
    let id = create_and_open(&alice, "Brake").await;
    start_editing(&alice).await;
    until(&bob, "online", online).await;
    bob.send(Command::OpenProject { id }).expect("sent");
    until(&bob, "bob sees the project", |vm| vm.project.is_some()).await;
    alice.send(Command::CreateSample).expect("sent");
    until(&alice, "saved", project_ready).await;
    alice.send(Command::EndEdit).expect("sent");
    until(&alice, "released", |vm| {
        vm.project
            .as_ref()
            .is_some_and(|p| p.mode == Mode::ReadOnly)
    })
    .await;

    start_editing(&bob).await;
    let view = until(&bob, "bob has the newest copy", |vm| {
        vm.project.as_ref().is_some_and(|p| {
            p.mode == Mode::Editing && p.confirmed_seq == SAMPLE_OPS && p.threats.len() == 2
        })
    })
    .await;
    assert!(!view.project.as_ref().expect("project").newer_version);
    drop(alice);
    drop(bob);
    server.stop().await;
}

#[tokio::test]
async fn only_a_read_only_view_checks_for_a_newer_version_and_not_too_often() {
    let server = TestServer::start(Duration::ZERO).await;
    let alice = server.session_with("alice", false, |c| {
        c.version_check = Duration::from_millis(200);
    });
    create_and_open(&alice, "Quiet").await;
    start_editing(&alice).await;
    let before = server.status_calls.load(Ordering::Relaxed);
    tokio::time::sleep(Duration::from_millis(1000)).await;
    assert_eq!(
        server.status_calls.load(Ordering::Relaxed),
        before,
        "an editor is the only writer and asks nothing"
    );

    alice.send(Command::EndEdit).expect("sent");
    until(&alice, "read-only", |vm| {
        vm.project
            .as_ref()
            .is_some_and(|p| p.mode == Mode::ReadOnly)
    })
    .await;
    let before = server.status_calls.load(Ordering::Relaxed);
    tokio::time::sleep(Duration::from_millis(1000)).await;
    let asked = server.status_calls.load(Ordering::Relaxed) - before;
    assert!(
        (2..=6).contains(&asked),
        "a read-only view asks about every 200 ms, not in a loop: {asked} calls in a second"
    );
    drop(alice);
    server.stop().await;
}

#[tokio::test]
async fn losing_the_lock_discards_unsaved_work_and_reloads() {
    // Saves take 400 ms, so there is time for an admin to break the lock
    // while Alice's change is still on its way.
    let server = TestServer::start(Duration::from_millis(400)).await;
    let alice = server.session("alice", false);
    let root = server.session("root", true);
    let id = create_and_open(&alice, "Brake").await;
    start_editing(&alice).await;
    alice.send(Command::CreateSample).expect("sent");
    until(&alice, "saved", |vm| {
        project_ready(vm) && vm.project.as_ref().is_some_and(|p| p.threats.len() == 2)
    })
    .await;

    // A second change, in flight, when the admin breaks the lock.
    alice.send(Command::SelectThreat(0)).expect("sent");
    let view = until(&alice, "tree", |vm| {
        vm.project.as_ref().is_some_and(|p| p.tree.is_some())
    })
    .await;
    let scene = view
        .project
        .as_ref()
        .expect("project")
        .scene()
        .expect("scene")
        .clone();
    let phish = (0..scene.len())
        .find(|i| scene.label(*i as u32).starts_with("Phish"))
        .expect("step");
    alice.send(Command::SelectNode(Some(phish))).expect("sent");
    let view = until(&alice, "inspector", |vm| {
        vm.project.as_ref().is_some_and(|p| p.inspector.is_some())
    })
    .await;
    let inspector = view
        .project
        .as_ref()
        .expect("project")
        .inspector
        .clone()
        .expect("inspector");
    let rating_before = inspector.rating;
    alice
        .send(Command::Edit(Edit::SetStepFeasibility {
            tree: inspector.tree,
            node: inspector.node,
            level: Some(3),
        }))
        .expect("sent");
    until(&alice, "edit visible", |vm| {
        vm.project
            .as_ref()
            .and_then(|p| p.inspector.as_ref())
            .is_some_and(|i| i.rating == Some(3))
    })
    .await;

    until(&root, "online", online).await;
    root.send(Command::OpenProject { id: id.clone() })
        .expect("sent");
    until(&root, "root sees the project", |vm| vm.project.is_some()).await;
    // The client has no "break lock" command (it is an admin tool), so call
    // the API directly.
    let channel = tonic::transport::Endpoint::from_shared(server.url.clone())
        .expect("uri")
        .connect()
        .await
        .expect("connect");
    let mut admin = aegis_proto::aegis_client::AegisClient::with_interceptor(
        channel,
        |mut r: tonic::Request<()>| {
            r.metadata_mut()
                .insert("x-aegis-dev-user", "root".parse().expect("ascii"));
            r.metadata_mut()
                .insert("x-aegis-dev-admin", "1".parse().expect("ascii"));
            Ok(r)
        },
    );
    admin
        .break_lock(aegis_proto::LockRequest { project_id: id })
        .await
        .expect("admin breaks the lock");

    // Alice is told, is read-only again, and sees the server's truth: the
    // in-flight change was not saved.
    let view = until(&alice, "lock lost", |vm| {
        vm.notices
            .iter()
            .any(|n| n.severity == Severity::Warning && n.text.contains("edit lock"))
            && vm
                .project
                .as_ref()
                .is_some_and(|p| p.mode == Mode::ReadOnly && p.sync == SyncView::Synced)
    })
    .await;
    let project = view.project.as_ref().expect("project");
    assert!(
        project
            .inspector
            .as_ref()
            .is_none_or(|i| i.rating == rating_before),
        "unsaved edit is gone"
    );
    assert_eq!(project.confirmed_seq, SAMPLE_OPS);
    assert!(!project.can_undo, "history was cleared");
    drop(alice);
    drop(root);
    server.stop().await;
}

#[tokio::test]
async fn closing_the_project_releases_the_lock() {
    let server = TestServer::start(Duration::ZERO).await;
    let alice = server.session("alice", false);
    let bob = server.session("bob", false);
    let id = create_and_open(&alice, "Brake").await;
    start_editing(&alice).await;
    alice.send(Command::CloseProject).expect("sent");
    until(&alice, "closed", |vm| vm.project.is_none()).await;

    until(&bob, "online", online).await;
    bob.send(Command::OpenProject { id }).expect("sent");
    until(&bob, "bob sees the project", |vm| vm.project.is_some()).await;
    // Give the best-effort release a moment, then Bob can edit.
    let started = Instant::now();
    loop {
        bob.send(Command::BeginEdit).expect("sent");
        tokio::time::sleep(Duration::from_millis(50)).await;
        if bob
            .view()
            .project
            .as_ref()
            .is_some_and(|p| p.mode == Mode::Editing)
        {
            break;
        }
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "the lock was never released"
        );
    }
    drop(alice);
    drop(bob);
    server.stop().await;
}

/// Serves `body` to every request over plain HTTP on loopback (the client
/// allows that for tests and lab setups only). Returns the URL.
async fn serve_manifest(body: String) -> String {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let url = format!("http://{}/m", listener.local_addr().expect("addr"));
    tokio::spawn(async move {
        while let Ok((mut tcp, _)) = listener.accept().await {
            let body = body.clone();
            tokio::spawn(async move {
                let mut buf = [0u8; 1024];
                let _ = tcp.read(&mut buf).await;
                let head = format!(
                    "HTTP/1.1 200 OK\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                    body.len()
                );
                let _ = tcp.write_all(head.as_bytes()).await;
                let _ = tcp.write_all(body.as_bytes()).await;
            });
        }
    });
    url
}

#[tokio::test]
async fn checking_for_updates_says_what_it_found() {
    use aegis_license::{Artifact, Issuer, Manifest, TrustedKeys, Version};
    let (pkcs8, public) = Issuer::generate().expect("key");
    let issuer = Issuer::from_pkcs8(&pkcs8).expect("issuer");
    let platform = aegis_client::updates::platform_name();
    let manifest = Manifest {
        version: 1,
        kid: "upd".into(),
        release: "9.0.0".into(),
        published_at: 1_800_000_000,
        artifacts: vec![Artifact {
            platform,
            url: "https://downloads.example.com/aegis".into(),
            sha256: "0".repeat(64),
            bytes: 5_000_000,
        }],
        notes: "Much better.".into(),
    };
    let file = issuer.sign_manifest(&manifest).expect("signs");
    let keys = TrustedKeys::single("upd", public).expect("keys");
    let url = serve_manifest(file).await;

    let server = TestServer::start(Duration::ZERO).await;
    // No address set: the user is told how to set one.
    let none = server.session("alice", false);
    until(&none, "online", online).await;
    none.send(Command::CheckForUpdates).expect("sent");
    until(&none, "told", |vm| {
        vm.notices
            .iter()
            .any(|n| n.text.contains("No update address"))
    })
    .await;

    // An address: the signed release is announced with its download address.
    let found = server.session_with("alice", false, |c| {
        c.update_url = url.clone();
        c.update_checker = Arc::new(aegis_client::updates::UpdateChecker::new(
            keys.clone(),
            Version::parse("0.1.0").expect("v"),
            aegis_client::updates::platform_name(),
        ));
    });
    until(&found, "online", online).await;
    found.send(Command::CheckForUpdates).expect("sent");
    let view = until(&found, "offer", |vm| {
        vm.notices.iter().any(|n| n.text.contains("9.0.0"))
    })
    .await;
    let text = &view
        .notices
        .iter()
        .find(|n| n.text.contains("9.0.0"))
        .expect("notice")
        .text;
    assert!(
        text.contains("https://downloads.example.com/aegis"),
        "{text}"
    );
    assert!(text.contains("5 MB"), "{text}");

    // A manifest this build does not trust is an error, not an offer.
    let (_, other) = Issuer::generate().expect("key");
    let distrust = server.session_with("alice", false, |c| {
        c.update_url = url.clone();
        c.update_checker = Arc::new(aegis_client::updates::UpdateChecker::new(
            TrustedKeys::single("upd", other).expect("keys"),
            Version::parse("0.1.0").expect("v"),
            aegis_client::updates::platform_name(),
        ));
    });
    until(&distrust, "online", online).await;
    distrust.send(Command::CheckForUpdates).expect("sent");
    let view = until(&distrust, "refused", |vm| {
        vm.notices.iter().any(|n| n.severity == Severity::Error)
    })
    .await;
    assert!(
        view.notices
            .iter()
            .any(|n| n.text.contains("Could not check for updates")),
        "{:?}",
        view.notices
    );
    drop((none, found, distrust));
    server.stop().await;
}

/// UI-09: the row-action + passes its parent as draft context, and Duplicate
/// clones an element. Exercises the new edits end to end against the model.
#[tokio::test(flavor = "current_thread")]
async fn row_actions_pass_the_parent_as_draft_context() {
    use aegis_client::view::{DraftKind, Page, Selection};
    use aegis_model::{ComponentId, ControlId, DamageId};

    let server = TestServer::start(Duration::ZERO).await;
    let alice = server.session("alice", false);
    create_and_open(&alice, "Brake").await;
    start_editing(&alice).await;

    let component = ComponentId::new_v7();
    let damage = DamageId::new_v7();
    let control = ControlId::new_v7();

    // Component -> asset bound to it.
    alice
        .send(Command::Edit(Edit::AddComponent {
            id: component,
            name: "Brake ECU".into(),
            parent: None,
        }))
        .expect("sent");
    alice
        .send(Command::Select {
            selection: Selection::Component(component),
            navigate: false,
        })
        .expect("sent");
    alice
        .send(Command::BeginDraft(DraftKind::Asset))
        .expect("sent");
    until(&alice, "asset draft", |vm| {
        vm.project
            .as_ref()
            .is_some_and(|p| p.draft == Some(DraftKind::Asset))
    })
    .await;
    alice
        .send(Command::CommitDraft {
            name: "Bound asset".into(),
            next: false,
        })
        .expect("sent");
    let vm = until(&alice, "asset bound", |vm| {
        vm.project.as_ref().is_some_and(|p| {
            p.draft.is_none()
                && p.data
                    .asset
                    .as_ref()
                    .is_some_and(|a| a.component == Some(component))
        })
    })
    .await;
    assert_eq!(vm.project.as_ref().unwrap().data.assets.len(), 1);

    // Asset -> threat on it (the asset is the only one, so it is the context).
    alice
        .send(Command::BeginDraft(DraftKind::Threat))
        .expect("sent");
    until(&alice, "threat draft", |vm| {
        vm.project
            .as_ref()
            .is_some_and(|p| p.draft == Some(DraftKind::Threat))
    })
    .await;
    alice.send(Command::SetDraftCategory(0)).expect("sent");
    alice
        .send(Command::CommitDraft {
            name: "On the asset".into(),
            next: false,
        })
        .expect("sent");
    let vm = until(&alice, "threat created", |vm| {
        vm.project
            .as_ref()
            .is_some_and(|p| p.threats.len() == 1 && p.draft.is_none())
    })
    .await;
    assert_eq!(vm.project.as_ref().unwrap().threats[0].asset, "Bound asset");

    // Damage -> threat that leads to it.
    alice
        .send(Command::Edit(Edit::AddDamage {
            id: damage,
            name: "Loss of braking".into(),
        }))
        .expect("sent");
    alice
        .send(Command::Select {
            selection: Selection::Damage(damage),
            navigate: false,
        })
        .expect("sent");
    alice
        .send(Command::BeginDraft(DraftKind::Threat))
        .expect("sent");
    until(&alice, "threat draft 2", |vm| {
        vm.project
            .as_ref()
            .is_some_and(|p| p.draft == Some(DraftKind::Threat))
    })
    .await;
    alice.send(Command::SetDraftCategory(0)).expect("sent");
    alice
        .send(Command::CommitDraft {
            name: "Leading threat".into(),
            next: false,
        })
        .expect("sent");
    let vm = until(&alice, "leading threat", |vm| {
        vm.project.as_ref().is_some_and(|p| {
            p.threats.len() == 2
                && p.data
                    .threat
                    .as_ref()
                    .is_some_and(|t| t.damages.iter().any(|d| d.linked))
        })
    })
    .await;

    // Threat -> attack tree for it.
    let threat = vm.project.as_ref().unwrap().threats[0].id;
    alice
        .send(Command::Select {
            selection: Selection::Threat(threat),
            navigate: false,
        })
        .expect("sent");
    alice
        .send(Command::BeginDraft(DraftKind::Tree))
        .expect("sent");
    until(&alice, "tree draft", |vm| {
        vm.project
            .as_ref()
            .is_some_and(|p| p.draft == Some(DraftKind::Tree))
    })
    .await;
    alice
        .send(Command::CommitDraft {
            name: "Reflash".into(),
            next: false,
        })
        .expect("sent");
    let _vm = until(&alice, "tree linked", |vm| {
        vm.project.as_ref().is_some_and(|p| {
            p.draft.is_none() && p.tree.as_ref().is_some_and(|t| t.threat == Some(threat))
        })
    })
    .await;

    // Control -> goal linked to it.
    alice
        .send(Command::Edit(Edit::AddControl {
            id: control,
            name: "Secure boot".into(),
        }))
        .expect("sent");
    alice
        .send(Command::Select {
            selection: Selection::Control(control),
            navigate: false,
        })
        .expect("sent");
    alice
        .send(Command::BeginDraft(DraftKind::Goal))
        .expect("sent");
    until(&alice, "goal draft", |vm| {
        vm.project
            .as_ref()
            .is_some_and(|p| p.draft == Some(DraftKind::Goal))
    })
    .await;
    alice
        .send(Command::CommitDraft {
            name: "No remote braking".into(),
            next: false,
        })
        .expect("sent");
    let _vm = until(&alice, "goal linked", |vm| {
        vm.project.as_ref().is_some_and(|p| {
            p.draft.is_none()
                && p.data
                    .goal
                    .as_ref()
                    .is_some_and(|g| g.controls.iter().any(|c| c.linked))
        })
    })
    .await;

    // Duplicate a component (the Item page fills the component data).
    alice
        .send(Command::Edit(Edit::DuplicateComponent {
            id: ComponentId::new_v7(),
            source: component,
            suffix: "copy".into(),
        }))
        .expect("sent");
    alice.send(Command::ShowPage(Page::Item)).expect("sent");
    let vm = until(&alice, "component duplicated", |vm| {
        vm.project
            .as_ref()
            .is_some_and(|p| p.data.components.len() == 2)
    })
    .await;
    let names: Vec<_> = vm
        .project
        .as_ref()
        .unwrap()
        .data
        .components
        .iter()
        .map(|c| c.name.clone())
        .collect();
    assert!(names.contains(&"Brake ECU copy".to_owned()), "{names:?}");

    // Undo the duplicate as one step: back to one component.
    alice.send(Command::Undo).expect("sent");
    let vm = until(&alice, "duplicate undone", |vm| {
        vm.project
            .as_ref()
            .is_some_and(|p| p.data.components.len() == 1)
    })
    .await;
    assert_eq!(
        vm.project.as_ref().unwrap().data.components[0].name,
        "Brake ECU"
    );

    drop(alice);
    server.stop().await;
}

#[tokio::test]
async fn switching_the_system_view_adds_no_op_also_when_read_only() {
    use aegis_client::systems::SystemView;
    use aegis_client::view::Page;
    use aegis_model::{ComponentId, ElementId};

    fn p(vm: &ViewModel) -> &aegis_client::state::ProjectView {
        vm.project.as_ref().expect("project")
    }
    let server = TestServer::start(Duration::ZERO).await;
    let alice = server.session("alice", false);
    create_and_open(&alice, "Brake").await;
    start_editing(&alice).await;
    let (brake, adas) = (ElementId::new_v7(), ElementId::new_v7());
    let (caliper, camera) = (ComponentId::new_v7(), ComponentId::new_v7());
    let edits = [
        Edit::AddSystem {
            id: brake,
            name: "Brake".into(),
        },
        Edit::AddSystem {
            id: adas,
            name: "ADAS".into(),
        },
        Edit::AddComponent {
            id: caliper,
            name: "Caliper".into(),
            parent: None,
        },
        Edit::AddComponent {
            id: camera,
            name: "Camera".into(),
            parent: None,
        },
        Edit::MoveToSystem {
            components: vec![caliper],
            system: Some(brake),
        },
        Edit::MoveToSystem {
            components: vec![camera],
            system: Some(adas),
        },
    ];
    for e in edits {
        alice.send(Command::Edit(e)).expect("sent");
    }
    alice.send(Command::ShowPage(Page::Item)).expect("sent");
    let view = until(&alice, "two components", |vm| {
        project_ready(vm) && p(vm).data.components.len() == 2
    })
    .await;
    let seq = p(&view).confirmed_seq;
    let label = p(&view).undo_label.clone();
    assert!(label.is_some());

    alice
        .send(Command::SetSystemView(SystemView::only(brake)))
        .expect("sent");
    let view = until(&alice, "one component", |vm| {
        p(vm).data.components.len() == 1
    })
    .await;
    assert_eq!(p(&view).confirmed_seq, seq);
    assert_eq!(p(&view).undo_label, label);
    assert_eq!(
        p(&view).view_menu.as_ref().map(|m| m.chip.as_str()),
        Some("Brake")
    );

    alice.send(Command::EndEdit).expect("sent");
    until(&alice, "read-only", |vm| p(vm).mode == Mode::ReadOnly).await;
    alice
        .send(Command::SetSystemView(SystemView::all()))
        .expect("sent");
    let view = until(&alice, "two rows again", |vm| {
        p(vm).data.components.len() == 2
    })
    .await;
    assert_eq!(p(&view).confirmed_seq, seq);
    drop(alice);
    server.stop().await;
}

#[tokio::test]
async fn an_undo_outside_the_view_adds_its_system_to_the_view() {
    use aegis_client::systems::SystemView;
    use aegis_client::view::Page;
    use aegis_model::{ComponentId, ElementId};

    fn p(vm: &ViewModel) -> &aegis_client::state::ProjectView {
        vm.project.as_ref().expect("project")
    }
    fn chip(vm: &ViewModel) -> Option<&str> {
        p(vm).view_menu.as_ref().map(|m| m.chip.as_str())
    }
    fn redo_toast(vm: &ViewModel) -> Option<String> {
        vm.notices
            .iter()
            .find(|n| n.action == Some(NoticeAction::Redo))
            .map(|n| n.text.clone())
    }
    fn undo_toast(vm: &ViewModel) -> Option<String> {
        vm.notices
            .iter()
            .find(|n| n.action == Some(NoticeAction::Undo) && n.text.starts_with("Redone"))
            .map(|n| n.text.clone())
    }
    let server = TestServer::start(Duration::ZERO).await;
    let alice = server.session("alice", false);
    create_and_open(&alice, "Brake").await;
    start_editing(&alice).await;
    let (brake, adas) = (ElementId::new_v7(), ElementId::new_v7());
    let (caliper, camera) = (ComponentId::new_v7(), ComponentId::new_v7());
    let edits = [
        Edit::AddSystem {
            id: brake,
            name: "Brake".into(),
        },
        Edit::AddSystem {
            id: adas,
            name: "ADAS".into(),
        },
        Edit::AddComponent {
            id: caliper,
            name: "Caliper".into(),
            parent: None,
        },
        Edit::AddComponent {
            id: camera,
            name: "Camera".into(),
            parent: None,
        },
        Edit::MoveToSystem {
            components: vec![caliper],
            system: Some(brake),
        },
        Edit::MoveToSystem {
            components: vec![camera],
            system: Some(adas),
        },
    ];
    for e in edits {
        alice.send(Command::Edit(e)).expect("sent");
    }
    alice.send(Command::ShowPage(Page::Item)).expect("sent");
    until(&alice, "two components", |vm| {
        project_ready(vm) && p(vm).data.components.len() == 2
    })
    .await;
    let settled = until(&alice, "settled", project_ready).await;
    let before = p(&settled).confirmed_seq;
    alice
        .send(Command::Edit(Edit::RenameComponent {
            component: camera,
            name: "Front camera".into(),
        }))
        .expect("sent");
    let view = until(&alice, "renamed", |vm| {
        project_ready(vm) && p(vm).confirmed_seq > before
    })
    .await;
    let seq = p(&view).confirmed_seq;

    alice
        .send(Command::SetSystemView(SystemView::only(brake)))
        .expect("sent");
    let view = until(&alice, "one component", |vm| {
        p(vm).data.components.len() == 1
    })
    .await;
    assert_eq!(chip(&view), Some("Brake"));
    assert_eq!(p(&view).confirmed_seq, seq);

    alice.send(Command::Undo).expect("sent");
    let view = until(&alice, "undo toast", |vm| {
        redo_toast(vm).is_some() && project_ready(vm)
    })
    .await;
    let text = redo_toast(&view).expect("toast");
    assert!(text.starts_with("Undone: "), "{text}");
    assert!(text.ends_with(" View now includes ADAS."), "{text}");
    assert_eq!(chip(&view), Some("Brake + ADAS"));
    assert_eq!(p(&view).page, Page::Item);
    let detail = p(&view).data.component.as_ref().expect("selected");
    assert_eq!(detail.id, camera);
    assert_eq!(detail.name, "Camera");
    let view = until(&alice, "two rows", |vm| {
        p(vm).data.components.len() == 2 && project_ready(vm)
    })
    .await;
    let seq = p(&view).confirmed_seq;

    alice
        .send(Command::SetSystemView(SystemView::only(brake)))
        .expect("sent");
    let view = until(&alice, "one row", |vm| p(vm).data.components.len() == 1).await;
    assert_eq!(p(&view).confirmed_seq, seq, "a view change is never an op");

    alice.send(Command::Redo).expect("sent");
    let view = until(&alice, "redo toast", |vm| {
        undo_toast(vm).is_some() && project_ready(vm)
    })
    .await;
    let text = undo_toast(&view).expect("toast");
    assert!(text.ends_with(" View now includes ADAS."), "{text}");
    assert_eq!(chip(&view), Some("Brake + ADAS"));

    alice
        .send(Command::SetSystemView(SystemView::only(adas)))
        .expect("sent");
    until(&alice, "adas only", |vm| chip(vm) == Some("ADAS")).await;
    let settled = until(&alice, "settled", project_ready).await;
    let before = p(&settled).confirmed_seq;
    alice
        .send(Command::Edit(Edit::RenameComponent {
            component: camera,
            name: "Rear camera".into(),
        }))
        .expect("sent");
    until(&alice, "renamed again", |vm| {
        project_ready(vm) && p(vm).confirmed_seq > before
    })
    .await;
    alice.send(Command::Undo).expect("sent");
    let view = until(&alice, "second undo toast", |vm| redo_toast(vm).is_some()).await;
    let text = redo_toast(&view).expect("toast");
    assert!(!text.contains("View now includes"), "{text}");
    assert_eq!(chip(&view), Some("ADAS"));
    drop(alice);
    server.stop().await;
}

#[tokio::test]
async fn a_stale_system_name_stops_the_report_export() {
    use aegis_client::report::{ExportKind, ReportFilters, ReportProfile};
    use aegis_client::view::Page;
    use aegis_model::{ComponentId, ElementId};

    fn export(session: &Session, dir: &std::path::Path, file: &str, systems: &[&str]) {
        session
            .send(Command::Export {
                kind: ExportKind::Report,
                path: dir.join(file),
                profile: Box::new(ReportProfile {
                    filters: ReportFilters {
                        systems: systems.iter().map(|s| (*s).to_owned()).collect(),
                        ..ReportFilters::default()
                    },
                    ..ReportProfile::default()
                }),
            })
            .expect("sent");
    }
    async fn saved(session: &Session, file: &str) {
        until(session, file, |vm| {
            vm.notices
                .iter()
                .any(|n| n.text.starts_with("Saved") && n.text.ends_with(file))
        })
        .await;
    }
    let dir = std::env::temp_dir().join(format!("aegis-stale-{}", std::process::id()));
    let server = TestServer::start(Duration::ZERO).await;
    let alice = server.session("alice", false);
    create_and_open(&alice, "Vehicle").await;

    // A project with no systems exports as before.
    export(&alice, &dir, "plain.html", &["Brake"]);
    saved(&alice, "plain.html").await;
    assert!(dir.join("plain.html").exists());

    start_editing(&alice).await;
    let (brake, caliper) = (ElementId::new_v7(), ComponentId::new_v7());
    let edits = [
        Edit::AddSystem {
            id: brake,
            name: "Brake".into(),
        },
        Edit::AddComponent {
            id: caliper,
            name: "Caliper".into(),
            parent: None,
        },
        Edit::MoveToSystem {
            components: vec![caliper],
            system: Some(brake),
        },
    ];
    for e in edits {
        alice.send(Command::Edit(e)).expect("sent");
    }
    alice.send(Command::ShowPage(Page::Item)).expect("sent");
    until(&alice, "one component", |vm| {
        project_ready(vm)
            && vm
                .project
                .as_ref()
                .is_some_and(|p| p.data.components.len() == 1)
    })
    .await;

    export(&alice, &dir, "stale.html", &["Brake", "Gone"]);
    let view = until(&alice, "stale warning", |vm| {
        vm.notices
            .iter()
            .any(|n| n.severity == Severity::Warning && n.text.contains("\"Gone\""))
    })
    .await;
    let warning = view
        .notices
        .iter()
        .find(|n| n.severity == Severity::Warning && n.text.contains("\"Gone\""))
        .expect("warning");
    assert!(!warning.text.contains("\"Brake\""), "{}", warning.text);
    assert!(!dir.join("stale.html").exists());

    export(&alice, &dir, "brake.html", &["Brake"]);
    saved(&alice, "brake.html").await;
    assert!(dir.join("brake.html").exists());

    let _ = std::fs::remove_dir_all(&dir);
    drop(alice);
    server.stop().await;
}

/// The snapshot is the version downloaded on open; local edits reach it only
/// through a new download (job 208).
#[tokio::test]
async fn snapshot_is_the_downloaded_version() {
    use aegis_model::AssetId;
    fn p(vm: &ViewModel) -> &aegis_client::state::ProjectView {
        vm.project.as_ref().expect("project")
    }
    let server = TestServer::start(Duration::ZERO).await;
    let alice = server.session("alice", false);
    let id = create_and_open(&alice, "Snapshot").await;
    start_editing(&alice).await;
    alice
        .send(Command::ShowPage(aegis_client::view::Page::Assets))
        .expect("sent");
    alice
        .send(Command::Edit(Edit::AddAsset {
            id: AssetId::new_v7(),
            name: "ECU".into(),
            properties: (0..6).collect(),
        }))
        .expect("sent");
    let view = until(&alice, "asset", |vm| p(vm).data.assets.len() == 1).await;
    assert_eq!(
        p(&view).snapshot.assets().count(),
        0,
        "a local edit reached the snapshot"
    );
    let alice_seq = p(&view).snapshot_seq;

    until(&alice, "synced", project_ready).await;
    alice.send(Command::EndEdit).expect("sent");
    until(&alice, "released", |vm| {
        vm.project
            .as_ref()
            .is_some_and(|p| p.mode == Mode::ReadOnly && p.lock_holder.is_none())
    })
    .await;

    let bob = server.session("bob", false);
    until(&bob, "online", online).await;
    bob.send(Command::OpenProject { id }).expect("sent");
    let view = until(&bob, "bob sees the project", |vm| vm.project.is_some()).await;
    let names: Vec<&str> = p(&view)
        .snapshot
        .assets()
        .map(|(_, _, a)| a.name())
        .collect();
    assert_eq!(names, ["ECU"]);
    assert!(p(&view).snapshot_seq > alice_seq);
    drop(alice);
    drop(bob);
    server.stop().await;
}
