//! From a server address to a running session (job 037).
//!
//! The launcher owns what happens before (and around) a [`Session`]:
//!
//! 1. **Choose a server.** The address and transport come from the settings
//!    file, or the user types them.
//! 2. **Ask the server how to log in** (`GetLoginOptions`, no credentials).
//! 3. **Start a session** with the matching credentials: a password session
//!    (restored from the keychain if the user asked to be kept logged in),
//!    company login, or a development user name.
//! 4. **Log in** when the session says a login is needed, and reconnect.
//!
//! Network calls run on the tokio runtime; their answers come back through
//! a channel that the UI thread drains in [`Launcher::pump`], so the window
//! is only touched on its own thread. `wake` asks the UI thread to pump (in
//! the app, `slint::invoke_from_event_loop`; in tests, nothing, because the
//! test pumps).

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::mpsc;

use aegis_client::commands::Command;
use aegis_client::connection::{DevLogin, LoginMethod, LoginOptions, PasswordSession, Public};
use aegis_client::error::{ClientError, ServerCode};
use aegis_client::session::{LogOutHook, Session, SessionConfig};
use aegis_client::settings::{Appearance, Settings};
use aegis_client::transport::TransportKind;
use slint::ComponentHandle;

use crate::AppWindow;
use crate::controller::{Controller, Hooks};
use crate::provider::{AppProvider, OidcProvider, OidcSettings, PasswordProvider, secrets};

/// What the network tasks report back.
enum Event {
    Options {
        url: String,
        kind: TransportKind,
        result: Result<LoginOptions, ClientError>,
    },
    LoggedIn {
        user: String,
        remember: bool,
        result: Result<PasswordSession, ClientError>,
    },
    OidcLoggedIn(Result<(), String>),
    Restored(bool),
    StandaloneStarted {
        project_folder: std::path::PathBuf,
        license_path: std::path::PathBuf,
        result: Result<crate::standalone::Standalone, String>,
    },
}

struct Current {
    url: String,
    kind: TransportKind,
    provider: Option<Arc<AppProvider>>,
}

type SessionHook = Box<dyn Fn(&Session)>;

/// See the module docs.
pub struct Launcher {
    window: slint::Weak<AppWindow>,
    controller: RefCell<Option<Rc<Controller>>>,
    runtime: tokio::runtime::Handle,
    settings: RefCell<Settings>,
    settings_path: Option<std::path::PathBuf>,
    events: mpsc::Sender<Event>,
    inbox: mpsc::Receiver<Event>,
    wake: Arc<dyn Fn() + Send + Sync>,
    current: RefCell<Option<Current>>,
    standalone: RefCell<Option<crate::standalone::Standalone>>,
    /// Called with every new session, so the app can wake the UI on its
    /// view model changes.
    on_session: RefCell<Option<SessionHook>>,
}

fn transport_position(kind: &str) -> i32 {
    match kind {
        "grpc" => 1,
        "grpc-web" => 2,
        _ => 0,
    }
}

fn transport_name(position: i32) -> &'static str {
    match position {
        1 => "grpc",
        2 => "grpc-web",
        _ => "auto",
    }
}

fn login_error(e: &ClientError) -> String {
    match e.code() {
        Some(ServerCode::InvalidCredentials) => "The user name or password is wrong.".to_owned(),
        Some(ServerCode::Throttled { retry_after_s }) => format!(
            "Too many failed logins. Try again in {} seconds.",
            retry_after_s.unwrap_or(60)
        ),
        Some(ServerCode::PasswordLoginOff) => {
            "Password login is off on this server. Use the company login.".to_owned()
        }
        _ => match e {
            ClientError::Transport(t) => format!("The server cannot be reached: {t}"),
            other => other.to_string(),
        },
    }
}

fn client_name() -> String {
    let host = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_default();
    if host.is_empty() {
        "AegisTARA desktop".to_owned()
    } else {
        format!("AegisTARA desktop on {host}")
    }
}

