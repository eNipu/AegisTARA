//! In-memory account storage: tests and development servers. Lost on exit.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::Mutex;
use uuid::Uuid;

use super::{AuditEvent, AuditKind, LoadedAccounts, SessionKey, SessionRecord, UserRecord};
use crate::store::StoreFault;
use crate::store::audit::{ChainReport, EventChainVerifier, Hash, event_genesis, event_link};

/// Audit events kept (R4). Older ones are dropped.
const MAX_EVENTS: usize = 10_000;

#[derive(Clone, Debug)]
struct ChainedEvent {
    event: AuditEvent,
    prev_hash: Hash,
    hash: Hash,
}

struct Inner {
    users: BTreeMap<String, UserRecord>,
    sessions: HashMap<SessionKey, SessionRecord>,
    events: VecDeque<ChainedEvent>,
    tail_hash: Hash,
    head_hash: Hash,
    password_login: Option<bool>,
    fail_record: bool,
}

impl Default for Inner {
    fn default() -> Self {
        let genesis = event_genesis(Uuid::nil());
        Inner {
            users: BTreeMap::new(),
            sessions: HashMap::new(),
            events: VecDeque::new(),
            tail_hash: genesis,
            head_hash: genesis,
            password_login: None,
            fail_record: false,
        }
    }
}

/// Accounts in memory.
#[derive(Default)]
pub struct MemoryAccounts {
    inner: Mutex<Inner>,
}

impl MemoryAccounts {
    /// An empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn inner(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn append_chained(inner: &mut Inner, event: AuditEvent) {
        if inner.events.len() == MAX_EVENTS
            && let Some(dropped) = inner.events.pop_front()
        {
            inner.tail_hash = dropped.hash;
        }
        let prev_hash = inner.head_hash;
        let hash = event_link(&prev_hash, &event);
        inner.head_hash = hash;
        inner.events.push_back(ChainedEvent {
            event,
            prev_hash,
            hash,
        });
    }

    /// Simulates a backend failure on `record` (for testing retry/error handling).
    pub fn set_fail_record(&self, fail: bool) {
        self.inner().fail_record = fail;
    }

    /// Tampers with a stored event's `detail` in place without updating its
    /// hash (for testing `VerifyAudit` tamper detection).
    pub fn tamper_event_detail(&self, index: usize, detail: &str) {
        let mut inner = self.inner();
        if let Some(entry) = inner.events.get_mut(index) {
            entry.event.detail = detail.to_owned();
        }
    }

    pub(super) async fn load(&self, now_ms: u64) -> Result<LoadedAccounts, StoreFault> {
        let inner = self.inner();
        Ok(LoadedAccounts {
            users: inner.users.values().cloned().collect(),
            sessions: inner
                .sessions
                .values()
                .filter(|s| s.expires_ms > now_ms)
                .cloned()
                .collect(),
            password_login: inner.password_login,
        })
    }

    pub(super) async fn put_password_login(&self, enabled: bool) -> Result<(), StoreFault> {
        self.inner().password_login = Some(enabled);
        Ok(())
    }

    pub(super) async fn put_password_login_and_delete_sessions(
        &self,
        enabled: bool,
        keys: &[SessionKey],
    ) -> Result<(), StoreFault> {
        let mut inner = self.inner();
        inner.password_login = Some(enabled);
        for key in keys {
            inner.sessions.remove(key);
        }
        Ok(())
    }

    pub(super) async fn put_user(&self, user: &UserRecord) -> Result<(), StoreFault> {
        self.inner()
            .users
            .insert(user.username.clone(), user.clone());
        Ok(())
    }

    pub(super) async fn put_login_session(
        &self,
        user: &UserRecord,
        session: &SessionRecord,
        evicted: &[SessionKey],
    ) -> Result<(), StoreFault> {
        let mut inner = self.inner();
        inner.users.insert(user.username.clone(), user.clone());
        inner.sessions.insert(session.key, session.clone());
        for key in evicted {
            inner.sessions.remove(key);
        }
        Ok(())
    }

    pub(super) async fn put_user_and_delete_sessions(
        &self,
        user: &UserRecord,
        keys: &[SessionKey],
    ) -> Result<(), StoreFault> {
        let mut inner = self.inner();
        inner.users.insert(user.username.clone(), user.clone());
        for key in keys {
            inner.sessions.remove(key);
        }
        Ok(())
    }

    pub(super) async fn delete_user(&self, username: &str) -> Result<(), StoreFault> {
        let mut inner = self.inner();
        inner.users.remove(username);
        inner.sessions.retain(|_, s| s.username != username);
        Ok(())
    }

    pub(super) async fn put_session(&self, session: &SessionRecord) -> Result<(), StoreFault> {
        self.inner().sessions.insert(session.key, session.clone());
        Ok(())
    }

    pub(super) async fn delete_sessions(&self, keys: &[SessionKey]) -> Result<(), StoreFault> {
        let mut inner = self.inner();
        for key in keys {
            inner.sessions.remove(key);
        }
        Ok(())
    }

    pub(super) async fn record(&self, event: &AuditEvent) -> Result<(), StoreFault> {
        let mut inner = self.inner();
        if inner.fail_record {
            return Err(StoreFault::Backend("simulated event store failure".into()));
        }
        Self::append_chained(&mut inner, event.clone());
        Ok(())
    }

    pub(super) async fn recent_events(&self, limit: usize) -> Result<Vec<AuditEvent>, StoreFault> {
        Ok(self
            .inner()
            .events
            .iter()
            .rev()
            .take(limit)
            .map(|c| c.event.clone())
            .collect())
    }

    pub(super) async fn trim_events(
        &self,
        cutoff_ms: u64,
        keep: usize,
        now_ms: u64,
    ) -> Result<u64, StoreFault> {
        let mut inner = self.inner();
        let age_cut = inner
            .events
            .iter()
            .rposition(|c| c.event.at_ms < cutoff_ms)
            .map_or(0, |idx| idx + 1);
        // The cap counts every row. A checkpoint that is still the newest row
        // gets one spare place, so an idle log is left alone.
        let spare = usize::from(
            inner
                .events
                .back()
                .is_some_and(|c| c.event.kind == AuditKind::EventsTrimmed),
        );
        let len = inner.events.len();
        let cap_cut = if keep == 0 {
            len
        } else if len > keep.saturating_add(spare) {
            len - keep
        } else {
            0
        };
        let cut = age_cut.max(cap_cut);
        if cut == 0 {
            return Ok(0);
        }
        for _ in 0..cut {
            if let Some(dropped) = inner.events.pop_front() {
                inner.tail_hash = dropped.hash;
            }
        }
        let removed = cut as u64;
        Self::append_chained(
            &mut inner,
            AuditEvent {
                at_ms: now_ms,
                actor: "system".to_owned(),
                kind: AuditKind::EventsTrimmed,
                target: String::new(),
                detail: format!("removed={removed}"),
            },
        );
        Ok(removed)
    }

    pub(super) async fn verify_events(&self) -> Result<ChainReport, StoreFault> {
        let inner = self.inner();
        let mut verifier = EventChainVerifier::new(inner.tail_hash);
        for entry in &inner.events {
            verifier.push(&entry.event, &entry.prev_hash, &entry.hash);
        }
        Ok(verifier.finish(&inner.head_hash))
    }
}
