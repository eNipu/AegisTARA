//! Cancelled OIDC work cannot commit tokens or clear a newer saved login.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use aegis_auth::http::{HttpClient, HttpRequest, HttpResponse, HyperClient};
use aegis_auth::oidc::{OidcClient, OidcConfig};
use aegis_auth::session::{BrowserLauncher, LoginAttempt, Session};
use aegis_auth::store::{MemorySecretStore, SecretStore, Vault};
use aegis_auth::testing::{CLIENT_ID, MockIdp};
use aegis_auth::{AuthError, system_clock};
use tokio::sync::Notify;
use tokio::task::JoinHandle;

const TIMEOUT: Duration = Duration::from_secs(10);
const ACCOUNT: &str = "alice@aegis";
const API: &str = "https://api.example.test/rpc";

#[derive(Default)]
struct ObservedStore {
    memory: MemorySecretStore,
    mutations: AtomicUsize,
}

impl SecretStore for ObservedStore {
    fn get(&self, name: &str) -> Result<Option<Vec<u8>>, AuthError> {
        self.memory.get(name)
    }

    fn set(&self, name: &str, value: &[u8]) -> Result<(), AuthError> {
        self.mutations.fetch_add(1, Ordering::SeqCst);
        self.memory.set(name, value)
    }

    fn delete(&self, name: &str) -> Result<(), AuthError> {
        self.mutations.fetch_add(1, Ordering::SeqCst);
        self.memory.delete(name)
    }
}

#[derive(Default)]
struct ResponseGate {
    reached: Notify,
    release: Notify,
}

impl ResponseGate {
    async fn wait(&self) {
        tokio::time::timeout(TIMEOUT, self.reached.notified())
            .await
            .expect("token response reached the gate");
    }
}

struct GatedHttp {
    inner: HyperClient,
    next_token_response: Mutex<Option<Arc<ResponseGate>>>,
}

impl GatedHttp {
    fn hold_next_token_response(&self) -> Arc<ResponseGate> {
        let gate = Arc::new(ResponseGate::default());
        assert!(
            self.next_token_response
                .lock()
                .unwrap()
                .replace(gate.clone())
                .is_none()
        );
        gate
    }
}

impl HttpClient for GatedHttp {
    async fn send(&self, request: HttpRequest) -> Result<HttpResponse, AuthError> {
        let gate = if request.method == "POST" && request.url.ends_with("/token") {
            self.next_token_response.lock().unwrap().take()
        } else {
            None
        };
        let response = self.inner.send(request).await?;
        if let Some(gate) = gate {
            gate.reached.notify_one();
            tokio::time::timeout(TIMEOUT, gate.release.notified())
                .await
                .expect("test released the token response");
        }
        Ok(response)
    }
}

type TestSession = Session<GatedHttp, Arc<ObservedStore>>;

struct Harness {
    idp: MockIdp,
    http: Arc<GatedHttp>,
    store: Arc<ObservedStore>,
    session: Arc<TestSession>,
}

impl Harness {
    async fn new() -> Self {
        let idp = MockIdp::start().await;
        let http = Arc::new(GatedHttp {
            inner: HyperClient::new(),
            next_token_response: Mutex::new(None),
        });
        let store = Arc::new(ObservedStore::default());
        let oidc = OidcClient::connect(
            http.clone(),
            OidcConfig::new(idp.issuer(), CLIENT_ID),
            system_clock,
        )
        .await
        .expect("discovery");
        let session = Arc::new(
            Session::new(oidc, Vault::new(store.clone(), ACCOUNT), system_clock).expect("session"),
        );
        Self {
            idp,
            http,
            store,
            session,
        }
    }

    fn saved(&self) -> Option<String> {
        Vault::new(self.store.clone(), ACCOUNT)
            .load_refresh_token()
            .unwrap()
    }

    fn mutations(&self) -> usize {
        self.store.mutations.load(Ordering::SeqCst)
    }

    async fn login(&self) {
        self.session
            .login(&self.idp.browser(), TIMEOUT)
            .await
            .unwrap();
    }

    async fn authorization(&self) -> String {
        self.session
            .credentials("POST", API)
            .await
            .unwrap()
            .authorization
    }

    fn start_login(&self, attempt: &LoginAttempt) -> JoinHandle<Result<(), AuthError>> {
        let session = self.session.clone();
        let browser = self.idp.browser();
        let attempt = attempt.clone();
        tokio::spawn(async move { session.login_guarded(&browser, TIMEOUT, &attempt).await })
    }

    fn start_restore(&self, attempt: &LoginAttempt) -> JoinHandle<Result<bool, AuthError>> {
        let session = self.session.clone();
        let attempt = attempt.clone();
        tokio::spawn(async move { session.restore_guarded(&attempt).await })
    }

