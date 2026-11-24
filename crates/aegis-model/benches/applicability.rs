//! Condition switches against the "recompute after one change" budget
//! (engineering standards section 2): at most 5 ms at the `large` size. A
//! project of 25 trees of 4,000 nodes each (100,000 nodes, every step rated),
//! one threat per tree, and 64 conditions needed by about 1,000 steps (about
//! 1% of the nodes) spread evenly over the trees. Each op goes through
//! `apply_cached`, which also updates the evaluation cache and the risks.
//!
//! - `set_condition_present`: one condition switched off and on in turn;
//!   applicability is derived again and only the affected steps, their
//!   ancestors and their threats are re-evaluated.
//! - `rating_edit_with_condition_off`: with one condition off, one step's
//!   rating changed back and forth; applicability is not derived again.
//!
//! Run: `cargo bench --bench applicability` (under `cargo test --benches`,
//! divan runs every bench once as a smoke test).

#![forbid(unsafe_code)]

use std::sync::Arc;

use aegis_model::{
    AssetId, AttackFilter, AttackTree, ConditionId, DamageId, GateKind, Methodology, Node, NodeId,
    Op, Project, ProjectEvalCache, Provenance, ThreatId, TreeId,
};
use divan::{Bencher, black_box};

const TREES: usize = 25;
const NODES: usize = 4_000;
const GATES: usize = 40;
const CONDITIONS: usize = 64;
const LINKED_STEPS: usize = 1_000;

struct Fixture {
    project: Project,
    cache: ProjectEvalCache,
    conditions: Vec<ConditionId>,
    /// A step of the first tree that needs no condition.
    free_step: (TreeId, NodeId),
    steps: Vec<(TreeId, NodeId)>,
}

fn apply(project: &mut Project, op: Op) {
    project.apply(op).expect("applies");
}

/// Builds the fixture described in the module doc.
fn fixture() -> Fixture {
    let m = Arc::new(Methodology::iso21434_r155().expect("builtin"));
    let integrity = m.property("integrity").expect("property");
    let tampering = m.threat_category("tampering").expect("category");
    let safety = m.impact_category("safety").expect("category");
    let severe = m.level(safety, "severe").expect("level");
    let low = m.feasibility_level("low").expect("level");
    let mut project = Project::new(Arc::clone(&m));
    let (asset, damage) = (AssetId::new_v7(), DamageId::new_v7());
    let p = Provenance::default();
    apply(
        &mut project,
        Op::AddAsset {
            id: asset,
            name: "ECU".into(),
            properties: [integrity].into_iter().collect(),
            provenance: p.clone(),
        },
    );
    apply(
        &mut project,
        Op::AddDamageScenario {
            id: damage,
            name: "Damage".into(),
            impacts: vec![(safety, severe)],
            provenance: p.clone(),
        },
    );
    let mut steps = Vec::new();
    for t in 0..TREES {
        let (tree, root, threat) = (TreeId::new_v7(), NodeId::new_v7(), ThreatId::new_v7());
        apply(
            &mut project,
            Op::AddAttackTree {
                id: tree,
                tree: AttackTree::new(root, Node::gate(GateKind::Or, "Goal")).expect("tree"),
                step_feasibility: vec![],
                provenance: p.clone(),
            },
        );
        let gates: Vec<NodeId> = (0..GATES).map(|_| NodeId::new_v7()).collect();
        for (i, &gate) in gates.iter().enumerate() {
            apply(
                &mut project,
                Op::AddTreeNode {
                    tree,
                    parent: root,
                    id: gate,
                    node: Node::gate(GateKind::And, "Gate"),
                    index: u32::try_from(i + 1).expect("fits"),
                    position: u32::try_from(i).expect("fits"),
                    provenance: p.clone(),
                },
            );
        }
        for i in (GATES + 1)..NODES {
            let id = NodeId::new_v7();
            let k = i - GATES - 1;
            apply(
                &mut project,
                Op::AddTreeNode {
                    tree,
                    parent: gates[k % GATES],
                    id,
                    node: Node::step(format!("step {t}.{i}")),
                    index: u32::try_from(i).expect("fits"),
                    position: u32::try_from(k / GATES).expect("fits"),
                    provenance: p.clone(),
                },
            );
            apply(
                &mut project,
                Op::SetStepFeasibility {
                    tree,
                    node: id,
                    level: Some(low),
                    provenance: p.clone(),
                },
            );
            steps.push((tree, id));
        }
        for op in [
            Op::AddThreatScenario {
                id: threat,
                name: "Threat".into(),
                asset,
                category: tampering,
                catalog_refs: vec![],
                damages: vec![damage],
                provenance: p.clone(),
            },
            Op::SetThreatAttackTree {
                threat,
                tree: Some(tree),
                provenance: p.clone(),
            },
        ] {
            apply(&mut project, op);
        }
    }
    let conditions: Vec<ConditionId> = (0..CONDITIONS).map(|_| ConditionId::new_v7()).collect();
    for &id in &conditions {
        apply(
            &mut project,
            Op::AddCondition {
                id,
                name: "Option fitted".into(),
                description: "".into(),
                present: true,
                provenance: p.clone(),
            },
        );
    }
    let stride = steps.len() / LINKED_STEPS;
    for (n, &(tree, node)) in steps.iter().step_by(stride).enumerate() {
        apply(
            &mut project,
            Op::NeedStepCondition {
                tree,
                node,
                condition: conditions[n % CONDITIONS],
                provenance: p.clone(),
            },
        );
    }
    let cache = project.eval_cache();
    let free_step = steps[1];
    Fixture {
        project,
        cache,
        conditions,
        free_step,
        steps,
    }
}

