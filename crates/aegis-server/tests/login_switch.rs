//! The password login switch (job 045) end to end: company login and
//! password login side by side, an admin turning the password form off and
//! on, over loopback with the real authentication layer.

mod common;

use std::sync::Arc;

use aegis_auth::dpop::DpopVerifier;
use aegis_auth::http::HyperClient;
use aegis_auth::jwks::JwksCache;
use aegis_auth::system_clock;
use aegis_auth::testing::{AUDIENCE, MockIdp};
use aegis_auth::validate::{AdminRule, Authenticator, DpopPolicy, ValidatorConfig};
use aegis_server::accounts::service::{AccountsService, LoginOptions};
use aegis_server::accounts::{AccountBackend, Accounts, AccountsConfig, MemoryAccounts};
use aegis_server::auth_layer::{AuthLayer, HybridAuthn, OidcAuthn, PasswordAuthn};
use aegis_server::methodologies::Methodologies;
use aegis_server::pb::accounts_client::AccountsClient;
use aegis_server::pb::aegis_client::AegisClient;
use aegis_server::pb::get_login_options_response::Method;
use aegis_server::{AegisService, MemoryStore, Registry, pb, server};
use common::fast_config;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::transport::{Channel, Endpoint};
use tonic::{Code, Request};

const ADMIN_PASSWORD: &str = "a long enough password";

struct Env {
    idp: MockIdp,
    url: String,
    accounts: Arc<Accounts>,
    stop: oneshot::Sender<()>,
    done: tokio::task::JoinHandle<()>,
}

