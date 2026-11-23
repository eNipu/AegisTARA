//! A logged-in session on the client: log in, stay logged in, sign requests.
//!
//! [`Session::credentials`] is what the gRPC layer calls before every
//! request. It returns the two headers to send, refreshing the access token
//! first if it is about to expire. Refreshes are single-flight: concurrent
//! requests wait for one refresh instead of each spending (and, with token
//! rotation, invalidating) the refresh token.

use std::sync::Arc;
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

/// Cancellation shared by one login/restore attempt and its owner.
///
/// Cancellation serializes with synchronous token/keychain commits. Once
/// [`cancel`](Self::cancel) returns, that attempt cannot commit or clear a
/// saved login. An earlier commit is not undone; use [`Session::logout`] to
/// forget it. In-flight network/browser work is not interrupted, so callers
/// may also drop its future when it is no longer needed.
#[derive(Clone, Debug)]
pub struct LoginAttempt {
    active: Arc<std::sync::Mutex<bool>>,
}

impl Default for LoginAttempt {
    fn default() -> Self {
        Self::new()
    }
}

impl LoginAttempt {
    /// A new, active attempt. Use a fresh attempt after cancellation.
    #[must_use]
    pub fn new() -> Self {
        Self {
            active: Arc::new(std::sync::Mutex::new(true)),
        }
    }

    /// Cancels every clone, waiting for any synchronous commit to finish.
    pub fn cancel(&self) {
        *self.lock() = false;
    }

