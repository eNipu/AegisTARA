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

impl Drop for UserRecord {
    fn drop(&mut self) {
        password::zeroize_string(&mut self.password_hash);
    }
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
    /// Old events were removed by the retention trimmer (job 062, SEC-06).
    EventsTrimmed,
}

impl AuditKind {
    /// Every kind, so a backend can read a stored name back. Adding a kind
    /// means adding it here; the test `every_kind_is_listed_and_round_trips`
    /// fails if it is missing.
    pub const ALL: [AuditKind; 13] = [
        AuditKind::LoginOk,
        AuditKind::LoginFailed,
        AuditKind::Logout,
        AuditKind::PasswordChanged,
        AuditKind::UserCreated,
        AuditKind::UserUpdated,
        AuditKind::UserDeleted,
        AuditKind::Export,
        AuditKind::ProjectDeleted,
        AuditKind::BaselineCreated,
        AuditKind::PasswordLoginChanged,
        AuditKind::SessionRevoked,
        AuditKind::EventsTrimmed,
    ];

    /// The kind with this stored name, if there is one.
    #[must_use]
    pub fn from_name(name: &str) -> Option<AuditKind> {
        AuditKind::ALL.into_iter().find(|k| k.as_str() == name)
    }

    /// A number for each kind, `0..ALL.len()`. The match has no wildcard, so
    /// a new kind will not compile until it is numbered here, which the test
    /// then checks against [`AuditKind::ALL`].
    #[cfg(test)]
    fn ordinal(self) -> usize {
        match self {
            AuditKind::LoginOk => 0,
            AuditKind::LoginFailed => 1,
            AuditKind::Logout => 2,
            AuditKind::PasswordChanged => 3,
            AuditKind::UserCreated => 4,
            AuditKind::UserUpdated => 5,
            AuditKind::UserDeleted => 6,
            AuditKind::Export => 7,
            AuditKind::ProjectDeleted => 8,
            AuditKind::BaselineCreated => 9,
            AuditKind::PasswordLoginChanged => 10,
            AuditKind::SessionRevoked => 11,
            AuditKind::EventsTrimmed => 12,
        }
    }

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
            AuditKind::EventsTrimmed => "events_trimmed",
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

    /// Atomically updates `user`, inserts `session`, and deletes `evicted`
    /// sessions (SEC-07).
    ///
    /// # Errors
    ///
    /// [`StoreFault`] from the backend.
    pub async fn put_login_session(
        &self,
        user: &UserRecord,
        session: &SessionRecord,
        evicted: &[SessionKey],
    ) -> Result<(), StoreFault> {
        dispatch!(self, b => b.put_login_session(user, session, evicted).await)
    }