impl Launcher {
    /// A launcher for `window`. Reads the settings from `settings_path`
    /// (`None`: nothing is remembered, as in tests).
    pub fn new(
        window: &AppWindow,
        runtime: tokio::runtime::Handle,
        settings_path: Option<std::path::PathBuf>,
        wake: Arc<dyn Fn() + Send + Sync>,
    ) -> Rc<Launcher> {
        let settings = settings_path
            .as_deref()
            .map(Settings::load)
            .unwrap_or_default();
        let (events, inbox) = mpsc::channel();
        let launcher = Rc::new(Launcher {
            window: window.as_weak(),
            controller: RefCell::new(None),
            runtime,
            settings: RefCell::new(settings),
            settings_path,
            events,
            inbox,
            wake,
            current: RefCell::new(None),
            standalone: RefCell::new(None),
            on_session: RefCell::new(None),
        });
        let hooks = Self::hooks(&launcher);
        let controller = Controller::attach_with(window, None, hooks);
        *launcher.controller.borrow_mut() = Some(controller);
        {
            let s = launcher.settings.borrow();
            window.set_server_url(s.server_url.as_str().into());
            window.set_standalone_folder(s.standalone_folder.as_str().into());
            window.set_standalone_license(s.standalone_license.as_str().into());
            window.set_transport_index(transport_position(&s.transport));
            window.set_login_user(s.username.as_str().into());
            window.set_export_dir(s.export_dir().display().to_string().into());
            window.set_appearance(s.appearance().index());
            if s.left_panel > 0 {
                window.set_left_size(s.left_panel as f32);
            }
            window.set_right_size(s.right_panel as f32);
            let language = s.language().code().to_owned();
            // The bundled gettext catalogs are selected after the first
            // component exists (the window is created before this runs).
            if let Err(e) = slint::select_bundled_translation(&language) {
                tracing::warn!(%language, error = %e, "the UI language was not found");
            }
        }
        launcher
    }

    fn hooks(this: &Rc<Launcher>) -> Hooks {
        let weak = Rc::downgrade(this);
        let w1 = weak.clone();
        let w2 = weak.clone();
        let w3 = weak.clone();
        let w4 = weak.clone();
        let w5 = weak.clone();
        let w_appearance = weak.clone();
        let w_panels = weak.clone();
        let w6 = weak.clone();
        let w7 = weak.clone();
        let w8 = weak.clone();
        let w9 = weak.clone();
        Hooks {
            use_password: Some(Box::new(move || {
                if let Some(l) = w6.upgrade() {
                    l.use_password();
                }
            })),
            use_company: Some(Box::new(move || {
                if let Some(l) = w7.upgrade() {
                    l.use_company();
                }
            })),
            log_in: Some(Box::new(move || {
                if let Some(l) = weak.upgrade() {
                    l.oidc_log_in();
                }
            })),
            connect_server: Some(Box::new(move |url, kind| {
                if let Some(l) = w1.upgrade() {
                    l.connect(&url, kind);
                }
            })),
            new_project_folder: Some(Box::new(move |folder, license| {
                if let Some(l) = w8.upgrade() {
                    l.create_standalone(folder.into(), license.into());
                }
            })),
            open_project_folder: Some(Box::new(move |folder, license| {
                if let Some(l) = w9.upgrade() {
                    l.open_standalone(folder.into(), license.into());
                }
            })),
            password_log_in: Some(Box::new(move |user, password, remember| {
                if let Some(l) = w2.upgrade() {
                    l.password_log_in(&user, &password, remember);
                }
            })),
            dev_log_in: Some(Box::new(move |user| {
                if let Some(l) = w3.upgrade() {
                    l.dev_log_in(&user);
                }
            })),
            change_server: Some(Box::new(move || {
                if let Some(l) = w4.upgrade() {
                    l.change_server();
                }
            })),
            export_dir_changed: Some(Box::new(move |dir| {
                if let Some(l) = w5.upgrade() {
                    l.settings.borrow_mut().export_dir = dir;
                    l.save_settings();
                }
            })),
            appearance_changed: Some(Box::new(move |index| {
                if let Some(l) = w_appearance.upgrade() {
                    l.settings.borrow_mut().appearance =
                        Appearance::from_index(index).code().into();
                    l.save_settings();
                }
            })),
            panels_resized: Some(Box::new(move |left, right| {
                if let Some(l) = w_panels.upgrade() {
                    let mut s = l.settings.borrow_mut();
                    s.left_panel = left.round() as u32;
                    s.right_panel = right.round() as u32;
                    drop(s);
                    l.save_settings();
                }
            })),
        }
    }