    /// Whether this attempt has not been cancelled. This is a snapshot,
    /// not permission to commit; guarded session methods recheck under lock.
    #[must_use]
    pub fn is_active(&self) -> bool {
        *self.lock()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, bool> {
        self.active
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    // The closure is synchronous: this lock must never span an await.
    fn with_active<T>(
        &self,
        action: impl FnOnce() -> Result<T, AuthError>,
    ) -> Result<T, AuthError> {
        let active = self.lock();
        if !*active {
            return Err(AuthError::LoginRequired);
        }
        action()
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
        Self::new_guarded(oidc, vault, clock, &LoginAttempt::new())
    }

    /// Like [`new`](Self::new), but cancelled lazy initialization cannot
    /// create a device key. A key created before cancellation is retained.
    ///
    /// # Errors
    ///
    /// [`AuthError::LoginRequired`] if cancelled, or the errors of `new`.
    pub fn new_guarded(
        oidc: OidcClient<H>,
        vault: Vault<S>,
        clock: Clock,
        attempt: &LoginAttempt,
    ) -> Result<Self, AuthError> {
        attempt.with_active(|| {
            let key = vault.device_key()?;
            Ok(Session {
                oidc,
                vault,
                key,
                tokens: Mutex::new(None),
                clock,
            })
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
        self.login_guarded(browser, timeout, &LoginAttempt::new())
            .await
    }

    /// Logs in only while `attempt` remains active. The refresh token and
    /// in-memory tokens are committed together, serialized with cancellation.
    ///
    /// # Errors
    ///
    /// [`AuthError::LoginRequired`] if cancellation is observed, or any
    /// error of [`login`](Self::login).
    pub async fn login_guarded<B: BrowserLauncher>(
        &self,
        browser: &B,
        timeout: Duration,
        attempt: &LoginAttempt,
    ) -> Result<(), AuthError> {
        attempt.with_active(|| Ok(()))?;
        let loopback = crate::loopback::Loopback::bind().await?;
        let pending = self.oidc.begin(loopback.redirect_uri(), &self.key)?;
        attempt.with_active(|| browser.open(pending.authorization_url()))?;
        let code = loopback.wait_for_code(pending.state(), timeout).await?;
        attempt.with_active(|| Ok(()))?;
        let tokens = self.oidc.exchange(&pending, &code, &self.key).await?;
        self.commit(tokens, attempt).await
    }

    async fn commit(&self, tokens: TokenSet, attempt: &LoginAttempt) -> Result<(), AuthError> {
        // Lock order matters: cancellation must remain possible while waiting
        // for an in-flight credentials refresh to release the async lock.
        let mut current = self.tokens.lock().await;
        attempt.with_active(|| {
            if let Some(refresh) = &tokens.refresh_token {
                self.vault.save_refresh_token(refresh)?;
            }
            *current = Some(tokens);
            Ok(())
        })
    }

    /// Tries to resume a previous login from the saved refresh token.
    /// Returns `false` if there is none or it is no longer valid (the user
    /// must log in again).
    ///
    /// # Errors
    ///
    /// Store and network errors. An `invalid_grant` answer is not an error.
    pub async fn restore(&self) -> Result<bool, AuthError> {
        self.restore_guarded(&LoginAttempt::new()).await
    }

    /// Restores a saved login only while `attempt` is active. Cancellation
    /// also prevents a stale `invalid_grant` response from clearing a newer
    /// login. Owners must cancel old attempts before starting replacements.
    ///
    /// # Errors
    ///
    /// [`AuthError::LoginRequired`] if cancellation is observed, or the
    /// network/store errors of [`restore`](Self::restore).
    pub async fn restore_guarded(&self, attempt: &LoginAttempt) -> Result<bool, AuthError> {
        let Some(refresh) = attempt.with_active(|| self.vault.load_refresh_token())? else {
            return Ok(false);
        };
        match self.oidc.refresh(&refresh, &self.key).await {
            Ok(tokens) => {
                self.commit(tokens, attempt).await?;
                Ok(true)
            }
            Err(AuthError::Idp(code)) if code == "invalid_grant" => {
                let _tokens = self.tokens.lock().await;
                attempt.with_active(|| {
                    self.vault.clear_session()?;
                    Ok(false)
                })
            }
            Err(e) => Err(e),
        }
    }

    /// Forgets the session on this machine.
    ///
    /// Cancel pending login/restore attempts first, then await this method
    /// before allowing a new login using the same vault account. A detached,
    /// delayed logout could otherwise erase that newer login.
    ///
    /// # Errors
    ///
    /// [`AuthError::Store`].
    pub async fn logout(&self) -> Result<(), AuthError> {
        let mut tokens = self.tokens.lock().await;
        *tokens = None;
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
        self.credentials_guarded(method, url, &LoginAttempt::new())
            .await
    }

    /// Signs a request only while the provider remains active. Cancellation
    /// also prevents an older automatic refresh from replacing or clearing a
    /// newer login in the shared vault.
    ///
    /// # Errors
    ///
    /// [`AuthError::LoginRequired`] if cancelled or logged out; otherwise the
    /// refresh, proof and storage errors of [`credentials`](Self::credentials).
    pub async fn credentials_guarded(
        &self,
        method: &str,
        url: &str,
        attempt: &LoginAttempt,
    ) -> Result<Credentials, AuthError> {
        attempt.with_active(|| Ok(()))?;
        let now = (self.clock)();
        let mut guard = self.tokens.lock().await;
        attempt.with_active(|| Ok(()))?;
        let current = guard.as_ref().ok_or(AuthError::LoginRequired)?;
        if current.expires_at <= now.saturating_add(REFRESH_MARGIN_SECS) {
            let refresh = current
                .refresh_token
                .clone()
                .ok_or(AuthError::LoginRequired)?;
            match self.oidc.refresh(&refresh, &self.key).await {
                Ok(fresh) => {
                    attempt.with_active(|| {
                        if let Some(new) = &fresh.refresh_token {
                            self.vault.save_refresh_token(new)?;
                        }
                        *guard = Some(fresh);
                        Ok(())
                    })?;
                }
                Err(AuthError::Idp(code)) if code == "invalid_grant" => {
                    attempt.with_active(|| {
                        *guard = None;
                        self.vault.clear_session()
                    })?;
                    return Err(AuthError::LoginRequired);
                }
                Err(e) => return Err(e),
            }
        }
        attempt.with_active(|| Ok(()))?;
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

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc;

    use super::*;
    use crate::http::HyperClient;
    use crate::oidc::OidcConfig;
    use crate::store::MemorySecretStore;
    use crate::testing::{CLIENT_ID, MockIdp};

    #[test]
    fn cancellation_serializes_with_an_in_progress_synchronous_commit() {
        let attempt = LoginAttempt::new();
        let writes = Arc::new(AtomicUsize::new(0));
        let (entered, entering) = mpsc::channel();
        let (release, released) = mpsc::channel();
        let committing = attempt.clone();
        let committed_writes = writes.clone();
        let commit = std::thread::spawn(move || {
            committing.with_active(|| {
                entered.send(()).unwrap();
                released.recv_timeout(Duration::from_secs(5)).unwrap();
                committed_writes.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
        });
        entering.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(
            attempt.active.try_lock().is_err(),
            "commit holds the cancellation lock"
        );
        let cancelling = attempt.clone();
        let (started, starting) = mpsc::channel();
        let (done, completed) = mpsc::channel();
        let cancel = std::thread::spawn(move || {
            started.send(()).unwrap();
            cancelling.cancel();
            done.send(()).unwrap();
        });
        starting.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(matches!(
            completed.recv_timeout(Duration::from_millis(25)),
            Err(mpsc::RecvTimeoutError::Timeout)
        ));
        release.send(()).unwrap();
        commit.join().unwrap().unwrap();
        cancel.join().unwrap();
        assert!(!attempt.is_active());
        assert_eq!(
            attempt.with_active(|| {
                writes.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }),
            Err(AuthError::LoginRequired)
        );
        assert_eq!(writes.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn waiting_for_the_async_token_lock_does_not_block_cancellation_or_persist() {
        let idp = MockIdp::start().await;
        let store = Arc::new(MemorySecretStore::new());
        let oidc = OidcClient::connect(
            Arc::new(HyperClient::new()),
            OidcConfig::new(idp.issuer(), CLIENT_ID),
            crate::system_clock,
        )
        .await
        .unwrap();
        let session = Arc::new(
            Session::new(oidc, Vault::new(store.clone(), "test"), crate::system_clock).unwrap(),
        );
        let attempt = LoginAttempt::new();
        let tokens_lock = session.tokens.lock().await;
        let committing_session = session.clone();
        let committing_attempt = attempt.clone();
        let (started, starting) = tokio::sync::oneshot::channel();
        let commit = tokio::spawn(async move {
            started.send(()).unwrap();
            committing_session
                .commit(
                    TokenSet {
                        access_token: "access".into(),
                        token_type: "Bearer".into(),
                        refresh_token: Some("refresh".into()),
                        expires_at: u64::MAX,
                    },
                    &committing_attempt,
                )
                .await
        });
        starting.await.unwrap();
        assert!(
            Vault::new(store.clone(), "test")
                .load_refresh_token()
                .unwrap()
                .is_none()
        );
        let cancel = tokio::task::spawn_blocking(move || attempt.cancel());
        let cancelled = tokio::time::timeout(Duration::from_secs(5), cancel).await;
        drop(tokens_lock);
        cancelled
            .expect("cancellation must not wait for the async token lock")
            .unwrap();
        assert_eq!(commit.await.unwrap(), Err(AuthError::LoginRequired));
        assert!(
            Vault::new(store, "test")
                .load_refresh_token()
                .unwrap()
                .is_none()
        );
        assert!(!session.is_logged_in().await);
    }
}
