//! The edit lease (decision D4): one editor per project at a time.
//!
//! Pure state, no I/O and no task: a [`LeaseState`] lives inside a project
//! actor and takes the current time as an argument, so every rule is a
//! plain unit test. Time is `tokio::time::Instant`, which tests pause and
//! advance.
//!
//! Rules:
//! - A lease is free, or held by one subject until `expires`.
//! - [`acquire`](LeaseState::acquire) succeeds if the lease is free or
//!   expired, or if the same subject already holds it (a restarted client
//!   takes its own lease back; the old token stops working).
//! - [`renew`](LeaseState::renew) and [`require`](LeaseState::require) need
//!   the exact token; an expired lease cannot be renewed, so a client that
//!   slept past expiry must acquire again and re-read the project.
//! - An admin may [`break_lease`](LeaseState::break_lease).

use std::sync::Arc;
use std::time::Duration;

use tokio::time::Instant;
use uuid::Uuid;

use crate::auth::Principal;
use crate::error::ServerError;

/// An unguessable bearer token for one lease: 122 random bits from the OS.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LeaseToken(String);

impl LeaseToken {
    fn generate() -> Self {
        LeaseToken(Uuid::new_v4().simple().to_string())
    }

    /// The token text sent to the client.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Compares in time independent of where the texts differ.
    fn matches(&self, other: &str) -> bool {
        let a = self.0.as_bytes();
        let b = other.as_bytes();
        let mut diff = a.len() ^ b.len();
        for (x, y) in a.iter().zip(b) {
            diff |= usize::from(x ^ y);
        }
        diff == 0
    }
}

/// A granted lease, as returned to the caller.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Grant {
    /// The bearer token.
    pub token: LeaseToken,
    /// Who holds it.
    pub holder: Arc<str>,
    /// Time left, at the moment of the grant.
    pub expires_in: Duration,
}

#[derive(Debug)]
struct Held {
    token: LeaseToken,
    holder: Arc<str>,
    expires: Instant,
}

/// The lease of one project.
#[derive(Debug)]
pub struct LeaseState {
    held: Option<Held>,
    length: Duration,
}

impl LeaseState {
    /// A free lease that grants `length` at a time.
    #[must_use]
    pub fn new(length: Duration) -> Self {
        LeaseState { held: None, length }
    }

    /// The holder, if the lease is held and not expired. Time O(1).
    #[must_use]
    pub fn holder(&self, now: Instant) -> Option<&Arc<str>> {
        self.live(now).map(|h| &h.holder)
    }

    /// When the current lease expires, expired or not. Time O(1).
    #[must_use]
    pub fn expiry(&self) -> Option<Instant> {
        self.held.as_ref().map(|h| h.expires)
    }

    /// Forgets an expired lease. Returns the old holder if one expired.
    /// Time O(1).
    pub fn sweep(&mut self, now: Instant) -> Option<Arc<str>> {
        if self.held.as_ref().is_some_and(|h| h.expires <= now) {
            return self.held.take().map(|h| h.holder);
        }
        None
    }

    fn live(&self, now: Instant) -> Option<&Held> {
        self.held.as_ref().filter(|h| h.expires > now)
    }

    /// Grants the lease to `who`.
    ///
    /// # Errors
    ///
    /// [`ServerError::LockHeld`] if another subject holds a live lease.
    pub fn acquire(&mut self, now: Instant, who: &Principal) -> Result<Grant, ServerError> {
        if let Some(h) = self.live(now)
            && h.holder != who.subject
        {
            return Err(ServerError::LockHeld {
                holder: h.holder.to_string(),
            });
        }
        let held = Held {
            token: LeaseToken::generate(),
            holder: Arc::clone(&who.subject),
            expires: now + self.length,
        };
        let grant = Grant {
            token: held.token.clone(),
            holder: Arc::clone(&held.holder),
            expires_in: self.length,
        };
        self.held = Some(held);
        Ok(grant)
    }

    /// Extends the lease to a full length from `now`.
    ///
    /// # Errors
    ///
    /// [`ServerError::LockInvalid`] if the token is wrong or the lease
    /// expired; [`ServerError::NotLocked`] if no lease exists.
    pub fn renew(
        &mut self,
        now: Instant,
        who: &Principal,
        token: &str,
    ) -> Result<Grant, ServerError> {
        let length = self.length;
        let held = self.require_mut(now, who, token)?;
        held.expires = now + length;
        Ok(Grant {
            token: held.token.clone(),
            holder: Arc::clone(&held.holder),
            expires_in: length,
        })
    }

    /// Checks that `token` is the live lease of `who`. Time O(1).
    ///
    /// # Errors
    ///
    /// As [`renew`](Self::renew).
    pub fn require(&self, now: Instant, who: &Principal, token: &str) -> Result<(), ServerError> {
        match self.live(now) {
            None if self.held.is_none() => Err(ServerError::NotLocked),
            Some(h) if h.holder == who.subject && h.token.matches(token) => Ok(()),
            _ => Err(ServerError::LockInvalid),
        }
    }

    fn require_mut(
        &mut self,
        now: Instant,
        who: &Principal,
        token: &str,
    ) -> Result<&mut Held, ServerError> {
        self.require(now, who, token)?;
        self.held.as_mut().ok_or(ServerError::NotLocked)
    }