    /// The controller (to apply view models and for tests).
    ///
    /// # Panics
    ///
    /// Never: the controller is set in [`Launcher::new`].
    #[must_use]
    pub fn controller(&self) -> Rc<Controller> {
        Rc::clone(
            self.controller
                .borrow()
                .as_ref()
                .expect("set in Launcher::new"),
        )
    }

    /// Called with every new session.
    pub fn on_session(&self, f: impl Fn(&Session) + 'static) {
        *self.on_session.borrow_mut() = Some(Box::new(f));
    }

    fn save_settings(&self) {
        if let Some(path) = &self.settings_path
            && let Err(e) = self.settings.borrow().save(path)
        {
            tracing::warn!(error = %e, "the settings could not be saved");
        }
    }

    /// Starts: connects to the remembered server, or shows the server
    /// screen.
    pub fn start(self: &Rc<Self>) {
        let (url, transport) = {
            let s = self.settings.borrow();
            (s.server_url.clone(), transport_position(&s.transport))
        };
        if url.is_empty() {
            self.show_server_screen("Enter the address of your AegisTARA server.");
        } else {
            self.connect(&url, transport);
        }
    }

    /// Starts directly with a development user (the `AEGIS_DEV_USER`
    /// variable), skipping the screens.
    pub fn start_dev(self: &Rc<Self>, url: &str, user: &str, admin: bool, kind: TransportKind) {
        *self.current.borrow_mut() = Some(Current {
            url: url.to_owned(),
            kind,
            provider: None,
        });
        self.start_session(Arc::new(AppProvider::Dev(DevLogin::new(user, admin))), user);
    }

    /// Opens a single-user project folder without showing a login screen.
    /// The license is checked before the loopback listener starts.
    pub fn open_standalone(
        self: &Rc<Self>,
        project_folder: std::path::PathBuf,
        license_path: std::path::PathBuf,
    ) {
        self.start_standalone(project_folder, license_path, false);
    }

    /// Creates a project in a folder that does not already contain one.
    pub fn create_standalone(
        self: &Rc<Self>,
        project_folder: std::path::PathBuf,
        license_path: std::path::PathBuf,
    ) {
        self.start_standalone(project_folder, license_path, true);
    }

    fn start_standalone(
        self: &Rc<Self>,
        project_folder: std::path::PathBuf,
        license_path: std::path::PathBuf,
        create: bool,
    ) {
        self.controller().set_session(None);
        // The old server holds the folder lock, so it must be gone before the
        // new one opens (it may be the same folder).
        let old = self.standalone.borrow_mut().take();
        if let Some(w) = self.window.upgrade() {
            w.set_login_busy(true);
            w.set_login_error("".into());
            w.set_connection(0);
            w.set_connection_text("Opening the project folder…".into());
        }
        let events = self.events.clone();
        let wake = Arc::clone(&self.wake);
        self.runtime.spawn(async move {
            if let Some(old) = old {
                let _ = old.stop().await;
            }
            let result = if create {
                crate::standalone::Standalone::create(&project_folder, &license_path).await
            } else {
                crate::standalone::Standalone::open(&project_folder, &license_path).await
            };
            let _ = events.send(Event::StandaloneStarted {
                project_folder,
                license_path,
                result,
            });
            wake();
        });
    }

    /// Stops the embedded server, if one runs, in the background.
    fn stop_standalone(&self) {
        if let Some(old) = self.standalone.borrow_mut().take() {
            self.runtime.spawn(async move {
                let _ = old.stop().await;
            });
        }
    }

