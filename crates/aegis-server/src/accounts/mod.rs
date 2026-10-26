//! Password login: local user accounts and sessions (job 037,
//! docs/adr/0035-password-login.md).
//!
//! This is the simple login for test servers and small installations
//! without an identity provider (direction section 6, "local accounts").
//! It is **off unless the server is started with `AEGIS_AUTH=password`**;
//! customer deployments use their company login (OIDC) instead, and single
//! sign-on with Entra ID, Google and others stays on the backlog.
//!
//! # Design
//!
//! - [`Accounts`] owns the users and the open sessions **in memory** and
//!   writes every change through to an [`AccountBackend`] before it answers
//!   (nothing is acknowledged before it is durable). Checking a request's
//!   session is one hash map lookup, no database round trip. This relies on
//!   one server process per database, which is the v1 deployment
//!   (direction section 2: no high availability in v1).
//! - A session token is 32 random bytes from the OS generator, sent as
//!   `authorization: Bearer ats_<base64url>`. The server keeps only its
//!   SHA-256 ([`SessionKey`]), so a database dump holds no usable token.
//! - Passwords are Argon2id hashes ([`password`]). Hashing runs on the
//!   blocking pool (R5) behind a small semaphore, so a burst of logins
//!   cannot starve the server (R4).
//! - Repeated failures for one user name slow that name down
//!   ([`Throttle`]): from the fifth failure on, each try must wait twice as
//!   long as the one before, up to 15 minutes.
//! - Every login, logout and account change is an [`AuditEvent`].
//!
//! Lock order (C3): `state` and `throttle` are never held together and never
//! across an `.await` (C2).

pub mod events;
pub mod memory;
pub mod password;
pub mod postgres;
pub mod service;

use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use base64::Engine as _;
use ring::rand::{SecureRandom, SystemRandom};
use sha2::{Digest, Sha256};
use tokio::sync::Semaphore;

pub use events::{EventLog, Retention};
pub use memory::MemoryAccounts;
pub use password::PasswordRule;
pub use postgres::PostgresAccounts;

use crate::auth::{AuthMethod, Principal};
use crate::store::StoreFault;

/// Prefix of every session token, so a leaked token is easy to recognise
/// (secret scanners) and never mistaken for an OIDC access token.
pub const TOKEN_PREFIX: &str = "ats_";
/// Longest user name, in bytes.
pub const MAX_USERNAME_BYTES: usize = 64;
/// Longest display name, in characters.
pub const MAX_DISPLAY_NAME_CHARS: usize = 128;
/// Longest client name kept with a session, in characters.
pub const MAX_CLIENT_NAME_CHARS: usize = 128;
/// Most sessions one `list_sessions` call returns (R3).
pub const MAX_SESSION_LIST: usize = 1000;

pub(crate) fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// The SHA-256 of a session token: what the server stores.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SessionKey(pub [u8; 32]);

impl SessionKey {
    /// The key of a presented token.
    #[must_use]
    pub fn of(token: &str) -> Self {
        SessionKey(Sha256::digest(token.as_bytes()).into())
    }

    /// A short name for the session in lists: the first 8 bytes of the key
    /// as 16 lowercase hex digits. It says nothing about the token (the key
    /// is already a hash of it), and an admin passes it back to revoke the
    /// session.
    #[must_use]
    pub fn id(&self) -> String {
        use std::fmt::Write as _;
        self.0[..8]
            .iter()
            .fold(String::with_capacity(16), |mut out, b| {
                let _ = write!(out, "{b:02x}");
                out
            })
    }
}

/// A user account.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UserRecord {
    /// Login name: lowercase, see [`normalize_username`].
    pub username: String,
    /// Name shown in the app.
    pub display_name: String,
    /// Argon2id PHC string.
    pub password_hash: String,
    /// May manage users, break edit locks, verify audit chains.
    pub admin: bool,
    /// A disabled user cannot log in and has no sessions.
    pub disabled: bool,
    /// Milliseconds since the Unix epoch.
    pub created_ms: u64,
    /// Last successful login, 0 if never.
    pub last_login_ms: u64,
}

/// An open session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionRecord {
    /// SHA-256 of the token.
    pub key: SessionKey,
    /// Whose session.
    pub username: String,
    /// Milliseconds since the Unix epoch.
    pub created_ms: u64,
    /// The session ends at this time (absolute, not sliding).
    pub expires_ms: u64,
    /// The device, as the client named it.
    pub client_name: String,
}

/// What an audit event records.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuditKind {
    /// A successful login.
    LoginOk,
    /// A failed login (wrong name, wrong password, disabled, throttled).
    LoginFailed,
    /// A logout.
    Logout,
    /// A user changed their own password.
    PasswordChanged,
    /// An admin created a user.
    UserCreated,
    /// An admin changed a user.
    UserUpdated,
    /// An admin deleted a user.
    UserDeleted,
    /// A user exported a report or table.
    Export,
    /// An admin deleted a project.
    ProjectDeleted,
    /// Someone created a baseline.
    BaselineCreated,
    /// An admin turned password login on or off (job 045).
    PasswordLoginChanged,
    /// An admin ended someone's session (job 055).
    SessionRevoked,
}

impl AuditKind {
    /// The stored name.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            AuditKind::LoginOk => "login_ok",
            AuditKind::LoginFailed => "login_failed",
            AuditKind::Logout => "logout",
            AuditKind::PasswordChanged => "password_changed",
            AuditKind::UserCreated => "user_created",
            AuditKind::UserUpdated => "user_updated",
            AuditKind::UserDeleted => "user_deleted",
            AuditKind::Export => "export",
            AuditKind::ProjectDeleted => "project_deleted",
            AuditKind::BaselineCreated => "baseline_created",
            AuditKind::PasswordLoginChanged => "password_login_changed",
            AuditKind::SessionRevoked => "session_revoked",
        }
    }
}

/// One entry of the server's event log (logins, exports, admin actions).
/// Never holds a password or a token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuditEvent {
    /// Milliseconds since the Unix epoch.
    pub at_ms: u64,
    /// Who did it (a subject), or the attempted user name for a failed login.
    pub actor: String,
    /// What happened.
    pub kind: AuditKind,
    /// What it was done to: a user name, a project ID, or empty.
    pub target: String,
    /// Short machine-readable detail, for example the export kind.
    pub detail: String,
}

/// Where accounts are stored. Enum dispatch (standards D11): two backends,
/// known at compile time.
pub enum AccountBackend {
    /// Memory only: lost on restart. Tests and development.
    Memory(MemoryAccounts),
    /// PostgreSQL, sharing the project store's pool.
    Postgres(PostgresAccounts),
}

/// Users and sessions as loaded at startup.
#[derive(Debug, Default)]
pub struct LoadedAccounts {
    /// Every user.
    pub users: Vec<UserRecord>,
    /// Every unexpired session.
    pub sessions: Vec<SessionRecord>,
    /// The stored password login switch, `None` if an admin never set it.
    pub password_login: Option<bool>,
}

macro_rules! dispatch {
    ($self:ident, $b:ident => $e:expr) => {
        match $self {
            AccountBackend::Memory($b) => $e,
            AccountBackend::Postgres($b) => $e,
        }
    };
}