    /// Frees the lease. Returns the holder that gave it up.
    ///
    /// # Errors
    ///
    /// As [`renew`](Self::renew).
    pub fn release(
        &mut self,
        now: Instant,
        who: &Principal,
        token: &str,
    ) -> Result<Arc<str>, ServerError> {
        self.require(now, who, token)?;
        self.held
            .take()
            .map(|h| h.holder)
            .ok_or(ServerError::NotLocked)
    }

    /// Frees the lease whoever holds it. Returns the old holder, if the
    /// lease was live.
    ///
    /// # Errors
    ///
    /// [`ServerError::NotAdmin`] if `who` is not an admin.
    pub fn break_lease(
        &mut self,
        now: Instant,
        who: &Principal,
    ) -> Result<Option<Arc<str>>, ServerError> {
        if !who.admin {
            return Err(ServerError::NotAdmin);
        }
        let live = self.live(now).is_some();
        let old = self.held.take();
        Ok(old.filter(|_| live).map(|h| h.holder))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LEN: Duration = Duration::from_secs(120);

    fn state() -> (LeaseState, Instant) {
        (LeaseState::new(LEN), Instant::now())
    }

    #[test]
    fn free_lease_is_granted_and_blocks_others() {
        let (mut s, t0) = state();
        let alice = Principal::user("alice");
        let bob = Principal::user("bob");
        let g = s.acquire(t0, &alice).expect("free");
        assert_eq!(g.expires_in, LEN);
        assert_eq!(s.holder(t0).map(|h| &**h), Some("alice"));
        assert!(matches!(
            s.acquire(t0, &bob),
            Err(ServerError::LockHeld { holder }) if holder == "alice"
        ));
    }

    #[test]
    fn same_subject_takes_its_own_lease_back_and_the_old_token_dies() {
        let (mut s, t0) = state();
        let alice = Principal::user("alice");
        let first = s.acquire(t0, &alice).expect("free");
        let second = s.acquire(t0, &alice).expect("own lease");
        assert_ne!(first.token, second.token);
        assert!(matches!(
            s.require(t0, &alice, first.token.as_str()),
            Err(ServerError::LockInvalid)
        ));
        assert!(s.require(t0, &alice, second.token.as_str()).is_ok());
    }

    #[test]
    fn lease_expires_and_can_be_taken_over() {
        let (mut s, t0) = state();
        let alice = Principal::user("alice");
        let bob = Principal::user("bob");
        let g = s.acquire(t0, &alice).expect("free");
        let later = t0 + LEN;
        assert_eq!(s.holder(later), None);
        assert!(matches!(
            s.require(later, &alice, g.token.as_str()),
            Err(ServerError::LockInvalid)
        ));
        assert_eq!(s.sweep(later).as_deref(), Some("alice"));
        assert!(s.acquire(later, &bob).is_ok());
    }

    #[test]
    fn renew_extends_from_now_and_needs_the_token() {
        let (mut s, t0) = state();
        let alice = Principal::user("alice");
        let g = s.acquire(t0, &alice).expect("free");
        let t1 = t0 + Duration::from_secs(100);
        assert!(s.renew(t1, &alice, "wrong").is_err());
        s.renew(t1, &alice, g.token.as_str()).expect("renews");
        assert_eq!(s.expiry(), Some(t1 + LEN));
        // An expired lease cannot be renewed.
        let t2 = t1 + LEN;
        assert!(matches!(
            s.renew(t2, &alice, g.token.as_str()),
            Err(ServerError::LockInvalid)
        ));
    }

    #[test]
    fn token_must_belong_to_the_caller() {
        let (mut s, t0) = state();
        let alice = Principal::user("alice");
        let mallory = Principal::user("mallory");
        let g = s.acquire(t0, &alice).expect("free");
        assert!(matches!(
            s.require(t0, &mallory, g.token.as_str()),
            Err(ServerError::LockInvalid)
        ));
    }

    #[test]
    fn require_on_a_never_locked_project_says_not_locked() {
        let (s, t0) = state();
        assert!(matches!(
            s.require(t0, &Principal::user("a"), "x"),
            Err(ServerError::NotLocked)
        ));
    }

    #[test]
    fn release_frees_the_lease() {
        let (mut s, t0) = state();
        let alice = Principal::user("alice");
        let g = s.acquire(t0, &alice).expect("free");
        assert_eq!(
            &*s.release(t0, &alice, g.token.as_str()).expect("held"),
            "alice"
        );
        assert_eq!(s.holder(t0), None);
        assert!(s.acquire(t0, &Principal::user("bob")).is_ok());
    }

    #[test]
    fn only_admins_break_leases() {
        let (mut s, t0) = state();
        let alice = Principal::user("alice");
        s.acquire(t0, &alice).expect("free");
        assert!(matches!(
            s.break_lease(t0, &Principal::user("bob")),
            Err(ServerError::NotAdmin)
        ));
        let old = s.break_lease(t0, &Principal::admin("root")).expect("admin");
        assert_eq!(old.as_deref(), Some("alice"));
        assert_eq!(s.holder(t0), None);
        // Breaking a free lease is fine and reports nobody.
        assert_eq!(
            s.break_lease(t0, &Principal::admin("root")).expect("admin"),
            None
        );
    }

    #[test]
    fn token_comparison_handles_length_differences() {
        let t = LeaseToken("abcd".into());
        assert!(t.matches("abcd"));
        assert!(!t.matches("abc"));
        assert!(!t.matches("abcde"));
        assert!(!t.matches("abce"));
        assert!(!t.matches(""));
    }
}
