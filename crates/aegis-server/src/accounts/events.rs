//! The server event log, on its own (job 052).
//!
//! The log (logins, exports, admin actions) used to live inside
//! [`Accounts`](super::Accounts), so a server with company login only had
//! none. [`EventLog`] is the same storage without the users: every server
//! can keep one, and `Accounts` hands out the log it already writes to.
//!
//! Storage is bounded (R4): [`EventLog::trim`] drops events older than the
//! retention and anything beyond a row cap, so a flood of failed logins
//! cannot fill the disk. See `docs/adr/0046-event-log-everywhere.md`.

use std::sync::Arc;
use std::time::Duration;

use super::{AccountBackend, AuditEvent, AuditKind, bounded};
use crate::store::StoreFault;

/// Longest text kept in the actor, target and detail columns, in characters.
const MAX_TEXT_CHARS: usize = 256;

/// Default retention: one year.
pub const DEFAULT_RETENTION: Duration = Duration::from_secs(365 * 24 * 3600);

/// Default most events kept.
pub const DEFAULT_MAX_EVENTS: usize = 1_000_000;

/// How long an event stays and how many are kept.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Retention {
    /// Events older than this are dropped.
    pub max_age: Duration,
    /// At most this many of the newest events are kept.
    pub max_events: usize,
}

impl Default for Retention {
    fn default() -> Self {
        Retention {
            max_age: DEFAULT_RETENTION,
            max_events: DEFAULT_MAX_EVENTS,
        }
    }
}

/// The append-only event log. Cheap to clone: it shares the backend.
#[derive(Clone)]
pub struct EventLog {
    backend: Arc<AccountBackend>,
    clock_ms: fn() -> u64,
}

impl EventLog {
    /// A log over `backend`, which may also serve [`Accounts`](super::Accounts).
    #[must_use]
    pub fn new(backend: Arc<AccountBackend>, clock_ms: fn() -> u64) -> Self {
        EventLog { backend, clock_ms }
    }

    /// A log that owns its backend and uses the system clock.
    #[must_use]
    pub fn standalone(backend: AccountBackend) -> Self {
        EventLog::new(Arc::new(backend), crate::config::system_clock_ms)
    }

    /// The log's clock, in milliseconds since the Unix epoch.
    #[must_use]
    pub fn now_ms(&self) -> u64 {
        (self.clock_ms)()
    }

    /// Writes an event. A failure is logged and does not fail the caller's
    /// action, which has already been stored. Control characters are
    /// removed and each text is cut to 256 characters.
    pub async fn record(&self, actor: &str, kind: AuditKind, target: &str, detail: &str) {
        let event = AuditEvent {
            at_ms: self.now_ms(),
            actor: bounded(actor, MAX_TEXT_CHARS),
            kind,
            target: bounded(target, MAX_TEXT_CHARS),
            detail: bounded(detail, MAX_TEXT_CHARS),
        };
        if let Err(fault) = self.backend.record(&event).await {
            tracing::error!(%fault, kind = kind.as_str(), "could not write an audit event");
        }
    }

    /// The newest events, newest first, at most `limit` (and never more than
    /// 1000).
    ///
    /// # Errors
    ///
    /// [`StoreFault`] from the backend.
    pub async fn recent(&self, limit: usize) -> Result<Vec<AuditEvent>, StoreFault> {
        self.backend.recent_events(limit.min(MAX_READ)).await
    }

    /// Drops events beyond `retention`. Returns how many were removed.
    ///
    /// # Errors
    ///
    /// [`StoreFault`] from the backend.
    pub async fn trim(&self, retention: Retention) -> Result<u64, StoreFault> {
        let age_ms = u64::try_from(retention.max_age.as_millis()).unwrap_or(u64::MAX);
        let cutoff = self.now_ms().saturating_sub(age_ms);
        self.backend
            .trim_events(cutoff, retention.max_events.max(1))
            .await
    }
}

/// Most events one read returns (R3).
const MAX_READ: usize = 1000;

