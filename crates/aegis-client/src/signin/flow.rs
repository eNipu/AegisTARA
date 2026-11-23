//! The sign-in controller: from a server address to a running session, with
//! no toolkit. The port of what the Slint launcher does before a project
//! opens.
//!
//! It lives on the screen's thread. Network work runs on the given tokio
//! runtime and reports back over a channel that [`SignIn::pump`] drains.

use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::mpsc;

use tokio::runtime::Handle;

use super::{
    AuthAttempt, AuthAttempts, LoginForm, SignInView, Stage, TRANSPORTS, client_name, login_error,
    login_form, options_error, stage_of, status_text, transport_kind,
};
use crate::commands::Command;
use crate::connection::{DevLogin, LoginMethod, LoginOptions, PasswordSession, Public};
use crate::error::{ClientError, ServerCode};
use crate::i18n::Catalog;
use crate::provider::{AppProvider, OidcProvider, OidcSettings, PasswordProvider, Secrets};
use crate::session::{LogOutHook, Session, SessionConfig};
use crate::settings::Settings;
use crate::state::ConnectionView;
use crate::transport::{TransportKind, check_url};

/// What the network tasks report back.
enum Event {
    Options {
        attempt: AuthAttempt,
        kind: TransportKind,
        transport: String,
        result: Result<LoginOptions, ClientError>,
    },
    LoggedIn {
        attempt: AuthAttempt,
        user: String,
        remember: bool,
        result: Result<PasswordSession, ClientError>,
    },
    OidcLoggedIn {
        attempt: AuthAttempt,
        result: Result<(), String>,
    },
    Restored {
        attempt: AuthAttempt,
        restored: bool,
    },
}

/// The server the user chose and how the session logs in to it.
struct Current {
    url: String,
    kind: TransportKind,
    provider: Option<Arc<AppProvider>>,
}

/// See the module docs.
pub struct SignIn {
    runtime: Handle,
    catalog: Rc<Catalog>,
    settings: Settings,
    settings_path: Option<PathBuf>,
    secrets: Secrets,
    events: mpsc::Sender<Event>,
    inbox: mpsc::Receiver<Event>,
    attempts: AuthAttempts,
    current: Option<Current>,
    session: Option<Session>,
    form: Option<LoginForm>,
    server_url: String,
    server_name: String,
    error: Option<String>,
    busy: bool,
    connecting: bool,
    seen_revision: u64,
}

impl SignIn {
    /// The sign-in screen opened: the settings are read from
    /// `settings_path` (defaults when `None`), nothing is connected yet.
    #[must_use]
    pub fn new(
        runtime: Handle,
        catalog: Rc<Catalog>,
        settings_path: Option<PathBuf>,
        secrets: Secrets,
    ) -> SignIn {
        let settings = settings_path
            .as_deref()
            .map(Settings::load)
            .unwrap_or_default();
        let (events, inbox) = mpsc::channel();
        SignIn {
            runtime,
            catalog,
            server_url: settings.server_url.clone(),
            settings,
            settings_path,
            secrets,
            events,
            inbox,
            attempts: AuthAttempts::default(),
            current: None,
            session: None,
            form: None,
            server_name: String::new(),
            error: None,
            busy: false,
            connecting: false,
            seen_revision: 0,
        }
    }

    /// The app started: connect to the remembered server, or wait on the
    /// server form when there is none.
    pub fn start(&mut self) {
        if self.settings.server_url.is_empty() {
            return;
        }
        let url = self.settings.server_url.clone();
        let transport = self.settings.transport.clone();
        self.connect(&url, &transport);
    }

    /// The user entered a server address and protocol: ask the server how
    /// to log in.
    pub fn connect(&mut self, url: &str, transport: &str) {
        self.attempts.cancel();
        self.stop_session();
        self.current = None;
        self.form = None;
        self.server_name.clear();
        let url = url.trim().trim_end_matches('/').to_owned();
        self.server_url.clone_from(&url);
        if let Err(e) = check_url(&url) {
            self.error = Some(e.to_string());
            self.busy = false;
            self.connecting = false;
            return;
        }
        let transport = if TRANSPORTS.contains(&transport) {
            transport.to_owned()
        } else {
            "auto".to_owned()
        };
        let kind = transport_kind(&transport, &url);
        self.error = None;
        self.busy = true;
        self.connecting = true;
        let attempt = self.attempts.begin(&url);
        let events = self.events.clone();
        let task = self.runtime.spawn(async move {
            let result = async { Public::connect(&url, kind).await?.login_options().await }.await;
            let _ = events.send(Event::Options {
                attempt,
                kind,
                transport,
                result,
            });
        });
        self.attempts.track(task);
    }

