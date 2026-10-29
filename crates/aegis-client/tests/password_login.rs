//! Password login end to end (job 037): the server stack exactly as the
//! binary builds it (`server::router`: health probe, gRPC-Web, the
//! authentication layer, both services), and the client over both
//! transports.

use std::sync::Arc;
use std::time::Duration;

use aegis_client::connection::{Connection, LoginMethod, PasswordLogin, Public};
use aegis_client::error::{ClientError, ServerCode};
use aegis_client::transport::TransportKind;
use aegis_proto as pb;
use aegis_server::accounts::service::{AccountsService, LoginOptions};
use aegis_server::accounts::{AccountBackend, Accounts, AccountsConfig, MemoryAccounts};
use aegis_server::auth_layer::{AuthLayer, PasswordAuthn};
use aegis_server::config::ServerConfig;
use aegis_server::methodologies::Methodologies;
use aegis_server::{AegisService, MemoryStore, Registry, server};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio_stream::wrappers::TcpListenerStream;

const ADMIN_PASSWORD: &str = "a long admin password";

struct TestServer {
    url: String,
    accounts: Arc<Accounts>,
    stop: Option<oneshot::Sender<()>>,
    done: Option<tokio::task::JoinHandle<()>>,
}

impl TestServer {
    async fn start() -> Self {
        let registry = Arc::new(Registry::new(
            Arc::new(MemoryStore::new()),
            Arc::new(Methodologies::builtin().expect("builtin")),
            Arc::new(ServerConfig::default()),
        ));
        let accounts = Arc::new(
            Accounts::open(
                AccountBackend::Memory(MemoryAccounts::new()),
                AccountsConfig::default(),
            )
            .await
            .expect("accounts"),
        );
        accounts
            .bootstrap_admin("admin", ADMIN_PASSWORD)
            .await
            .expect("bootstrap");
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let url = format!("http://{}", listener.local_addr().expect("addr"));
        let service = AegisService::new(Arc::clone(&registry)).with_accounts(Arc::clone(&accounts));
        let public =
            AccountsService::new(Some(Arc::clone(&accounts)), LoginOptions::password("Test"));
        let auth = AuthLayer::new(Arc::new(PasswordAuthn(Arc::clone(&accounts))), "");
        let (stop, stopped) = oneshot::channel::<()>();
        let done = tokio::spawn(async move {
            server::router(service, public, auth)
                .serve_with_incoming_shutdown(TcpListenerStream::new(listener), async {
                    let _ = stopped.await;
                    registry.shutdown().await;
                })
                .await
                .expect("serve");
        });
        TestServer {
            url,
            accounts,
            stop: Some(stop),
            done: Some(done),
        }
    }

    async fn stop(mut self) {
        if let Some(s) = self.stop.take() {
            let _ = s.send(());
        }
        if let Some(d) = self.done.take() {
            let _ = d.await;
        }
    }

    async fn log_in(
        &self,
        kind: TransportKind,
        user: &str,
        password: &str,
    ) -> (Connection<PasswordLogin>, Arc<PasswordLogin>) {
        let public = Public::connect(&self.url, kind).await.expect("public");
        let session = public.log_in(user, password, "test").await.expect("login");
        let login = Arc::new(PasswordLogin::new());
        login.set(Some(session.token));
        let conn = Connection::connect_with(&self.url, kind, Arc::clone(&login))
            .await
            .expect("connect");
        (conn, login)
    }
}

fn code(e: &ClientError) -> Option<&ServerCode> {
    e.code()
}

#[tokio::test]
async fn health_probe_answers_without_credentials() {
    let server = TestServer::start().await;
    let addr = server.url.trim_start_matches("http://").to_owned();
    let stream = tokio::net::TcpStream::connect(&addr).await.expect("tcp");
    let (mut sender, connection) =
        hyper::client::conn::http1::handshake(hyper_util::rt::TokioIo::new(stream))
            .await
            .expect("handshake");
    tokio::spawn(connection);
    let request = http::Request::get(format!("{}/healthz", server.url))
        .header("host", &addr)
        .body(http_body_util::Empty::<bytes::Bytes>::new())
        .expect("request");
    let response = sender.send_request(request).await.expect("response");
    assert_eq!(response.status(), http::StatusCode::OK);
    server.stop().await;
}

