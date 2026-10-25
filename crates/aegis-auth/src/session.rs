//! A logged-in session on the client: log in, stay logged in, sign requests.
//!
//! [`Session::credentials`] is what the gRPC layer calls before every
//! request. It returns the two headers to send, refreshing the access token
//! first if it is about to expire. Refreshes are single-flight: concurrent
//! requests wait for one refresh instead of each spending (and, with token
//! rotation, invalidating) the refresh token.

use std::time::Duration;

use tokio::sync::Mutex;

use crate::clock::Clock;
use crate::dpop::DeviceKey;
use crate::error::AuthError;
use crate::http::HttpClient;
use crate::oidc::{OidcClient, TokenSet};
use crate::store::{SecretStore, Vault};

/// Refresh this many seconds before the access token expires.
const REFRESH_MARGIN_SECS: u64 = 30;

/// Opens the system browser. Replaceable, so tests need no browser.
pub trait BrowserLauncher: Send + Sync {
    /// Opens `url`.
    ///
    /// # Errors
    ///
    /// [`AuthError::Http`] if no browser could be started.
    fn open(&self, url: &str) -> Result<(), AuthError>;
}

/// Starts the platform's default browser.
///
/// The URL is passed as one argument to the platform opener and never
/// through a shell. Only `http` and `https` URLs are opened.
pub struct SystemBrowser;

impl BrowserLauncher for SystemBrowser {
    fn open(&self, url: &str) -> Result<(), AuthError> {
        if !(url.starts_with("https://") || url.starts_with("http://"))
            || url.contains(char::is_control)
        {
            return Err(AuthError::Http("refusing to open this URL".to_owned()));
        }
        let status = if cfg!(target_os = "windows") {
            // `start` is a cmd built-in and would re-parse `&` in the URL.
            std::process::Command::new("rundll32")
                .arg("url.dll,FileProtocolHandler")
                .arg(url)
                .status()
        } else if cfg!(target_os = "macos") {
            std::process::Command::new("open").arg(url).status()
        } else {
            std::process::Command::new("xdg-open").arg(url).status()
        };
        match status {
            Ok(s) if s.success() => Ok(()),
            _ => Err(AuthError::Http("could not start the browser".to_owned())),
        }
    }
}

/// The headers one request needs.
#[derive(Clone, PartialEq, Eq)]
pub struct Credentials {
    /// Value of `Authorization`: `DPoP <token>` or `Bearer <token>`.
    pub authorization: String,
    /// Value of the `DPoP` header, for bound tokens.
    pub dpop: Option<String>,
    /// Further headers to send as they are. Empty in production; the
    /// development login uses it for its user header.
    pub extra: Vec<(&'static str, String)>,
}

impl std::fmt::Debug for Credentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Credentials").finish_non_exhaustive()
    }
}

/// The client's login state.
pub struct Session<H, S> {
    oidc: OidcClient<H>,
    vault: Vault<S>,
    key: DeviceKey,
    tokens: Mutex<Option<TokenSet>>,
    clock: Clock,
}

impl<H: HttpClient, S: SecretStore> Session<H, S> {
    /// A session with nobody logged in. Loads (or creates) the device key.
    ///
    /// # Errors
    ///
    /// [`AuthError::Store`] or [`AuthError::BadKey`].
    pub fn new(oidc: OidcClient<H>, vault: Vault<S>, clock: Clock) -> Result<Self, AuthError> {
        let key = vault.device_key()?;
        Ok(Session {
            oidc,
            vault,
            key,
            tokens: Mutex::new(None),
            clock,
        })
    }

    /// The device key thumbprint, as the provider and server see it.
    #[must_use]
    pub fn key_thumbprint(&self) -> &str {
        self.key.thumbprint()
    }

