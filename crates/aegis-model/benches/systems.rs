//! Time to derive the systems of every element (`Project::system_use`,
//! 119c) on the typical project (`benches/store_kit`: 300 assets, 2,000
//! damage scenarios, 2,000 threats) with 64 systems, 640 top-level
//! components (ten per system) and every asset bound to a component.
//! Building the project is outside the timed region.
//!
//! Run: `cargo bench --bench systems` (under `cargo test --benches`, divan
//! runs every bench once as a smoke test).

#![forbid(unsafe_code)]

#[path = "store_kit/mod.rs"]
mod store_kit;

use std::sync::OnceLock;

use aegis_model::{ComponentId, ElementId, ElementKind, Op, Project, Provenance};
use divan::black_box;

/// Systems in the project ([`aegis_model::MAX_SYSTEMS`]).
const SYSTEMS: usize = 64;
/// Top-level components per system.
const PER_SYSTEM: usize = 10;

fn main() {
    divan::main();
}

fn apply(project: &mut Project, op: Op) {
    if let Err(e) = project.apply(op) {
        panic!("bench setup op failed: {e}");
    }
}

fn project() -> &'static Project {
    static PROJECT: OnceLock<Project> = OnceLock::new();
    PROJECT.get_or_init(|| {
        let m = store_kit::methodology();
        let mut p = store_kit::typical(&m);
        let mut components = Vec::with_capacity(SYSTEMS * PER_SYSTEM);
        for s in 0..SYSTEMS {
            let system = ElementId::new_v7();
            apply(
                &mut p,
                Op::AddElement {
                    id: system,
                    kind: ElementKind::System,
                    name: format!("System {s}").into(),
                    provenance: Provenance::default(),
                },
            );
            for c in 0..PER_SYSTEM {
                let id = ComponentId::new_v7();
                apply(
                    &mut p,
                    Op::AddComponent {
                        id,
                        name: format!("Component {s}.{c}").into(),
                        parent: None,
                        provenance: Provenance::default(),
                    },
                );
                apply(
                    &mut p,
                    Op::SetComponentSystem {
                        id,
                        system: Some(system),
                        provenance: Provenance::default(),
                    },
                );
                components.push(id);
            }
        }
        let assets: Vec<_> = p.assets().map(|(_, id, _)| id).collect();
        for (n, asset) in assets.into_iter().enumerate() {
            let component = components.get(n % components.len()).copied();
            apply(
                &mut p,
                Op::SetAssetComponent {
                    asset,
                    component,
                    provenance: Provenance::default(),
                },
            );
        }
        p
    })
}

#[divan::bench]
fn system_use(bencher: divan::Bencher) {
    let p = project();
    bencher.bench(|| black_box(black_box(p).system_use()));
}