    /// Atomically updates `user` and deletes `keys` sessions (SEC-07).
    ///
    /// # Errors
    ///
    /// [`StoreFault`] from the backend.
    pub async fn put_user_and_delete_sessions(
        &self,
        user: &UserRecord,
        keys: &[SessionKey],
    ) -> Result<(), StoreFault> {
        dispatch!(self, b => b.put_user_and_delete_sessions(user, keys).await)
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

    /// Atomically stores the password login switch and deletes `keys`
    /// sessions (SEC-07).
    ///
    /// # Errors
    ///
    /// [`StoreFault`] from the backend.
    pub async fn put_password_login_and_delete_sessions(
        &self,
        enabled: bool,
        keys: &[SessionKey],
    ) -> Result<(), StoreFault> {
        dispatch!(self, b => b.put_password_login_and_delete_sessions(enabled, keys).await)
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
    /// `keep`, advances the hash-chain tail anchor, and records an
    /// [`AuditKind::EventsTrimmed`] event at `now_ms`. Returns how many went.
    ///
    /// # Errors
    ///
    /// [`StoreFault`] from the backend.
    pub async fn trim_events(
        &self,
        cutoff_ms: u64,
        keep: usize,
        now_ms: u64,
    ) -> Result<u64, StoreFault> {
        dispatch!(self, b => b.trim_events(cutoff_ms, keep, now_ms).await)
    }

    /// Verifies the SHA-256 hash chain over all surviving audit events
    /// (SEC-06).
    ///
    /// # Errors
    ///
    /// [`StoreFault`] from the backend.
    pub async fn verify_events(&self) -> Result<crate::store::audit::ChainReport, StoreFault> {
        dispatch!(self, b => b.verify_events().await)
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
    /// Maximum concurrent `LogIn` checks in flight from one peer address
    /// before additional concurrent requests from that address are refused
    /// in front of the global hashing semaphore (SEC-04).
    pub max_login_in_flight_per_addr: usize,
    /// Failures from one peer address across any usernames before that
    /// address is slowed down in front of the hashing semaphore (SEC-04).
    pub free_addr_failures: u32,
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
            max_login_in_flight_per_addr: 2,
            free_addr_failures: 20,
            clock_ms: crate::config::system_clock_ms,
            company_login: false,
        }
    }
}

/// Failure tracking for one user name or peer address.
#[derive(Clone, Copy, Debug, Default)]
struct Failures {
    count: u32,
    /// No attempt is checked before this time.
    blocked_until_ms: u64,
    /// Most recent failure timestamp, for LRU eviction when at capacity.
    last_failed_ms: u64,
    /// Whether the entry belongs to a known account (never evicted ahead of
    /// unknown-username guesses).
    known: bool,
}

/// Slows down guessing: per user name, exponential backoff after
/// `free_failures` failures. Bounded (R4): at most [`Throttle::CAPACITY`]
/// names are tracked; when full, the single oldest/unblocked entry (preferring
/// unknown usernames over real accounts) is evicted so a flood of unknown
/// usernames never wipes active lockouts (SEC-01).
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

    #[cfg(test)]
    fn fail(&mut self, name: &str, now_ms: u64, config: &AccountsConfig) {
        self.fail_with(name, false, now_ms, config);
    }

