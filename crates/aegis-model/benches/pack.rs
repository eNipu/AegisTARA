//! Methodology pack read and write time (ADR 0011).
//!
//! Run: `cargo bench --bench pack` (under `cargo test --benches`, divan
//! runs every bench once as a smoke test).
//!
//! Workload: the built-in pack (`packs/iso21434-r155.json`, about 3.8 KB,
//! 40 vocabulary entries) and the maximal pack from the test kit
//! (`max_def()`: every vocabulary at its maximum count, 64-byte keys,
//! labels of 256 escaped control chars, about 1.4 MB, 689 entries).
//! Building the inputs is outside the timed region.
//!
//! The lesson (L2): `iso21434_r155` decodes on every call and keeps no
//! cache. The numbers show that this costs microseconds, so a global cache
//! (and its lifetime questions) is not worth adding; callers share the
//! result with an `Arc`.

#![forbid(unsafe_code)]

#[path = "../tests/common/mod.rs"]
mod common;

use std::sync::OnceLock;

use aegis_model::{Methodology, MethodologyDef};
use divan::counter::BytesCount;
use divan::{Bencher, black_box};

fn main() {
    divan::main();
}

struct Inputs {
    builtin: &'static [u8],
    max_def: MethodologyDef,
    max_pack: Vec<u8>,
}

fn inputs() -> &'static Inputs {
    static INPUTS: OnceLock<Inputs> = OnceLock::new();
    INPUTS.get_or_init(|| {
        let max_def = common::max_def();
        let max_pack = common::pack_bytes(&max_def);
        let builtin: &'static [u8] = include_bytes!("../packs/iso21434-r155.json");
        eprintln!(
            "built-in pack: {} bytes; maximal pack: {} bytes",
            builtin.len(),
            max_pack.len()
        );
        Inputs {
            builtin,
            max_def,
            max_pack,
        }
    })
}

/// The built-in methodology, decoded from the embedded pack.
#[divan::bench]
fn iso21434_r155(bencher: Bencher) {
    let i = inputs();
    bencher
        .counter(BytesCount::new(i.builtin.len()))
        .bench_local(|| Methodology::iso21434_r155().is_ok());
}

/// Read and validate the maximal pack.
#[divan::bench]
fn read_pack_max(bencher: Bencher) {
    let i = inputs();
    bencher
        .counter(BytesCount::new(i.max_pack.len()))
        .bench_local(|| Methodology::read_pack(black_box(i.max_pack.as_slice())).is_ok());
}

/// Decode the maximal pack only, without `Methodology::new`.
#[divan::bench]
fn decode_pack_max(bencher: Bencher) {
    let i = inputs();
    bencher
        .counter(BytesCount::new(i.max_pack.len()))
        .bench_local(|| MethodologyDef::read_pack(black_box(i.max_pack.as_slice())).is_ok());
}

/// Write the maximal pack.
#[divan::bench]
fn write_pack_max(bencher: Bencher) {
    let i = inputs();
    bencher
        .counter(BytesCount::new(i.max_pack.len()))
        .with_inputs(|| Vec::with_capacity(i.max_pack.len()))
        .bench_local_values(|mut out: Vec<u8>| {
            let r = black_box(&i.max_def).write_pack(&mut out);
            black_box((r.is_ok(), out))
        });
}