    /// Logs in through the browser (authorization code with PKCE and a
    /// loopback redirect) and remembers the refresh token.
    ///
    /// # Errors
    ///
    /// Any [`AuthError`] of the flow; [`AuthError::Timeout`] if the user
    /// does not finish in `timeout`.
    pub async fn login<B: BrowserLauncher>(
        &self,
        browser: &B,
        timeout: Duration,
    ) -> Result<(), AuthError> {
        let loopback = crate::loopback::Loopback::bind().await?;
        let pending = self.oidc.begin(loopback.redirect_uri(), &self.key)?;
        browser.open(pending.authorization_url())?;
        let code = loopback.wait_for_code(pending.state(), timeout).await?;
        let tokens = self.oidc.exchange(&pending, &code, &self.key).await?;
        if let Some(refresh) = &tokens.refresh_token {
            self.vault.save_refresh_token(refresh)?;
        }
        *self.tokens.lock().await = Some(tokens);
        Ok(())
    }

    /// Tries to resume a previous login from the saved refresh token.
    /// Returns `false` if there is none or it is no longer valid (the user
    /// must log in again).
    ///
    /// # Errors
    ///
    /// Store and network errors. An `invalid_grant` answer is not an error.
    pub async fn restore(&self) -> Result<bool, AuthError> {
        let Some(refresh) = self.vault.load_refresh_token()? else {
            return Ok(false);
        };
        match self.oidc.refresh(&refresh, &self.key).await {
            Ok(tokens) => {
                if let Some(new) = &tokens.refresh_token {
                    self.vault.save_refresh_token(new)?;
                }
                *self.tokens.lock().await = Some(tokens);
                Ok(true)
            }
            Err(AuthError::Idp(code)) if code == "invalid_grant" => {
                self.vault.clear_session()?;
                Ok(false)
            }
            Err(e) => Err(e),
        }
    }

    /// Forgets the session on this machine.
    ///
    /// # Errors
    ///
    /// [`AuthError::Store`].
    pub async fn logout(&self) -> Result<(), AuthError> {
        *self.tokens.lock().await = None;
        self.vault.clear_session()
    }

    /// Whether an access token is held.
    pub async fn is_logged_in(&self) -> bool {
        self.tokens.lock().await.is_some()
    }

    /// The headers for one request: `method` and `url` are what DPoP binds
    /// the proof to.
    ///
    /// # Errors
    ///
    /// [`AuthError::LoginRequired`] if nobody is logged in or the refresh
    /// token was refused; otherwise the refresh errors.
    pub async fn credentials(&self, method: &str, url: &str) -> Result<Credentials, AuthError> {
        let now = (self.clock)();
        let mut guard = self.tokens.lock().await;
        let current = guard.as_ref().ok_or(AuthError::LoginRequired)?;
        if current.expires_at <= now.saturating_add(REFRESH_MARGIN_SECS) {
            let refresh = current
                .refresh_token
                .clone()
                .ok_or(AuthError::LoginRequired)?;
            match self.oidc.refresh(&refresh, &self.key).await {
                Ok(fresh) => {
                    if let Some(new) = &fresh.refresh_token {
                        self.vault.save_refresh_token(new)?;
                    }
                    *guard = Some(fresh);
                }
                Err(AuthError::Idp(code)) if code == "invalid_grant" => {
                    *guard = None;
                    self.vault.clear_session()?;
                    return Err(AuthError::LoginRequired);
                }
                Err(e) => return Err(e),
            }
        }
        let tokens = guard.as_ref().ok_or(AuthError::LoginRequired)?;
        if tokens.token_type == "DPoP" {
            let proof = self
                .key
                .proof(method, url, Some(&tokens.access_token), None, now)?;
            Ok(Credentials {
                authorization: format!("DPoP {}", tokens.access_token),
                dpop: Some(proof),
                extra: Vec::new(),
            })
        } else {
            Ok(Credentials {
                authorization: format!("Bearer {}", tokens.access_token),
                dpop: None,
                extra: Vec::new(),
            })
        }
    }
}
