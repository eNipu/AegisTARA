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
    LoginForm, SignInView, Stage, TRANSPORTS, client_name, login_error, login_form, options_error,
    stage_of, status_text, transport_kind,
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
        url: String,
        kind: TransportKind,
        transport: String,
        result: Result<LoginOptions, ClientError>,
    },
    LoggedIn {
        user: String,
        remember: bool,
        result: Result<PasswordSession, ClientError>,
    },
    OidcLoggedIn(Result<(), String>),
    Restored(bool),
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
        self.stop_session();
        let url = url.trim().trim_end_matches('/').to_owned();
        self.server_url.clone_from(&url);
        if let Err(e) = check_url(&url) {
            self.error = Some(e.to_string());
            self.current = None;
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
        let events = self.events.clone();
        self.runtime.spawn(async move {
            let result = async { Public::connect(&url, kind).await?.login_options().await }.await;
            let _ = events.send(Event::Options {
                url,
                kind,
                transport,
                result,
            });
        });
    }

    fn on_options(
        &mut self,
        url: String,
        kind: TransportKind,
        transport: String,
        result: Result<LoginOptions, ClientError>,
    ) {
        self.busy = false;
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
                    issuer,
                    client_id,
                    resource,
                    account: url,
                })));
                self.start_session(Arc::clone(&provider), "you");
                // A saved login: try the refresh token before asking.
                let events = self.events.clone();
                self.runtime.spawn(async move {
                    if let AppProvider::Oidc(oidc) = &*provider {
                        let ok = matches!(oidc.restore().await, Ok(true));
                        let _ = events.send(Event::Restored(ok));
                    }
                });
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
        let Some((url, kind)) = self.current.as_ref().map(|c| (c.url.clone(), c.kind)) else {
            return;
        };
        self.busy = true;
        self.error = None;
        let (user, password) = (user.trim().to_owned(), password.to_owned());
        let events = self.events.clone();
        self.runtime.spawn(async move {
            let result = async {
                Public::connect(&url, kind)
                    .await?
                    .log_in(&user, &password, &client_name())
                    .await
            }
            .await;
            let _ = events.send(Event::LoggedIn {
                user,
                remember,
                result,
            });
        });
    }

    fn on_logged_in(
        &mut self,
        user: String,
        remember: bool,
        result: Result<PasswordSession, ClientError>,
    ) {
        self.busy = false;
        match result {
            Ok(answer) => {
                let Some(c) = self.current.as_ref() else {
                    return;
                };
                if let Some(provider) = &c.provider
                    && let AppProvider::Password(p) = &**provider
                {
                    p.set(&c.url, &answer.user.username, answer.token, remember);
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
        let Some(provider) = self.current.as_ref().and_then(|c| c.provider.clone()) else {
            return;
        };
        self.error = None;
        let events = self.events.clone();
        self.runtime.spawn(async move {
            if let AppProvider::Oidc(oidc) = &*provider {
                let result = oidc.log_in().await.map_err(|e| e.to_string());
                let _ = events.send(Event::OidcLoggedIn(result));
            }
        });
    }

    /// The user entered a user name on a development server.
    pub fn dev_log_in(&mut self, user: &str) {
        let user = user.trim();
        if user.is_empty() {
            return;
        }
        self.settings.username = user.to_owned();
        self.save_settings();
        self.start_session(Arc::new(AppProvider::Dev(DevLogin::new(user, false))), user);
    }

    /// The user went back to the server form.
    pub fn change_server(&mut self) {
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
        if let Some(s) = &self.session {
            let _ = s.send(Command::LogOut);
        }
    }

    /// Handles every answer that came back. Call it on the screen's thread;
    /// `true` if anything changed that the screen shows.
    pub fn pump(&mut self) -> bool {
        let mut changed = false;
        while let Ok(event) = self.inbox.try_recv() {
            changed = true;
            match event {
                Event::Options {
                    url,
                    kind,
                    transport,
                    result,
                } => self.on_options(url, kind, transport, result),
                Event::LoggedIn {
                    user,
                    remember,
                    result,
                } => self.on_logged_in(user, remember, result),
                Event::OidcLoggedIn(Ok(())) | Event::Restored(true) => self.reconnect(),
                Event::OidcLoggedIn(Err(e)) => {
                    tracing::warn!(error = %e, "login failed");
                    self.error = Some(
                        self.catalog
                            .tr_with("signin-error-login-unfinished", &[("error", &e)]),
                    );
                }
                Event::Restored(false) => {}
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
        self.stop_session();
    }
}
