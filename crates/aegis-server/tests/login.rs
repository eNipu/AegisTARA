//! Login end to end: mock identity provider, real client session and signing
//! layer, real server authentication layer, over loopback.

mod common;

use std::sync::Arc;
use std::time::Duration;

use aegis_auth::dpop::DpopVerifier;
use aegis_auth::http::HyperClient;
use aegis_auth::jwks::JwksCache;
use aegis_auth::layer::SignRequests;
use aegis_auth::oidc::{OidcClient, OidcConfig};
use aegis_auth::session::Session;
use aegis_auth::store::{MemorySecretStore, Vault};
use aegis_auth::system_clock;
use aegis_auth::testing::{AUDIENCE, CLIENT_ID, MockIdp};
use aegis_auth::validate::{AdminRule, Authenticator, DpopPolicy, ValidatorConfig};
use aegis_server::auth_layer::{AuthLayer, OidcAuthn};
use aegis_server::methodologies::Methodologies;
use aegis_server::pb::aegis_client::AegisClient;
use aegis_server::{AegisService, MemoryStore, Registry, pb, server};
use common::{METHODOLOGY, fast_config};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::Code;
use tonic::transport::Endpoint;

type Sess = Session<HyperClient, MemorySecretStore>;

struct Env {
    idp: MockIdp,
    base_url: String,
    registry: Arc<Registry<MemoryStore>>,
    stop: oneshot::Sender<()>,
    done: tokio::task::JoinHandle<()>,
}

async fn start(policy: DpopPolicy) -> Env {
    let idp = MockIdp::start().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let base_url = format!("http://{}", listener.local_addr().expect("addr"));

    let jwks = JwksCache::new(
        Arc::new(HyperClient::new()),
        &format!("{}/jwks", idp.issuer()),
        system_clock,
    );
    let authenticator = Arc::new(Authenticator::new(
        ValidatorConfig {
            issuer: idp.issuer().to_owned(),
            audience: AUDIENCE.to_owned(),
            dpop: policy,
            admin: Some(AdminRule {
                claim: "roles".into(),
                value: "aegis-admin".into(),
            }),
        },
        jwks,
        DpopVerifier::standard(),
        system_clock,
    ));
    let registry = Arc::new(Registry::new(
        Arc::new(MemoryStore::new()),
        Arc::new(Methodologies::builtin().expect("builtin")),
        Arc::new(fast_config()),
    ));
    let (stop, stopped) = oneshot::channel::<()>();
    let service = AegisService::new(Arc::clone(&registry)).into_server();
    let reg = Arc::clone(&registry);
    let layer = AuthLayer::new(Arc::new(OidcAuthn(authenticator)), &base_url);
    let done = tokio::spawn(async move {
        server::builder()
            .layer(layer)
            .add_service(service)
            .serve_with_incoming_shutdown(TcpListenerStream::new(listener), async {
                let _ = stopped.await;
                reg.shutdown().await;
            })
            .await
            .expect("serve");
    });
    Env {
        idp,
        base_url,
        registry,
        stop,
        done,
    }
}

impl Env {
    async fn session(&self) -> Arc<Sess> {
        let oidc = OidcClient::connect(
            Arc::new(HyperClient::new()),
            OidcConfig::new(self.idp.issuer(), CLIENT_ID),
            system_clock,
        )
        .await
        .expect("discovery");
        let vault = Vault::new(MemorySecretStore::new(), "test");
        let session = Arc::new(Session::new(oidc, vault, system_clock).expect("session"));
        session
            .login(&self.idp.browser(), Duration::from_secs(10))
            .await
            .expect("login");
        session
    }

    async fn channel(&self) -> tonic::transport::Channel {
        Endpoint::from_shared(self.base_url.clone())
            .expect("uri")
            .connect()
            .await
            .expect("connect")
    }

    async fn client(
        &self,
        session: Arc<Sess>,
    ) -> AegisClient<aegis_auth::layer::Signed<Sess, tonic::transport::Channel>> {
        use tower::Layer;
        AegisClient::new(SignRequests::new(session, &self.base_url).layer(self.channel().await))
    }

    async fn stop(self) {
        let _ = self.stop.send(());
        self.done.await.expect("server task");
    }
}