#[tokio::test]
async fn login_options_and_wrong_passwords() {
    let server = TestServer::start().await;
    for kind in [TransportKind::Grpc, TransportKind::GrpcWeb] {
        let public = Public::connect(&server.url, kind).await.expect("public");
        let options = public.login_options().await.expect("options");
        assert_eq!(options.method, LoginMethod::Password);
        assert_eq!(options.server_name, "Test");
        assert!(
            options.password_enabled,
            "a password server accepts passwords"
        );
        let wrong = public
            .log_in("admin", "not the password", "test")
            .await
            .expect_err("refused");
        assert_eq!(
            code(&wrong),
            Some(&ServerCode::InvalidCredentials),
            "{kind:?}"
        );
        let unknown = public
            .log_in("nobody", ADMIN_PASSWORD, "test")
            .await
            .expect_err("refused");
        assert_eq!(code(&unknown), Some(&ServerCode::InvalidCredentials));
    }
    server.stop().await;
}

#[tokio::test]
async fn without_a_session_every_call_needs_a_login() {
    let server = TestServer::start().await;
    for kind in [TransportKind::Grpc, TransportKind::GrpcWeb] {
        let conn = Connection::connect_with(&server.url, kind, Arc::new(PasswordLogin::new()))
            .await
            .expect("connect");
        assert!(matches!(
            conn.list_projects().await,
            Err(ClientError::LoginRequired)
        ));
        // A forged token reaches the server and is refused there.
        let forged = Arc::new(PasswordLogin::new());
        forged.set(Some("ats_forged".to_owned()));
        let conn = Connection::connect_with(&server.url, kind, forged)
            .await
            .expect("connect");
        assert!(matches!(conn.info().await, Err(ClientError::LoginRequired)));
    }
    server.stop().await;
}

/// Everything a project needs, over one transport: create, open, edit lock,
/// the project status, baselines, history, export audit.
async fn project_round_trip(server: &TestServer, kind: TransportKind) {
    let (conn, _) = server.log_in(kind, "admin", ADMIN_PASSWORD).await;
    let me = conn.who_am_i().await.expect("whoami");
    assert_eq!(
        (me.subject.as_str(), me.admin, me.method.as_str()),
        ("admin", true, "password")
    );

    let info = conn.info().await.expect("info");
    let key = info.methodologies[0].key.clone();
    let project = conn
        .create_project(&format!("P {kind:?}"), &key)
        .await
        .expect("create");
    let id = project.project_id.clone();
    let downloaded = conn.open_project(&id).await.expect("open");
    assert_eq!(downloaded.last_seq, 0);

    // Anyone may ask where the project stands without taking the lock.
    let status = conn.status(&id).await.expect("status");
    assert_eq!((status.last_seq, status.lock_holder.as_str()), (0, ""));

    let lease = conn.acquire(&id).await.expect("lock");
    let renamed = conn
        .rename_project(&id, &lease.token, "Renamed")
        .await
        .expect("rename");
    assert_eq!(renamed.name, "Renamed");
    conn.release(&id, &lease.token).await.expect("release");

    let baseline = conn.create_baseline(&id, "B1").await.expect("baseline");
    assert_eq!(baseline.seq, 0);
    let list = conn.baselines(&id).await.expect("list");
    assert_eq!(list.len(), 1);
    let (seq, bytes) = conn
        .baseline(&id, &baseline.baseline_id)
        .await
        .expect("get baseline");
    assert_eq!((seq, bytes.len() as u64), (0, baseline.snapshot_bytes));
    assert!(conn.history(&id, 10).await.expect("history").is_empty());
    conn.record_export(&id, "html-report", "profile=Default lang=en")
        .await
        .expect("export");
    let events = server.accounts.recent_events(5).await.expect("events");
    assert!(
        events
            .iter()
            .any(|e| e.kind == aegis_server::accounts::AuditKind::Export)
    );

    conn.delete_project(&id).await.expect("delete");
    let gone = conn.open_project(&id).await.expect_err("deleted");
    assert_eq!(code(&gone), Some(&ServerCode::UnknownProject));
    conn.log_out().await.expect("logout");
    assert!(matches!(
        conn.list_projects().await,
        Err(ClientError::LoginRequired)
    ));
}

#[tokio::test]
async fn projects_work_over_grpc() {
    let server = TestServer::start().await;
    project_round_trip(&server, TransportKind::Grpc).await;
    server.stop().await;
}

#[tokio::test]
async fn projects_work_over_grpc_web() {
    let server = TestServer::start().await;
    project_round_trip(&server, TransportKind::GrpcWeb).await;
    server.stop().await;
}