    /// Connects the client to a started embedded server and remembers the
    /// folder and license paths. The license was checked before `server`
    /// existed.
    pub fn attach_standalone(
        self: &Rc<Self>,
        server: crate::standalone::Standalone,
        project_folder: std::path::PathBuf,
        license_path: std::path::PathBuf,
    ) {
        {
            let mut s = self.settings.borrow_mut();
            s.standalone_folder = project_folder.display().to_string();
            s.standalone_license = license_path.display().to_string();
        }
        self.save_settings();
        let url = server.url().to_owned();
        *self.standalone.borrow_mut() = Some(server);
        if let Some(w) = self.window.upgrade() {
            w.set_login_busy(false);
            w.set_login_error("".into());
            w.set_standalone_mode(true);
        }
        self.start_dev(&url, "standalone", true, TransportKind::Grpc);
    }

    fn show_server_screen(&self, text: &str) {
        let Some(w) = self.window.upgrade() else {
            return;
        };
        w.set_connection(4);
        w.set_standalone_mode(false);
        w.set_connection_text(text.into());
        w.set_login_busy(false);
        w.set_has_project(false);
    }

    /// Connects to `url`: asks it how to log in.
    pub fn connect(self: &Rc<Self>, url: &str, transport: i32) {
        self.controller().set_session(None);
        self.stop_standalone();
        let url = url.trim().trim_end_matches('/').to_owned();
        let Some(w) = self.window.upgrade() else {
            return;
        };
        if let Err(e) = aegis_client::transport::check_url(&url) {
            self.show_server_screen(&e.to_string());
            return;
        }
        let name = transport_name(transport);
        let kind = TransportKind::parse(name, &url).unwrap_or_else(|_| TransportKind::auto(&url));
        w.set_server_url(url.as_str().into());
        w.set_standalone_mode(false);
        w.set_login_busy(true);
        w.set_login_error("".into());
        w.set_connection(0);
        w.set_connection_text(format!("Connecting to {url}…").into());
        let events = self.events.clone();
        let wake = Arc::clone(&self.wake);
        self.runtime.spawn(async move {
            let result = async { Public::connect(&url, kind).await?.login_options().await }.await;
            let _ = events.send(Event::Options { url, kind, result });
            wake();
        });
        {
            let mut s = self.settings.borrow_mut();
            s.transport = name.to_owned();
        }
    }

    fn on_options(
        self: &Rc<Self>,
        url: String,
        kind: TransportKind,
        result: Result<LoginOptions, ClientError>,
    ) {
        let Some(w) = self.window.upgrade() else {
            return;
        };
        w.set_login_busy(false);
        let options = match result {
            Ok(o) => o,
            Err(e) => {
                let text = match e.code() {
                    Some(ServerCode::Other(c)) if c.to_lowercase().contains("not implemented") => {
                        "This server is too old for this app (it cannot tell how to log in). Update the server.".to_owned()
                    }
                    _ => format!("Could not reach {url}. {}", login_error(&e)),
                };
                self.show_server_screen(&text);
                return;
            }
        };
        {
            let mut s = self.settings.borrow_mut();
            s.server_url.clone_from(&url);
        }
        self.save_settings();
        w.set_server_name(options.server_name.as_str().into());
        let user = self.settings.borrow().username.clone();
        *self.current.borrow_mut() = Some(Current {
            url: url.clone(),
            kind,
            provider: None,
        });
        match options.method {
            LoginMethod::Password => {
                w.set_password_also(false);
                self.begin_password(&w, &url, &user);
            }
            LoginMethod::Oidc {
                issuer,
                client_id,
                resource,
            } => {
                w.set_login_method(0);
                // Offer the password form only while an admin keeps it on.
                w.set_password_also(options.password_enabled);
                let provider = Arc::new(AppProvider::Oidc(OidcProvider::new(OidcSettings {
                    issuer,
                    client_id,
                    resource,
                    account: url.clone(),
                })));
                self.start_session(Arc::clone(&provider), "you");
                // A saved login: try the refresh token before asking.
                let events = self.events.clone();
                let wake = Arc::clone(&self.wake);
                self.runtime.spawn(async move {
                    if let AppProvider::Oidc(oidc) = &*provider {
                        let ok = matches!(oidc.restore().await, Ok(true));
                        let _ = events.send(Event::Restored(ok));
                        wake();
                    }
                });
            }
            LoginMethod::Development => {
                w.set_password_also(false);
                w.set_login_method(2);
                w.set_connection(3);
                w.set_connection_text("Enter a user name.".into());
            }
        }
    }