    fn on_options(
        &mut self,
        url: String,
        kind: TransportKind,
        transport: String,
        result: Result<LoginOptions, ClientError>,
    ) {
        self.connecting = false;
        let options = match result {
            Ok(o) => o,
            Err(e) => {
                self.error = Some(options_error(&url, &e, &self.catalog));
                self.current = None;
                return;
            }
        };
        self.settings.server_url.clone_from(&url);
        self.settings.transport = transport;
        self.save_settings();
        self.server_url.clone_from(&url);
        self.server_name.clone_from(&options.server_name);
        self.form = Some(login_form(&options));
        self.current = Some(Current {
            url: url.clone(),
            kind,
            provider: None,
        });
        match options.method {
            LoginMethod::Password => self.begin_password(),
            LoginMethod::Oidc {
                issuer,
                client_id,
                resource,
            } => {
                let provider = Arc::new(AppProvider::Oidc(OidcProvider::new(OidcSettings {
                    server_url: url.clone(),
                    issuer,
                    client_id,
                    resource,
                    account: url.clone(),
                })));
                self.start_session(Arc::clone(&provider), "you");
                // A saved login: try the refresh token before asking.
                self.busy = true;
                let attempt = self.attempts.begin(&url);
                let events = self.events.clone();
                let task = self.runtime.spawn(async move {
                    if let AppProvider::Oidc(oidc) = &*provider {
                        let restored =
                            matches!(oidc.restore_guarded(attempt.guard()).await, Ok(true));
                        let _ = events.send(Event::Restored { attempt, restored });
                    }
                });
                self.attempts.track(task);
            }
            LoginMethod::Development => {}
        }
    }

    /// Starts a password session, restored from the keychain if the user
    /// asked to be kept logged in; it waits for a login otherwise.
    fn begin_password(&mut self) {
        let Some(url) = self.current.as_ref().map(|c| c.url.clone()) else {
            return;
        };
        let user = self.settings.username.clone();
        let provider = PasswordProvider::new(Arc::clone(&self.secrets));
        if !user.is_empty() {
            provider.restore(&url, &user);
        }
        let shown = if user.is_empty() { "you" } else { &user };
        self.start_session(Arc::new(AppProvider::Password(provider)), shown);
    }

    /// The user chose "Use a user name and password" on the company login.
    pub fn use_password(&mut self) {
        self.attempts.cancel();
        self.busy = false;
        self.connecting = false;
        if self.current.is_none() {
            return;
        }
        self.error = None;
        self.form = Some(LoginForm::Password);
        self.begin_password();
    }

    /// The user chose "Use company login" on the password form: ask the
    /// server again, so a switch an admin turned off meanwhile is respected.
    pub fn use_company(&mut self) {
        self.attempts.cancel();
        self.busy = false;
        self.connecting = false;
        let Some(url) = self.current.as_ref().map(|c| c.url.clone()) else {
            return;
        };
        let transport = self.settings.transport.clone();
        self.connect(&url, &transport);
    }

    fn start_session(&mut self, provider: Arc<AppProvider>, user: &str) {
        let Some((url, kind)) = self.current.as_ref().map(|c| (c.url.clone(), c.kind)) else {
            return;
        };
        self.stop_session();
        let forget = Arc::clone(&provider);
        let mut config = SessionConfig::new(user).with_transport(kind);
        config.update_url = std::env::var("AEGIS_UPDATE_URL")
            .ok()
            .filter(|u| !u.is_empty())
            .unwrap_or_else(|| self.settings.update_url.clone());
        if let Some(dir) = std::env::var_os("AEGIS_UPDATE_DIR").filter(|d| !d.is_empty()) {
            config.update_dir = crate::updates::DownloadDir::Folder(dir.into());
        }
        config.on_log_out = Some(LogOutHook(Arc::new(move || {
            if let AppProvider::Password(p) = &*forget {
                p.forget();
            }
        })));
        let session = {
            let _enter = self.runtime.enter();
            Session::start(&url, Arc::clone(&provider), config).0
        };
        if let Some(c) = self.current.as_mut() {
            c.provider = Some(provider);
        }
        self.session = Some(session);
    }

