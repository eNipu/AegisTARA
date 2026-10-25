//! The session against a real server: optimistic editing, saving, live
//! viewing, lock loss, and reloading, over loopback gRPC.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use aegis_client::commands::{Command, Edit};
use aegis_client::connection::DevLogin;
use aegis_client::session::{Session, SessionConfig};
use aegis_client::state::{ConnectionView, Mode, Severity, SyncView, ViewModel};
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

/// Delays every request whose path ends in `SubmitOps`, to model a slow link.
#[derive(Clone)]
struct SlowSubmit(Duration);

impl<S> Layer<S> for SlowSubmit {
    type Service = SlowSubmitService<S>;
    fn layer(&self, inner: S) -> SlowSubmitService<S> {
        SlowSubmitService {
            inner,
            delay: self.0,
        }
    }
}

#[derive(Clone)]
struct SlowSubmitService<S> {
    inner: S,
    delay: Duration,
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
    stop: Option<oneshot::Sender<()>>,
    done: Option<tokio::task::JoinHandle<()>>,
}

impl TestServer {
    async fn start(submit_delay: Duration) -> Self {
        let registry = Arc::new(Registry::new(
            Arc::new(MemoryStore::new()),
            Arc::new(Methodologies::builtin().expect("builtin")),
            Arc::new(ServerConfig {
                lease: Duration::from_secs(30),
                ..ServerConfig::default()
            }),
        ));
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let url = format!("http://{}", listener.local_addr().expect("addr"));
        let (stop, stopped) = oneshot::channel::<()>();
        let service = AegisService::new(Arc::clone(&registry)).into_server();
        let layer = AuthLayer::new(Arc::new(DevAuthn), &url);
        let done = tokio::spawn(async move {
            server::builder()
                .layer(layer)
                .layer(SlowSubmit(submit_delay))
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
            stop: Some(stop),
            done: Some(done),
        }
    }

    fn session(&self, user: &str, admin: bool) -> Client {
        let mut config = SessionConfig::new(user);
        config.retry_delay = Duration::from_millis(100);
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

#[tokio::test]
async fn a_second_user_follows_the_editor_live_and_cannot_edit() {
    let server = TestServer::start(Duration::ZERO).await;
    let alice = server.session("alice", false);
    let bob = server.session("bob", false);
    let id = create_and_open(&alice, "Brake").await;
    start_editing(&alice).await;

    until(&bob, "online", online).await;
    bob.send(Command::OpenProject { id }).expect("sent");
    let view = until(&bob, "bob sees the project", |vm| vm.project.is_some()).await;
    assert_eq!(
        view.project
            .as_ref()
            .expect("project")
            .lock_holder
            .as_deref(),
        Some("alice")
    );

    // Alice builds the sample; Bob's screen fills in without Bob doing anything.
    alice.send(Command::CreateSample).expect("sent");
    let view = until(&bob, "bob sees the threats", |vm| {
        vm.project.as_ref().is_some_and(|p| p.threats.len() == 2)
    })
    .await;
    assert_eq!(view.project.as_ref().expect("project").mode, Mode::ReadOnly);
    assert_eq!(
        view.project.as_ref().expect("project").confirmed_seq,
        SAMPLE_OPS
    );

    // Bob asks for the lock and is told no.
    bob.send(Command::BeginEdit).expect("sent");
    let view = until(&bob, "lock refused", |vm| {
        vm.notices.iter().any(|n| n.text.contains("alice"))
    })
    .await;
    assert_eq!(view.project.as_ref().expect("project").mode, Mode::ReadOnly);

    // Alice finishes; the lock is free and Bob can take it.
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