    /// Shows the password form and starts a session that waits for it.
    fn begin_password(self: &Rc<Self>, w: &AppWindow, url: &str, user: &str) {
        w.set_login_method(1);
        let provider = PasswordProvider::new(secrets());
        if !user.is_empty() {
            provider.restore(url, user);
        }
        let shown = if user.is_empty() { "you" } else { user };
        self.start_session(Arc::new(AppProvider::Password(provider)), shown);
    }

    /// "Use a user name and password" on the company login screen.
    fn use_password(self: &Rc<Self>) {
        let Some(w) = self.window.upgrade() else {
            return;
        };
        let Some(url) = self.current.borrow().as_ref().map(|c| c.url.clone()) else {
            return;
        };
        w.set_login_error("".into());
        let user = self.settings.borrow().username.clone();
        self.begin_password(&w, &url, &user);
    }

    /// "Use company login" on the password form: ask the server again, so
    /// a switch an admin turned off meanwhile is respected.
    fn use_company(self: &Rc<Self>) {
        let Some(w) = self.window.upgrade() else {
            return;
        };
        let Some(url) = self.current.borrow().as_ref().map(|c| c.url.clone()) else {
            return;
        };
        let transport = w.get_transport_index();
        self.connect(&url, transport);
    }

    fn start_session(self: &Rc<Self>, provider: Arc<AppProvider>, user: &str) {
        let (url, kind) = {
            let current = self.current.borrow();
            let Some(c) = current.as_ref() else {
                return;
            };
            (c.url.clone(), c.kind)
        };
        let forget = Arc::clone(&provider);
        let mut config = SessionConfig::new(user).with_transport(kind);
        config.update_url = std::env::var("AEGIS_UPDATE_URL")
            .ok()
            .filter(|u| !u.is_empty())
            .unwrap_or_else(|| self.settings.borrow().update_url.clone());
        if let Some(dir) = std::env::var_os("AEGIS_UPDATE_DIR").filter(|d| !d.is_empty()) {
            config.update_dir = aegis_client::updates::DownloadDir::Folder(dir.into());
        }
        config.on_log_out = Some(LogOutHook(Arc::new(move || {
            if let AppProvider::Password(p) = &*forget {
                p.forget();
            }
        })));
        let _enter = self.runtime.enter();
        let (session, _task) = Session::start(&url, Arc::clone(&provider), config);
        if let Some(c) = self.current.borrow_mut().as_mut() {
            c.provider = Some(provider);
        }
        if let Some(f) = self.on_session.borrow().as_ref() {
            f(&session);
        }
        self.controller().set_session(Some(session));
        self.controller().apply_latest();
    }