#[tokio::test]
async fn a_logged_in_user_edits_a_project_as_themselves() {
    let env = start(DpopPolicy::Required).await;
    env.idp.user("alice", &[]);
    let mut client = env.client(env.session().await).await;

    let project = client
        .create_project(pb::CreateProjectRequest {
            name: "Brake".into(),
            methodology_key: METHODOLOGY.into(),
        })
        .await
        .expect("create")
        .into_inner()
        .project
        .expect("summary");
    let lease = client
        .acquire_lock(pb::LockRequest {
            project_id: project.project_id.clone(),
        })
        .await
        .expect("lock")
        .into_inner();
    // The lease holder is the identity from the access token, not a header
    // the client chose.
    assert_eq!(lease.holder, "alice");

    // A streaming call works too (the proof covers the opening request).
    let mut open = client
        .open_project(pb::OpenProjectRequest {
            project_id: project.project_id.clone(),
        })
        .await
        .expect("open")
        .into_inner();
    let first = open.message().await.expect("ok").expect("chunk");
    assert_eq!(first.header.expect("header").lock_holder, "alice");
    drop(open);
    let status = client
        .get_project_status(pb::GetProjectStatusRequest {
            project_id: project.project_id,
        })
        .await
        .expect("status")
        .into_inner();
    assert_eq!(status.lock_holder, "alice");
    env.stop().await;
}

#[tokio::test]
async fn calls_without_a_valid_login_are_refused_with_one_generic_message() {
    let env = start(DpopPolicy::Required).await;

    // No credentials at all.
    let mut anonymous = AegisClient::new(env.channel().await);
    let err = anonymous
        .list_projects(pb::ListProjectsRequest {})
        .await
        .expect_err("refused");
    assert_eq!(err.code(), Code::Unauthenticated);
    assert_eq!(err.message(), "authentication failed");

    // A real token used as a plain bearer token, without the device key.
    let token = env.idp.access_token("mallory", &[], None, 600);
    let mut bearer =
        AegisClient::with_interceptor(env.channel().await, move |mut r: tonic::Request<()>| {
            r.metadata_mut().insert(
                "authorization",
                format!("Bearer {token}").parse().expect("ascii"),
            );
            Ok(r)
        });
    let err = bearer
        .list_projects(pb::ListProjectsRequest {})
        .await
        .expect_err("refused: required policy");
    assert_eq!(err.code(), Code::Unauthenticated);
    assert_eq!(
        err.message(),
        "authentication failed",
        "no hint about the reason"
    );

    // Garbage.
    let mut junk =
        AegisClient::with_interceptor(env.channel().await, |mut r: tonic::Request<()>| {
            r.metadata_mut()
                .insert("authorization", "DPoP not.a.jwt".parse().expect("ascii"));
            Ok(r)
        });
    assert_eq!(
        junk.list_projects(pb::ListProjectsRequest {})
            .await
            .expect_err("refused")
            .code(),
        Code::Unauthenticated
    );
    env.stop().await;
}

#[tokio::test]
async fn the_admin_role_comes_from_the_token() {
    let env = start(DpopPolicy::Required).await;
    env.idp.user("carol", &["aegis-admin"]);
    let mut admin = env.client(env.session().await).await;
    env.idp.user("dave", &[]);
    let mut user = env.client(env.session().await).await;

    let id = admin
        .create_project(pb::CreateProjectRequest {
            name: "p".into(),
            methodology_key: METHODOLOGY.into(),
        })
        .await
        .expect("create")
        .into_inner()
        .project
        .expect("summary")
        .project_id;
    admin
        .acquire_lock(pb::LockRequest {
            project_id: id.clone(),
        })
        .await
        .expect("lock");
    let request = || pb::LockRequest {
        project_id: id.clone(),
    };
    assert_eq!(
        user.break_lock(request())
            .await
            .expect_err("not admin")
            .code(),
        Code::PermissionDenied
    );
    admin
        .break_lock(request())
        .await
        .expect("admin breaks the lock");
    assert_eq!(env.registry.loaded_count(), 1);
    env.stop().await;
}

#[tokio::test]
async fn a_provider_without_dpop_works_only_under_the_optional_policy() {
    // Some identity providers cannot bind tokens to a key.
    for (policy, works) in [(DpopPolicy::Required, false), (DpopPolicy::Optional, true)] {
        let env = start(policy).await;
        env.idp.bind_tokens(false);
        let oidc = OidcClient::connect(
            Arc::new(HyperClient::new()),
            OidcConfig::new(env.idp.issuer(), CLIENT_ID),
            system_clock,
        )
        .await
        .expect("discovery");
        // The mock refuses a token request without a proof when binding is
        // on; with binding off it still receives one and answers "Bearer".
        let session = Arc::new(
            Session::new(
                oidc,
                Vault::new(MemorySecretStore::new(), "t"),
                system_clock,
            )
            .expect("session"),
        );
        session
            .login(&env.idp.browser(), Duration::from_secs(10))
            .await
            .expect("login");
        let mut client = env.client(session).await;
        let result = client.list_projects(pb::ListProjectsRequest {}).await;
        assert_eq!(result.is_ok(), works, "{policy:?}");
        env.stop().await;
    }
}