impl AccountBackend {
    /// Loads users and the sessions that end after `now_ms`.
    ///
    /// # Errors
    ///
    /// [`StoreFault`] from the backend.
    pub async fn load(&self, now_ms: u64) -> Result<LoadedAccounts, StoreFault> {
        dispatch!(self, b => b.load(now_ms).await)
    }

    /// Inserts or replaces a user.
    ///
    /// # Errors
    ///
    /// [`StoreFault`] from the backend.
    pub async fn put_user(&self, user: &UserRecord) -> Result<(), StoreFault> {
        dispatch!(self, b => b.put_user(user).await)
    }

    /// Deletes a user and their sessions.
    ///
    /// # Errors
    ///
    /// [`StoreFault`] from the backend.
    pub async fn delete_user(&self, username: &str) -> Result<(), StoreFault> {
        dispatch!(self, b => b.delete_user(username).await)
    }

    /// Stores a new session.
    ///
    /// # Errors
    ///
    /// [`StoreFault`] from the backend.
    pub async fn put_session(&self, session: &SessionRecord) -> Result<(), StoreFault> {
        dispatch!(self, b => b.put_session(session).await)
    }

    /// Deletes sessions by key.
    ///
    /// # Errors
    ///
    /// [`StoreFault`] from the backend.
    pub async fn delete_sessions(&self, keys: &[SessionKey]) -> Result<(), StoreFault> {
        dispatch!(self, b => b.delete_sessions(keys).await)
    }

    /// Stores the password login switch (job 045).
    ///
    /// # Errors
    ///
    /// [`StoreFault`] from the backend.
    pub async fn put_password_login(&self, enabled: bool) -> Result<(), StoreFault> {
        dispatch!(self, b => b.put_password_login(enabled).await)
    }

    /// Appends an audit event.
    ///
    /// # Errors
    ///
    /// [`StoreFault`] from the backend.
    pub async fn record(&self, event: &AuditEvent) -> Result<(), StoreFault> {
        dispatch!(self, b => b.record(event).await)
    }

    /// The newest audit events, newest first, at most `limit`.
    ///
    /// # Errors
    ///
    /// [`StoreFault`] from the backend.
    pub async fn recent_events(&self, limit: usize) -> Result<Vec<AuditEvent>, StoreFault> {
        dispatch!(self, b => b.recent_events(limit).await)
    }

    /// Removes events older than `cutoff_ms`, then the oldest ones beyond
    /// `keep`. Returns how many went.
    ///
    /// # Errors
    ///
    /// [`StoreFault`] from the backend.
    pub async fn trim_events(&self, cutoff_ms: u64, keep: usize) -> Result<u64, StoreFault> {
        dispatch!(self, b => b.trim_events(cutoff_ms, keep).await)
    }
}

/// Why an account operation was refused.
#[derive(Debug, thiserror::Error)]
pub enum AccountError {
    /// Wrong user name, wrong password, or a disabled user: one answer for
    /// all three, so a caller learns nothing about which names exist.
    #[error("the user name or password is wrong")]
    InvalidCredentials,
    /// Too many failures for this user name; try again later.
    #[error("too many failed logins; try again in {retry_after_s} seconds")]
    Throttled {
        /// Seconds to wait.
        retry_after_s: u64,
    },
    /// A new password breaks a rule.
    #[error("{0}")]
    Rule(#[from] PasswordRule),
    /// A user name with characters outside the allowed set, or too long.
    #[error("a user name has 1 to 64 characters: letters, digits, '.', '_', '-' and '@'")]
    InvalidUsername,
    /// A display name that is too long.
    #[error("the display name is too long")]
    InvalidDisplayName,
    /// No such user.
    #[error("no such user")]
    UnknownUser,
    /// No open session has this ID (it may have ended already).
    #[error("no such session")]
    UnknownSession,
    /// The user name is taken.
    #[error("that user name is taken")]
    UserExists,
    /// The change would leave no enabled admin.
    #[error("the last administrator cannot be removed, disabled or demoted")]
    LastAdmin,
    /// Password login is switched off on this server.
    #[error("password login is off on this server")]
    PasswordLoginOff,
    /// Password login cannot be turned off: no company login exists.
    #[error("password login cannot be turned off because this server has no company login")]
    NoCompanyLogin,
    /// The server holds the most users or sessions it allows.
    #[error("the account limit is reached")]
    Full,
    /// Too many logins are being checked at once.
    #[error("the server is busy; try again")]
    Busy,
    /// Storage failed. Details are in the server log.
    #[error("storage failure")]
    Store(#[from] StoreFault),
    /// Hashing failed (the random generator), or a task was cancelled.
    #[error("internal error")]
    Internal,
}

/// Normalizes and checks a user name: trimmed, lowercased, 1 to 64 bytes of
/// `a-z 0-9 . _ - @`, starting with a letter or digit.
///
/// # Errors
///
/// [`AccountError::InvalidUsername`].
pub fn normalize_username(raw: &str) -> Result<String, AccountError> {
    let name = raw.trim().to_ascii_lowercase();
    let ok = !name.is_empty()
        && name.len() <= MAX_USERNAME_BYTES
        && name.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'_' | b'-' | b'@')
        })
        && name.as_bytes()[0].is_ascii_alphanumeric();
    if ok {
        Ok(name)
    } else {
        Err(AccountError::InvalidUsername)
    }
}

fn display_name(raw: &str, username: &str) -> Result<String, AccountError> {
    let name = raw.trim();
    if name.chars().count() > MAX_DISPLAY_NAME_CHARS {
        return Err(AccountError::InvalidDisplayName);
    }
    Ok(if name.is_empty() {
        username.to_owned()
    } else {
        name.to_owned()
    })
}

fn bounded(text: &str, chars: usize) -> String {
    text.chars()
        .filter(|c| !c.is_control())
        .take(chars)
        .collect()
}

/// Tunables. [`Default`] holds the production values.
#[derive(Clone, Debug)]
pub struct AccountsConfig {
    /// How long a session lasts (absolute).
    pub session_ttl: Duration,
    /// Most open sessions per user; the oldest is closed when exceeded.
    pub max_sessions_per_user: usize,
    /// Most users.
    pub max_users: usize,
    /// Failures for one name before it is slowed down.
    pub free_failures: u32,
    /// Longest wait the throttle imposes.
    pub max_backoff: Duration,
    /// Password checks running at once.
    pub hashing_permits: usize,
    /// How long a login waits for a hashing permit before it is refused.
    pub hashing_wait: Duration,
    /// Wall clock in milliseconds since the Unix epoch.
    pub clock_ms: fn() -> u64,
    /// The server also has a company login (OIDC), so an admin may turn
    /// password login off (job 045).
    pub company_login: bool,
}

impl Default for AccountsConfig {
    fn default() -> Self {
        AccountsConfig {
            session_ttl: Duration::from_secs(12 * 3600),
            max_sessions_per_user: 10,
            max_users: 10_000,
            free_failures: 5,
            max_backoff: Duration::from_secs(15 * 60),
            hashing_permits: 4,
            hashing_wait: Duration::from_secs(5),
            clock_ms: crate::config::system_clock_ms,
            company_login: false,
        }
    }
}

