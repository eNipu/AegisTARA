//! One outstanding authentication operation, shared by both desktop launchers.

use std::sync::Arc;

use aegis_auth::session::LoginAttempt;
use tokio::task::{AbortHandle, JoinHandle};

/// Identity and server of one discovery, restore or login attempt.
#[derive(Clone)]
pub struct AuthAttempt {
    id: Arc<()>,
    server: Arc<str>,
    guard: LoginAttempt,
}

impl AuthAttempt {
    /// The API server selected when this operation began.
    pub fn server(&self) -> &str {
        &self.server
    }

    /// Cancels OIDC token/keychain commits as well as their UI completion.
    pub fn guard(&self) -> &LoginAttempt {
        &self.guard
    }
}

/// A generation changes even when retrying the same server. Dropping or
/// cancelling the owner also cancels network work; already queued events
/// still have to pass `complete` before changing any UI or settings.
#[derive(Default)]
pub struct AuthAttempts {
    current: Option<AuthAttempt>,
    task: Option<AbortHandle>,
}

impl AuthAttempts {
    /// Invalidates the previous operation and starts a new generation.
    pub fn begin(&mut self, server: &str) -> AuthAttempt {
        self.cancel();
        let attempt = AuthAttempt {
            id: Arc::new(()),
            server: server.into(),
            guard: LoginAttempt::new(),
        };
        self.current = Some(attempt.clone());
        attempt
    }

    /// Owns cancellation of the task spawned for the current attempt.
    pub fn track(&mut self, task: JoinHandle<()>) {
        if self.current.is_some() {
            self.task = Some(task.abort_handle());
        } else {
            task.abort();
        }
    }

    /// Accepts a completion exactly once. Stale successes and failures have
    /// no effect, including on busy state, settings and remembered tokens.
    pub fn complete(&mut self, attempt: &AuthAttempt) -> bool {
        if !self.current.as_ref().is_some_and(|current| {
            Arc::ptr_eq(&current.id, &attempt.id)
                && current.server == attempt.server
                && attempt.guard.is_active()
        }) {
            return false;
        }
        self.current = None;
        self.task = None;
        true
    }

    /// Cancels pending work before switching servers, login methods or signing out.
    pub fn cancel(&mut self) {
        if let Some(attempt) = self.current.take() {
            attempt.guard.cancel();
        }
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

impl Drop for AuthAttempts {
    fn drop(&mut self) {
        self.cancel();
    }
}

#[cfg(test)]
mod tests {
    use super::AuthAttempts;

    #[test]
    fn server_switch_and_same_server_retry_reject_old_completions() {
        let mut attempts = AuthAttempts::default();
        let a = attempts.begin("https://a.test");
        let b = attempts.begin("https://b.test");
        assert!(!a.guard().is_active());
        assert!(!attempts.complete(&a));
        let retry = attempts.begin("https://b.test");
        assert!(!attempts.complete(&b));
        assert!(attempts.complete(&retry));
        assert!(!attempts.complete(&retry));
    }

    #[test]
    fn cancellation_and_drop_invalidate_the_commit_guard() {
        let mut attempts = AuthAttempts::default();
        let cancelled = attempts.begin("https://a.test");
        attempts.cancel();
        assert!(!attempts.complete(&cancelled));
        assert!(!cancelled.guard().is_active());
        let dropped = attempts.begin("https://a.test");
        drop(attempts);
        assert!(!dropped.guard().is_active());
    }
}