    fn start_credentials(&self, attempt: &LoginAttempt) -> JoinHandle<Result<String, AuthError>> {
        let session = self.session.clone();
        let attempt = attempt.clone();
        tokio::spawn(async move {
            session
                .credentials_guarded("POST", API, &attempt)
                .await
                .map(|credentials| credentials.authorization)
        })
    }
}

async fn joined<T>(task: JoinHandle<T>) -> T {
    tokio::time::timeout(TIMEOUT, task)
        .await
        .expect("task finished")
        .expect("task did not panic")
}

#[tokio::test]
async fn already_cancelled_attempt_starts_no_browser_or_token_request() {
    struct NeverOpen;
    impl BrowserLauncher for NeverOpen {
        fn open(&self, _: &str) -> Result<(), AuthError> {
            panic!("cancelled login must not open a browser");
        }
    }
    let h = Harness::new().await;
    Vault::new(h.store.clone(), ACCOUNT)
        .save_refresh_token("existing")
        .unwrap();
    let attempt = LoginAttempt::new();
    let clone = attempt.clone();
    assert!(clone.is_active());
    attempt.cancel();
    attempt.cancel();
    assert!(!clone.is_active());
    let mutations = h.mutations();
    assert_eq!(
        h.session.login_guarded(&NeverOpen, TIMEOUT, &clone).await,
        Err(AuthError::LoginRequired)
    );
    assert_eq!(
        h.session.restore_guarded(&clone).await,
        Err(AuthError::LoginRequired)
    );
    assert_eq!(h.idp.token_requests(), 0);
    assert_eq!(h.mutations(), mutations);
    assert_eq!(h.saved().as_deref(), Some("existing"));
    assert!(!h.session.is_logged_in().await);
}

#[tokio::test]
async fn cancelled_initialization_does_not_create_a_device_key() {
    let h = Harness::new().await;
    let oidc = OidcClient::connect(
        h.http.clone(),
        OidcConfig::new(h.idp.issuer(), CLIENT_ID),
        system_clock,
    )
    .await
    .unwrap();
    let attempt = LoginAttempt::new();
    attempt.cancel();
    let mutations = h.mutations();
    let result = Session::new_guarded(
        oidc,
        Vault::new(h.store.clone(), "cancelled"),
        system_clock,
        &attempt,
    );
    assert_eq!(result.err(), Some(AuthError::LoginRequired));
    assert_eq!(h.mutations(), mutations);
    assert!(h.store.get("cancelled/device-key").unwrap().is_none());
}

#[tokio::test]
async fn cancelled_login_cannot_overwrite_a_newer_login() {
    let h = Harness::new().await;
    let gate = h.http.hold_next_token_response();
    let attempt = LoginAttempt::new();
    let stale = h.start_login(&attempt);
    gate.wait().await;
    attempt.cancel();
    assert!(h.saved().is_none());
    assert!(!h.session.is_logged_in().await);

    h.login().await;
    let saved = h.saved();
    let authorization = h.authorization().await;
    let mutations = h.mutations();
    gate.release.notify_one();
    assert_eq!(joined(stale).await, Err(AuthError::LoginRequired));
    assert_eq!(h.saved(), saved);
    assert_eq!(h.authorization().await, authorization);
    assert_eq!(h.mutations(), mutations);
}

#[tokio::test]
async fn cancelled_restore_cannot_rotate_the_saved_token_or_replace_a_newer_login() {
    let h = Harness::new().await;
    h.login().await;
    let original = h.saved();
    let gate = h.http.hold_next_token_response();
    let attempt = LoginAttempt::new();
    let stale = h.start_restore(&attempt);
    gate.wait().await;
    attempt.cancel();
    assert_eq!(h.saved(), original);

    h.login().await;
    let saved = h.saved();
    assert_ne!(saved, original);
    let authorization = h.authorization().await;
    let mutations = h.mutations();
    gate.release.notify_one();
    assert_eq!(joined(stale).await, Err(AuthError::LoginRequired));
    assert_eq!(h.saved(), saved);
    assert_eq!(h.authorization().await, authorization);
    assert_eq!(h.mutations(), mutations);
}

#[tokio::test]
async fn cancelled_invalid_grant_cannot_clear_a_newer_login() {
    let h = Harness::new().await;
    h.login().await;
    h.idp.revoke_refresh_tokens();
    let gate = h.http.hold_next_token_response();
    let attempt = LoginAttempt::new();
    let stale = h.start_restore(&attempt);
    gate.wait().await;
    attempt.cancel();

    h.login().await;
    let saved = h.saved();
    let authorization = h.authorization().await;
    let mutations = h.mutations();
    gate.release.notify_one();
    assert_eq!(joined(stale).await, Err(AuthError::LoginRequired));
    assert_eq!(h.saved(), saved);
    assert_eq!(h.authorization().await, authorization);
    assert_eq!(h.mutations(), mutations);
}