    /// The user logged in with a user name and password; `remember` keeps
    /// the login in the keychain.
    pub fn password_log_in(&mut self, user: &str, password: &str, remember: bool) {
        let Some(current) = &self.current else {
            return;
        };
        if !matches!(current.provider.as_deref(), Some(AppProvider::Password(_))) {
            return;
        }
        let (url, kind) = (current.url.clone(), current.kind);
        let attempt = self.attempts.begin(&url);
        self.busy = true;
        self.error = None;
        let (user, password) = (user.trim().to_owned(), password.to_owned());
        let events = self.events.clone();
        let task = self.runtime.spawn(async move {
            let result = async {
                Public::connect(&url, kind)
                    .await?
                    .log_in(&user, &password, &client_name())
                    .await
            }
            .await;
            let _ = events.send(Event::LoggedIn {
                attempt,
                user,
                remember,
                result,
            });
        });
        self.attempts.track(task);
    }

    fn on_logged_in(
        &mut self,
        server: &str,
        user: String,
        remember: bool,
        result: Result<PasswordSession, ClientError>,
    ) {
        match result {
            Ok(answer) => {
                let Some(AppProvider::Password(p)) =
                    self.current.as_ref().and_then(|c| c.provider.as_deref())
                else {
                    return;
                };
                if let Err(e) = p.set(server, &answer.user.username, answer.token, remember) {
                    self.error = Some(e.to_string());
                    return;
                }
                self.settings.username = user;
                self.save_settings();
                self.reconnect();
            }
            Err(e) => {
                if matches!(e.code(), Some(ServerCode::PasswordLoginOff)) {
                    // An admin turned the password form off while this
                    // screen was open: ask the server again, which shows
                    // the company login only.
                    self.use_company();
                    self.error = Some(self.catalog.tr("signin-error-password-turned-off"));
                } else {
                    self.error = Some(login_error(&e, &self.catalog));
                }
            }
        }
    }

    /// The user chose the company login: the browser opens.
    pub fn company_log_in(&mut self) {
        let Some(current) = &self.current else {
            return;
        };
        let Some(provider) = &current.provider else {
            return;
        };
        if !matches!(&**provider, AppProvider::Oidc(_)) {
            return;
        }
        let provider = Arc::clone(provider);
        let attempt = self.attempts.begin(&current.url);
        self.busy = true;
        self.error = None;
        let events = self.events.clone();
        let task = self.runtime.spawn(async move {
            if let AppProvider::Oidc(oidc) = &*provider {
                let result = oidc
                    .log_in_guarded(attempt.guard())
                    .await
                    .map_err(|e| e.to_string());
                let _ = events.send(Event::OidcLoggedIn { attempt, result });
            }
        });
        self.attempts.track(task);
    }

    /// The user entered a user name on a development server.
    pub fn dev_log_in(&mut self, user: &str) {
        let user = user.trim();
        if user.is_empty() || self.form != Some(LoginForm::Development) || self.current.is_none() {
            return;
        }
        self.attempts.cancel();
        self.settings.username = user.to_owned();
        self.save_settings();
        self.start_session(Arc::new(AppProvider::Dev(DevLogin::new(user, false))), user);
    }

    /// The user went back to the server form.
    pub fn change_server(&mut self) {
        self.attempts.cancel();
        self.stop_session();
        self.current = None;
        self.form = None;
        self.error = None;
        self.server_name.clear();
        self.busy = false;
        self.connecting = false;
    }

    /// The user signed out; the session then reports that a login is
    /// needed.
    pub fn sign_out(&mut self) {
        self.attempts.cancel();
        self.busy = false;
        self.connecting = false;
        if let Some(s) = &self.session {
            let _ = s.send(Command::LogOut);
        }
    }