/// Failure tracking for one user name.
#[derive(Clone, Copy, Debug, Default)]
struct Failures {
    count: u32,
    /// No attempt is checked before this time.
    blocked_until_ms: u64,
}

/// Slows down guessing: per user name, exponential backoff after
/// `free_failures` failures. Bounded (R4): at most [`Throttle::CAPACITY`]
/// names are tracked; when full, the table is cleared (an attacker who
/// floods it only resets everyone's counters to the free allowance).
#[derive(Debug, Default)]
struct Throttle {
    names: HashMap<String, Failures>,
}

impl Throttle {
    const CAPACITY: usize = 10_000;

    fn check(&self, name: &str, now_ms: u64) -> Result<(), u64> {
        match self.names.get(name) {
            Some(f) if f.blocked_until_ms > now_ms => {
                Err((f.blocked_until_ms - now_ms).div_ceil(1000))
            }
            _ => Ok(()),
        }
    }

    fn fail(&mut self, name: &str, now_ms: u64, config: &AccountsConfig) {
        if self.names.len() >= Self::CAPACITY && !self.names.contains_key(name) {
            self.names.clear();
        }
        let f = self.names.entry(name.to_owned()).or_default();
        f.count = f.count.saturating_add(1);
        if f.count >= config.free_failures {
            let doublings = (f.count - config.free_failures).min(20);
            let wait_ms = (1000u64 << doublings)
                .min(u64::try_from(config.max_backoff.as_millis()).unwrap_or(u64::MAX));
            f.blocked_until_ms = now_ms.saturating_add(wait_ms);
        }
    }

    fn succeed(&mut self, name: &str) {
        self.names.remove(name);
    }
}

#[derive(Default)]
struct State {
    /// By user name. A `BTreeMap` because listing order reaches the admin
    /// screen (D7).
    users: BTreeMap<String, UserRecord>,
    sessions: HashMap<SessionKey, SessionRecord>,
}

impl State {
    fn enabled_admins(&self) -> usize {
        self.users
            .values()
            .filter(|u| u.admin && !u.disabled)
            .count()
    }

    fn sessions_of(&self, username: &str) -> Vec<SessionKey> {
        self.sessions
            .values()
            .filter(|s| s.username == username)
            .map(|s| s.key)
            .collect()
    }
}

/// A newly opened session, as the client receives it.
#[derive(Clone, Debug)]
pub struct NewSession {
    /// The bearer token. Shown once; the server keeps only its hash.
    pub token: String,
    /// When it ends.
    pub expires_ms: u64,
    /// The user.
    pub user: UserRecord,
}

/// A change an admin asks for. `None` leaves a field as it is.
#[derive(Clone, Debug, Default)]
pub struct UserUpdate {
    /// New display name.
    pub display_name: Option<String>,
    /// Grant or revoke admin.
    pub admin: Option<bool>,
    /// Disable or enable.
    pub disabled: Option<bool>,
    /// A new password.
    pub password: Option<String>,
}

/// Password accounts: see the module docs.
pub struct Accounts {
    backend: Arc<AccountBackend>,
    config: AccountsConfig,
    state: Mutex<State>,
    throttle: Mutex<Throttle>,
    hashing: Semaphore,
    decoy: String,
    random: SystemRandom,
    /// The admin switch. Read on every login and request, so an atomic.
    password_enabled: AtomicBool,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    // A poisoned lock means a panic while it was held; the maps are still
    // structurally valid, so keep serving rather than failing every login.
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> Result<T, AccountError> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|_| AccountError::Internal)
}

impl Accounts {
    /// Loads accounts from `backend`.
    ///
    /// # Errors
    ///
    /// [`AccountError::Store`] if loading fails.
    pub async fn open(
        backend: AccountBackend,
        config: AccountsConfig,
    ) -> Result<Self, AccountError> {
        let now = (config.clock_ms)();
        let loaded = backend.load(now).await?;
        let password_login = loaded.password_login;
        let decoy = blocking(password::decoy)
            .await?
            .map_err(|_| AccountError::Internal)?;
        let mut state = State::default();
        for user in loaded.users {
            state.users.insert(user.username.clone(), user);
        }
        for session in loaded.sessions {
            if state.users.contains_key(&session.username) {
                state.sessions.insert(session.key, session);
            }
        }
        Ok(Accounts {
            backend: Arc::new(backend),
            hashing: Semaphore::new(config.hashing_permits.max(1)),
            config,
            state: Mutex::new(state),
            throttle: Mutex::new(Throttle::default()),
            decoy,
            random: SystemRandom::new(),
            password_enabled: AtomicBool::new(password_login.unwrap_or(true)),
        })
    }

    fn now(&self) -> u64 {
        (self.config.clock_ms)()
    }

    /// The accounts clock, in milliseconds since the Unix epoch.
    #[must_use]
    pub fn now_ms(&self) -> u64 {
        self.now()
    }

    /// Writes an audit event. A failure is logged and does not fail the
    /// caller's action, which has already been stored.
    pub async fn audit(&self, actor: &str, kind: AuditKind, target: &str, detail: &str) {
        self.event_log().record(actor, kind, target, detail).await;
    }

    /// The event log these accounts write to. It shares the backend, so the
    /// server can read, trim and extend it without the user table.
    #[must_use]
    pub fn event_log(&self) -> EventLog {
        EventLog::new(Arc::clone(&self.backend), self.config.clock_ms)
    }

    /// The newest audit events, newest first.
    ///
    /// # Errors
    ///
    /// [`AccountError::Store`].
    pub async fn recent_events(&self, limit: usize) -> Result<Vec<AuditEvent>, AccountError> {
        Ok(self.event_log().recent(limit).await?)
    }

    /// Number of users.
    #[must_use]
    pub fn user_count(&self) -> usize {
        lock(&self.state).users.len()
    }

    /// Creates the first admin if there is no user at all. Does nothing
    /// otherwise, so a restart with the same settings never resets a
    /// password that was changed in the app.
    ///
    /// # Errors
    ///
    /// As [`create_user`](Self::create_user).
    pub async fn bootstrap_admin(
        &self,
        username: &str,
        password: &str,
    ) -> Result<bool, AccountError> {
        if self.user_count() > 0 {
            return Ok(false);
        }
        self.create_user("bootstrap", username, "", password, true)
            .await?;
        tracing::info!("created the first administrator account");
        Ok(true)
    }

    async fn hash_new(&self, username: &str, password: &str) -> Result<String, AccountError> {
        password::check_new(password, username)?;
        let _permit = self.permit().await?;
        let password = password.to_owned();
        blocking(move || password::hash(&password))
            .await?
            .map_err(|_| AccountError::Internal)
    }

