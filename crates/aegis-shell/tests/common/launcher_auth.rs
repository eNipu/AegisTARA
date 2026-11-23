//! Private completion injection tests, included by `launcher.rs`. Holding or
//! queueing a reply avoids real servers, browsers, keychains and timing races.

use super::*;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use aegis_auth::layer::CredentialProvider;
use aegis_auth::store::{MemorySecretStore, SecretStore};
use aegis_auth::{AuthError, Credentials};

use crate::headless::Headless;

const A: &str = "https://a.example.test";
const B: &str = "https://b.example.test";

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

/// Observes the real session's signing path but refuses transport, so no
/// credentials or requests leave the test process.
struct ObservedProvider {
    inner: Arc<AppProvider>,
    requests: AtomicUsize,
    signed: Mutex<Vec<(String, String)>>,
}

impl CredentialProvider for ObservedProvider {
    async fn credentials(&self, method: &str, url: &str) -> Result<Credentials, AuthError> {
        self.requests.fetch_add(1, Ordering::SeqCst);
        if let Ok(credentials) = self.inner.credentials(method, url).await {
            self.signed
                .lock()
                .unwrap()
                .push((url.into(), credentials.authorization));
        }
        Err(AuthError::LoginRequired)
    }
}

struct App {
    _headless: Headless,
    window: AppWindow,
    launcher: Rc<Launcher>,
}

impl App {
    fn new() -> Self {
        let headless = Headless::install(1100, 760);
        let window = AppWindow::new().expect("window");
        let launcher = Launcher::new(
            &window,
            tokio::runtime::Handle::current(),
            None,
            Arc::new(|| {}),
        );
        Self {
            _headless: headless,
            window,
            launcher,
        }
    }

    fn begin(&self, server: &str) -> AuthAttempt {
        let attempt = self.launcher.attempts.borrow_mut().begin(server);
        self.window.set_login_busy(true);
        self.window.set_login_error("current attempt".into());
        attempt
    }

    fn queue(&self, attempt: &AuthAttempt, completion: Completion) {
        assert!(
            self.launcher
                .events
                .send(Event {
                    attempt: attempt.clone(),
                    completion,
                })
                .is_ok()
        );
    }

    fn password(&self, server: &str, store: Arc<ObservedStore>) -> Arc<AppProvider> {
        let provider = Arc::new(AppProvider::Password(PasswordProvider::new(store)));
        *self.launcher.current.borrow_mut() = Some(Current {
            url: server.into(),
            kind: TransportKind::GrpcWeb,
            provider: Some(provider.clone()),
        });
        self.launcher.settings.borrow_mut().server_url = server.into();
        self.launcher.settings.borrow_mut().username = "selected-user".into();
        self.window.set_server_url(server.into());
        self.window.set_login_method(1);
        provider
    }

    fn observe(&self, server: &str, provider: Arc<AppProvider>) -> Arc<ObservedProvider> {
        let observed = Arc::new(ObservedProvider {
            inner: provider,
            requests: AtomicUsize::new(0),
            signed: Mutex::new(Vec::new()),
        });
        let config = SessionConfig::new("selected-user").with_transport(TransportKind::GrpcWeb);
        let (session, _) = Session::start(server, observed.clone(), config);
        self.launcher.controller().set_session(Some(session));
        observed
    }

    fn assert_pending(&self) {
        assert!(self.window.get_login_busy());
        assert_eq!(self.window.get_login_error().as_str(), "current attempt");
    }
}

fn options(name: &str) -> Completion {
    Completion::Options {
        kind: TransportKind::GrpcWeb,
        result: Ok(LoginOptions {
            method: LoginMethod::Development,
            server_name: name.into(),
            password_enabled: false,
        }),
    }
}

fn password_result(user: &str, token: &str) -> Completion {
    Completion::LoggedIn {
        user: user.into(),
        remember: true,
        result: Ok(PasswordSession {
            token: token.into(),
            expires_in: Duration::from_secs(3600),
            user: aegis_server::pb::UserInfo {
                username: user.into(),
                ..Default::default()
            },
        }),
    }
}

fn password_off() -> Completion {
    Completion::LoggedIn {
        user: "old-user".into(),
        remember: true,
        result: Err(ClientError::Server {
            code: ServerCode::PasswordLoginOff,
            message: "old password form disabled".into(),
        }),
    }
}