    /// Handles every answer that came back. Call it on the screen's thread;
    /// `true` if anything changed that the screen shows.
    pub fn pump(&mut self) -> bool {
        let mut changed = false;
        while let Ok(event) = self.inbox.try_recv() {
            let attempt = match &event {
                Event::Options { attempt, .. }
                | Event::LoggedIn { attempt, .. }
                | Event::OidcLoggedIn { attempt, .. }
                | Event::Restored { attempt, .. } => attempt,
            };
            if !self.attempts.complete(attempt) {
                continue;
            }
            changed = true;
            self.busy = false;
            match event {
                Event::Options {
                    attempt,
                    kind,
                    transport,
                    result,
                } => self.on_options(attempt.server().to_owned(), kind, transport, result),
                Event::LoggedIn {
                    attempt,
                    user,
                    remember,
                    result,
                } => self.on_logged_in(attempt.server(), user, remember, result),
                Event::OidcLoggedIn { result: Ok(()), .. }
                | Event::Restored { restored: true, .. } => self.reconnect(),
                Event::OidcLoggedIn { result: Err(e), .. } => {
                    tracing::warn!(error = %e, "login failed");
                    self.error = Some(
                        self.catalog
                            .tr_with("signin-error-login-unfinished", &[("error", &e)]),
                    );
                }
                Event::Restored {
                    restored: false, ..
                } => {}
            }
        }
        if let Some(s) = &self.session {
            let revision = s.view().revision;
            if revision != self.seen_revision {
                self.seen_revision = revision;
                changed = true;
            }
        }
        changed
    }

    /// The running session, once one is started.
    #[must_use]
    pub fn session(&self) -> Option<&Session> {
        self.session.as_ref()
    }

    /// The settings as remembered now.
    #[must_use]
    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    /// What the sign-in screen shows now.
    #[must_use]
    pub fn view(&self) -> SignInView {
        let model = self.session.as_ref().map(Session::view);
        let connection = model.as_ref().map(|m| &m.connection);
        let stage = if self.connecting {
            Stage::Connecting
        } else {
            match (&self.current, connection) {
                (None, _) => Stage::ChooseServer,
                (Some(_), Some(c)) => stage_of(c),
                (Some(_), None) if self.form == Some(LoginForm::Development) => {
                    Stage::LoginRequired
                }
                (Some(_), None) => Stage::ChooseServer,
            }
        };
        let online = matches!(connection, Some(ConnectionView::Online { .. }));
        let (projects, user) = match (&model, online) {
            (Some(m), true) => (m.projects.clone(), m.user.clone()),
            _ => (Vec::new(), String::new()),
        };
        let status = status_text(
            stage,
            self.form,
            &self.server_url,
            connection,
            &user,
            &self.catalog,
        );
        let transport = if self.settings.transport.is_empty() {
            "auto".to_owned()
        } else {
            self.settings.transport.clone()
        };
        SignInView {
            stage,
            status,
            form: self.form,
            server_url: self.server_url.clone(),
            transport,
            server_name: self.server_name.clone(),
            user_name: self.settings.username.clone(),
            error: self.error.clone(),
            busy: self.busy,
            projects,
            user,
        }
    }

    fn reconnect(&self) {
        if let Some(s) = &self.session {
            let _ = s.send(Command::Reconnect);
        }
    }

    fn stop_session(&mut self) {
        if let Some(provider) = self.current.as_ref().and_then(|c| c.provider.as_ref()) {
            provider.invalidate();
        }
        if let Some(s) = self.session.take() {
            let _ = s.send(Command::Shutdown);
        }
    }

    fn save_settings(&self) {
        if let Some(path) = &self.settings_path
            && let Err(e) = self.settings.save(path)
        {
            tracing::warn!(error = %e, "the settings could not be saved");
        }
    }
}

impl Drop for SignIn {
    fn drop(&mut self) {
        self.attempts.cancel();
        self.stop_session();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use aegis_auth::AuthError;
    use aegis_auth::store::{MemorySecretStore, SecretStore};

    use super::*;
    use crate::i18n::Language;
    use crate::provider::memory_secrets;

    const A: &str = "https://a.example.test";
    const B: &str = "https://b.example.test";

    #[derive(Default)]
    struct CountedSecrets {
        inner: MemorySecretStore,
        calls: AtomicUsize,
    }

    impl SecretStore for CountedSecrets {
        fn get(&self, name: &str) -> Result<Option<Vec<u8>>, AuthError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.inner.get(name)
        }

        fn set(&self, name: &str, value: &[u8]) -> Result<(), AuthError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.inner.set(name, value)
        }

