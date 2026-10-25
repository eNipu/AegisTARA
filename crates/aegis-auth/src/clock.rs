//! Wall-clock seconds, injected so tests can fix time.

/// Seconds since the Unix epoch. Tokens carry whole seconds.
pub type Clock = fn() -> u64;

/// The system clock.
#[must_use]
pub fn system_clock() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}
