//! Heap cost of three linked-parent layouts, measured with `dhat`.
//!
//! Run: `cargo bench --bench parents_memory` (also runs once, as a smoke
//! test, under `cargo test --benches`). Prints one Markdown table to stdout,
//! then checks two facts and exits non-zero if either fails:
//! - (a) `BoxStore` live bytes <= `VecStore` live bytes in every case;
//! - (b) `SparseStore` live bytes < `VecStore` live bytes at 1% share.
//!
//! What it teaches (ADR 0004): the dense layouts pay a header for every node
//! whether it is shared or not; the sparse map pays only for shared nodes,
//! but more per shared node. "Build allocations" counts every allocation and
//! reallocation during the build, which shows the churn of exact-size boxes.
//!
//! The profiler runs in testing mode, so it never writes `dhat-heap.json`.

#![forbid(unsafe_code)]

mod common;

use std::process::ExitCode;

use common::{BoxStore, CASES, Case, Ix, ParentStore, SEED, Shape, SparseStore, VecStore, build};

#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

/// One table row: the heap cost of one store for one case.
struct Row {
    case: Case,
    store: &'static str,
    /// Live bytes held by the built store (`curr_bytes` delta).
    live_bytes: usize,
    /// Live heap blocks held by the built store (`curr_blocks` delta).
    live_blocks: usize,
    /// Allocations plus reallocations during the build (`total_blocks`
    /// delta).
    build_allocs: u64,
}

/// Builds a store of type `S` for `shape` and measures its heap with dhat.
/// The shape is built before the first reading, so only the store counts.
///
/// Time O(V + L x cost of `S::link`), space: the store itself.
fn measure<S: ParentStore>(case: Case, shape: &Shape) -> Row {
    let before = dhat::HeapStats::get();
    let store: S = build(shape.owners.len(), &shape.links);
    let after = dhat::HeapStats::get();
    // Sanity check after both readings (so it is not measured): the store
    // holds every link. Also keeps the store alive past the second reading.
    let held: usize = (0..shape.owners.len())
        .map(|i| store.linked(Ix::from_usize(i)).count())
        .sum();
    assert_eq!(held, shape.links.len(), "{} lost links", S::NAME);
    Row {
        case,
        store: S::NAME,
        live_bytes: after.curr_bytes.saturating_sub(before.curr_bytes),
        live_blocks: after.curr_blocks.saturating_sub(before.curr_blocks),
        build_allocs: after.total_blocks.saturating_sub(before.total_blocks),
    }
}

/// Measures all three stores for every case, in case order (size, then
/// share), with stores in the order `VecStore` (today), `BoxStore`,
/// `SparseStore`. Time O(cases x (V + L)).
fn measure_all() -> Vec<Row> {
    let mut rows = Vec::with_capacity(CASES.len() * 3);
    for case in CASES {
        let shape = common::shape(case.nodes, case.share_pct, SEED);
        rows.push(measure::<VecStore>(case, &shape));
        rows.push(measure::<BoxStore>(case, &shape));
        rows.push(measure::<SparseStore>(case, &shape));
    }
    rows
}

/// Bytes per node with two decimals, as a string. Time O(1).
fn per_node(bytes: usize, nodes: usize) -> String {
    let nodes = nodes.max(1);
    let hundredths = bytes.saturating_mul(100) / nodes;
    format!("{}.{:02}", hundredths / 100, hundredths % 100)
}

/// Prints the rows as one Markdown table. Time O(rows).
fn print_table(rows: &[Row], links_per_case: &[usize]) {
    println!(
        "| size | nodes | share % | links | store | live bytes | bytes/node | live blocks | build allocations |"
    );
    println!("|---|---:|---:|---:|---|---:|---:|---:|---:|");
    for (i, r) in rows.iter().enumerate() {
        let links = links_per_case.get(i / 3).copied().unwrap_or(0);
        println!(
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} |",
            r.case.name,
            r.case.nodes,
            r.case.share_pct,
            links,
            r.store,
            r.live_bytes,
            per_node(r.live_bytes, r.case.nodes),
            r.live_blocks,
            r.build_allocs,
        );
    }
}

/// Live bytes of the row for `store` in `case`, if measured. Time O(rows).
fn live(rows: &[Row], case: Case, store: &str) -> Option<usize> {
    rows.iter()
        .find(|r| r.case == case && r.store == store)
        .map(|r| r.live_bytes)
}

/// Checks facts (a) and (b); returns one message per failure. Time
/// O(cases x rows).
fn check_facts(rows: &[Row]) -> Vec<String> {
    let mut failures = Vec::new();
    for case in CASES {
        let (Some(vec), Some(boxed), Some(sparse)) = (
            live(rows, case, VecStore::NAME),
            live(rows, case, BoxStore::NAME),
            live(rows, case, SparseStore::NAME),
        ) else {
            failures.push(format!("{case}: missing a row"));
            continue;
        };
        if boxed > vec {
            failures.push(format!("(a) {case}: BoxStore {boxed} B > VecStore {vec} B"));
        }
        if case.share_pct == 1 && sparse >= vec {
            failures.push(format!(
                "(b) {case}: SparseStore {sparse} B >= VecStore {vec} B"
            ));
        }
    }
    failures
}

fn main() -> ExitCode {
    let profiler = dhat::Profiler::builder().testing().build();

    let rows = measure_all();
    let links: Vec<usize> = CASES
        .iter()
        .map(|c| common::shape(c.nodes, c.share_pct, SEED).links.len())
        .collect();
    drop(profiler);

    print_table(&rows, &links);
    let failures = check_facts(&rows);
    println!();
    if failures.is_empty() {
        println!(
            "facts hold: (a) BoxStore <= VecStore in every case; (b) SparseStore < VecStore at 1% share"
        );
        ExitCode::SUCCESS
    } else {
        for f in &failures {
            eprintln!("fact failed: {f}");
        }
        ExitCode::FAILURE
    }
}
