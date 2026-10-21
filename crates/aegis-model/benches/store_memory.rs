//! Peak heap of `Project::load` on the `typical` snapshot, measured with
//! `dhat` (ADR 0009 records the numbers). The spec asks for the peak load
//! memory as a factor of the file size: loading reads the file into one
//! buffer (B bytes), decodes it into owned wire types, drops the buffer,
//! then builds the project through `Project::apply`.
//!
//! Run: `cargo bench --bench store_memory` (also runs under
//! `cargo test --benches`). The profiler runs in testing mode, so it never
//! writes `dhat-heap.json`. Fails if the peak exceeds `MAX_FACTOR` times
//! the file size, so a regression shows up in CI.

#![forbid(unsafe_code)]

#[path = "store_kit/mod.rs"]
mod store_kit;

use std::sync::Arc;

use aegis_model::Project;

#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

/// Upper bound on peak load heap / file bytes (measured 3.65 on the typical snapshot).
const MAX_FACTOR: f64 = 5.0;

fn main() {
    let m = store_kit::methodology();
    let project = store_kit::typical(&m);
    let mut snapshot = Vec::new();
    if let Err(e) = project.save(&mut snapshot) {
        panic!("save: {e}");
    }
    let file = snapshot.len();

    let profiler = dhat::Profiler::builder().testing().build();
    let loaded = Project::load(snapshot.as_slice(), Arc::clone(&m));
    let stats = dhat::HeapStats::get();
    drop(profiler);

    let Ok(loaded) = loaded else {
        panic!("load failed");
    };
    assert!(loaded == project, "loaded project differs");
    #[allow(clippy::cast_precision_loss)] // Sizes far below 2^52.
    let (peak, kept, file_f) = (stats.max_bytes as f64, stats.curr_bytes as f64, file as f64);
    println!(
        "typical snapshot: {file} bytes; load peak heap {} bytes ({:.2} x file); \
         project kept {} bytes ({:.2} x file); {} allocations",
        stats.max_bytes,
        peak / file_f,
        stats.curr_bytes,
        kept / file_f,
        stats.total_blocks,
    );
    assert!(
        peak / file_f <= MAX_FACTOR,
        "peak load heap {:.2} x file exceeds {MAX_FACTOR}",
        peak / file_f
    );
}