    fn password_log_in(self: &Rc<Self>, user: &str, password: &str, remember: bool) {
        let Some(w) = self.window.upgrade() else {
            return;
        };
        let Some((url, kind)) = self
            .current
            .borrow()
            .as_ref()
            .map(|c| (c.url.clone(), c.kind))
        else {
            return;
        };
        w.set_login_busy(true);
        w.set_login_error("".into());
        let (user, password) = (user.trim().to_owned(), password.to_owned());
        let events = self.events.clone();
        let wake = Arc::clone(&self.wake);
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
            wake();
        });
    }

    fn on_logged_in(
        self: &Rc<Self>,
        user: String,
        remember: bool,
        result: Result<PasswordSession, ClientError>,
    ) {
        let Some(w) = self.window.upgrade() else {
            return;
        };
        w.set_login_busy(false);
        match result {
            Ok(session) => {
                w.set_login_password("".into());
                let current = self.current.borrow();
                let Some(c) = current.as_ref() else {
                    return;
                };
                if let Some(provider) = &c.provider
                    && let AppProvider::Password(p) = &**provider
                {
                    p.set(&c.url, &session.user.username, session.token, remember);
                }
                drop(current);
                {
                    let mut s = self.settings.borrow_mut();
                    s.username = user;
                }
                self.save_settings();
                if let Some(session) = self.controller().session() {
                    let _ = session.send(Command::Reconnect);
                }
            }
            Err(e) => {
                if matches!(e.code(), Some(ServerCode::PasswordLoginOff)) {
                    // An admin turned the password form off while this
                    // screen was open: ask the server again, which shows
                    // the company login only.
                    self.use_company();
                    w.set_login_error(
                        "Password login was turned off. Log in with your company account.".into(),
                    );
                } else {
                    w.set_login_error(login_error(&e).into());
                }
            }
        }
    }

    fn dev_log_in(self: &Rc<Self>, user: &str) {
        let user = user.trim();
        if user.is_empty() {
            return;
        }
        self.settings.borrow_mut().username = user.to_owned();
        self.save_settings();
        self.start_session(Arc::new(AppProvider::Dev(DevLogin::new(user, false))), user);
    }

    fn oidc_log_in(self: &Rc<Self>) {
        let provider = self
            .current
            .borrow()
            .as_ref()
            .and_then(|c| c.provider.clone());
        let Some(provider) = provider else {
            return;
        };
        let events = self.events.clone();
        let wake = Arc::clone(&self.wake);
        self.runtime.spawn(async move {
            if let AppProvider::Oidc(oidc) = &*provider {
                let result = oidc.log_in().await.map_err(|e| e.to_string());
                let _ = events.send(Event::OidcLoggedIn(result));
                wake();
            }
        });
    }

    /// Back to the server screen.
    pub fn change_server(self: &Rc<Self>) {
        self.controller().set_session(None);
        self.stop_standalone();
        self.current.borrow_mut().take();
        self.show_server_screen("Enter the address of your AegisTARA server.");
    }

    /// Shuts down the client and, in standalone mode, the embedded server.
    pub async fn shutdown(&self) {
        if let Some(session) = self.controller().session() {
            let _ = session.send(Command::Shutdown);
        }
        let server = self.standalone.borrow_mut().take();
        if let Some(server) = server {
            let _ = server.stop().await;
        }
    }

    /// Handles the answers of network tasks. Call it on the UI thread.
    pub fn pump(self: &Rc<Self>) {
        while let Ok(event) = self.inbox.try_recv() {
            match event {
                Event::Options { url, kind, result } => self.on_options(url, kind, result),
                Event::LoggedIn {
                    user,
                    remember,
                    result,
                } => self.on_logged_in(user, remember, result),
                Event::OidcLoggedIn(result) => match result {
                    Ok(()) => {
                        if let Some(s) = self.controller().session() {
                            let _ = s.send(Command::Reconnect);
                        }
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "login failed");
                        if let Some(w) = self.window.upgrade() {
                            w.set_login_error(format!("The login did not finish: {e}").into());
                        }
                    }
                },
                Event::Restored(true) => {
                    if let Some(s) = self.controller().session() {
                        let _ = s.send(Command::Reconnect);
                    }
                }
                Event::Restored(false) => {}
                Event::StandaloneStarted {
                    project_folder,
                    license_path,
                    result,
                } => match result {
                    Ok(server) => self.attach_standalone(server, project_folder, license_path),
                    Err(e) => {
                        self.show_server_screen(&e);
                        if let Some(w) = self.window.upgrade() {
                            w.set_login_error(e.into());
                        }
                    }
                },
            }
        }
        self.controller().apply_latest();
    }
}

impl Drop for Launcher {
    fn drop(&mut self) {
        let session = self.controller.borrow().as_ref().and_then(|c| c.session());
        if let Some(session) = session {
            let _ = session.send(Command::Shutdown);
        }
    }
}