/// With Tokio time paused, this drains runnable tasks without wall-clock sleeps.
async fn drain_tasks() {
    tokio::time::sleep(Duration::from_millis(1)).await;
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn late_password_result_cannot_install_send_or_store_for_new_server() {
    let app = App::new();
    let store = Arc::new(ObservedStore::default());
    let old_provider = app.password(A, store.clone());
    let old = app.begin(A);
    let events = app.launcher.events.clone();
    let delayed_attempt = old.clone();
    let (release, released) = tokio::sync::oneshot::channel::<()>();
    // An untracked producer models a reply that won the race with task abort.
    let delayed = tokio::spawn(async move {
        released.await.unwrap();
        assert!(
            events
                .send(Event {
                    attempt: delayed_attempt,
                    completion: password_result("alice", "ats_from_a"),
                })
                .is_ok()
        );
    });

    app.window.invoke_connect_server(B.into(), 2);
    assert!(!old.guard().is_active());
    let provider = app.password(B, store.clone());
    let current = app.begin(B); // replace actual discovery with injected replies
    let observed = app.observe(B, provider.clone());
    drain_tasks().await;
    assert_eq!(observed.requests.load(Ordering::SeqCst), 1);

    release.send(()).unwrap();
    delayed.await.unwrap();
    app.queue(&old, password_off());
    app.launcher.pump();
    drain_tasks().await;
    app.assert_pending();
    assert_eq!(app.launcher.settings.borrow().server_url, B);
    assert_eq!(app.launcher.settings.borrow().username, "selected-user");
    assert_eq!(store.mutations.load(Ordering::SeqCst), 0);
    assert_eq!(
        observed.requests.load(Ordering::SeqCst),
        1,
        "no stale reconnect"
    );
    assert!(observed.signed.lock().unwrap().is_empty());
    for provider in [&old_provider, &provider] {
        let AppProvider::Password(password) = &**provider else {
            panic!("password")
        };
        assert!(!password.login.has_token());
    }
    assert!(matches!(
        provider.credentials("POST", B).await,
        Err(AuthError::LoginRequired)
    ));

    // The selected generation still works, and duplicate delivery is consumed
    // before it can overwrite the first token or the remembered user name.
    app.queue(&current, password_result("bob", "ats_from_b"));
    app.queue(&current, password_result("duplicate", "ats_duplicate"));
    app.launcher.pump();
    drain_tasks().await;
    assert!(!app.window.get_login_busy());
    assert_eq!(app.launcher.settings.borrow().username, "bob");
    assert_eq!(store.mutations.load(Ordering::SeqCst), 1);
    assert_eq!(observed.requests.load(Ordering::SeqCst), 2);
    let signed = observed.signed.lock().unwrap();
    assert_eq!(signed.len(), 1);
    assert!(signed[0].0.starts_with(B));
    assert_eq!(signed[0].1, "Bearer ats_from_b");
}

#[tokio::test(flavor = "current_thread")]
async fn stale_discovery_does_not_replace_server_or_clear_busy() {
    let app = App::new();
    let old = app.begin(A);
    app.queue(&old, options("Old server"));
    app.queue(
        &old,
        Completion::Options {
            kind: TransportKind::GrpcWeb,
            result: Err(ClientError::Transport("old discovery failed".into())),
        },
    );
    app.window.invoke_connect_server(B.into(), 2);
    app.launcher.pump();
    assert!(!old.guard().is_active());
    assert!(app.window.get_login_busy());
    assert!(app.window.get_login_error().is_empty());
    assert_eq!(app.window.get_server_url().as_str(), B);
    assert_eq!(app.window.get_connection(), 0);
    assert!(app.launcher.settings.borrow().server_url.is_empty());
    assert!(app.launcher.current.borrow().is_none());
    assert!(app.launcher.controller().session().is_none());
}

#[tokio::test(flavor = "current_thread")]
async fn same_server_discovery_retry_accepts_only_latest_once() {
    let app = App::new();
    let old = app.begin(A);
    app.window.invoke_connect_server(A.into(), 2);
    assert!(!old.guard().is_active());
    let retry = app.begin(A);
    app.queue(&old, options("Old discovery"));
    app.queue(
        &old,
        Completion::Options {
            kind: TransportKind::GrpcWeb,
            result: Err(ClientError::Transport("old failure".into())),
        },
    );
    app.launcher.pump();
    app.assert_pending();
    assert!(app.launcher.current.borrow().is_none());

    app.queue(&retry, options("Latest discovery"));
    app.queue(&retry, options("Duplicate discovery"));
    app.launcher.pump();
    assert!(!app.window.get_login_busy());
    assert_eq!(app.window.get_server_name().as_str(), "Latest discovery");
    assert_eq!(app.launcher.settings.borrow().server_url, A);
    assert_eq!(app.launcher.current.borrow().as_ref().unwrap().url, A);
}

#[tokio::test(flavor = "current_thread")]
async fn same_server_password_retry_rejects_old_success_and_error() {
    let app = App::new();
    let store = Arc::new(ObservedStore::default());
    app.password(A, store.clone());
    let old = app.begin(A);
    app.window
        .invoke_password_log_in("new-user".into(), "password".into(), true);
    assert!(!old.guard().is_active());
    let retry = app.begin(A);
    app.queue(&old, password_result("old-user", "ats_old"));
    app.queue(&old, password_off());
    app.launcher.pump();
    app.assert_pending();
    assert_eq!(app.window.get_login_method(), 1);
    assert_eq!(store.mutations.load(Ordering::SeqCst), 0);
    assert_eq!(app.launcher.settings.borrow().username, "selected-user");

    app.queue(
        &retry,
        Completion::LoggedIn {
            user: "new-user".into(),
            remember: true,
            result: Err(ClientError::Server {
                code: ServerCode::InvalidCredentials,
                message: "wrong password".into(),
            }),
        },
    );
    app.launcher.pump();
    assert!(!app.window.get_login_busy());
    assert!(app.window.get_login_error().contains("wrong"));
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn cancelled_oidc_completions_do_not_reconnect_or_clear_new_busy() {
    let app = App::new();
    let store = Arc::new(ObservedStore::default());
    let provider = app.password(B, store);
    let observed = app.observe(B, provider);
    drain_tasks().await;
    assert_eq!(observed.requests.load(Ordering::SeqCst), 1);

    let old = app.begin(A);
    app.window.invoke_log_out();
    assert!(!old.guard().is_active(), "UI hook cancels synchronously");
    drain_tasks().await;
    let current = app.begin(B);
    app.queue(&old, Completion::OidcLoggedIn(Ok(())));
    app.queue(
        &old,
        Completion::OidcLoggedIn(Err("cancelled browser".into())),
    );
    app.queue(&old, Completion::Restored(true));
    app.queue(&old, Completion::Restored(false));
    app.launcher.pump();
    drain_tasks().await;
    app.assert_pending();
    assert_eq!(observed.requests.load(Ordering::SeqCst), 1);

    // The same guard supplied to the provider prevents even discovery after
    // cancellation; no browser or OS keychain is used by this call.
    let oidc = OidcProvider::new(OidcSettings {
        server_url: A.into(),
        issuer: "https://idp.example.test".into(),
        client_id: "test".into(),
        resource: None,
        account: A.into(),
    });
    assert!(matches!(
        oidc.log_in_guarded(old.guard()).await,
        Err(AuthError::LoginRequired)
    ));
    assert!(matches!(
        oidc.restore_guarded(old.guard()).await,
        Err(AuthError::LoginRequired)
    ));

    app.queue(&current, Completion::OidcLoggedIn(Ok(())));
    app.launcher.pump();
    drain_tasks().await;
    assert!(!app.window.get_login_busy());
    assert_eq!(observed.requests.load(Ordering::SeqCst), 2);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn password_origin_error_does_not_persist_or_reconnect() {
    let app = App::new();
    let store = Arc::new(ObservedStore::default());
    let provider = app.password(B, store.clone());
    let AppProvider::Password(password) = &*provider else {
        panic!("password")
    };
    password.login.set(A, None).unwrap();
    let observed = app.observe(B, provider);
    drain_tasks().await;
    let current = app.begin(B);
    app.window.set_login_password("still entered".into());
    app.queue(&current, password_result("new-user", "ats_rejected"));
    app.launcher.pump();
    drain_tasks().await;
    assert!(!app.window.get_login_busy());
    assert!(!app.window.get_login_error().is_empty());
    assert_ne!(app.window.get_login_error().as_str(), "current attempt");
    assert_eq!(app.window.get_login_password().as_str(), "still entered");
    assert_eq!(app.launcher.settings.borrow().username, "selected-user");
    assert_eq!(store.mutations.load(Ordering::SeqCst), 0);
    assert_eq!(observed.requests.load(Ordering::SeqCst), 1);
    assert!(observed.signed.lock().unwrap().is_empty());
}

#[tokio::test(flavor = "current_thread")]
async fn invalid_connect_and_method_switches_cancel_pending_work() {
    let app = App::new();
    let old = app.begin(A);
    app.queue(&old, options("Must not attach"));
    app.window.invoke_connect_server("not a url".into(), 0);
    let error = app.window.get_connection_text();
    app.launcher.pump();
    assert!(!old.guard().is_active());
    assert!(!app.window.get_login_busy());
    assert_eq!(app.window.get_connection(), 4);
    assert_eq!(app.window.get_connection_text(), error);
    assert!(app.launcher.current.borrow().is_none());

    // Invalidation precedes even early returns when there is no current server.
    for action in 0..4 {
        let old = app.begin(A);
        match action {
            0 => app.window.invoke_use_password_login(),
            1 => app.window.invoke_use_company_login(),
            2 => app.window.invoke_change_server(),
            _ => app.window.invoke_dev_log_in("".into()),
        }
        assert!(!old.guard().is_active());
        app.queue(&old, options("Must not attach"));
        app.launcher.pump();
        assert!(app.launcher.current.borrow().is_none());
        assert!(!app.window.get_login_busy());
    }
}

#[tokio::test(flavor = "current_thread")]
async fn standalone_and_dev_entry_reject_hosted_discovery() {
    let app = App::new();
    let old = app.begin(A);
    app.queue(&old, options("Hosted discovery"));
    // This task is cancelled before polling, so no folder or license is touched.
    app.launcher
        .open_standalone("test-project".into(), "test-license.aegis".into());
    app.launcher.pump();
    assert!(!old.guard().is_active());
    assert!(app.launcher.current.borrow().is_none());
    assert!(app.window.get_login_busy());
    assert_eq!(
        app.window.get_connection_text().as_str(),
        "Opening the project folder…"
    );
    assert!(app.launcher.settings.borrow().server_url.is_empty());

    let hosted = app.begin(A);
    app.queue(&hosted, options("Late hosted discovery"));
    app.launcher
        .start_dev(B, "developer", false, TransportKind::GrpcWeb);
    app.launcher.pump();
    assert!(!hosted.guard().is_active());
    assert_eq!(app.launcher.current.borrow().as_ref().unwrap().url, B);
    assert!(matches!(
        app.launcher
            .current
            .borrow()
            .as_ref()
            .unwrap()
            .provider
            .as_deref(),
        Some(AppProvider::Dev(_))
    ));
}

#[tokio::test(flavor = "current_thread")]
async fn leaving_standalone_rejects_its_queued_result() {
    let app = App::new();
    let old = app.begin("test-project");
    app.queue(
        &old,
        Completion::StandaloneStarted {
            project_folder: "test-project".into(),
            license_path: "test-license.aegis".into(),
            result: Err("late standalone failure".into()),
        },
    );
    app.window.invoke_connect_server(B.into(), 2);
    app.launcher.pump();
    assert!(!old.guard().is_active());
    assert!(app.window.get_login_busy());
    assert!(app.window.get_login_error().is_empty());
    assert_eq!(app.window.get_server_url().as_str(), B);
    assert!(app.launcher.settings.borrow().standalone_folder.is_empty());
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn logout_shutdown_and_drop_cancel_task_and_guard() {
    for action in 0..3 {
        let app = App::new();
        let old = app.begin(A);
        let task = tokio::spawn(std::future::pending::<()>());
        let abort = task.abort_handle();
        app.launcher.attempts.borrow_mut().track(task);
        app.queue(&old, options("Queued before cancellation"));
        match action {
            0 => {
                app.window.invoke_log_out();
                app.launcher.pump();
                assert!(app.launcher.current.borrow().is_none());
            }
            1 => {
                app.launcher.shutdown().await;
                app.launcher.pump();
                assert!(app.launcher.current.borrow().is_none());
            }
            _ => drop(app.launcher),
        }
        assert!(!old.guard().is_active());
        drain_tasks().await;
        assert!(abort.is_finished(), "cancel aborts the network task");
    }
}

#[tokio::test(flavor = "current_thread")]
async fn controller_logout_hook_is_optional_and_precedes_command() {
    for with_hook in [true, false] {
        let _headless = Headless::install(1100, 760);
        let window = AppWindow::new().unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let before = calls.clone();
        let observed = calls.clone();
        let (done, mut completed) = tokio::sync::mpsc::unbounded_channel();
        let mut config = SessionConfig::new("test");
        config.on_log_out = Some(LogOutHook(Arc::new(move || {
            done.send(observed.load(Ordering::SeqCst)).unwrap();
        })));
        // Invalid URL: no connection or actual logout request can be sent.
        let (session, task) =
            Session::start("not a url", Arc::new(DevLogin::new("test", false)), config);
        let controller = Controller::attach_with(
            &window,
            Some(session.clone()),
            Hooks {
                before_log_out: if with_hook {
                    Some(Box::new(move || {
                        before.fetch_add(1, Ordering::SeqCst);
                    }))
                } else {
                    None
                },
                ..Hooks::default()
            },
        );
        window.invoke_log_out();
        assert_eq!(calls.load(Ordering::SeqCst), usize::from(with_hook));
        let observed = tokio::time::timeout(Duration::from_secs(5), completed.recv())
            .await
            .unwrap();
        assert_eq!(observed, Some(usize::from(with_hook)));

        controller.set_session(None);
        tokio::time::timeout(Duration::from_secs(5), task)
            .await
            .unwrap()
            .unwrap();
        assert!(completed.try_recv().is_err(), "exactly one LogOut command");
    }
}
