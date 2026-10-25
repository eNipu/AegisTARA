//! Server limits and timings. Every bound has a number (R4, R6).

use std::time::Duration;

/// Tunables of a running server. [`Default`] holds the production values;
/// tests shrink them.
#[derive(Clone, Debug)]
pub struct ServerConfig {
    /// Edit lease length (decision D4: 2 minutes). Clients renew well before.
    pub lease: Duration,
    /// A project with no lease and no observers is unloaded after this long.
    pub idle_unload: Duration,
    /// Commands queued per project before callers wait (R4).
    pub mailbox: usize,
    /// How long a caller waits for room in a full mailbox before the call
    /// fails with `RESOURCE_EXHAUSTED` (R6).
    pub mailbox_wait: Duration,
    /// Events buffered per observer before it is told to resync (R4).
    pub observer_buffer: usize,
    /// Accepted ops kept in memory so a new observer can catch up without
    /// reopening the project.
    pub recent_ops: usize,
    /// Write a snapshot once this many ops follow the last one, bounding
    /// replay time at load.
    pub snapshot_every: u64,
    /// Most ops in one `SubmitOps` call.
    pub max_batch_ops: usize,
    /// Most projects loaded at once.
    pub max_open_projects: usize,
    /// Bytes per `OpenProject` chunk.
    pub chunk_bytes: usize,
    /// Most projects the server hosts.
    pub max_projects: usize,
    /// Longest project name, in chars.
    pub max_project_name_chars: usize,
    /// Wall clock in milliseconds since the Unix epoch, for audit
    /// timestamps. A function pointer so tests can fix it.
    pub wall_clock: fn() -> u64,
}

/// The system clock in milliseconds since the Unix epoch.
#[must_use]
pub fn system_clock_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

impl Default for ServerConfig {
    fn default() -> Self {
        ServerConfig {
            lease: Duration::from_secs(120),
            idle_unload: Duration::from_secs(15 * 60),
            mailbox: 64,
            mailbox_wait: Duration::from_secs(5),
            observer_buffer: 1024,
            recent_ops: 1024,
            snapshot_every: 1000,
            max_batch_ops: 256,
            max_open_projects: 256,
            chunk_bytes: 256 * 1024,
            max_projects: 10_000,
            max_project_name_chars: 256,
            wall_clock: system_clock_ms,
        }
    }
}