#[tokio::test]
async fn admins_manage_users_and_users_change_their_password() {
    let server = TestServer::start().await;
    let (admin, _) = server
        .log_in(TransportKind::GrpcWeb, "admin", ADMIN_PASSWORD)
        .await;
    let created = admin
        .create_user(pb::CreateUserRequest {
            username: "Bob".into(),
            display_name: "Bob Builder".into(),
            password: "bobs long password".into(),
            admin: false,
        })
        .await
        .expect("create user");
    assert_eq!(created.username, "bob");
    let weak = admin
        .create_user(pb::CreateUserRequest {
            username: "carol".into(),
            display_name: String::new(),
            password: "short".into(),
            admin: false,
        })
        .await
        .expect_err("weak");
    assert_eq!(code(&weak), Some(&ServerCode::WeakPassword));
    let taken = admin
        .create_user(pb::CreateUserRequest {
            username: "bob".into(),
            display_name: String::new(),
            password: "another long one".into(),
            admin: false,
        })
        .await
        .expect_err("taken");
    assert_eq!(code(&taken), Some(&ServerCode::UserExists));

    let (bob, _) = server
        .log_in(TransportKind::Grpc, "bob", "bobs long password")
        .await;
    assert_eq!(
        bob.who_am_i().await.expect("me").display_name,
        "Bob Builder"
    );
    let denied = bob.users().await.expect_err("not admin");
    assert_eq!(code(&denied), Some(&ServerCode::NotAdmin));
    let wrong = bob
        .change_password("wrong current pw", "bobs new password")
        .await
        .expect_err("wrong current");
    assert_eq!(code(&wrong), Some(&ServerCode::InvalidCredentials));
    bob.change_password("bobs long password", "bobs new password")
        .await
        .expect("change");
    // The current session survives its own password change.
    bob.who_am_i().await.expect("still logged in");

    let users = admin.users().await.expect("users");
    assert_eq!(
        users
            .iter()
            .map(|u| u.username.as_str())
            .collect::<Vec<_>>(),
        ["admin", "bob"]
    );
    admin
        .update_user(pb::UpdateUserRequest {
            username: "bob".into(),
            disabled: Some(true),
            ..pb::UpdateUserRequest::default()
        })
        .await
        .expect("disable");
    assert!(matches!(
        bob.who_am_i().await,
        Err(ClientError::LoginRequired)
    ));
    let last = admin.delete_user("admin").await.expect_err("own account");
    assert!(matches!(last, ClientError::Server { .. }));
    admin.delete_user("bob").await.expect("delete bob");
    server.stop().await;
}

#[tokio::test]
async fn the_login_switch_is_read_and_refused_without_a_company_login() {
    let server = TestServer::start().await;
    let (conn, _login) = server
        .log_in(TransportKind::Grpc, "admin", ADMIN_PASSWORD)
        .await;
    let settings = conn.server_settings().await.expect("settings");
    assert!(settings.password_login_enabled);
    assert!(!settings.company_login_available);
    // This server has no company login, so the switch must stay on, and the
    // client gets a code it can word for the administrator.
    let err = conn.set_password_login(false).await.expect_err("refused");
    assert_eq!(code(&err), Some(&ServerCode::NoCompanyLogin));
    assert!(
        conn.server_settings()
            .await
            .expect("again")
            .password_login_enabled
    );
    server.stop().await;
}

#[tokio::test]
async fn an_admin_reads_the_event_log_and_a_user_is_refused() {
    let server = TestServer::start().await;
    let (admin, _l) = server
        .log_in(TransportKind::Grpc, "admin", ADMIN_PASSWORD)
        .await;
    admin
        .create_user(pb::CreateUserRequest {
            username: "bob".into(),
            display_name: String::new(),
            password: "bobs long password".into(),
            admin: false,
        })
        .await
        .expect("create");
    let (bob, _l2) = server
        .log_in(TransportKind::Grpc, "bob", "bobs long password")
        .await;
    let events = admin.events(50).await.expect("events");
    assert_eq!(events[0].kind, "login_ok");
    assert_eq!(events[0].actor, "bob");
    let refused = bob.events(50).await.expect_err("not an admin");
    assert_eq!(code(&refused), Some(&ServerCode::NotAdmin));
    server.stop().await;
}