fn switch(id: ConditionId, present: bool) -> Op {
    Op::SetConditionPresent {
        id,
        present,
        provenance: Provenance::default(),
    }
}

#[divan::bench(sample_count = 50)]
fn set_condition_present(bencher: Bencher) {
    let mut f = fixture();
    let id = f.conditions[0];
    let mut present = true;
    bencher.bench_local(|| {
        present = !present;
        let inv = f
            .project
            .apply_cached(switch(id, present), &mut f.cache)
            .expect("applies");
        black_box(inv);
    });
}

#[divan::bench(sample_count = 50)]
fn rating_edit_with_condition_off(bencher: Bencher) {
    let mut f = fixture();
    f.project
        .apply_cached(switch(f.conditions[0], false), &mut f.cache)
        .expect("applies");
    let m = Arc::clone(f.project.methodology());
    let levels = [
        m.feasibility_level("high").expect("level"),
        m.feasibility_level("low").expect("level"),
    ];
    let (tree, node) = f.free_step;
    let mut turn = 0;
    bencher.bench_local(|| {
        turn ^= 1;
        let op = Op::SetStepFeasibility {
            tree,
            node,
            level: Some(levels[turn]),
            provenance: Provenance::default(),
        };
        black_box(f.project.apply_cached(op, &mut f.cache).expect("applies"));
    });
}

#[divan::bench(sample_count = 50)]
fn set_attack_filter(bencher: Bencher) {
    let mut f = fixture();
    let m = Arc::clone(f.project.methodology());
    let factor = m.feasibility_factors().next().expect("factor");
    let levels: Vec<_> = m.factor_levels(factor).collect();
    let excluded_level = levels[0];
    let allowed_count = levels.len() - 1;
    for (n, &(tree, node)) in f.steps.iter().enumerate() {
        let level = if n.is_multiple_of(100) {
            excluded_level
        } else {
            levels[1 + (n % allowed_count)]
        };
        apply(
            &mut f.project,
            Op::SetStepFactor {
                tree,
                node,
                factor,
                level: Some(level),
                provenance: Provenance::default(),
            },
        );
    }
    f.cache = f.project.eval_cache();
    let active = AttackFilter::default().without(factor, excluded_level);
    let default = AttackFilter::default();
    let mut on = false;
    bencher.bench_local(|| {
        on = !on;
        let filter = if on { active } else { default };
        let op = Op::SetAttackFilter {
            filter,
            provenance: Provenance::default(),
        };
        black_box(f.project.apply_cached(op, &mut f.cache).expect("applies"));
    });
}

#[divan::bench(sample_count = 50)]
fn rating_edit_with_default_filter(bencher: Bencher) {
    let mut f = fixture();
    let m = Arc::clone(f.project.methodology());
    let factor = m.feasibility_factors().next().expect("factor");
    let levels: Vec<_> = m.factor_levels(factor).collect();
    let (tree, node) = f.free_step;
    let mut turn = 0;
    bencher.bench_local(|| {
        turn ^= 1;
        let op = Op::SetStepFactor {
            tree,
            node,
            factor,
            level: Some(levels[turn]),
            provenance: Provenance::default(),
        };
        black_box(f.project.apply_cached(op, &mut f.cache).expect("applies"));
    });
}

fn main() {
    divan::main();
}