/// Runs [`EventLog::trim`] every `every` until the returned task is aborted.
/// The first trim runs at once, so a long-stopped server catches up at start.
#[must_use]
pub fn spawn_trimmer(
    log: EventLog,
    retention: Retention,
    every: Duration,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(every);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tick.tick().await;
            match log.trim(retention).await {
                Ok(0) => {}
                Ok(n) => tracing::info!(n, "old events removed from the event log"),
                Err(e) => tracing::warn!(error = %e, "could not trim the event log"),
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;
    use crate::accounts::MemoryAccounts;

    // One clock per test thread: `#[tokio::test]` runs everything, including
    // the spawned trimmer, on the test's own thread.
    thread_local! {
        static NOW: Cell<u64> = const { Cell::new(0) };
    }

    fn clock() -> u64 {
        NOW.with(Cell::get)
    }

    fn set_clock(ms: u64) {
        NOW.with(|n| n.set(ms));
    }

    fn log() -> EventLog {
        EventLog::new(
            Arc::new(AccountBackend::Memory(MemoryAccounts::new())),
            clock,
        )
    }

    async fn write(log: &EventLog, at: u64, n: usize) {
        set_clock(at);
        for i in 0..n {
            log.record("alice", AuditKind::Export, &format!("p{at}-{i}"), "pdf")
                .await;
        }
    }

    #[tokio::test]
    async fn events_come_back_newest_first_and_reads_are_capped() {
        let log = log();
        write(&log, 10, 3).await;
        write(&log, 20, 1).await;
        let all = log.recent(100).await.expect("recent");
        assert_eq!(all.len(), 4);
        assert_eq!(all[0].at_ms, 20);
        assert_eq!(all[3].target, "p10-0");
        assert_eq!(log.recent(2).await.expect("two").len(), 2);
        assert!(
            log.recent(usize::MAX).await.is_ok(),
            "a huge limit is capped"
        );
    }

    #[tokio::test]
    async fn text_is_cleaned_and_cut() {
        let log = log();
        set_clock(1);
        let long = "x".repeat(1000);
        log.record("al\nice\u{7}", AuditKind::Logout, &long, "a\tb")
            .await;
        let e = &log.recent(1).await.expect("recent")[0];
        assert_eq!(e.actor, "alice");
        assert_eq!(e.target.chars().count(), MAX_TEXT_CHARS);
        assert_eq!(e.detail, "ab");
    }

    #[tokio::test]
    async fn trim_drops_old_events_and_keeps_young_ones() {
        let log = log();
        let day = 86_400_000;
        write(&log, 1, 5).await;
        write(&log, 10 * day, 4).await;
        set_clock(11 * day);
        let removed = log
            .trim(Retention {
                max_age: Duration::from_secs(5 * 86_400),
                max_events: 1000,
            })
            .await
            .expect("trim");
        assert_eq!(removed, 5);
        let left = log.recent(100).await.expect("recent");
        assert_eq!(left.len(), 4);
        assert!(left.iter().all(|e| e.at_ms == 10 * day));
        // Nothing more to do on a second pass.
        let again = log
            .trim(Retention {
                max_age: Duration::from_secs(5 * 86_400),
                max_events: 1000,
            })
            .await
            .expect("trim");
        assert_eq!(again, 0);
    }

    #[tokio::test]
    async fn trim_keeps_only_the_newest_when_over_the_row_cap() {
        let log = log();
        write(&log, 100, 10).await;
        let removed = log
            .trim(Retention {
                max_age: Duration::from_secs(3600),
                max_events: 3,
            })
            .await
            .expect("trim");
        assert_eq!(removed, 7);
        let left = log.recent(100).await.expect("recent");
        assert_eq!(left.len(), 3);
        // The newest three: p100-9, p100-8, p100-7.
        assert_eq!(left[0].target, "p100-9");
        assert_eq!(left[2].target, "p100-7");
    }

    #[tokio::test]
    async fn a_clock_before_the_retention_does_not_underflow() {
        let log = log();
        write(&log, 5, 2).await;
        set_clock(5);
        let removed = log.trim(Retention::default()).await.expect("trim");
        assert_eq!(removed, 0);
    }

    #[tokio::test(start_paused = true)]
    async fn the_trimmer_runs_at_once_and_then_on_its_interval() {
        let log = log();
        write(&log, 100, 6).await;
        let task = spawn_trimmer(
            log.clone(),
            Retention {
                max_age: Duration::from_secs(3600),
                max_events: 2,
            },
            Duration::from_secs(60),
        );
        tokio::time::sleep(Duration::from_secs(1)).await;
        assert_eq!(log.recent(100).await.expect("recent").len(), 2);
        task.abort();
    }
}