        fn delete(&self, name: &str) -> Result<(), AuthError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.inner.delete(name)
        }
    }

    fn sign_in() -> SignIn {
        SignIn::new(
            Handle::current(),
            Rc::new(Catalog::load(Language::En)),
            None,
            memory_secrets(),
        )
    }

    fn queue(signin: &SignIn, event: Event) {
        assert!(signin.events.send(event).is_ok());
    }

    // Replace the network task with a controlled completion. These current-thread
    // tests never yield, so no discovery, browser or session task contacts a server.
    fn discovery(signin: &mut SignIn, server: &str) -> AuthAttempt {
        signin.connect(server, "auto");
        signin.attempts.begin(server)
    }

    fn options(attempt: AuthAttempt, method: LoginMethod, name: &str) -> Event {
        let kind = TransportKind::auto(attempt.server());
        Event::Options {
            attempt,
            kind,
            transport: "auto".into(),
            result: Ok(LoginOptions {
                method,
                server_name: name.into(),
                password_enabled: true,
            }),
        }
    }

    fn ready(signin: &mut SignIn, server: &str, method: LoginMethod) {
        let attempt = discovery(signin, server);
        queue(signin, options(attempt, method, server));
        assert!(signin.pump());
    }

    fn oidc() -> LoginMethod {
        LoginMethod::Oidc {
            issuer: "https://identity.example.test".into(),
            client_id: "desktop".into(),
            resource: None,
        }
    }

    fn password_success(attempt: AuthAttempt, user: &str, token: &str) -> Event {
        Event::LoggedIn {
            attempt,
            user: user.into(),
            remember: true,
            result: Ok(PasswordSession {
                token: token.into(),
                expires_in: Duration::from_secs(3600),
                user: aegis_proto::UserInfo {
                    username: user.into(),
                    ..Default::default()
                },
            }),
        }
    }

    fn password_off(attempt: AuthAttempt) -> Event {
        Event::LoggedIn {
            attempt,
            user: "old-user".into(),
            remember: true,
            result: Err(ClientError::Server {
                code: ServerCode::PasswordLoginOff,
                message: "password login disabled".into(),
            }),
        }
    }

    fn queue_all_completions(signin: &SignIn, attempt: &AuthAttempt) {
        queue(signin, options(attempt.clone(), oidc(), "old server"));
        queue(
            signin,
            Event::Options {
                attempt: attempt.clone(),
                kind: TransportKind::auto(attempt.server()),
                transport: "grpc".into(),
                result: Err(ClientError::Transport("old failure".into())),
            },
        );
        queue(
            signin,
            password_success(attempt.clone(), "old-user", "ats_old"),
        );
        queue(signin, password_off(attempt.clone()));
        for result in [Ok(()), Err("old OIDC failure".into())] {
            queue(
                signin,
                Event::OidcLoggedIn {
                    attempt: attempt.clone(),
                    result,
                },
            );
        }
        for restored in [true, false] {
            queue(
                signin,
                Event::Restored {
                    attempt: attempt.clone(),
                    restored,
                },
            );
        }
    }

    fn pump_ignored(signin: &mut SignIn) {
        let before = signin.view();
        let settings = signin.settings.clone();
        assert!(!signin.pump(), "stale events must not report a UI change");
        assert_eq!(signin.view(), before);
        assert_eq!(signin.settings, settings);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn late_discovery_success_and_failure_cannot_replace_b() {
        for failed in [false, true] {
            let mut signin = sign_in();
            let a = discovery(&mut signin, A);
            let b = discovery(&mut signin, B);
            assert!(!a.guard().is_active());
            let late = || {
                if failed {
                    Event::Options {
                        attempt: a.clone(),
                        kind: TransportKind::auto(A),
                        transport: "grpc".into(),
                        result: Err(ClientError::Transport("A failed late".into())),
                    }
                } else {
                    options(a.clone(), oidc(), "A")
                }
            };
            queue(&signin, late());
            pump_ignored(&mut signin);
            assert!(signin.busy && signin.connecting);
            assert!(signin.current.is_none());
            assert!(signin.settings.server_url.is_empty());

            queue(&signin, options(b, LoginMethod::Development, "B"));
            assert!(signin.pump());
            queue(&signin, late());
            pump_ignored(&mut signin);
            assert_eq!(signin.server_name, "B");
            assert_eq!(signin.current.as_ref().unwrap().url, B);
            assert_eq!(signin.settings.server_url, B);
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn same_server_discovery_retry_and_duplicate_results_are_isolated() {
        let mut signin = sign_in();
        let old = discovery(&mut signin, A);
        let retry = discovery(&mut signin, A);
        queue_all_completions(&signin, &old);
        pump_ignored(&mut signin);
        assert!(signin.busy);
        assert!(!old.guard().is_active());

        queue(
            &signin,
            options(retry.clone(), LoginMethod::Development, "retry"),
        );
        assert!(signin.pump());
        queue_all_completions(&signin, &retry);
        pump_ignored(&mut signin);
        assert_eq!(signin.server_name, "retry");
        assert!(!signin.busy);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn new_discovery_clears_the_old_provider_and_login_form() {
        let mut signin = sign_in();
        ready(&mut signin, A, LoginMethod::Password);
        assert!(signin.session.is_some());
        let old = signin.attempts.begin(A);

        signin.connect(B, "auto");
        assert!(!old.guard().is_active());
        assert!(signin.current.is_none());
        assert!(signin.session.is_none());
        assert!(signin.form.is_none());
        assert!(signin.server_name.is_empty());
        let before = signin.view();
        signin.password_log_in("admin", "password", true);
        signin.company_log_in();
        signin.dev_log_in("admin");
        assert_eq!(signin.view(), before);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn late_password_result_cannot_touch_b_provider_keychain_or_settings() {
        let mut signin = sign_in();
        let secrets = Arc::new(CountedSecrets::default());
        signin.secrets = secrets.clone();
        let path = std::env::temp_dir().join(format!(
            "aegis-signin-isolation-{}.json",
            uuid::Uuid::new_v4()
        ));
        signin.settings_path = Some(path.clone());
        ready(&mut signin, A, LoginMethod::Password);
        let a = signin.attempts.begin(A);
        ready(&mut signin, B, LoginMethod::Password);
        let provider = signin.current.as_ref().unwrap().provider.clone().unwrap();
        let AppProvider::Password(password) = &*provider else {
            panic!("B must have its own password provider");
        };
        assert!(!password.login.has_token());
        let calls = secrets.calls.load(Ordering::SeqCst);
        let saved = std::fs::read(&path).unwrap();

        queue(&signin, password_success(a.clone(), "alice", "ats_from_a"));
        queue(&signin, password_off(a));
        pump_ignored(&mut signin);
        assert!(!password.login.has_token());
        assert!(Arc::ptr_eq(
            signin.current.as_ref().unwrap().provider.as_ref().unwrap(),
            &provider
        ));
        assert_eq!(secrets.calls.load(Ordering::SeqCst), calls);
        assert_eq!(std::fs::read(&path).unwrap(), saved);
        assert_eq!(Settings::load(&path), signin.settings);
        std::fs::remove_file(path).unwrap();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn password_retry_invalidates_successes_and_failures_from_same_server() {
        let mut signin = sign_in();
        ready(&mut signin, A, LoginMethod::Password);
        let old = signin.attempts.begin(A);
        signin.password_log_in("new-user", "password", true);
        assert!(!old.guard().is_active());
        queue(
            &signin,
            password_success(old.clone(), "old-user", "ats_old"),
        );
        queue(&signin, password_off(old));
        pump_ignored(&mut signin);
        assert!(signin.busy);

        let retry = signin.attempts.begin(A);
        queue(
            &signin,
            password_success(retry.clone(), "new-user", "ats_new"),
        );
        assert!(signin.pump());
        assert!(!signin.busy);
        assert_eq!(signin.settings.username, "new-user");
        assert_eq!(
            signin
                .secrets
                .get(&format!("password-session|{A}|new-user"))
                .unwrap(),
            Some(b"ats_new".to_vec())
        );
        queue(&signin, password_success(retry, "old-user", "ats_old"));
        pump_ignored(&mut signin);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn navigation_signout_and_method_switches_reject_queued_completions() {
        let actions: [fn(&mut SignIn); 5] = [
            SignIn::change_server,
            SignIn::sign_out,
            |s| s.connect("not a URL", "auto"),
            SignIn::use_password,
            SignIn::use_company,
        ];
        for action in actions {
            let mut signin = sign_in();
            ready(&mut signin, A, LoginMethod::Password);
            let attempt = signin.attempts.begin(A);
            signin.busy = true;
            queue_all_completions(&signin, &attempt);
            action(&mut signin);
            assert!(!attempt.guard().is_active());
            pump_ignored(&mut signin);
            assert!(
                signin
                    .secrets
                    .get(&format!("password-session|{A}|old-user"))
                    .unwrap()
                    .is_none()
            );
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn oidc_restore_and_login_completions_are_invalidated_by_retry_and_switch() {
        let mut signin = sign_in();
        ready(&mut signin, A, oidc());
        let restore = signin.attempts.begin(A);
        signin.company_log_in();
        assert!(!restore.guard().is_active());
        for restored in [true, false] {
            queue(
                &signin,
                Event::Restored {
                    attempt: restore.clone(),
                    restored,
                },
            );
        }
        pump_ignored(&mut signin);
        assert!(signin.busy);

        let login = signin.attempts.begin(A);
        signin.company_log_in();
        assert!(!login.guard().is_active());
        for result in [Ok(()), Err("cancelled login".into())] {
            queue(
                &signin,
                Event::OidcLoggedIn {
                    attempt: login.clone(),
                    result,
                },
            );
        }
        pump_ignored(&mut signin);
        assert!(signin.busy);

        let switched = signin.attempts.begin(A);
        signin.use_password();
        assert!(!switched.guard().is_active());
        queue_all_completions(&signin, &switched);
        pump_ignored(&mut signin);
        assert_eq!(signin.form, Some(LoginForm::Password));
        assert!(!signin.busy);

        signin.use_company();
        ready(&mut signin, A, oidc());
        let old = signin.attempts.begin(A);
        ready(&mut signin, B, LoginMethod::Password);
        queue_all_completions(&signin, &old);
        pump_ignored(&mut signin);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn current_oidc_completions_clear_busy_and_are_consumed_once() {
        for completion in 0..4 {
            let mut signin = sign_in();
            ready(&mut signin, A, oidc());
            let attempt = signin.attempts.begin(A);
            let event = match completion {
                0 => Event::Restored {
                    attempt: attempt.clone(),
                    restored: false,
                },
                1 => Event::Restored {
                    attempt: attempt.clone(),
                    restored: true,
                },
                2 => Event::OidcLoggedIn {
                    attempt: attempt.clone(),
                    result: Ok(()),
                },
                _ => Event::OidcLoggedIn {
                    attempt: attempt.clone(),
                    result: Err("login failed".into()),
                },
            };
            queue(&signin, event);
            assert!(signin.pump());
            assert!(!signin.busy);
            assert_eq!(signin.error.is_some(), completion == 3);
            queue_all_completions(&signin, &attempt);
            pump_ignored(&mut signin);
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn login_requires_the_matching_provider_without_cancelling_valid_work() {
        for method in [LoginMethod::Password, oidc(), LoginMethod::Development] {
            let mut signin = sign_in();
            ready(&mut signin, A, method.clone());
            let attempt = signin.attempts.begin(A);
            signin.error = Some("keep this error".into());
            let before = signin.view();
            if method != LoginMethod::Password {
                signin.password_log_in("admin", "password", true);
            }
            if !matches!(method, LoginMethod::Oidc { .. }) {
                signin.company_log_in();
            }
            if method != LoginMethod::Development {
                signin.dev_log_in("admin");
            }
            assert_eq!(signin.view(), before);
            assert!(attempt.guard().is_active());
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn rejected_password_provider_set_does_not_save_the_user_or_token() {
        let mut signin = sign_in();
        let secrets = Arc::new(CountedSecrets::default());
        signin.secrets = secrets.clone();
        ready(&mut signin, B, LoginMethod::Password);
        let provider = signin.current.as_ref().unwrap().provider.clone().unwrap();
        let AppProvider::Password(password) = &*provider else {
            panic!("password provider");
        };
        // An inconsistent provider must fail closed even for a current event.
        assert!(!password.restore(A, "bound-to-a"));
        let calls = secrets.calls.load(Ordering::SeqCst);
        let settings = signin.settings.clone();
        let attempt = signin.attempts.begin(B);
        signin.busy = true;
        queue(
            &signin,
            password_success(attempt, "alice", "ats_wrong_origin"),
        );
        assert!(signin.pump());
        assert!(!signin.busy);
        assert!(signin.error.is_some());
        assert_eq!(signin.settings, settings);
        assert_eq!(secrets.calls.load(Ordering::SeqCst), calls);
        assert!(!password.login.has_token());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn dropping_signin_cancels_the_pending_commit_guard() {
        let mut signin = sign_in();
        let attempt = discovery(&mut signin, A);
        queue_all_completions(&signin, &attempt);
        drop(signin);
        assert!(!attempt.guard().is_active());
    }
}