async fn start(company_login: bool) -> Env {
    let idp = MockIdp::start().await;
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let url = format!("http://{}", listener.local_addr().expect("addr"));
    let jwks = JwksCache::new(
        Arc::new(HyperClient::new()),
        &format!("{}/jwks", idp.issuer()),
        system_clock,
    );
    let authenticator = Arc::new(Authenticator::new(
        ValidatorConfig {
            issuer: idp.issuer().to_owned(),
            audience: AUDIENCE.to_owned(),
            // Plain bearer tokens keep the test small; DPoP has its own tests.
            dpop: DpopPolicy::Optional,
            admin: Some(AdminRule {
                claim: "roles".into(),
                value: "aegis-admin".into(),
            }),
        },
        jwks,
        DpopVerifier::standard(),
        system_clock,
    ));
    let accounts = Arc::new(
        Accounts::open(
            AccountBackend::Memory(MemoryAccounts::new()),
            AccountsConfig {
                company_login,
                ..AccountsConfig::default()
            },
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
    let service = AegisService::new(Arc::clone(&registry)).with_accounts(Arc::clone(&accounts));
    let options = LoginOptions {
        method: Method::Oidc,
        oidc_issuer: idp.issuer().to_owned(),
        oidc_client_id: "app".into(),
        oidc_resource: String::new(),
        server_name: "Test".into(),
    };
    let public = AccountsService::new(Some(Arc::clone(&accounts)), options);
    let auth = AuthLayer::new(
        Arc::new(HybridAuthn {
            password: PasswordAuthn(Arc::clone(&accounts)),
            oidc: OidcAuthn(authenticator),
        }),
        &url,
    );
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
    Env {
        idp,
        url,
        accounts,
        stop,
        done,
    }
}

impl Env {
    async fn channel(&self) -> Channel {
        Endpoint::from_shared(self.url.clone())
            .expect("uri")
            .connect()
            .await
            .expect("connect")
    }

    async fn public(&self) -> AccountsClient<Channel> {
        AccountsClient::new(self.channel().await)
    }

    async fn with_token(
        &self,
        token: String,
    ) -> AegisClient<tonic::service::interceptor::InterceptedService<Channel, Bearer>> {
        AegisClient::with_interceptor(self.channel().await, Bearer(token))
    }

    /// A client logged in with the OIDC access token of `subject`.
    async fn company(
        &self,
        subject: &str,
        roles: &[&str],
    ) -> AegisClient<tonic::service::interceptor::InterceptedService<Channel, Bearer>> {
        self.idp.user(subject, roles);
        self.with_token(self.idp.access_token(subject, roles, None, 600))
            .await
    }

    async fn password_token(&self) -> String {
        self.public()
            .await
            .log_in(pb::LogInRequest {
                username: "admin".into(),
                password: ADMIN_PASSWORD.into(),
                client_name: "test".into(),
            })
            .await
            .expect("password login")
            .into_inner()
            .session_token
    }

    async fn options(&self) -> pb::GetLoginOptionsResponse {
        self.public()
            .await
            .get_login_options(pb::GetLoginOptionsRequest {})
            .await
            .expect("options")
            .into_inner()
    }

    async fn stop(self) {
        let _ = self.stop.send(());
        self.done.await.expect("server task");
    }
}

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

fn error_code(status: &tonic::Status) -> String {
    status
        .metadata()
        .get(aegis_proto::ERROR_CODE_HEADER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_owned()
}

#[tokio::test]
async fn an_admin_turns_password_login_off_and_on() {
    let env = start(true).await;

    // Both ways in at first. The company login leads; the password form is
    // offered too.
    let options = env.options().await;
    assert_eq!(options.method(), Method::Oidc);
    assert!(options.password_enabled);

    let password_token = env.password_token().await;
    let mut as_password = env.with_token(password_token.clone()).await;
    as_password
        .who_am_i(pb::WhoAmIRequest {})
        .await
        .expect("password session works");

    // A company admin turns the password form off.
    let mut company_admin = env.company("carol", &["aegis-admin"]).await;
    let settings = company_admin
        .get_server_settings(pb::GetServerSettingsRequest {})
        .await
        .expect("settings")
        .into_inner();
    assert!(settings.password_login_enabled && settings.company_login_available);
    let after = company_admin
        .set_password_login(pb::SetPasswordLoginRequest { enabled: false })
        .await
        .expect("off")
        .into_inner();
    assert!(!after.password_login_enabled);

    // The app is now told to show only the company login.
    assert!(!env.options().await.password_enabled);
    // Logging in with a password is refused with a code the app understands.
    let err = env
        .public()
        .await
        .log_in(pb::LogInRequest {
            username: "admin".into(),
            password: ADMIN_PASSWORD.into(),
            client_name: "test".into(),
        })
        .await
        .expect_err("refused");
    assert_eq!(err.code(), Code::FailedPrecondition);
    assert_eq!(error_code(&err), "password_login_off");
    // The session that was open before ended at once.
    let err = as_password
        .who_am_i(pb::WhoAmIRequest {})
        .await
        .expect_err("session ended");
    assert_eq!(err.code(), Code::Unauthenticated);
    // The company login is unaffected.
    company_admin
        .who_am_i(pb::WhoAmIRequest {})
        .await
        .expect("company login still works");

    // It was written to the event log.
    let events = env.accounts.recent_events(10).await.expect("events");
    assert!(
        events
            .iter()
            .any(|e| e.kind.as_str() == "password_login_changed"
                && e.actor == "carol"
                && e.detail == "off"),
        "{events:?}"
    );

    // And back on.
    company_admin
        .set_password_login(pb::SetPasswordLoginRequest { enabled: true })
        .await
        .expect("on");
    assert!(env.options().await.password_enabled);
    env.password_token().await;
    env.stop().await;
}

#[tokio::test]
async fn only_an_admin_may_change_the_switch() {
    let env = start(true).await;
    let mut user = env.company("dave", &[]).await;
    // Anyone logged in may read it.
    user.get_server_settings(pb::GetServerSettingsRequest {})
        .await
        .expect("read");
    let err = user
        .set_password_login(pb::SetPasswordLoginRequest { enabled: false })
        .await
        .expect_err("refused");
    assert_eq!(err.code(), Code::PermissionDenied);
    assert!(env.options().await.password_enabled);

    // No login at all: refused before the service runs.
    let mut anonymous = AegisClient::new(env.channel().await);
    let err = anonymous
        .get_server_settings(pb::GetServerSettingsRequest {})
        .await
        .expect_err("refused");
    assert_eq!(err.code(), Code::Unauthenticated);
    env.stop().await;
}

#[tokio::test]
async fn without_a_company_login_the_switch_cannot_lock_everyone_out() {
    let env = start(false).await;
    let mut admin = env.with_token(env.password_token().await).await;
    let err = admin
        .set_password_login(pb::SetPasswordLoginRequest { enabled: false })
        .await
        .expect_err("refused");
    assert_eq!(err.code(), Code::FailedPrecondition);
    assert_eq!(error_code(&err), "no_company_login");
    // Nothing changed, and the admin is still logged in.
    let settings = admin
        .get_server_settings(pb::GetServerSettingsRequest {})
        .await
        .expect("still logged in")
        .into_inner();
    assert!(settings.password_login_enabled && !settings.company_login_available);
    env.stop().await;
}
