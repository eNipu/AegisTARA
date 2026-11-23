//! How the desktop apps log in.
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

use crate::connection::{DevLogin, PasswordLogin};
use crate::origin::Origin;
use aegis_auth::http::HyperClient;
use aegis_auth::layer::CredentialProvider;
use aegis_auth::oidc::{OidcClient, OidcConfig};
use aegis_auth::session::{LoginAttempt, Session as AuthSession, SystemBrowser};
#[cfg(feature = "os-keychain")]
use aegis_auth::store::OsKeychain;
use aegis_auth::store::{MemorySecretStore, SecretStore, Vault};
use aegis_auth::{AuthError, Credentials, system_clock};
use tokio::sync::OnceCell;

/// Where remembered logins are kept.
pub type Secrets = Arc<dyn SecretStore>;

/// Longest the user may take to finish logging in.
const LOGIN_TIMEOUT: Duration = Duration::from_secs(300);

/// Settings of the OIDC login.
#[derive(Clone, Debug)]
pub struct OidcSettings {
    /// The API server URL credentials may be sent to. This is independent
    /// of the identity provider's issuer and the OAuth resource/audience.
    pub server_url: String,
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
    lifetime: LoginAttempt,
}

impl OidcProvider {
    /// A provider that has not contacted anyone yet.
    #[must_use]
    pub fn new(settings: OidcSettings) -> Self {
        OidcProvider {
            settings,
            session: OnceCell::new(),
            lifetime: LoginAttempt::new(),
        }
    }

    async fn session(
        &self,
        attempt: &LoginAttempt,
    ) -> Result<&Arc<AuthSession<HyperClient, Secrets>>, AuthError> {
        // Invalid API configuration must not start discovery or touch secrets.
        Origin::parse(&self.settings.server_url)?;
        if !self.lifetime.is_active() || !attempt.is_active() {
            return Err(AuthError::LoginRequired);
        }
        self.session
            .get_or_try_init(|| async {
                #[cfg(feature = "os-keychain")]
                let secrets: Secrets = match OsKeychain::open("AegisTARA") {
                    Ok(keychain) => Arc::new(keychain),
                    Err(e) => {
                        // A machine without a Secret Service still works; the
                        // login just is not remembered.
                        tracing::warn!(error = %e, "no OS keychain; the login will not be remembered");
                        Arc::new(MemorySecretStore::new())
                    }
                };
                #[cfg(not(feature = "os-keychain"))]
                let secrets: Secrets = {
                    tracing::warn!("no OS keychain; the login will not be remembered");
                    Arc::new(MemorySecretStore::new())
                };
                let mut config = OidcConfig::new(&self.settings.issuer, &self.settings.client_id);
                config.resource.clone_from(&self.settings.resource);
                let oidc = OidcClient::connect(Arc::new(HyperClient::new()), config, system_clock).await?;
                let vault = Vault::new(secrets, &self.settings.account);
                Ok(Arc::new(AuthSession::new_guarded(
                    oidc, vault, system_clock, attempt,
                )?))
            })
            .await
    }

    /// Tries to resume a saved login. `true` if the user is logged in.
    ///
    /// # Errors
    ///
    /// Network or keychain errors.
    pub async fn restore(&self) -> Result<bool, AuthError> {
        self.restore_guarded(&LoginAttempt::new()).await
    }

    /// Resumes a saved login unless its owning attempt has been cancelled.
    ///
    /// # Errors
    ///
    /// [`AuthError::LoginRequired`] if cancelled, or network/keychain errors.
    pub async fn restore_guarded(&self, attempt: &LoginAttempt) -> Result<bool, AuthError> {
        self.session(attempt).await?.restore_guarded(attempt).await
    }

    /// Runs the browser login.
    ///
    /// # Errors
    ///
    /// Any [`AuthError`] of the flow.
    pub async fn log_in(&self) -> Result<(), AuthError> {
        self.log_in_guarded(&LoginAttempt::new()).await
    }

    /// Runs the browser login without accepting tokens after cancellation.
    ///
    /// # Errors
    ///
    /// [`AuthError::LoginRequired`] if cancelled, or any login-flow error.
    pub async fn log_in_guarded(&self, attempt: &LoginAttempt) -> Result<(), AuthError> {
        self.session(attempt)
            .await?
            .login_guarded(&SystemBrowser, LOGIN_TIMEOUT, attempt)
            .await
    }

    /// Forgets an initialized session without triggering discovery or login.
    /// Cancel its pending attempts first and await completion before starting
    /// another login/restore for the same vault account. A fire-and-forget
    /// logout hook alone cannot safely order logout against a newer login.
    ///
    /// # Errors
    ///
    /// [`AuthError::Store`] if the saved refresh token could not be removed.
    pub async fn log_out(&self) -> Result<(), AuthError> {
        match self.session.get() {
            Some(session) => session.logout().await,
            None => Ok(()),
        }
    }
}

