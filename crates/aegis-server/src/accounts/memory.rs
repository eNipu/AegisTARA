//! In-memory account storage: tests and development servers. Lost on exit.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::Mutex;

use super::{AuditEvent, LoadedAccounts, SessionKey, SessionRecord, UserRecord};
use crate::store::StoreFault;

/// Audit events kept (R4). Older ones are dropped.
const MAX_EVENTS: usize = 10_000;

#[derive(Default)]
struct Inner {
    users: BTreeMap<String, UserRecord>,
    sessions: HashMap<SessionKey, SessionRecord>,
    events: VecDeque<AuditEvent>,
    password_login: Option<bool>,
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

    pub(super) async fn put_user(&self, user: &UserRecord) -> Result<(), StoreFault> {
        self.inner()
            .users
            .insert(user.username.clone(), user.clone());
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
        if inner.events.len() == MAX_EVENTS {
            inner.events.pop_front();
        }
        inner.events.push_back(event.clone());
        Ok(())
    }

    pub(super) async fn recent_events(&self, limit: usize) -> Result<Vec<AuditEvent>, StoreFault> {
        Ok(self
            .inner()
            .events
            .iter()
            .rev()
            .take(limit)
            .cloned()
            .collect())
    }
}
