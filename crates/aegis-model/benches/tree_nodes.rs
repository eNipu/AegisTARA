//! Node-level attack tree ops against the "recompute after one edit" budget
//! (engineering standards section 2): at most 5 ms. A project with one threat
//! on a full tree of `MAX_ATTACK_TREE_NODES` (4,096) nodes, each op applied
//! through `apply_cached`, which also updates the evaluation cache and the
//! threat's risk. Results are recorded in ADR 0014.
//!
//! Run: `cargo bench --bench tree_nodes` (under `cargo test --benches`, divan
//! runs every bench once as a smoke test).
//!
//! The worst cases are the ones that shift the arena: an insert at index 1
//! and the removal of the node at index 1 move every later node.

#![forbid(unsafe_code)]

use std::sync::Arc;

use aegis_model::{
    AssetId, AttackTree, DamageId, GateKind, MAX_ATTACK_TREE_NODES, Methodology, Node, NodeId, Op,
    Project, ProjectEvalCache, Provenance, StepTarget, ThreatId, TreeId,
};
use divan::{Bencher, black_box};

struct Fixture {
    project: Project,
    cache: ProjectEvalCache,
    tree: TreeId,
    root: NodeId,
    first_step: NodeId,
    gate: NodeId,
}

/// A threat on a tree with a gate and `MAX_ATTACK_TREE_NODES - 2` steps under
/// the root, rated low.
fn fixture(nodes: usize) -> Fixture {
    let m = Arc::new(Methodology::iso21434_r155().expect("builtin"));
    let integrity = m.property("integrity").expect("property");
    let tampering = m.threat_category("tampering").expect("category");
    let safety = m.impact_category("safety").expect("category");
    let severe = m.level(safety, "severe").expect("level");
    let low = m.feasibility_level("low").expect("level");
    let mut project = Project::new(Arc::clone(&m));
    let (asset, damage, threat, tree, root, gate) = (
        AssetId::new_v7(),
        DamageId::new_v7(),
        ThreatId::new_v7(),
        TreeId::new_v7(),
        NodeId::new_v7(),
        NodeId::new_v7(),
    );
    let p = Provenance::default();
    for op in [
        Op::AddAsset {
            id: asset,
            name: "ECU".into(),
            properties: [integrity].into_iter().collect(),
            provenance: p.clone(),
        },
        Op::AddDamageScenario {
            id: damage,
            name: "Damage".into(),
            impacts: vec![(safety, severe)],
            provenance: p.clone(),
        },
        Op::AddThreatScenario {
            id: threat,
            name: "Threat".into(),
            asset,
            category: tampering,
            catalog_refs: vec![],
            damages: vec![damage],
            provenance: p.clone(),
        },
        Op::AddAttackTree {
            role: Default::default(),
            id: tree,
            tree: AttackTree::new(root, Node::gate(GateKind::Or, "Goal")).expect("tree"),
            step_feasibility: vec![],
            provenance: p.clone(),
        },
        Op::SetThreatAttackTree {
            threat,
            tree: Some(tree),
            provenance: p.clone(),
        },
        Op::AddTreeNode {
            tree,
            parent: root,
            id: gate,
            node: Node::gate(GateKind::And, "Gate"),
            index: 1,
            position: 0,
            provenance: p.clone(),
        },
    ] {
        project.apply(op).expect("applies");
    }
    let mut first_step = gate;
    for i in 2..nodes {
        let id = NodeId::new_v7();
        project
            .apply(Op::AddTreeNode {
                tree,
                parent: root,
                id,
                node: Node::step(format!("step {i}")),
                index: u32::try_from(i).expect("fits"),
                position: u32::try_from(i - 1).expect("fits"),
                provenance: p.clone(),
            })
            .expect("adds");
        project
            .apply(Op::SetStepFeasibility {
                target: StepTarget::Step(tree, id),
                level: Some(low),
                provenance: p.clone(),
            })
            .expect("rates");
        if first_step == gate {
            first_step = id;
        }
    }
    let cache = project.eval_cache();
    Fixture {
        project,
        cache,
        tree,
        root,
        first_step,
        gate,
    }
}

/// Applies `op` and its inverse, so the state is the same for every sample.
fn round_trip(f: &mut Fixture, op: Op) {
    let inverse = f
        .project
        .apply_cached(black_box(op), &mut f.cache)
        .expect("applies");
    f.project
        .apply_cached(inverse, &mut f.cache)
        .expect("undoes");
}

#[divan::bench(sample_count = 20, max_time = 10)]
fn insert_at_the_front_then_undo(bencher: Bencher) {
    let mut f = fixture(MAX_ATTACK_TREE_NODES - 1);
    bencher.bench_local(|| {
        let op = Op::AddTreeNode {
            tree: f.tree,
            parent: f.root,
            id: NodeId::new_v7(),
            node: Node::step("new"),
            index: 1,
            position: 0,
            provenance: Provenance::default(),
        };
        round_trip(&mut f, op);
    });
}

#[divan::bench(sample_count = 20, max_time = 10)]
fn remove_a_leaf_at_the_front_then_undo(bencher: Bencher) {
    let mut f = fixture(MAX_ATTACK_TREE_NODES);
    // Unrate the first step so it can go, once.
    f.project
        .apply_cached(
            Op::SetStepFeasibility {
                target: StepTarget::Step(f.tree, f.first_step),
                level: None,
                provenance: Provenance::default(),
            },
            &mut f.cache,
        )
        .expect("clear");
    bencher.bench_local(|| {
        let op = Op::RemoveTreeNode {
            tree: f.tree,
            id: f.first_step,
            provenance: Provenance::default(),
        };
        round_trip(&mut f, op);
    });
}

#[divan::bench(sample_count = 20, max_time = 10)]
fn change_a_gate_then_undo(bencher: Bencher) {
    let mut f = fixture(MAX_ATTACK_TREE_NODES);
    bencher.bench_local(|| {
        let op = Op::SetTreeNodeGate {
            tree: f.tree,
            id: f.gate,
            gate: GateKind::SeqAnd,
            provenance: Provenance::default(),
        };
        round_trip(&mut f, op);
    });
}

#[divan::bench(sample_count = 20, max_time = 10)]
fn rename_a_node_then_undo(bencher: Bencher) {
    let mut f = fixture(MAX_ATTACK_TREE_NODES);
    bencher.bench_local(|| {
        let op = Op::RenameTreeNode {
            tree: f.tree,
            id: f.first_step,
            label: "renamed".into(),
            provenance: Provenance::default(),
        };
        round_trip(&mut f, op);
    });
}

#[divan::bench(sample_count = 20, max_time = 10)]
fn rate_a_step_then_undo(bencher: Bencher) {
    let m = Methodology::iso21434_r155().expect("builtin");
    let high = m.feasibility_level("high").expect("level");
    let mut f = fixture(MAX_ATTACK_TREE_NODES);
    bencher.bench_local(|| {
        let op = Op::SetStepFeasibility {
            target: StepTarget::Step(f.tree, f.first_step),
            level: Some(high),
            provenance: Provenance::default(),
        };
        round_trip(&mut f, op);
    });
}

fn main() {
    divan::main();
}
