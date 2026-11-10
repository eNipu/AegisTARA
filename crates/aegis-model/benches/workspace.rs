//! Workspace file write and read time (ADR 0065), beside the snapshot's save
//! and load in `benches/store.rs`, on the same workload (`benches/store_kit`:
//! the `typical` project, 300 assets, 2,000 damage scenarios, 2,000 threats).
//! Compare `to_workspace_files` with `save` and `from_workspace_files` with
//! `load`. Building the inputs is outside the timed region.
//!
//! Run: `cargo bench --bench workspace` (under `cargo test --benches`, divan
//! runs every bench once as a smoke test).

#![forbid(unsafe_code)]

#[path = "store_kit/mod.rs"]
mod store_kit;

use std::sync::{Arc, OnceLock};

use aegis_model::{Methodology, Project, WorkspaceFile};
use divan::counter::BytesCount;
use divan::{Bencher, black_box};

fn main() {
    divan::main();
}

struct Inputs {
    m: Arc<Methodology>,
    project: Project,
    files: Vec<WorkspaceFile>,
    bytes: usize,
}

fn inputs() -> &'static Inputs {
    static INPUTS: OnceLock<Inputs> = OnceLock::new();
    INPUTS.get_or_init(|| {
        let m = store_kit::methodology();
        let project = store_kit::typical(&m);
        let files = match project.to_workspace_files() {
            Ok(files) => files,
            Err(e) => panic!("to_workspace_files: {e}"),
        };
        let bytes: usize = files.iter().map(|f| f.bytes.len()).sum();
        let back = match Project::from_workspace_files(
            files.iter().map(|f| (f.path.as_str(), f.bytes.as_slice())),
            Arc::clone(&m),
        ) {
            Ok(back) => back,
            Err(e) => panic!("from_workspace_files: {e}"),
        };
        let (mut a, mut b) = (Vec::new(), Vec::new());
        if let Err(e) = project.save(&mut a) {
            panic!("save: {e}");
        }
        if let Err(e) = back.save(&mut b) {
            panic!("save: {e}");
        }
        assert!(a == b, "workspace round trip changed the saved bytes");
        eprintln!(
            "workspace: {} files, {} bytes; snapshot: {} bytes",
            files.len(),
            bytes,
            a.len(),
        );
        Inputs {
            m,
            project,
            files,
            bytes,
        }
    })
}

#[divan::bench]
fn write(bencher: Bencher) {
    let i = inputs();
    bencher
        .counter(BytesCount::new(i.bytes))
        .bench_local(|| black_box(i.project.to_workspace_files().is_ok()));
}

#[divan::bench]
fn read(bencher: Bencher) {
    let i = inputs();
    bencher
        .counter(BytesCount::new(i.bytes))
        .with_inputs(|| {
            i.files
                .iter()
                .map(|f| (f.path.as_str(), f.bytes.as_slice()))
                .collect::<Vec<_>>()
        })
        .bench_local_values(|files| {
            black_box(Project::from_workspace_files(files, Arc::clone(&i.m)).is_ok())
        });
}