    async fn permit(&self) -> Result<tokio::sync::SemaphorePermit<'_>, AccountError> {
        tokio::time::timeout(self.config.hashing_wait, self.hashing.acquire())
            .await
            .map_err(|_| AccountError::Busy)?
            .map_err(|_| AccountError::Internal)
    }

    async fn verify(&self, password: &str, hash: String) -> Result<bool, AccountError> {
        let _permit = self.permit().await?;
        let password = password.to_owned();
        blocking(move || password::verify(&password, &hash)).await
    }

    fn new_token(&self) -> Result<String, AccountError> {
        let mut bytes = [0u8; 32];
        self.random
            .fill(&mut bytes)
            .map_err(|_| AccountError::Internal)?;
        Ok(format!("{TOKEN_PREFIX}{}", b64(&bytes)))
    }

    /// Checks a name and password and opens a session.
    ///
    /// Time: one Argon2id verification (about 25 ms), whether or not the
    /// name exists.
    ///
    /// # Errors
    ///
    /// [`AccountError::InvalidCredentials`] for any wrong input,
    /// [`AccountError::Throttled`], [`AccountError::Busy`], or a store fault.
    pub async fn log_in(
        &self,
        raw_username: &str,
        password: &str,
        client_name: &str,
    ) -> Result<NewSession, AccountError> {
        let now = self.now();
        // A malformed name is answered like a wrong one, after the same work.
        let name = normalize_username(raw_username).unwrap_or_default();
        if !self.password_login_enabled() {
            self.audit(&name, AuditKind::LoginFailed, &name, "password_login_off")
                .await;
            return Err(AccountError::PasswordLoginOff);
        }
        let throttled = lock(&self.throttle).check(&name, now);
        if let Err(retry_after_s) = throttled {
            self.audit(&name, AuditKind::LoginFailed, &name, "throttled")
                .await;
            return Err(AccountError::Throttled { retry_after_s });
        }
        let user = lock(&self.state).users.get(&name).cloned();
        let hash = user
            .as_ref()
            .map_or_else(|| self.decoy.clone(), |u| u.password_hash.clone());
        let matches = self.verify(password, hash).await?;
        let Some(mut user) = user.filter(|u| matches && !u.disabled && !name.is_empty()) else {
            lock(&self.throttle).fail(&name, now, &self.config);
            self.audit(&name, AuditKind::LoginFailed, &name, "").await;
            return Err(AccountError::InvalidCredentials);
        };
        lock(&self.throttle).succeed(&name);

        if password::needs_rehash(&user.password_hash)
            && let Ok(h) = self.hash_new(&name, password).await
        {
            user.password_hash = h;
        }
        user.last_login_ms = now;
        self.backend.put_user(&user).await?;

        let token = self.new_token()?;
        let ttl = u64::try_from(self.config.session_ttl.as_millis()).unwrap_or(u64::MAX);
        let session = SessionRecord {
            key: SessionKey::of(&token),
            username: name.clone(),
            created_ms: now,
            expires_ms: now.saturating_add(ttl),
            client_name: bounded(client_name, MAX_CLIENT_NAME_CHARS),
        };
        self.backend.put_session(&session).await?;
        // Close the oldest sessions past the per-user limit.
        let evicted = {
            let mut state = lock(&self.state);
            // `set_password_login(false)` stores the switch before it clears
            // the sessions under this lock. A login that got past the first
            // check while the switch went off must not leave a session
            // behind: it would come alive again when the switch goes on.
            if !self.password_login_enabled() {
                drop(state);
                self.backend.delete_sessions(&[session.key]).await?;
                return Err(AccountError::PasswordLoginOff);
            }
            state.users.insert(name.clone(), user.clone());
            state.sessions.insert(session.key, session.clone());
            let mut mine: Vec<(u64, SessionKey)> = state
                .sessions
                .values()
                .filter(|s| s.username == name)
                .map(|s| (s.created_ms, s.key))
                .collect();
            mine.sort_unstable();
            let excess = mine.len().saturating_sub(self.config.max_sessions_per_user);
            let evicted: Vec<SessionKey> = mine.iter().take(excess).map(|(_, k)| *k).collect();
            for key in &evicted {
                state.sessions.remove(key);
            }
            evicted
        };
        if !evicted.is_empty() {
            self.backend.delete_sessions(&evicted).await?;
        }
        self.audit(&name, AuditKind::LoginOk, &name, &session.client_name)
            .await;
        Ok(NewSession {
            token,
            expires_ms: session.expires_ms,
            user,
        })
    }

    /// The caller behind a bearer token, if the session is open and the
    /// user is enabled. O(1); no I/O.
    #[must_use]
    pub fn authenticate(&self, token: &str) -> Option<Principal> {
        if !self.password_login_enabled() || !token.starts_with(TOKEN_PREFIX) || token.len() > 128 {
            return None;
        }
        let key = SessionKey::of(token);
        let now = self.now();
        let state = lock(&self.state);
        let session = state.sessions.get(&key).filter(|s| s.expires_ms > now)?;
        let user = state.users.get(&session.username).filter(|u| !u.disabled)?;
        Some(Principal {
            subject: user.username.as_str().into(),
            admin: user.admin,
            method: AuthMethod::Password,
        })
    }

    /// Whether a user name and password are accepted now.
    #[must_use]
    pub fn password_login_enabled(&self) -> bool {
        self.password_enabled.load(Ordering::Acquire)
    }

    /// Whether the server has a company login to fall back on.
    #[must_use]
    pub fn company_login_available(&self) -> bool {
        self.config.company_login
    }

    /// Turns password login on or off (job 045). Turning it off first
    /// stores the switch, then refuses new password requests and ends every
    /// password session. Turning it on changes nothing else.
    ///
    /// # Errors
    ///
    /// [`AccountError::NoCompanyLogin`] when `enabled` is false and no
    /// company login exists (the server would lock everyone out), or a
    /// store fault.
    pub async fn set_password_login(&self, actor: &str, enabled: bool) -> Result<(), AccountError> {
        if !enabled && !self.config.company_login {
            return Err(AccountError::NoCompanyLogin);
        }
        self.backend.put_password_login(enabled).await?;
        self.password_enabled.store(enabled, Ordering::Release);
        if !enabled {
            let keys: Vec<SessionKey> = {
                let mut state = lock(&self.state);
                let keys = state.sessions.keys().copied().collect();
                state.sessions.clear();
                keys
            };
            self.backend.delete_sessions(&keys).await?;
        }
        self.audit(
            actor,
            AuditKind::PasswordLoginChanged,
            "",
            if enabled { "on" } else { "off" },
        )
        .await;
        Ok(())
    }

    /// The display name of a user, if the user exists.
    #[must_use]
    pub fn display_name(&self, username: &str) -> Option<String> {
        lock(&self.state)
            .users
            .get(username)
            .map(|u| u.display_name.clone())
    }

    /// Ends the session of `token`. Unknown tokens are ignored.
    ///
    /// # Errors
    ///
    /// [`AccountError::Store`].
    pub async fn log_out(&self, token: &str) -> Result<(), AccountError> {
        let key = SessionKey::of(token);
        let removed = lock(&self.state).sessions.remove(&key);
        if let Some(session) = removed {
            self.backend.delete_sessions(&[key]).await?;
            self.audit(&session.username, AuditKind::Logout, &session.username, "")
                .await;
        }
        Ok(())
    }

    /// Drops expired sessions from memory and storage. Run it now and then;
    /// expired sessions are refused anyway. O(sessions).
    ///
    /// # Errors
    ///
    /// [`AccountError::Store`].
    pub async fn purge_expired(&self) -> Result<usize, AccountError> {
        let now = self.now();
        let expired: Vec<SessionKey> = {
            let mut state = lock(&self.state);
            let keys: Vec<SessionKey> = state
                .sessions
                .values()
                .filter(|s| s.expires_ms <= now)
                .map(|s| s.key)
                .collect();
            for key in &keys {
                state.sessions.remove(key);
            }
            keys
        };
        if !expired.is_empty() {
            self.backend.delete_sessions(&expired).await?;
        }
        Ok(expired.len())
    }

    /// Changes the caller's own password after checking the current one.
    /// Every other session of the caller ends; `keep` (the caller's current
    /// token) stays open.
    ///
    /// # Errors
    ///
    /// [`AccountError::InvalidCredentials`] if `current` is wrong, a
    /// [`PasswordRule`], or a store fault.
    pub async fn change_password(
        &self,
        username: &str,
        current: &str,
        new: &str,
        keep: Option<&str>,
    ) -> Result<(), AccountError> {
        let user = lock(&self.state)
            .users
            .get(username)
            .cloned()
            .ok_or(AccountError::UnknownUser)?;
        if !self.verify(current, user.password_hash.clone()).await? {
            return Err(AccountError::InvalidCredentials);
        }
        let hash = self.hash_new(username, new).await?;
        let mut user = user;
        user.password_hash = hash;
        self.backend.put_user(&user).await?;
        let keep = keep.map(SessionKey::of);
        let ended = {
            let mut state = lock(&self.state);
            state.users.insert(user.username.clone(), user);
            let ended: Vec<SessionKey> = state
                .sessions_of(username)
                .into_iter()
                .filter(|k| Some(*k) != keep)
                .collect();
            for key in &ended {
                state.sessions.remove(key);
            }
            ended
        };
        if !ended.is_empty() {
            self.backend.delete_sessions(&ended).await?;
        }
        self.audit(username, AuditKind::PasswordChanged, username, "")
            .await;
        Ok(())
    }

    /// Every user, ordered by name, with their open session counts.
    /// O(users + sessions).
    #[must_use]
    pub fn list_users(&self) -> Vec<(UserRecord, u32)> {
        let state = lock(&self.state);
        let now = self.now();
        let mut counts: HashMap<&str, u32> = HashMap::new();
        for s in state.sessions.values().filter(|s| s.expires_ms > now) {
            *counts.entry(s.username.as_str()).or_default() += 1;
        }
        state
            .users
            .values()
            .map(|u| {
                (
                    u.clone(),
                    counts.get(u.username.as_str()).copied().unwrap_or(0),
                )
            })
            .collect()
    }

    /// One user with their open session count.
    #[must_use]
    pub fn user(&self, username: &str) -> Option<(UserRecord, u32)> {
        let state = lock(&self.state);
        let now = self.now();
        let user = state.users.get(username)?.clone();
        let sessions = state
            .sessions
            .values()
            .filter(|s| s.username == username && s.expires_ms > now)
            .count();
        Some((user, u32::try_from(sessions).unwrap_or(u32::MAX)))
    }

    /// Creates a user. `actor` is the admin, for the audit log.
    ///
    /// # Errors
    ///
    /// [`AccountError::InvalidUsername`], [`AccountError::UserExists`],
    /// [`AccountError::Full`], a [`PasswordRule`], or a store fault.
    pub async fn create_user(
        &self,
        actor: &str,
        username: &str,
        display: &str,
        password: &str,
        admin: bool,
    ) -> Result<UserRecord, AccountError> {
        let username = normalize_username(username)?;
        let display = display_name(display, &username)?;
        {
            let state = lock(&self.state);
            if state.users.contains_key(&username) {
                return Err(AccountError::UserExists);
            }
            if state.users.len() >= self.config.max_users {
                return Err(AccountError::Full);
            }
        }
        let hash = self.hash_new(&username, password).await?;
        let user = UserRecord {
            username: username.clone(),
            display_name: display,
            password_hash: hash,
            admin,
            disabled: false,
            created_ms: self.now(),
            last_login_ms: 0,
        };
        {
            // Checked again: another admin may have created it meanwhile.
            let mut state = lock(&self.state);
            if state.users.contains_key(&username) {
                return Err(AccountError::UserExists);
            }
            state.users.insert(username.clone(), user.clone());
        }
        if let Err(e) = self.backend.put_user(&user).await {
            lock(&self.state).users.remove(&username);
            return Err(e.into());
        }
        self.audit(
            actor,
            AuditKind::UserCreated,
            &username,
            if admin { "admin" } else { "" },
        )
        .await;
        Ok(user)
    }

    /// Applies an admin's change to a user. Disabling, or setting a new
    /// password, ends the user's sessions.
    ///
    /// # Errors
    ///
    /// [`AccountError::UnknownUser`], [`AccountError::LastAdmin`], a
    /// [`PasswordRule`], or a store fault.
    pub async fn update_user(
        &self,
        actor: &str,
        username: &str,
        change: UserUpdate,
    ) -> Result<UserRecord, AccountError> {
        let username = normalize_username(username)?;
        let mut user = lock(&self.state)
            .users
            .get(&username)
            .cloned()
            .ok_or(AccountError::UnknownUser)?;
        if let Some(d) = &change.display_name {
            user.display_name = display_name(d, &username)?;
        }
        if let Some(a) = change.admin {
            user.admin = a;
        }
        if let Some(d) = change.disabled {
            user.disabled = d;
        }
        if let Some(p) = &change.password {
            user.password_hash = self.hash_new(&username, p).await?;
        }
        let end_sessions = change.password.is_some() || user.disabled;
        let (ended, before) = {
            let mut state = lock(&self.state);
            let before = state
                .users
                .get(&username)
                .cloned()
                .ok_or(AccountError::UnknownUser)?;
            let was_admin = before.admin && !before.disabled;
            let is_admin = user.admin && !user.disabled;
            if was_admin && !is_admin && state.enabled_admins() <= 1 {
                return Err(AccountError::LastAdmin);
            }
            state.users.insert(username.clone(), user.clone());
            let ended = if end_sessions {
                let keys = state.sessions_of(&username);
                for key in &keys {
                    state.sessions.remove(key);
                }
                keys
            } else {
                Vec::new()
            };
            (ended, before)
        };
        if let Err(e) = self.backend.put_user(&user).await {
            lock(&self.state).users.insert(username.clone(), before);
            return Err(e.into());
        }
        if !ended.is_empty() {
            self.backend.delete_sessions(&ended).await?;
        }
        let mut detail = Vec::new();
        if change.display_name.is_some() {
            detail.push("display_name");
        }
        if let Some(a) = change.admin {
            detail.push(if a { "admin" } else { "not_admin" });
        }
        if let Some(d) = change.disabled {
            detail.push(if d { "disabled" } else { "enabled" });
        }
        if change.password.is_some() {
            detail.push("password");
        }
        self.audit(actor, AuditKind::UserUpdated, &username, &detail.join(","))
            .await;
        Ok(user)
    }

    /// Deletes a user and their sessions.
    ///
    /// # Errors
    ///
    /// [`AccountError::UnknownUser`], [`AccountError::LastAdmin`], or a
    /// store fault.
    pub async fn delete_user(&self, actor: &str, username: &str) -> Result<(), AccountError> {
        let username = normalize_username(username)?;
        let removed = {
            let mut state = lock(&self.state);
            let user = state
                .users
                .get(&username)
                .ok_or(AccountError::UnknownUser)?;
            if user.admin && !user.disabled && state.enabled_admins() <= 1 {
                return Err(AccountError::LastAdmin);
            }
            let keys = state.sessions_of(&username);
            for key in &keys {
                state.sessions.remove(key);
            }
            state.users.remove(&username)
        };
        if let Err(e) = self.backend.delete_user(&username).await {
            if let Some(user) = removed {
                lock(&self.state).users.insert(username, user);
            }
            return Err(e.into());
        }
        self.audit(actor, AuditKind::UserDeleted, &username, "")
            .await;
        Ok(())
    }

    /// The open sessions, newest first, at most [`MAX_SESSION_LIST`]. Time
    /// O(sessions log sessions); no I/O.
    #[must_use]
    pub fn list_sessions(&self) -> Vec<SessionRecord> {
        let now = self.now();
        let mut open: Vec<SessionRecord> = lock(&self.state)
            .sessions
            .values()
            .filter(|s| s.expires_ms > now)
            .cloned()
            .collect();
        open.sort_unstable_by(|a, b| {
            b.created_ms
                .cmp(&a.created_ms)
                .then_with(|| a.key.cmp(&b.key))
        });
        open.truncate(MAX_SESSION_LIST);
        open
    }

    /// Ends one session by its [`SessionKey::id`]. The owner's next request
    /// is refused as unauthenticated. An admin may end their own session
    /// too (it is a log out).
    ///
    /// # Errors
    ///
    /// [`AccountError::UnknownSession`] for an ID that is not 16 hex digits
    /// or names no open session, or a store fault (the session then stays
    /// open).
    pub async fn revoke_session(
        &self,
        actor: &str,
        id: &str,
    ) -> Result<SessionRecord, AccountError> {
        let Some(prefix) = parse_session_id(id) else {
            return Err(AccountError::UnknownSession);
        };
        let removed = {
            let mut state = lock(&self.state);
            // One pass over raw bytes, no allocation: O(open sessions).
            let mut found = state.sessions.keys().filter(|k| k.0[..8] == prefix);
            let key = match (found.next().copied(), found.next()) {
                (Some(key), None) => key,
                // None, or two sessions share the 64-bit prefix (about one
                // chance in 10^10 at 100 000 sessions): end neither.
                _ => return Err(AccountError::UnknownSession),
            };
            state.sessions.remove(&key)
        };
        let Some(session) = removed else {
            return Err(AccountError::UnknownSession);
        };
        if let Err(e) = self.backend.delete_sessions(&[session.key]).await {
            lock(&self.state)
                .sessions
                .insert(session.key, session.clone());
            return Err(e.into());
        }
        self.audit(
            actor,
            AuditKind::SessionRevoked,
            &session.username,
            &session.client_name,
        )
        .await;
        Ok(session)
    }

    /// The backend, for tests.
    #[must_use]
    pub fn backend(&self) -> &AccountBackend {
        &self.backend
    }
}

