//! Frame-time statistics for `--measure`.

/// Percentiles of a set of samples, in milliseconds.
pub struct Stats {
    pub p50: f64,
    pub p95: f64,
    pub max: f64,
}

/// Nearest-rank percentiles of `samples_ms` (sorted in place); `None` when empty.
pub fn stats(samples_ms: &mut [f64]) -> Option<Stats> {
    if samples_ms.is_empty() {
        return None;
    }
    samples_ms.sort_by(f64::total_cmp);
    let n = samples_ms.len();
    let rank = |p: f64| samples_ms[((p * n as f64).ceil() as usize).clamp(1, n) - 1];
    Some(Stats {
        p50: rank(0.50),
        p95: rank(0.95),
        max: samples_ms[n - 1],
    })
}

/// `"<name> <p50> <p95> <max>"` with one decimal each.
pub fn line(name: &str, s: &Stats) -> String {
    format!("{name} {:.1} {:.1} {:.1}", s.p50, s.p95, s.max)
}