#[tokio::test]
async fn cancelling_pending_login_then_awaiting_logout_prevents_resurrection() {
    let h = Harness::new().await;
    h.login().await;
    let gate = h.http.hold_next_token_response();
    let attempt = LoginAttempt::new();
    let stale = h.start_login(&attempt);
    gate.wait().await;
    attempt.cancel();
    h.session.logout().await.unwrap();
    let mutations = h.mutations();
    gate.release.notify_one();
    assert_eq!(joined(stale).await, Err(AuthError::LoginRequired));
    assert!(!h.session.is_logged_in().await);
    assert!(h.saved().is_none());
    assert_eq!(h.mutations(), mutations);

    h.login().await;
    assert!(h.session.is_logged_in().await);
    assert!(h.saved().is_some());
}

#[tokio::test]
async fn cancelling_pending_restore_then_awaiting_logout_prevents_resurrection() {
    let h = Harness::new().await;
    h.login().await;
    let gate = h.http.hold_next_token_response();
    let attempt = LoginAttempt::new();
    let stale = h.start_restore(&attempt);
    gate.wait().await;
    attempt.cancel();
    h.session.logout().await.unwrap();
    let mutations = h.mutations();
    gate.release.notify_one();
    assert_eq!(joined(stale).await, Err(AuthError::LoginRequired));
    assert!(!h.session.is_logged_in().await);
    assert!(h.saved().is_none());
    assert_eq!(h.mutations(), mutations);
}

#[tokio::test]
async fn cancelled_automatic_refresh_cannot_replace_a_newer_saved_token() {
    let h = Harness::new().await;
    h.idp.access_ttl(0);
    h.login().await;
    let gate = h.http.hold_next_token_response();
    let attempt = LoginAttempt::new();
    let stale = h.start_credentials(&attempt);
    gate.wait().await;
    attempt.cancel();
    Vault::new(h.store.clone(), ACCOUNT)
        .save_refresh_token("newer-login")
        .unwrap();
    let mutations = h.mutations();
    gate.release.notify_one();
    assert_eq!(joined(stale).await, Err(AuthError::LoginRequired));
    assert_eq!(h.saved().as_deref(), Some("newer-login"));
    assert_eq!(h.mutations(), mutations);
    assert_eq!(
        h.session.credentials_guarded("POST", API, &attempt).await,
        Err(AuthError::LoginRequired)
    );
}

#[tokio::test]
async fn cancelled_automatic_invalid_grant_cannot_clear_a_newer_saved_token() {
    let h = Harness::new().await;
    h.idp.access_ttl(0);
    h.login().await;
    h.idp.revoke_refresh_tokens();
    let gate = h.http.hold_next_token_response();
    let attempt = LoginAttempt::new();
    let stale = h.start_credentials(&attempt);
    gate.wait().await;
    attempt.cancel();
    Vault::new(h.store.clone(), ACCOUNT)
        .save_refresh_token("newer-login")
        .unwrap();
    let mutations = h.mutations();
    gate.release.notify_one();
    assert_eq!(joined(stale).await, Err(AuthError::LoginRequired));
    assert_eq!(h.saved().as_deref(), Some("newer-login"));
    assert_eq!(h.mutations(), mutations);
}

#[tokio::test]
async fn active_automatic_refresh_still_rotates_the_saved_token() {
    let h = Harness::new().await;
    h.idp.access_ttl(0);
    h.login().await;
    let original = h.saved();
    let attempt = LoginAttempt::new();
    h.start_credentials(&attempt).await.unwrap().unwrap();
    assert_ne!(h.saved(), original);
}

#[tokio::test]
async fn active_attempts_commit_and_invalid_grant_still_clears_the_saved_token() {
    let h = Harness::new().await;
    let attempt = LoginAttempt::default();
    assert!(attempt.is_active());
    assert!(!h.session.restore_guarded(&attempt).await.unwrap());
    h.session
        .login_guarded(&h.idp.browser(), TIMEOUT, &attempt)
        .await
        .unwrap();
    let original = h.saved();
    assert!(original.is_some());
    assert!(h.session.restore_guarded(&attempt).await.unwrap());
    assert_ne!(h.saved(), original);
    h.idp.revoke_refresh_tokens();
    assert!(!h.session.restore_guarded(&attempt).await.unwrap());
    assert!(h.saved().is_none());
}