/// The 8 bytes a [`SessionKey::id`] stands for, or `None` if `id` is not
/// exactly 16 lowercase hex digits.
fn parse_session_id(id: &str) -> Option<[u8; 8]> {
    if id.len() != 16 {
        return None;
    }
    let mut out = [0u8; 8];
    let (pairs, _) = id.as_bytes().as_chunks::<2>();
    for (byte, pair) in out.iter_mut().zip(pairs) {
        let digit = |c: u8| match c {
            b'0'..=b'9' => Some(c - b'0'),
            b'a'..=b'f' => Some(c - b'a' + 10),
            _ => None,
        };
        *byte = digit(pair[0])? << 4 | digit(pair[1])?;
    }
    Some(out)
}

/// Reads `authorization: Bearer <token>` from request headers.
#[must_use]
pub fn bearer(headers: &http::HeaderMap) -> Option<&str> {
    let value = headers.get(http::header::AUTHORIZATION)?.to_str().ok()?;
    let (scheme, token) = value.split_once(' ')?;
    scheme
        .eq_ignore_ascii_case("bearer")
        .then(|| token.trim())
        .filter(|t| !t.is_empty())
}

/// Runs [`Accounts::purge_expired`] every `every` until the returned task is
/// aborted.
#[must_use]
pub fn spawn_purger(accounts: Arc<Accounts>, every: Duration) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(every);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tick.tick().await;
            match accounts.purge_expired().await {
                Ok(0) => {}
                Ok(n) => tracing::debug!(n, "purged expired sessions"),
                Err(e) => tracing::warn!(error = %e, "could not purge expired sessions"),
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    // Per test thread: `#[tokio::test]` polls everything on the test's own
    // thread, and the clock is read only there (never in blocking tasks).
    thread_local! {
        static NOW: Cell<u64> = const { Cell::new(1_000_000) };
    }

    fn clock() -> u64 {
        NOW.with(Cell::get)
    }

    fn advance(ms: u64) {
        NOW.with(|n| n.set(n.get() + ms));
    }

    fn config() -> AccountsConfig {
        AccountsConfig {
            clock_ms: clock,
            max_sessions_per_user: 2,
            ..AccountsConfig::default()
        }
    }

    async fn accounts() -> Accounts {
        Accounts::open(AccountBackend::Memory(MemoryAccounts::new()), config())
            .await
            .expect("open")
    }

    async fn accounts_with_company_login(company_login: bool) -> Accounts {
        let config = AccountsConfig {
            company_login,
            ..config()
        };
        Accounts::open(AccountBackend::Memory(MemoryAccounts::new()), config)
            .await
            .expect("open")
    }

    #[tokio::test]
    async fn sessions_are_listed_newest_first_without_the_expired_and_can_be_ended() {
        let a = accounts().await;
        a.bootstrap_admin("root", "correct horse battery")
            .await
            .expect("admin");
        a.create_user("root", "bob", "", "a long enough password", false)
            .await
            .expect("bob");
        let old = a
            .log_in("bob", "a long enough password", "old laptop")
            .await
            .expect("old");
        advance(1000);
        let new = a
            .log_in("bob", "a long enough password", "new laptop")
            .await
            .expect("new");
        let list = a.list_sessions();
        assert_eq!(
            list.iter()
                .map(|s| s.client_name.as_str())
                .collect::<Vec<_>>(),
            ["new laptop", "old laptop"]
        );
        // The ID is 16 hex digits and is not the token.
        let id = list[0].key.id();
        assert_eq!(id.len(), 16);
        assert!(id.bytes().all(|b| b.is_ascii_hexdigit()));
        assert!(!new.token.contains(&id));

        // Ending a session refuses its token at once and leaves the other.
        let ended = a.revoke_session("root", &id).await.expect("revoke");
        assert_eq!(ended.username, "bob");
        assert!(a.authenticate(&new.token).is_none());
        assert!(a.authenticate(&old.token).is_some());
        assert_eq!(a.list_sessions().len(), 1);
        let events = a.recent_events(5).await.expect("events");
        assert_eq!(events[0].kind, AuditKind::SessionRevoked);
        assert_eq!(events[0].actor, "root");
        assert_eq!(events[0].target, "bob");
        assert_eq!(events[0].detail, "new laptop");

        // Ending it again, a malformed ID and an unknown one are all refused.
        for bad in [
            id.as_str(),
            "",
            "xyz",
            "0123456789abcdeg",
            "0123456789ABCDEF",
            "00000000000000000",
        ] {
            assert!(
                matches!(
                    a.revoke_session("root", bad).await,
                    Err(AccountError::UnknownSession)
                ),
                "{bad:?}"
            );
        }
        // An expired session is no longer listed.
        advance(a.config.session_ttl.as_millis() as u64 + 1);
        assert!(a.list_sessions().is_empty());
    }

    /// Stress: `AEGIS_STRESS_SESSIONS` sessions (default 5000) loaded from a
    /// backend. The list stays capped and ordered, and many concurrent ends
    /// of distinct sessions all succeed exactly once.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn stress_many_sessions_are_capped_in_the_list_and_ended_concurrently() {
        let total: u64 = std::env::var("AEGIS_STRESS_SESSIONS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(5000);
        let backend = AccountBackend::Memory(MemoryAccounts::new());
        for u in 0..50 {
            backend
                .put_user(&UserRecord {
                    username: format!("user{u}"),
                    display_name: String::new(),
                    password_hash: "unused".into(),
                    admin: u == 0,
                    disabled: false,
                    created_ms: 1,
                    last_login_ms: 0,
                })
                .await
                .expect("user");
        }
        for i in 0..total {
            backend
                .put_session(&SessionRecord {
                    key: SessionKey::of(&format!("{TOKEN_PREFIX}token-{i}")),
                    username: format!("user{}", i % 50),
                    created_ms: 1000 + i,
                    expires_ms: u64::MAX / 2,
                    client_name: "stress".into(),
                })
                .await
                .expect("session");
        }
        let a = Arc::new(Accounts::open(backend, config()).await.expect("open"));
        let list = a.list_sessions();
        assert_eq!(list.len(), (total as usize).min(MAX_SESSION_LIST));
        assert!(list.windows(2).all(|w| w[0].created_ms >= w[1].created_ms));
        assert_eq!(list[0].created_ms, 1000 + total - 1, "the newest first");

        let started = std::time::Instant::now();
        let ids: Vec<String> = list.iter().map(|s| s.key.id()).collect();
        let tasks: Vec<_> = ids
            .iter()
            .cloned()
            .map(|id| {
                let a = Arc::clone(&a);
                tokio::spawn(async move { a.revoke_session("user0", &id).await.is_ok() })
            })
            .collect();
        let mut ended = 0;
        for t in tasks {
            ended += usize::from(t.await.expect("task"));
        }
        assert_eq!(ended, ids.len(), "every distinct session ended once");
        // The same IDs again: all gone.
        for id in &ids {
            assert!(a.revoke_session("user0", id).await.is_err());
        }
        eprintln!(
            "session stress: {} sessions loaded, {} ended in {:?}",
            total,
            ids.len(),
            started.elapsed()
        );
        assert_eq!(
            a.list_sessions().len(),
            (total as usize)
                .saturating_sub(ids.len())
                .min(MAX_SESSION_LIST)
        );
    }

    /// A login that is hashing while an admin turns the switch off must not
    /// leave a session behind, or it would come alive when the switch goes on.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_login_racing_the_switch_leaves_no_session() {
        let a = Arc::new(accounts_with_company_login(true).await);
        a.bootstrap_admin("root", "correct horse battery")
            .await
            .expect("admin");
        let names: Vec<String> = (0..8).map(|i| format!("user{i}")).collect();
        for n in &names {
            a.create_user("root", n, "", "a long enough password", false)
                .await
                .expect("user");
        }
        for round in 0..12 {
            let logins: Vec<_> = names
                .iter()
                .map(|n| {
                    let (a, n) = (Arc::clone(&a), n.clone());
                    tokio::spawn(async move {
                        a.log_in(&n, "a long enough password", "test")
                            .await
                            .map(|_| ())
                    })
                })
                .collect();
            // Land in the middle of the hashing.
            tokio::time::sleep(Duration::from_millis(5 + round)).await;
            a.set_password_login("root", false).await.expect("off");
            for l in logins {
                match l.await.expect("task") {
                    Ok(()) | Err(AccountError::PasswordLoginOff) => {}
                    Err(e) => panic!("unexpected: {e}"),
                }
            }
            for n in &names {
                let open = a.user(n).map_or(0, |(_, sessions)| sessions);
                assert_eq!(open, 0, "round {round}: {n} kept a session");
            }
            a.set_password_login("root", true).await.expect("on");
        }
    }

    #[tokio::test]
    async fn password_login_cannot_be_turned_off_without_a_company_login() {
        let a = accounts_with_company_login(false).await;
        a.bootstrap_admin("root", "correct horse battery")
            .await
            .expect("admin");
        assert!(a.password_login_enabled());
        let err = a
            .set_password_login("root", false)
            .await
            .expect_err("refused");
        assert!(matches!(err, AccountError::NoCompanyLogin));
        assert!(a.password_login_enabled());
        // Turning it on is always fine.
        a.set_password_login("root", true).await.expect("on");
    }

    #[tokio::test]
    async fn turning_password_login_off_ends_sessions_and_refuses_logins() {
        let a = accounts_with_company_login(true).await;
        a.bootstrap_admin("root", "correct horse battery")
            .await
            .expect("admin");
        let s = a
            .log_in("root", "correct horse battery", "t")
            .await
            .expect("login");
        assert!(a.authenticate(&s.token).is_some());

        a.set_password_login("boss", false).await.expect("off");
        assert!(!a.password_login_enabled());
        assert!(a.authenticate(&s.token).is_none(), "old session must end");
        let err = a
            .log_in("root", "correct horse battery", "t")
            .await
            .expect_err("refused");
        assert!(matches!(err, AccountError::PasswordLoginOff));

        a.set_password_login("boss", true).await.expect("on");
        let again = a
            .log_in("root", "correct horse battery", "t")
            .await
            .expect("login");
        assert!(a.authenticate(&again.token).is_some());
        // The old token stays dead: sessions are not revived.
        assert!(a.authenticate(&s.token).is_none());

        let events = a.recent_events(20).await.expect("events");
        let changes: Vec<_> = events
            .iter()
            .filter(|e| e.kind == AuditKind::PasswordLoginChanged)
            .map(|e| (e.actor.as_str(), e.detail.as_str()))
            .collect();
        assert_eq!(changes, [("boss", "on"), ("boss", "off")]);
        // A refused login is recorded as a failure with the reason.
        assert!(
            events
                .iter()
                .any(|e| e.kind == AuditKind::LoginFailed && e.detail == "password_login_off")
        );
    }

    #[test]
    fn usernames_are_normalized_and_checked() {
        assert_eq!(
            normalize_username("  Alice.B@x ").ok().as_deref(),
            Some("alice.b@x")
        );
        for bad in ["", " ", ".dot", "a b", "ü", &"a".repeat(65)] {
            assert!(normalize_username(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn bearer_parsing() {
        let mut h = http::HeaderMap::new();
        assert_eq!(bearer(&h), None);
        h.insert(
            http::header::AUTHORIZATION,
            "Bearer ats_x".parse().expect("v"),
        );
        assert_eq!(bearer(&h), Some("ats_x"));
        h.insert(http::header::AUTHORIZATION, "DPoP abc".parse().expect("v"));
        assert_eq!(bearer(&h), None);
    }

    #[test]
    fn throttle_doubles_after_the_free_failures() {
        let c = config();
        let mut t = Throttle::default();
        for _ in 0..4 {
            t.fail("bob", 0, &c);
            assert!(t.check("bob", 0).is_ok());
        }
        t.fail("bob", 0, &c);
        assert_eq!(t.check("bob", 0), Err(1));
        assert!(t.check("bob", 1000).is_ok());
        t.fail("bob", 1000, &c);
        assert_eq!(t.check("bob", 1000), Err(2));
        for _ in 0..40 {
            t.fail("bob", 1000, &c);
        }
        assert_eq!(t.check("bob", 1000), Err(15 * 60));
        t.succeed("bob");
        assert!(t.check("bob", 1000).is_ok());
        assert!(t.check("carol", 0).is_ok());
    }

    #[tokio::test]
    async fn bootstrap_creates_one_admin_once() {
        let a = accounts().await;
        assert!(
            a.bootstrap_admin("root", "a long admin password")
                .await
                .expect("ok")
        );
        assert!(
            !a.bootstrap_admin("root2", "another long password")
                .await
                .expect("ok")
        );
        let users = a.list_users();
        assert_eq!(users.len(), 1);
        assert!(users[0].0.admin);
    }

    #[tokio::test]
    async fn login_session_logout() {
        let a = accounts().await;
        a.create_user("t", "Bob", "Bob B.", "bobs long password", false)
            .await
            .expect("create");
        assert!(matches!(
            a.log_in("bob", "wrong password!", "").await,
            Err(AccountError::InvalidCredentials)
        ));
        assert!(matches!(
            a.log_in("nobody", "bobs long password", "").await,
            Err(AccountError::InvalidCredentials)
        ));
        let s = a
            .log_in(" BOB ", "bobs long password", "laptop")
            .await
            .expect("login");
        assert!(s.token.starts_with(TOKEN_PREFIX));
        let p = a.authenticate(&s.token).expect("principal");
        assert_eq!(
            (&*p.subject, p.admin, p.method),
            ("bob", false, AuthMethod::Password)
        );
        assert!(a.authenticate("ats_forged").is_none());
        a.log_out(&s.token).await.expect("logout");
        assert!(a.authenticate(&s.token).is_none());
        let kinds: Vec<AuditKind> = a
            .recent_events(10)
            .await
            .expect("events")
            .iter()
            .map(|e| e.kind)
            .collect();
        assert_eq!(
            kinds,
            [
                AuditKind::Logout,
                AuditKind::LoginOk,
                AuditKind::LoginFailed,
                AuditKind::LoginFailed,
                AuditKind::UserCreated
            ]
        );
    }

    #[tokio::test]
    async fn sessions_expire_and_are_capped_per_user() {
        let a = accounts().await;
        a.create_user("t", "cap", "", "cap long password", false)
            .await
            .expect("create");
        let first = a.log_in("cap", "cap long password", "").await.expect("1");
        advance(1);
        let second = a.log_in("cap", "cap long password", "").await.expect("2");
        advance(1);
        let third = a.log_in("cap", "cap long password", "").await.expect("3");
        assert!(a.authenticate(&first.token).is_none(), "oldest evicted");
        assert!(a.authenticate(&second.token).is_some());
        assert!(a.authenticate(&third.token).is_some());
        advance(13 * 3600 * 1000);
        assert!(a.authenticate(&third.token).is_none(), "expired");
        assert_eq!(a.purge_expired().await.expect("purge"), 2);
    }

    #[tokio::test]
    async fn disabling_ends_sessions_and_blocks_login() {
        let a = accounts().await;
        a.create_user("t", "root", "", "root long password", true)
            .await
            .expect("root");
        a.create_user("t", "eve", "", "eves long password", false)
            .await
            .expect("eve");
        let s = a
            .log_in("eve", "eves long password", "")
            .await
            .expect("login");
        a.update_user(
            "root",
            "eve",
            UserUpdate {
                disabled: Some(true),
                ..UserUpdate::default()
            },
        )
        .await
        .expect("disable");
        assert!(a.authenticate(&s.token).is_none());
        assert!(matches!(
            a.log_in("eve", "eves long password", "").await,
            Err(AccountError::InvalidCredentials)
        ));
    }

    #[tokio::test]
    async fn the_last_admin_is_protected() {
        let a = accounts().await;
        a.create_user("t", "root", "", "root long password", true)
            .await
            .expect("root");
        let demote = UserUpdate {
            admin: Some(false),
            ..UserUpdate::default()
        };
        assert!(matches!(
            a.update_user("root", "root", demote.clone()).await,
            Err(AccountError::LastAdmin)
        ));
        assert!(matches!(
            a.delete_user("root", "root").await,
            Err(AccountError::LastAdmin)
        ));
        a.create_user("t", "root2", "", "root2 long password", true)
            .await
            .expect("second");
        a.update_user("root2", "root", demote)
            .await
            .expect("now allowed");
        a.delete_user("root2", "root").await.expect("delete");
        assert!(matches!(
            a.create_user("t", "root2", "", "root2 long password", true)
                .await,
            Err(AccountError::UserExists)
        ));
    }

    #[tokio::test]
    async fn changing_the_password_keeps_only_the_current_session() {
        let a = accounts().await;
        a.create_user("t", "dan", "", "dans long password", false)
            .await
            .expect("dan");
        let one = a.log_in("dan", "dans long password", "").await.expect("1");
        let two = a.log_in("dan", "dans long password", "").await.expect("2");
        assert!(matches!(
            a.change_password("dan", "wrong one!!", "dans new password", Some(&one.token))
                .await,
            Err(AccountError::InvalidCredentials)
        ));
        assert!(matches!(
            a.change_password("dan", "dans long password", "short", Some(&one.token))
                .await,
            Err(AccountError::Rule(PasswordRule::TooShort))
        ));
        a.change_password(
            "dan",
            "dans long password",
            "dans new password",
            Some(&one.token),
        )
        .await
        .expect("change");
        assert!(a.authenticate(&one.token).is_some());
        assert!(a.authenticate(&two.token).is_none());
        assert!(a.log_in("dan", "dans new password", "").await.is_ok());
    }

    #[tokio::test]
    async fn repeated_failures_throttle_the_name() {
        let a = accounts().await;
        a.create_user("t", "fay", "", "fays long password", false)
            .await
            .expect("fay");
        for _ in 0..5 {
            let _ = a.log_in("fay", "not the password", "").await;
        }
        assert!(matches!(
            a.log_in("fay", "fays long password", "").await,
            Err(AccountError::Throttled { .. })
        ));
        advance(2000);
        assert!(a.log_in("fay", "fays long password", "").await.is_ok());
    }
}