#[tokio::test]
async fn the_session_shows_events_and_the_license_to_an_admin() {
    use aegis_client::audit::EventGroup;
    use aegis_client::commands::Command;
    use aegis_client::session::{Session, SessionConfig};
    let server = TestServer::start().await;
    let public = Public::connect(&server.url, TransportKind::Grpc)
        .await
        .expect("public");
    let s = public
        .log_in("admin", ADMIN_PASSWORD, "test")
        .await
        .expect("login");
    let login = Arc::new(PasswordLogin::new());
    login.set(Some(s.token));
    let (session, _task) = Session::start(&server.url, login, SessionConfig::new("admin"));
    let mut view = session.subscribe();
    let wait = |ok: fn(&aegis_client::state::ViewModel) -> bool| {
        let mut view = view.clone();
        async move {
            tokio::time::timeout(Duration::from_secs(10), async {
                loop {
                    if ok(&view.borrow_and_update()) {
                        return;
                    }
                    view.changed().await.expect("session alive");
                }
            })
            .await
            .expect("timed out");
        }
    };
    wait(|vm| vm.me.as_ref().is_some_and(|m| m.admin)).await;
    session.send(Command::LoadEvents).expect("sent");
    wait(|vm| !vm.events.is_empty()).await;
    {
        let vm = view.borrow_and_update();
        assert_eq!(vm.events[0].what, "Logged in");
        assert_eq!(vm.events[0].group, EventGroup::Logins);
        assert_eq!(vm.events_error, None);
    }
    // The test server has no license (development), so there is no license page.
    assert!(view.borrow().connection != aegis_client::state::ConnectionView::LoginRequired);
    server.stop().await;
}

#[tokio::test]
async fn an_admin_sees_sessions_ends_one_and_the_user_is_logged_out() {
    use aegis_client::commands::Command;
    use aegis_client::session::{Session, SessionConfig};
    use aegis_client::view::AdminAction;

    let server = TestServer::start().await;
    let (admin_conn, _) = server
        .log_in(TransportKind::Grpc, "admin", ADMIN_PASSWORD)
        .await;
    admin_conn
        .create_user(pb::CreateUserRequest {
            username: "bob".into(),
            display_name: String::new(),
            password: "bobs long password".into(),
            admin: false,
        })
        .await
        .expect("create");
    let (bob, _) = server
        .log_in(TransportKind::Grpc, "bob", "bobs long password")
        .await;
    bob.who_am_i().await.expect("bob works");

    // A normal user may not list or end sessions.
    let refused = bob.sessions().await.expect_err("refused");
    assert_eq!(code(&refused), Some(&ServerCode::NotAdmin));

    // The admin's window: the session loads the list on request.
    let public = Public::connect(&server.url, TransportKind::Grpc)
        .await
        .expect("public");
    let s = public
        .log_in("admin", ADMIN_PASSWORD, "the admin's laptop")
        .await
        .expect("login");
    let login = Arc::new(PasswordLogin::new());
    login.set(Some(s.token));
    let (session, _task) = Session::start(&server.url, login, SessionConfig::new("admin"));
    let view = session.subscribe();
    let wait = |ok: fn(&aegis_client::state::ViewModel) -> bool| {
        let mut view = view.clone();
        async move {
            tokio::time::timeout(Duration::from_secs(10), async {
                loop {
                    if ok(&view.borrow_and_update()) {
                        return;
                    }
                    view.changed().await.expect("session alive");
                }
            })
            .await
            .expect("timed out");
        }
    };
    wait(|vm| vm.me.as_ref().is_some_and(|m| m.admin)).await;
    session.send(Command::LoadSessions).expect("sent");
    wait(|vm| vm.sessions.iter().any(|r| r.user == "bob")).await;
    let bobs = view
        .borrow()
        .sessions
        .iter()
        .find(|r| r.user == "bob")
        .cloned()
        .expect("bob's row");
    assert_eq!(bobs.device, "test");
    assert!(
        view.borrow()
            .sessions
            .iter()
            .any(|r| r.device == "the admin's laptop")
    );

    // Ending it: the row goes, the notice says so, and bob is logged out.
    session
        .send(Command::Admin(AdminAction::RevokeSession {
            id: bobs.id.clone(),
            username: "bob".into(),
        }))
        .expect("sent");
    wait(|vm| !vm.sessions.iter().any(|r| r.user == "bob")).await;
    assert!(
        view.borrow()
            .notices
            .iter()
            .any(|n| n.text.contains("End the session of bob: done")),
        "{:?}",
        view.borrow().notices
    );
    let out = bob.who_am_i().await.expect_err("logged out");
    assert!(matches!(out, ClientError::LoginRequired), "{out:?}");
    server.stop().await;
}
