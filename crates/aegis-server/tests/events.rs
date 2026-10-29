//! The event view (job 049): an admin reads the server's event log (logins,
//! exports, admin actions) through `ListEvents`. Password login server, real
//! authentication layer, over loopback.

mod common;

use std::sync::Arc;

use aegis_server::accounts::service::{AccountsService, LoginOptions};
use aegis_server::accounts::{AccountBackend, Accounts, AccountsConfig, EventLog, MemoryAccounts};
use aegis_server::auth_layer::{AuthLayer, PasswordAuthn};
use aegis_server::methodologies::Methodologies;
use aegis_server::pb::accounts_client::AccountsClient;
use aegis_server::pb::aegis_client::AegisClient;
use aegis_server::{AegisService, MemoryStore, Registry, pb, server};
use common::fast_config;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::service::interceptor::InterceptedService;
use tonic::transport::{Channel, Endpoint};
use tonic::{Code, Request};

const ADMIN_PASSWORD: &str = "a long enough password";

#[derive(Clone)]
struct Bearer(String);

impl tonic::service::Interceptor for Bearer {
    fn call(&mut self, mut r: Request<()>) -> Result<Request<()>, tonic::Status> {
        r.metadata_mut().insert(
            "authorization",
            format!("Bearer {}", self.0).parse().expect("ascii"),
        );
        Ok(r)
    }
}

type Client = AegisClient<InterceptedService<Channel, Bearer>>;

struct Env {
    url: String,
    stop: oneshot::Sender<()>,
    done: tokio::task::JoinHandle<()>,
}

async fn start() -> Env {
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
    let registry = Arc::new(Registry::new(
        Arc::new(MemoryStore::new()),
        Arc::new(Methodologies::builtin().expect("builtin")),
        Arc::new(fast_config()),
    ));
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let url = format!("http://{}", listener.local_addr().expect("addr"));
    let service = AegisService::new(Arc::clone(&registry)).with_accounts(Arc::clone(&accounts));
    let public = AccountsService::new(Some(Arc::clone(&accounts)), LoginOptions::password("T"));
    let auth = AuthLayer::new(Arc::new(PasswordAuthn(accounts)), "");
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
    Env { url, stop, done }
}

impl Env {
    async fn channel(&self) -> Channel {
        Endpoint::from_shared(self.url.clone())
            .expect("uri")
            .connect()
            .await
            .expect("connect")
    }

    async fn log_in(&self, user: &str, password: &str) -> Result<Client, tonic::Status> {
        let token = AccountsClient::new(self.channel().await)
            .log_in(pb::LogInRequest {
                username: user.into(),
                password: password.into(),
                client_name: "test".into(),
            })
            .await?
            .into_inner()
            .session_token;
        Ok(AegisClient::with_interceptor(
            self.channel().await,
            Bearer(token),
        ))
    }

    async fn stop(self) {
        let _ = self.stop.send(());
        self.done.await.expect("server task");
    }
}

fn kinds(events: &[pb::EventInfo]) -> Vec<&str> {
    events.iter().map(|e| e.kind.as_str()).collect()
}

#[tokio::test]
async fn an_admin_reads_logins_exports_and_admin_actions_newest_first() {
    let env = start().await;
    let mut admin = env.log_in("admin", ADMIN_PASSWORD).await.expect("admin");
    admin
        .create_user(pb::CreateUserRequest {
            username: "bob".into(),
            display_name: "Bob".into(),
            password: "bobs long password".into(),
            admin: false,
        })
        .await
        .expect("create");
    // A failed login and a good one by bob.
    assert!(env.log_in("bob", "wrong wrong wrong").await.is_err());
    let mut bob = env.log_in("bob", "bobs long password").await.expect("bob");
    bob.who_am_i(pb::WhoAmIRequest {}).await.expect("who");

    let events = admin
        .list_events(pb::ListEventsRequest { limit: 0 })
        .await
        .expect("events")
        .into_inner()
        .events;
    // Newest first: bob's login, bob's failed login, the user creation, the
    // admin's own login, and the creation of the first admin at start.
    assert_eq!(
        kinds(&events),
        [
            "login_ok",
            "login_failed",
            "user_created",
            "login_ok",
            "user_created"
        ]
    );
    assert_eq!(events[4].actor, "bootstrap");
    assert_eq!(events[0].actor, "bob");
    assert_eq!(events[2].actor, "admin");
    assert_eq!(events[2].target, "bob");
    assert!(events[0].at_ms >= events[3].at_ms && events[3].at_ms > 0);
    assert_eq!(events[1].actor, "bob");

    // A limit cuts the list, and the cap is enforced by the server.
    let one = admin
        .list_events(pb::ListEventsRequest { limit: 1 })
        .await
        .expect("one")
        .into_inner()
        .events;
    assert_eq!(kinds(&one), ["login_ok"]);
    admin
        .list_events(pb::ListEventsRequest { limit: u32::MAX })
        .await
        .expect("a huge limit is capped, not refused");

    // Nothing in the log holds a password or a token.
    let text = format!("{events:?}");
    assert!(
        !text.contains("long password") && !text.contains("ats_"),
        "{text}"
    );
    env.stop().await;
}

