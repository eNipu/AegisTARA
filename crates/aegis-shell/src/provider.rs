//! How the shell logs in.
//!
//! Three ways, one trait (`CredentialProvider`). The server says which one
//! it wants (`GetLoginOptions`), so the user only types the server address:
//!
//! - **OIDC** against the company identity provider (the real thing): the
//!   login button starts the browser flow; the device key and refresh token
//!   live in the OS keychain, or in memory if there is none.
//! - **Password** (test servers, `AEGIS_AUTH=password`): user name and
//!   password, checked by the server, which answers with a session token.
//!   With "keep me logged in", the token is kept in the OS keychain under
//!   the server and user name, never in a file.
//! - **Development**: a user name sent as a header to a server started with
//!   `AEGIS_DEV_AUTH=1`. Never in a release configuration.

use std::sync::Arc;
use std::time::Duration;

use aegis_auth::http::HyperClient;
use aegis_auth::layer::CredentialProvider;
use aegis_auth::oidc::{OidcClient, OidcConfig};
use aegis_auth::session::{Session as AuthSession, SystemBrowser};
use aegis_auth::store::{MemorySecretStore, OsKeychain, SecretStore, Vault};
use aegis_auth::{AuthError, Credentials, system_clock};
use aegis_client::connection::{DevLogin, PasswordLogin};
use tokio::sync::OnceCell;

type Secrets = Arc<dyn SecretStore>;

/// Longest the user may take to finish logging in.
const LOGIN_TIMEOUT: Duration = Duration::from_secs(300);

/// Settings of the OIDC login.
#[derive(Clone, Debug)]
pub struct OidcSettings {
    /// The identity provider's issuer URL.
    pub issuer: String,
    /// The public client ID registered for the desktop app.
    pub client_id: String,
    /// The API the access token is for, if the provider needs it.
    pub resource: Option<String>,
    /// Account name in the keychain: user and server, so two servers never
    /// share a device key.
    pub account: String,
}

/// OIDC login, set up lazily: the provider is contacted when the user logs
/// in (or when a saved session is restored), not at startup.
pub struct OidcProvider {
    settings: OidcSettings,
    session: OnceCell<Arc<AuthSession<HyperClient, Secrets>>>,
}

impl OidcProvider {
    /// A provider that has not contacted anyone yet.
    #[must_use]
    pub fn new(settings: OidcSettings) -> Self {
        OidcProvider {
            settings,
            session: OnceCell::new(),
        }
    }

    async fn session(&self) -> Result<&Arc<AuthSession<HyperClient, Secrets>>, AuthError> {
        self.session
            .get_or_try_init(|| async {
                let secrets: Secrets = match OsKeychain::open("AegisTARA") {
                    Ok(keychain) => Arc::new(keychain),
                    Err(e) => {
                        // A machine without a Secret Service still works; the
                        // login just is not remembered.
                        tracing::warn!(error = %e, "no OS keychain; the login will not be remembered");
                        Arc::new(MemorySecretStore::new())
                    }
                };
                let mut config = OidcConfig::new(&self.settings.issuer, &self.settings.client_id);
                config.resource.clone_from(&self.settings.resource);
                let oidc = OidcClient::connect(Arc::new(HyperClient::new()), config, system_clock).await?;
                let vault = Vault::new(secrets, &self.settings.account);
                Ok(Arc::new(AuthSession::new(oidc, vault, system_clock)?))
            })
            .await
    }

    /// Tries to resume a saved login. `true` if the user is logged in.
    ///
    /// # Errors
    ///
    /// Network or keychain errors.
    pub async fn restore(&self) -> Result<bool, AuthError> {
        self.session().await?.restore().await
    }

    /// Runs the browser login.
    ///
    /// # Errors
    ///
    /// Any [`AuthError`] of the flow.
    pub async fn log_in(&self) -> Result<(), AuthError> {
        self.session()
            .await?
            .login(&SystemBrowser, LOGIN_TIMEOUT)
            .await
    }
}

/// Opens the OS keychain, or a memory store on a machine without one.
#[must_use]
pub fn secrets() -> Secrets {
    match OsKeychain::open("AegisTARA") {
        Ok(keychain) => Arc::new(keychain),
        Err(e) => {
            tracing::warn!(error = %e, "no OS keychain; logins will not be remembered");
            Arc::new(MemorySecretStore::new())
        }
    }
}

/// Password login, with the session token optionally kept in the keychain.
pub struct PasswordProvider {
    /// The token sent with every call.
    pub login: PasswordLogin,
    secrets: Secrets,
    /// Keychain entry name: server and user.
    account: std::sync::Mutex<Option<String>>,
}

impl PasswordProvider {
    /// No session yet.
    #[must_use]
    pub fn new(secrets: Secrets) -> Self {
        PasswordProvider {
            login: PasswordLogin::new(),
            secrets,
            account: std::sync::Mutex::new(None),
        }
    }

    fn entry(server: &str, user: &str) -> String {
        format!(
            "password-session|{}|{}",
            server.trim_end_matches('/'),
            user.trim().to_lowercase()
        )
    }

    /// Restores a kept token for `server` and `user`. `true` if one was found.
    pub fn restore(&self, server: &str, user: &str) -> bool {
        let name = Self::entry(server, user);
        let token = self
            .secrets
            .get(&name)
            .ok()
            .flatten()
            .and_then(|b| String::from_utf8(b).ok())
            .filter(|t| t.starts_with("ats_") && t.len() <= 128);
        let found = token.is_some();
        self.login.set(token);
        *self.lock() = Some(name);
        found
    }

    /// Uses a new session token; keeps it in the keychain when `remember`.
    pub fn set(&self, server: &str, user: &str, token: String, remember: bool) {
        let name = Self::entry(server, user);
        if remember {
            if let Err(e) = self.secrets.set(&name, token.as_bytes()) {
                tracing::warn!(error = %e, "the login could not be kept in the keychain");
            }
        } else {
            let _ = self.secrets.delete(&name);
        }
        self.login.set(Some(token));
        *self.lock() = Some(name);
    }

    /// Forgets the token, here and in the keychain.
    pub fn forget(&self) {
        self.login.set(None);
        if let Some(name) = self.lock().take() {
            let _ = self.secrets.delete(&name);
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Option<String>> {
        self.account
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// The credentials the app uses.
pub enum AppProvider {
    /// OIDC with DPoP.
    Oidc(OidcProvider),
    /// User name and password.
    Password(PasswordProvider),
    /// The development login.
    Dev(DevLogin),
}

impl CredentialProvider for AppProvider {
    async fn credentials(&self, method: &str, url: &str) -> Result<Credentials, AuthError> {
        match self {
            AppProvider::Dev(dev) => dev.credentials(method, url).await,
            AppProvider::Password(p) => p.login.credentials(method, url).await,
            AppProvider::Oidc(oidc) => match oidc.session.get() {
                Some(session) => session.credentials(method, url).await,
                None => Err(AuthError::LoginRequired),
            },
        }
    }
}
