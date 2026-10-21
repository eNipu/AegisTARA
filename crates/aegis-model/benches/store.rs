//! Save and load time (ADR 0009), against the budget "open project from
//! local cache <= 1.0 s (large)" in engineering standards section 2. This
//! job covers only the TARA part of a project, so the ADR reports what
//! share of that budget it takes.
//!
//! Run: `cargo bench --bench store` (under `cargo test --benches`, divan
//! runs every bench once as a smoke test).
//!
//! Workload (`benches/store_kit`): the `typical` shape, 300 assets, 2,000
//! damage scenarios, and 2,000 threats with 2 catalog refs and 2 damage
//! links each; and a 10,000-op log (the 4,300 ops that build it, then
//! renames and rating changes). Building the inputs is outside the timed
//! region.
//!
//! The lesson (L2): `load` and `replay_log` rebuild the same project, one
//! from a snapshot, one from the ops that made it. Both go through
//! `Project::apply`; the numbers show what a snapshot saves over replay.

#![forbid(unsafe_code)]

#[path = "store_kit/mod.rs"]
mod store_kit;

use std::sync::{Arc, OnceLock};

use aegis_model::{Methodology, Op, OpLogReader, OpLogWriter, Project};
use divan::counter::{BytesCount, ItemsCount};
use divan::{Bencher, black_box};

fn main() {
    divan::main();
}

struct Inputs {
    m: Arc<Methodology>,
    project: Project,
    snapshot: Vec<u8>,
    ops: Vec<Op>,
    log: Vec<u8>,
}

fn inputs() -> &'static Inputs {
    static INPUTS: OnceLock<Inputs> = OnceLock::new();
    INPUTS.get_or_init(|| {
        let m = store_kit::methodology();
        let project = store_kit::typical(&m);
        let mut snapshot = Vec::new();
        if let Err(e) = project.save(&mut snapshot) {
            panic!("save: {e}");
        }
        let ops = store_kit::ops(&m);
        let log = write(&m, &ops);
        let entities = project.asset_count() + project.damage_scenario_count()
            + project.threat_scenario_count();
        eprintln!(
            "snapshot: {} bytes, {} entities, {} bytes per entity; log: {} bytes, {} ops, {} bytes per op",
            snapshot.len(),
            entities,
            snapshot.len() / entities.max(1),
            log.len(),
            ops.len(),
            log.len() / ops.len().max(1),
        );
        Inputs { m, project, snapshot, ops, log }
    })
}

fn write(m: &Arc<Methodology>, ops: &[Op]) -> Vec<u8> {
    let run = || {
        let mut log = OpLogWriter::create(Vec::with_capacity(2 << 20), Arc::clone(m))?;
        for op in ops {
            log.append(op)?;
        }
        log.into_inner()
    };
    match run() {
        Ok(bytes) => bytes,
        Err(e) => panic!("write log: {e}"),
    }
}

#[divan::bench]
fn save(bencher: Bencher) {
    let i = inputs();
    bencher
        .counter(BytesCount::new(i.snapshot.len()))
        .with_inputs(|| Vec::with_capacity(i.snapshot.len()))
        .bench_local_values(|mut out: Vec<u8>| {
            let r = i.project.save(&mut out);
            black_box((r.is_ok(), out))
        });
}

#[divan::bench]
fn load(bencher: Bencher) {
    let i = inputs();
    bencher
        .counter(BytesCount::new(i.snapshot.len()))
        .bench_local(|| Project::load(black_box(i.snapshot.as_slice()), Arc::clone(&i.m)).is_ok());
}

#[divan::bench]
fn append_10k(bencher: Bencher) {
    let i = inputs();
    bencher
        .counter(ItemsCount::new(i.ops.len()))
        .bench_local(|| write(&i.m, black_box(&i.ops)).len());
}

#[divan::bench]
fn read_10k(bencher: Bencher) {
    let i = inputs();
    bencher
        .counter(ItemsCount::new(i.ops.len()))
        .bench_local(
            || match OpLogReader::open(black_box(i.log.as_slice()), Arc::clone(&i.m)) {
                Ok(reader) => reader.filter(Result::is_ok).count(),
                Err(_) => 0,
            },
        );
}

/// Rebuilds the final project from the whole log: read plus apply.
#[divan::bench]
fn replay_log(bencher: Bencher) {
    let i = inputs();
    bencher
        .counter(ItemsCount::new(i.ops.len()))
        .bench_local(|| {
            let mut p = Project::new(Arc::clone(&i.m));
            if let Ok(reader) = OpLogReader::open(black_box(i.log.as_slice()), Arc::clone(&i.m)) {
                for r in reader.flatten() {
                    let _ = p.apply(r.op);
                }
            }
            p.threat_scenario_count()
        });
}