#[tokio::test]
async fn only_an_admin_reads_the_event_log() {
    let env = start().await;
    let mut admin = env.log_in("admin", ADMIN_PASSWORD).await.expect("admin");
    admin
        .create_user(pb::CreateUserRequest {
            username: "bob".into(),
            display_name: String::new(),
            password: "bobs long password".into(),
            admin: false,
        })
        .await
        .expect("create");
    let mut bob = env.log_in("bob", "bobs long password").await.expect("bob");
    let err = bob
        .list_events(pb::ListEventsRequest { limit: 10 })
        .await
        .expect_err("refused");
    assert_eq!(err.code(), Code::PermissionDenied);
    env.stop().await;
}

#[tokio::test]
async fn a_server_without_an_event_store_says_so() {
    // Development login: no accounts, so no event store.
    let server = common::TestServer::start(fast_config()).await;
    let mut admin = server.admin("root").await;
    let err = admin
        .list_events(pb::ListEventsRequest { limit: 10 })
        .await
        .expect_err("no event log");
    assert_eq!(err.code(), Code::FailedPrecondition);
    assert_eq!(
        err.metadata()
            .get(aegis_proto::ERROR_CODE_HEADER)
            .and_then(|v| v.to_str().ok()),
        Some("no_event_log")
    );
    server.stop().await;
}

#[tokio::test]
async fn a_server_without_password_accounts_can_keep_an_event_log() {
    // Company login only has no user table; the log stands on its own.
    let log = EventLog::standalone(AccountBackend::Memory(MemoryAccounts::new()));
    let server = common::TestServer::start_with_event_log(fast_config(), log).await;
    let mut admin = server.admin("root@example.com").await;
    let project = uuid::Uuid::new_v4().to_string();
    admin
        .record_export(pb::RecordExportRequest {
            project_id: project.clone(),
            kind: "report-pdf".into(),
            detail: "profile=Default lang=en".into(),
        })
        .await
        .expect("export");
    let events = admin
        .list_events(pb::ListEventsRequest { limit: 0 })
        .await
        .expect("events")
        .into_inner()
        .events;
    assert_eq!(kinds(&events), ["export"]);
    assert_eq!(events[0].actor, "root@example.com");
    assert_eq!(events[0].target, project);
    assert_eq!(events[0].detail, "report-pdf (profile=Default lang=en)");

    // Still admin only.
    let mut bob = server.client("bob@example.com").await;
    let err = bob
        .list_events(pb::ListEventsRequest { limit: 10 })
        .await
        .expect_err("refused");
    assert_eq!(err.code(), Code::PermissionDenied);
    server.stop().await;
}

#[tokio::test]
async fn an_admin_lists_and_ends_sessions() {
    let env = start().await;
    let mut admin = env.log_in("admin", ADMIN_PASSWORD).await.expect("admin");
    admin
        .create_user(pb::CreateUserRequest {
            username: "bob".into(),
            display_name: String::new(),
            password: "bobs long password".into(),
            admin: false,
        })
        .await
        .expect("create");
    let mut bob = env.log_in("bob", "bobs long password").await.expect("bob");
    bob.who_am_i(pb::WhoAmIRequest {}).await.expect("bob works");

    let list = admin
        .list_sessions(pb::ListSessionsRequest {})
        .await
        .expect("list")
        .into_inner()
        .sessions;
    assert_eq!(list.len(), 2, "admin and bob");
    let bobs = list.iter().find(|s| s.username == "bob").expect("bob");
    assert_eq!(bobs.client_name, "test");
    assert_eq!(bobs.id.len(), 16);
    assert!(bobs.expires_ms > bobs.created_ms);
    // Nothing in the list can be used as a token.
    assert!(!format!("{list:?}").contains("ats_"));

    // A user who is not an admin sees and ends nothing.
    let err = bob
        .list_sessions(pb::ListSessionsRequest {})
        .await
        .expect_err("refused");
    assert_eq!(err.code(), Code::PermissionDenied);
    let err = bob
        .revoke_session(pb::RevokeSessionRequest {
            id: bobs.id.clone(),
        })
        .await
        .expect_err("refused");
    assert_eq!(err.code(), Code::PermissionDenied);
    bob.who_am_i(pb::WhoAmIRequest {}).await.expect("still in");

    // The admin ends bob's session: bob's next call fails, the log says so.
    admin
        .revoke_session(pb::RevokeSessionRequest {
            id: bobs.id.clone(),
        })
        .await
        .expect("revoked");
    let err = bob.who_am_i(pb::WhoAmIRequest {}).await.expect_err("ended");
    assert_eq!(err.code(), Code::Unauthenticated);
    let err = admin
        .revoke_session(pb::RevokeSessionRequest {
            id: bobs.id.clone(),
        })
        .await
        .expect_err("already ended");
    assert_eq!(err.code(), Code::NotFound);
    let events = admin
        .list_events(pb::ListEventsRequest { limit: 1 })
        .await
        .expect("events")
        .into_inner()
        .events;
    assert_eq!(events[0].kind, "session_revoked");
    assert_eq!(events[0].actor, "admin");
    assert_eq!(events[0].target, "bob");
    // Bob can log in again: ending a session is not a ban.
    env.log_in("bob", "bobs long password")
        .await
        .expect("again");
    env.stop().await;
}
