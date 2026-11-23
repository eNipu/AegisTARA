//! What a sign-in screen shows: the stage, the login form and the texts. No
//! toolkit, no network; the `SignIn` controller (job 208) fills it.

use crate::connection::{LoginMethod, LoginOptions};
use crate::error::{ClientError, ServerCode};
use crate::i18n::Catalog;
use crate::state::{ConnectionView, ProjectEntry};
use crate::transport::TransportKind;

mod attempt;
mod flow;
pub use attempt::{AuthAttempt, AuthAttempts};
pub use flow::SignIn;

/// Where the sign-in stands: the five states of the Slint shell (codes 4, 0,
/// 1, 2 and 3).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Stage {
    /// No server chosen yet: the address form.
    #[default]
    ChooseServer,
    /// Connecting to the chosen server.
    Connecting,
    /// Connected and signed in.
    Online,
    /// The server cannot be reached; the session keeps retrying.
    Offline,
    /// The server needs a login.
    LoginRequired,
}

/// Which login form the server asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoginForm {
    /// The company login; `password_also` offers the password form too.
    Company {
        /// An admin keeps password login on as well.
        password_also: bool,
    },
    /// User name and password.
    Password,
    /// A user name only, for development servers.
    Development,
}

/// Everything the sign-in screen shows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SignInView {
    /// Where the sign-in stands.
    pub stage: Stage,
    /// The status line, translated.
    pub status: String,
    /// The login form, once the server said which one.
    pub form: Option<LoginForm>,
    /// The server address the user typed.
    pub server_url: String,
    /// The protocol: `"auto"`, `"grpc"` or `"grpc-web"`.
    pub transport: String,
    /// The server's name, shown on the login form.
    pub server_name: String,
    /// The remembered user name of the password login.
    pub user_name: String,
    /// The last error, translated.
    pub error: Option<String>,
    /// A request is running; the buttons wait.
    pub busy: bool,
    /// The projects on the server, once signed in.
    pub projects: Vec<ProjectEntry>,
    /// Who is signed in.
    pub user: String,
}

/// The protocol names the transport picker offers.
pub const TRANSPORTS: [&str; 3] = ["auto", "grpc", "grpc-web"];

/// The protocol for a picker name and server address; an unknown name means
/// `auto`.
#[must_use]
pub fn transport_kind(name: &str, url: &str) -> TransportKind {
    TransportKind::parse(name, url).unwrap_or_else(|_| TransportKind::auto(url))
}

/// The login form for the server's login options.
#[must_use]
pub fn login_form(options: &LoginOptions) -> LoginForm {
    match options.method {
        LoginMethod::Password => LoginForm::Password,
        LoginMethod::Oidc { .. } => LoginForm::Company {
            password_also: options.password_enabled,
        },
        LoginMethod::Development => LoginForm::Development,
    }
}

/// The stage for a connection state.
#[must_use]
pub fn stage_of(connection: &ConnectionView) -> Stage {
    match connection {
        ConnectionView::Connecting => Stage::Connecting,
        ConnectionView::Online { .. } => Stage::Online,
        ConnectionView::Offline { .. } => Stage::Offline,
        ConnectionView::LoginRequired => Stage::LoginRequired,
    }
}

/// The status line for a stage.
#[must_use]
pub fn status_text(
    stage: Stage,
    form: Option<LoginForm>,
    url: &str,
    connection: Option<&ConnectionView>,
    user: &str,
    catalog: &Catalog,
) -> String {
    match stage {
        Stage::ChooseServer => catalog.tr("signin-choose-server"),
        Stage::Connecting => catalog.tr_with("signin-connecting", &[("url", url)]),
        Stage::Online => catalog.tr_with("signin-online", &[("user", user)]),
        Stage::Offline => {
            let reason = match connection {
                Some(ConnectionView::Offline { reason }) => reason.as_str(),
                _ => "",
            };
            catalog.tr_with("signin-offline", &[("reason", reason)])
        }
        Stage::LoginRequired => match form {
            Some(LoginForm::Password) => catalog.tr("signin-login-password"),
            Some(LoginForm::Development) => catalog.tr("signin-login-dev"),
            _ => catalog.tr("signin-login-company"),
        },
    }
}

/// A login error in words for the user.
#[must_use]
pub fn login_error(e: &ClientError, catalog: &Catalog) -> String {
    match e.code() {
        Some(ServerCode::InvalidCredentials) => catalog.tr("signin-error-credentials"),
        Some(ServerCode::Throttled { retry_after_s }) => {
            let seconds = retry_after_s.unwrap_or(60).to_string();
            catalog.tr_with("signin-error-throttled", &[("seconds", &seconds)])
        }
        Some(ServerCode::PasswordLoginOff) => catalog.tr("signin-error-password-off"),
        _ => match e {
            ClientError::Transport(t) => {
                catalog.tr_with("signin-error-unreachable", &[("error", t)])
            }
            other => other.to_string(),
        },
    }
}

/// Why the server's login options could not be read.
#[must_use]
pub fn options_error(url: &str, e: &ClientError, catalog: &Catalog) -> String {
    match e.code() {
        Some(ServerCode::Other(c)) if c.to_lowercase().contains("not implemented") => {
            catalog.tr("signin-error-too-old")
        }
        _ => {
            let error = login_error(e, catalog);
            catalog.tr_with(
                "signin-error-not-reached",
                &[("url", url), ("error", &error)],
            )
        }
    }
}

/// The name this device shows in the server's session list.
#[must_use]
pub fn client_name() -> String {
    let host = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_default();
    if host.is_empty() {
        "AegisTARA desktop".to_owned()
    } else {
        format!("AegisTARA desktop on {host}")
    }
}