    fn fail_with(&mut self, name: &str, known: bool, now_ms: u64, config: &AccountsConfig) {
        if self.names.len() >= Self::CAPACITY
            && !self.names.contains_key(name)
            && let Some(evict) = self
                .names
                .iter()
                .min_by_key(|(k, f)| {
                    (
                        f.known,
                        f.blocked_until_ms > now_ms,
                        f.blocked_until_ms,
                        f.last_failed_ms,
                        f.count,
                        *k,
                    )
                })
                .map(|(k, _)| k.clone())
        {
            self.names.remove(&evict);
        }
        let f = self.names.entry(name.to_owned()).or_default();
        f.count = f.count.saturating_add(1);
        f.last_failed_ms = now_ms;
        f.known |= known;
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

/// Per-address concurrency and failure limiter in front of the Argon2id
/// semaphore for unauthenticated `LogIn` calls (SEC-04).
#[derive(Clone, Copy, Debug, Default)]
struct AddrState {
    in_flight: usize,
    failures: Failures,
}

#[derive(Debug, Default)]
struct AddrThrottle {
    addrs: HashMap<std::net::IpAddr, AddrState>,
}

impl AddrThrottle {
    const CAPACITY: usize = 10_000;

    fn acquire(
        &mut self,
        ip: std::net::IpAddr,
        now_ms: u64,
        config: &AccountsConfig,
    ) -> Result<(), AccountError> {
        if let Some(st) = self.addrs.get(&ip) {
            if st.failures.blocked_until_ms > now_ms {
                let retry_after_s = (st.failures.blocked_until_ms - now_ms).div_ceil(1000);
                return Err(AccountError::Throttled { retry_after_s });
            }
            if st.in_flight >= config.max_login_in_flight_per_addr.max(1) {
                return Err(AccountError::Busy);
            }
        }
        if self.addrs.len() >= Self::CAPACITY
            && !self.addrs.contains_key(&ip)
            && let Some(evict) = self
                .addrs
                .iter()
                .filter(|(_, st)| st.in_flight == 0)
                .min_by_key(|(addr, st)| {
                    (
                        st.failures.blocked_until_ms > now_ms,
                        st.failures.blocked_until_ms,
                        st.failures.last_failed_ms,
                        st.failures.count,
                        **addr,
                    )
                })
                .map(|(addr, _)| *addr)
        {
            self.addrs.remove(&evict);
        }
        if self.addrs.len() >= Self::CAPACITY && !self.addrs.contains_key(&ip) {
            return Err(AccountError::Busy);
        }
        let st = self.addrs.entry(ip).or_default();
        st.in_flight = st.in_flight.saturating_add(1);
        Ok(())
    }

    fn release(&mut self, ip: std::net::IpAddr) {
        if let Some(st) = self.addrs.get_mut(&ip) {
            st.in_flight = st.in_flight.saturating_sub(1);
            if st.in_flight == 0 && st.failures.count == 0 {
                self.addrs.remove(&ip);
            }
        }
    }

    fn fail(&mut self, ip: std::net::IpAddr, now_ms: u64, config: &AccountsConfig) {
        if let Some(st) = self.addrs.get_mut(&ip) {
            let f = &mut st.failures;
            f.count = f.count.saturating_add(1);
            f.last_failed_ms = now_ms;
            let free = config.free_addr_failures.max(1);
            if f.count >= free {
                let doublings = (f.count - free).min(20);
                let wait_ms = (1000u64 << doublings)
                    .min(u64::try_from(config.max_backoff.as_millis()).unwrap_or(u64::MAX));
                f.blocked_until_ms = now_ms.saturating_add(wait_ms);
            }
        }
    }

    fn succeed(&mut self, ip: std::net::IpAddr) {
        if let Some(st) = self.addrs.get_mut(&ip) {
            st.failures = Failures::default();
        }
    }
}

struct AddrInFlightGuard<'a> {
    throttle: &'a Mutex<AddrThrottle>,
    peer: Option<std::net::IpAddr>,
}

impl Drop for AddrInFlightGuard<'_> {
    fn drop(&mut self) {
        if let Some(ip) = self.peer {
            lock(self.throttle).release(ip);
        }
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

impl Drop for NewSession {
    fn drop(&mut self) {
        password::zeroize_string(&mut self.token);
    }
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
    events: EventLog,
    config: AccountsConfig,
    state: Mutex<State>,
    throttle: Mutex<Throttle>,
    addr_throttle: Mutex<AddrThrottle>,
    hashing: Semaphore,
    auth_hashing: Semaphore,
    decoy: String,
    random: SystemRandom,
    /// The admin switch. Read on every login and request, so an atomic.
    password_enabled: AtomicBool,
}

impl Drop for Accounts {
    fn drop(&mut self) {
        password::zeroize_string(&mut self.decoy);
    }
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
        let backend = Arc::new(backend);
        let events = EventLog::new(Arc::clone(&backend), config.clock_ms);
        Ok(Accounts {
            backend,
            events,
            hashing: Semaphore::new(config.hashing_permits.max(1)),
            auth_hashing: Semaphore::new(config.hashing_permits.max(1)),
            config,
            state: Mutex::new(state),
            throttle: Mutex::new(Throttle::default()),
            addr_throttle: Mutex::new(AddrThrottle::default()),
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

    /// Writes an audit event. A failure is logged and queued for retry so
    /// events are never silently dropped (SEC-06).
    pub async fn audit(&self, actor: &str, kind: AuditKind, target: &str, detail: &str) {
        self.events.record(actor, kind, target, detail).await;
    }

    /// The event log these accounts write to. It shares the backend and
    /// retry queue, so the server can read, trim and extend it without the
    /// user table.
    #[must_use]
    pub fn event_log(&self) -> EventLog {
        self.events.clone()
    }

    /// The newest audit events, newest first.
    ///
    /// # Errors
    ///
    /// [`AccountError::Store`].
    pub async fn recent_events(&self, limit: usize) -> Result<Vec<AuditEvent>, AccountError> {
        Ok(self.events.recent(limit).await?)
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
        let _permit = self.auth_permit().await?;
        let mut password = password.to_owned();
        blocking(move || {
            let res = password::hash(&password);
            password::zeroize_string(&mut password);
            res
        })
        .await?
        .map_err(|_| AccountError::Internal)
    }

    async fn permit(&self) -> Result<tokio::sync::SemaphorePermit<'_>, AccountError> {
        tokio::time::timeout(self.config.hashing_wait, self.hashing.acquire())
            .await
            .map_err(|_| AccountError::Busy)?
            .map_err(|_| AccountError::Internal)
    }

    async fn auth_permit(&self) -> Result<tokio::sync::SemaphorePermit<'_>, AccountError> {
        tokio::time::timeout(self.config.hashing_wait, self.auth_hashing.acquire())
            .await
            .map_err(|_| AccountError::Busy)?
            .map_err(|_| AccountError::Internal)
    }

    async fn verify(&self, password: &str, mut hash: String) -> Result<bool, AccountError> {
        let _permit = self.permit().await?;
        let mut password = password.to_owned();
        blocking(move || {
            let ok = password::verify(&password, &hash);
            password::zeroize_string(&mut password);
            password::zeroize_string(&mut hash);
            ok
        })
        .await
    }

    async fn verify_auth(&self, password: &str, mut hash: String) -> Result<bool, AccountError> {
        let _permit = self.auth_permit().await?;
        let mut password = password.to_owned();
        blocking(move || {
            let ok = password::verify(&password, &hash);
            password::zeroize_string(&mut password);
            password::zeroize_string(&mut hash);
            ok
        })
        .await
    }

    fn new_token(&self) -> Result<String, AccountError> {
        let mut bytes = [0u8; 32];
        self.random
            .fill(&mut bytes)
            .map_err(|_| AccountError::Internal)?;
        let token = format!("{TOKEN_PREFIX}{}", b64(&bytes));
        password::zeroize_bytes(&mut bytes);
        Ok(token)
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
        self.log_in_from(raw_username, password, client_name, None)
            .await
    }

    /// Like [`log_in`](Self::log_in), enforcing the per-address concurrency
    /// and failure limits in front of the Argon2id semaphore when `peer` is
    /// known (SEC-04).
    ///
    /// # Errors
    ///
    /// As [`log_in`](Self::log_in).
    pub async fn log_in_from(
        &self,
        raw_username: &str,
        password: &str,
        client_name: &str,
        peer: Option<std::net::IpAddr>,
    ) -> Result<NewSession, AccountError> {
        let now = self.now();
        if !self.password_login_enabled() {
            let label = bounded(raw_username.trim(), MAX_USERNAME_BYTES);
            self.audit(&label, AuditKind::LoginFailed, &label, "password_login_off")
                .await;
            return Err(AccountError::PasswordLoginOff);
        }
        if let Some(ip) = peer {
            let acquired = lock(&self.addr_throttle).acquire(ip, now, &self.config);
            if let Err(err) = acquired {
                if matches!(err, AccountError::Throttled { .. }) {
                    let label = bounded(raw_username.trim(), MAX_USERNAME_BYTES);
                    self.audit(&label, AuditKind::LoginFailed, &label, "addr_throttled")
                        .await;
                }
                return Err(err);
            }
        }
        let _addr_guard = AddrInFlightGuard {
            throttle: &self.addr_throttle,
            peer,
        };
        // Reject syntactically invalid usernames before touching the throttle
        // table or Argon2id permits so malformed names never share an empty
        // throttle bucket or consume hashing permits (SEC-01).
        let Ok(name) = normalize_username(raw_username) else {
            if let Some(ip) = peer {
                lock(&self.addr_throttle).fail(ip, now, &self.config);
            }
            let label = bounded(raw_username.trim(), MAX_USERNAME_BYTES);
            self.audit(&label, AuditKind::LoginFailed, &label, "invalid_username")
                .await;
            return Err(AccountError::InvalidCredentials);
        };
        let throttled = lock(&self.throttle).check(&name, now);
        if let Err(retry_after_s) = throttled {
            self.audit(&name, AuditKind::LoginFailed, &name, "throttled")
                .await;
            return Err(AccountError::Throttled { retry_after_s });
        }
        let user = lock(&self.state).users.get(&name).cloned();
        let known = user.is_some();
        let hash = user
            .as_ref()
            .map_or_else(|| self.decoy.clone(), |u| u.password_hash.clone());
        let matches = self.verify(password, hash).await?;
        let Some(mut user) = user.filter(|u| matches && !u.disabled && !name.is_empty()) else {
            lock(&self.throttle).fail_with(&name, known, now, &self.config);
            if let Some(ip) = peer {
                lock(&self.addr_throttle).fail(ip, now, &self.config);
            }
            self.audit(&name, AuditKind::LoginFailed, &name, "").await;
            return Err(AccountError::InvalidCredentials);
        };
        lock(&self.throttle).succeed(&name);
        if let Some(ip) = peer {
            lock(&self.addr_throttle).succeed(ip);
        }

        if password::needs_rehash(&user.password_hash)
            && let Ok(h) = self.hash_new(&name, password).await
        {
            user.password_hash = h;
        }
        user.last_login_ms = now;

        let token = self.new_token()?;
        let ttl = u64::try_from(self.config.session_ttl.as_millis()).unwrap_or(u64::MAX);
        let session = SessionRecord {
            key: SessionKey::of(&token),
            username: name.clone(),
            created_ms: now,
            expires_ms: now.saturating_add(ttl),
            client_name: bounded(client_name, MAX_CLIENT_NAME_CHARS),
        };
        // Update in-memory state and compute sessions past the per-user limit,
        // then commit user + session + evictions atomically to the backend
        // with full in-memory rollback on failure (SEC-07).
        let (before_user, evicted_keys, evicted_records) = {
            let mut state = lock(&self.state);
            if !self.password_login_enabled() {
                return Err(AccountError::PasswordLoginOff);
            }
            let before_user = state.users.insert(name.clone(), user.clone());
            state.sessions.insert(session.key, session.clone());
            let mut mine: Vec<(u64, SessionKey)> = state
                .sessions
                .values()
                .filter(|s| s.username == name)
                .map(|s| (s.created_ms, s.key))
                .collect();
            mine.sort_unstable();
            let excess = mine.len().saturating_sub(self.config.max_sessions_per_user);
            let evicted_keys: Vec<SessionKey> = mine.iter().take(excess).map(|(_, k)| *k).collect();
            let mut evicted_records = Vec::with_capacity(evicted_keys.len());
            for key in &evicted_keys {
                if let Some(rec) = state.sessions.remove(key) {
                    evicted_records.push(rec);
                }
            }
            (before_user, evicted_keys, evicted_records)
        };
        if let Err(e) = self
            .backend
            .put_login_session(&user, &session, &evicted_keys)
            .await
        {
            let mut state = lock(&self.state);
            if let Some(prev) = before_user {
                state.users.insert(name.clone(), prev);
            }
            state.sessions.remove(&session.key);
            for rec in evicted_records {
                state.sessions.insert(rec.key, rec);
            }
            return Err(e.into());
        }
        // `set_password_login(false)` stores the switch and clears sessions.
        // A login that finished its backend transaction while the switch went
        // off must not leave a session behind.
        if !self.password_login_enabled() {
            lock(&self.state).sessions.remove(&session.key);
            self.backend.delete_sessions(&[session.key]).await?;
            return Err(AccountError::PasswordLoginOff);
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
        if enabled {
            self.backend.put_password_login(true).await?;
            self.password_enabled.store(true, Ordering::Release);
        } else {
            let (prev_enabled, keys, removed_sessions) = {
                let mut state = lock(&self.state);
                let prev_enabled = self.password_enabled.swap(false, Ordering::AcqRel);
                let removed_sessions: Vec<SessionRecord> =
                    state.sessions.values().cloned().collect();
                let keys: Vec<SessionKey> = removed_sessions.iter().map(|s| s.key).collect();
                state.sessions.clear();
                (prev_enabled, keys, removed_sessions)
            };
            if let Err(e) = self
                .backend
                .put_password_login_and_delete_sessions(false, &keys)
                .await
            {
                let mut state = lock(&self.state);
                for rec in removed_sessions {
                    state.sessions.insert(rec.key, rec);
                }
                self.password_enabled.store(prev_enabled, Ordering::Release);
                return Err(e.into());
            }
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
            if let Err(e) = self.backend.delete_sessions(&[key]).await {
                lock(&self.state).sessions.insert(key, session);
                return Err(e.into());
            }
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
        let expired: Vec<SessionKey> = lock(&self.state)
            .sessions
            .values()
            .filter(|s| s.expires_ms <= now)
            .map(|s| s.key)
            .collect();
        if !expired.is_empty() {
            self.backend.delete_sessions(&expired).await?;
            let mut state = lock(&self.state);
            for key in &expired {
                state.sessions.remove(key);
            }
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
        if !self
            .verify_auth(current, user.password_hash.clone())
            .await?
        {
            return Err(AccountError::InvalidCredentials);
        }
        let hash = self.hash_new(username, new).await?;
        let mut user = user;
        user.password_hash = hash;
        let keep = keep.map(SessionKey::of);
        let (before, ended_keys, ended_records) = {
            let mut state = lock(&self.state);
            let before = state
                .users
                .get(username)
                .cloned()
                .ok_or(AccountError::UnknownUser)?;
            state.users.insert(user.username.clone(), user.clone());
            let ended_keys: Vec<SessionKey> = state
                .sessions_of(username)
                .into_iter()
                .filter(|k| Some(*k) != keep)
                .collect();
            let mut ended_records = Vec::with_capacity(ended_keys.len());
            for key in &ended_keys {
                if let Some(rec) = state.sessions.remove(key) {
                    ended_records.push(rec);
                }
            }
            (before, ended_keys, ended_records)
        };
        if let Err(e) = self
            .backend
            .put_user_and_delete_sessions(&user, &ended_keys)
            .await
        {
            let mut state = lock(&self.state);
            state.users.insert(username.to_owned(), before);
            for rec in ended_records {
                state.sessions.insert(rec.key, rec);
            }
            return Err(e.into());
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
        mut change: UserUpdate,
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
        let changed_password = change.password.is_some();
        if let Some(mut p) = change.password.take() {
            let hash_res = self.hash_new(&username, &p).await;
            password::zeroize_string(&mut p);
            user.password_hash = hash_res?;
        }
        let end_sessions = changed_password || user.disabled;
        let (ended_keys, ended_records, before) = {
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
            let (ended_keys, ended_records) = if end_sessions {
                let keys = state.sessions_of(&username);
                let mut records = Vec::with_capacity(keys.len());
                for key in &keys {
                    if let Some(rec) = state.sessions.remove(key) {
                        records.push(rec);
                    }
                }
                (keys, records)
            } else {
                (Vec::new(), Vec::new())
            };
            (ended_keys, ended_records, before)
        };
        if let Err(e) = self
            .backend
            .put_user_and_delete_sessions(&user, &ended_keys)
            .await
        {
            let mut state = lock(&self.state);
            state.users.insert(username.clone(), before);
            for rec in ended_records {
                state.sessions.insert(rec.key, rec);
            }
            return Err(e.into());
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
        if changed_password {
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
        let (removed, removed_sessions) = {
            let mut state = lock(&self.state);
            let user = state
                .users
                .get(&username)
                .ok_or(AccountError::UnknownUser)?;
            if user.admin && !user.disabled && state.enabled_admins() <= 1 {
                return Err(AccountError::LastAdmin);
            }
            let keys = state.sessions_of(&username);
            let mut sessions = Vec::with_capacity(keys.len());
            for key in &keys {
                if let Some(rec) = state.sessions.remove(key) {
                    sessions.push(rec);
                }
            }
            (state.users.remove(&username), sessions)
        };
        if let Err(e) = self.backend.delete_user(&username).await {
            let mut state = lock(&self.state);
            if let Some(user) = removed {
                state.users.insert(username, user);
            }
            for rec in removed_sessions {
                state.sessions.insert(rec.key, rec);
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

    #[test]
    fn every_kind_is_listed_and_round_trips() {
        for (i, kind) in AuditKind::ALL.into_iter().enumerate() {
            assert_eq!(kind.ordinal(), i, "{kind:?} is out of place in ALL");
            assert_eq!(AuditKind::from_name(kind.as_str()), Some(kind));
        }
        assert_eq!(AuditKind::from_name("no_such_kind"), None);
        let mut names: Vec<_> = AuditKind::ALL.iter().map(|k| k.as_str()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), AuditKind::ALL.len(), "names are unique");
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

    #[test]
    fn throttle_evicts_oldest_unblocked_entry_without_clearing_active_lockouts() {
        // SEC-01 regression test: filling Throttle::CAPACITY with unknown
        // usernames must never wipe an active lockout on a target account.
        let c = config();
        let mut t = Throttle::default();
        for _ in 0..10 {
            t.fail_with("admin", true, 100, &c);
        }
        assert!(t.check("admin", 100).is_err(), "admin is locked out");

        for i in 0..(Throttle::CAPACITY + 500) {
            t.fail(&format!("guess_{i}"), 100 + i as u64, &c);
        }
        assert_eq!(t.names.len(), Throttle::CAPACITY);
        assert!(
            t.check("admin", 100).is_err(),
            "admin lockout survives capacity flood"
        );
    }

    #[tokio::test]
    async fn malformed_usernames_do_not_share_a_throttle_bucket_or_lock_out_valid_users() {
        // SEC-01 regression test: malformed usernames are rejected before
        // touching the per-username throttle map.
        let a = accounts().await;
        a.create_user("t", "alice", "", "alices long password", false)
            .await
            .expect("alice");
        for bad in ["", "   ", ".dot", "-dash", "bad space", "bad$char"] {
            assert!(matches!(
                a.log_in(bad, "alices long password", "").await,
                Err(AccountError::InvalidCredentials)
            ));
        }
        assert!(lock(&a.throttle).names.is_empty());
        assert!(a.log_in("alice", "alices long password", "").await.is_ok());
    }

    #[tokio::test]
    async fn per_address_limit_throttles_one_peer_without_blocking_another() {
        // SEC-04 regression test: a peer flooding failed logins across
        // rotating usernames is throttled in front of the Argon2id semaphore,
        // while another peer can still log in and authenticated users can
        // still change their password.
        let mut cfg = config();
        cfg.free_addr_failures = 4;
        cfg.max_login_in_flight_per_addr = 1;
        let a = Accounts::open(AccountBackend::Memory(MemoryAccounts::new()), cfg)
            .await
            .expect("open");
        a.create_user("t", "alice", "", "alices long password", false)
            .await
            .expect("alice");

        let attacker: std::net::IpAddr = "192.0.2.10".parse().expect("ip");
        let honest: std::net::IpAddr = "192.0.2.20".parse().expect("ip");

        for i in 0..4 {
            let _ = a
                .log_in_from(&format!("user{i}"), "wrong password", "", Some(attacker))
                .await;
        }
        assert!(matches!(
            a.log_in_from("alice", "alices long password", "attacker", Some(attacker))
                .await,
            Err(AccountError::Throttled { .. })
        ));
        let session = a
            .log_in_from("alice", "alices long password", "honest", Some(honest))
            .await
            .expect("honest peer is not blocked");
        a.change_password(
            "alice",
            "alices long password",
            "alices newer password",
            Some(&session.token),
        )
        .await
        .expect("authenticated password change uses dedicated permits");
    }
}