/// Opens the OS keychain, or a memory store on a machine without one.
#[must_use]
pub fn secrets() -> Secrets {
    #[cfg(feature = "os-keychain")]
    {
        match OsKeychain::open("AegisTARA") {
            Ok(keychain) => Arc::new(keychain),
            Err(e) => {
                tracing::warn!(error = %e, "no OS keychain; logins will not be remembered");
                Arc::new(MemorySecretStore::new())
            }
        }
    }
    #[cfg(not(feature = "os-keychain"))]
    {
        tracing::warn!("no OS keychain; logins will not be remembered");
        Arc::new(MemorySecretStore::new())
    }
}

/// A store in memory only: for tests and for machines without a keychain.
#[must_use]
pub fn memory_secrets() -> Secrets {
    Arc::new(MemorySecretStore::new())
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

    fn entry(server: &str, user: &str) -> Result<String, AuthError> {
        Ok(format!(
            "password-session|{}|{}",
            Origin::parse(server)?.key(),
            user.trim().to_lowercase()
        ))
    }

    fn legacy_entry(server: &str, user: &str) -> String {
        format!(
            "password-session|{}|{}",
            server.trim_end_matches('/'),
            user.trim().to_lowercase()
        )
    }

    /// Restores a kept token for `server` and `user`. `true` if one was found.
    /// Invalid URLs or attempts to change the bound origin return `false`
    /// without reading the keychain or changing the existing login.
    pub fn restore(&self, server: &str, user: &str) -> bool {
        let mut account = self.lock();
        if self.login.bind(server).is_err() {
            return false;
        }
        let Ok(name) = Self::entry(server, user) else {
            return false;
        };
        let legacy = Self::legacy_entry(server, user);
        let mut token = self.secrets.get(&name).ok().flatten();
        let from_legacy = token.is_none() && legacy != name;
        if from_legacy {
            token = self.secrets.get(&legacy).ok().flatten();
        }
        let token = token
            .and_then(|b| String::from_utf8(b).ok())
            .filter(|t| t.starts_with("ats_") && t.len() <= 128);
        let found = token.is_some();
        if let Some(token) = &token
            && legacy != name
        {
            // Migrate only after the canonical write succeeds. If it fails,
            // retain the legacy account so forget() still removes that entry.
            if from_legacy {
                if self.secrets.set(&name, token.as_bytes()).is_ok() {
                    let _ = self.secrets.delete(&legacy);
                } else {
                    if self.login.set(server, Some(token.clone())).is_err() {
                        return false;
                    }
                    *account = Some(legacy);
                    return true;
                }
            } else {
                let _ = self.secrets.delete(&legacy);
            }
        }
        if self.login.set(server, token).is_err() {
            return false;
        }
        *account = Some(name);
        found
    }

    /// Uses a new session token; keeps it in the keychain when `remember`.
    ///
    /// # Errors
    ///
    /// [`AuthError::Http`] for an invalid server URL or a different origin.
    /// Rejected servers cannot change the login or access the keychain.
    pub fn set(
        &self,
        server: &str,
        user: &str,
        token: String,
        remember: bool,
    ) -> Result<(), AuthError> {
        let mut account = self.lock();
        self.login.bind(server)?;
        let name = Self::entry(server, user)?;
        let legacy = Self::legacy_entry(server, user);
        if remember {
            match self.secrets.set(&name, token.as_bytes()) {
                Ok(()) => {
                    if legacy != name {
                        let _ = self.secrets.delete(&legacy);
                    }
                }
                Err(e) => {
                    tracing::warn!(error = %e, "the login could not be kept in the keychain");
                }
            }
        } else {
            let _ = self.secrets.delete(&name);
            if legacy != name {
                let _ = self.secrets.delete(&legacy);
            }
        }
        self.login.set(server, Some(token))?;
        *account = Some(name);
        Ok(())
    }

    /// Forgets the token, here and in the keychain, retaining the bound origin.
    pub fn forget(&self) {
        let mut account = self.lock();
        self.login.clear();
        if let Some(name) = account.take() {
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

impl AppProvider {
    /// Retires this provider before a replacement can use the same account.
    /// Password keychain entries belong to the remembered login, so only the
    /// old in-memory token is cleared; OIDC refresh commits are cancelled.
    pub fn invalidate(&self) {
        match self {
            AppProvider::Oidc(oidc) => oidc.lifetime.cancel(),
            AppProvider::Password(password) => password.login.clear(),
            AppProvider::Dev(_) => {}
        }
    }
}

impl CredentialProvider for AppProvider {
    async fn credentials(&self, method: &str, url: &str) -> Result<Credentials, AuthError> {
        match self {
            AppProvider::Dev(dev) => dev.credentials(method, url).await,
            AppProvider::Password(p) => p.login.credentials(method, url).await,
            AppProvider::Oidc(oidc) => {
                // Check before Session::credentials can refresh a token or
                // generate bearer/DPoP headers, even for an existing session.
                Origin::parse(&oidc.settings.server_url)?.check(url)?;
                match oidc.session.get() {
                    Some(session) => {
                        session
                            .credentials_guarded(method, url, &oidc.lifetime)
                            .await
                    }
                    None => Err(AuthError::LoginRequired),
                }
            }
        }
    }
}
