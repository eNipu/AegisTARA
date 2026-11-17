//! Incremental cache updates for applicability switches (job 134, slice
//! 134c): a condition switch re-evaluates only the steps and threats it
//! touches, and the cache equals a full evaluation in any switch order.

mod common;

use aegis_model::{
    AttackTree, ConditionId, ElementRef, GateKind, ItemRef, Node, NodeId, Op, Project,
    ProjectEvalCache, Provenance, ThreatId, TreeId,
};
use common::{accepted, asset_id, builtin, cat, component_id, condition_id, damage_id, feas};
use common::{interface_id, lvl, node_id, seeds, threat_id, tree_id};
use proptest::prelude::*;

type Res<T = ()> = Result<T, Box<dyn std::error::Error>>;

fn prov() -> Provenance {
    Provenance::human("alice", 1_700_000_000_000)
}

/// One asset bound to nothing, one damage; returns the project.
fn base() -> Res<Project> {
    let m = builtin();
    let mut p = Project::new(m.clone());
    for a in [1, 2] {
        p.apply(Op::AddAsset {
            id: asset_id(a),
            name: "ECU".into(),
            properties: m.all_properties(),
            provenance: prov(),
        })?;
    }
    let safety = cat(&m, "safety");
    p.apply(Op::AddDamageScenario {
        id: damage_id(1),
        name: "Crash".into(),
        impacts: vec![(safety, lvl(&m, safety, "severe"))],
        provenance: prov(),
    })?;
    Ok(p)
}

fn add_threat(p: &mut Project, n: u64, asset: u64, tree: TreeId) -> Res {
    let category = p.methodology().threat_categories().next().ok_or("cat")?;
    p.apply(Op::AddThreatScenario {
        id: threat_id(n),
        name: "Threat".into(),
        asset: asset_id(asset),
        category,
        catalog_refs: vec![],
        damages: vec![damage_id(1)],
        provenance: prov(),
    })?;
    p.apply(Op::SetThreatAttackTree {
        threat: threat_id(n),
        tree: Some(tree),
        provenance: prov(),
    })?;
    Ok(())
}

fn add_condition(p: &mut Project, n: u64) -> Res<ConditionId> {
    let id = condition_id(n);
    p.apply(Op::AddCondition {
        id,
        name: "Condition".into(),
        description: "".into(),
        present: true,
        provenance: prov(),
    })?;
    Ok(id)
}

/// Adds tree `t` built by `shape` with every step rated by its position.
fn add_tree(p: &mut Project, t: u64, tree: AttackTree) -> Res<TreeId> {
    let m = p.methodology().clone();
    let keys = ["high", "medium", "low", "very-low"];
    let rated = tree
        .iter()
        .filter(|(_, _, n)| n.gate_kind().is_none())
        .enumerate()
        .map(|(i, (_, id, _))| (id, feas(&m, keys[i % keys.len()])))
        .collect();
    let id = tree_id(t);
    p.apply(Op::AddAttackTree {
        id,
        tree,
        step_feasibility: rated,
        provenance: prov(),
    })?;
    Ok(id)
}

fn need_step(p: &mut Project, c: ConditionId, t: TreeId, n: u64) -> Res {
    p.apply(Op::NeedStepCondition {
        tree: t,
        node: node_id(n),
        condition: c,
        provenance: prov(),
    })?;
    Ok(())
}

fn switch(id: ConditionId, present: bool) -> Op {
    Op::SetConditionPresent {
        id,
        present,
        provenance: prov(),
    }
}

#[test]
fn switch_touches_only_its_step_and_threats() -> Res {
    let mut p = base()?;
    let mut trees = Vec::new();
    for t in 1..=3 {
        let mut tree = AttackTree::new(node_id(t * 100), Node::gate(GateKind::Or, "root"))?;
        for s in 1..=3 {
            tree.add_child(tree.root(), node_id(t * 100 + s), Node::step("step"))?;
        }
        let id = add_tree(&mut p, t, tree)?;
        add_threat(&mut p, t, 1, id)?;
        trees.push(id);
    }
    let c = add_condition(&mut p, 1)?;
    need_step(&mut p, c, trees[0], 101)?;
    let mut cache = ProjectEvalCache::new(&p);
    let others: Vec<_> = trees[1..]
        .iter()
        .map(|&t| cache.evaluate_tree(&p, t).cloned())
        .collect();

    let (nodes, threats) = (cache.recomputed_nodes(), cache.recomputed_threats());
    p.apply_cached(switch(c, false), &mut cache)?;
    assert!(cache.recomputed_nodes() - nodes <= 2 * (1 + 1));
    assert!(cache.recomputed_threats() - threats <= 2);
    assert!(p.applicability().is_node_excluded(trees[0], node_id(101)));
    for (t, before) in trees[1..].iter().zip(&others) {
        assert_eq!(&cache.evaluate_tree(&p, *t).cloned(), before);
    }
    assert_eq!(cache, ProjectEvalCache::new(&p));

    let m = p.methodology().clone();
    let rate = |level: &str| Op::SetStepFeasibility {
        tree: trees[1],
        node: node_id(201),
        level: Some(feas(&m, level)),
        provenance: prov(),
    };
    let nodes = cache.recomputed_nodes();
    p.apply_cached(rate("low"), &mut cache)?;
    let off_cost = cache.recomputed_nodes() - nodes;
    p.apply_cached(switch(c, true), &mut cache)?;
    let nodes = cache.recomputed_nodes();
    p.apply_cached(rate("high"), &mut cache)?;
    assert_eq!(cache.recomputed_nodes() - nodes, off_cost);
    assert!(off_cost <= 4);
    assert_eq!(cache, ProjectEvalCache::new(&p));
    Ok(())
}

/// The shuffled-order fixture: nested OR, AND and SEQ gates, steps via a
/// component and an interface, and six conditions linked to steps and to
/// every element kind.
struct Fixture {
    p: Project,
    conditions: Vec<ConditionId>,
    threats: Vec<ThreatId>,
    toggles: Vec<Toggle>,
}

#[derive(Clone, Copy, Debug)]
enum Toggle {
    Step(ConditionId, TreeId, NodeId),
    Element(ConditionId, ElementRef),
}

fn gate(kind: GateKind) -> Node {
    Node::gate(kind, "gate")
}

#[allow(clippy::too_many_lines)] // one fixture
fn fixture() -> Res<Fixture> {
    let mut p = base()?;
    let (k1, k2, i1) = (component_id(1), component_id(2), interface_id(1));
    for (id, parent) in [(k1, None), (k2, Some(k1))] {
        p.apply(Op::AddComponent {
            id,
            name: "Component".into(),
            parent,
            provenance: prov(),
        })?;
    }
    let kind = p.methodology().interface_kinds().next().ok_or("kind")?;
    p.apply(Op::AddInterface {
        id: i1,
        name: "CAN".into(),
        component: k2,
        kind,
        provenance: prov(),
    })?;
    p.apply(Op::SetAssetComponent {
        asset: asset_id(1),
        component: Some(k2),
        provenance: prov(),
    })?;

    // X: OR(AND(s11, s12), s13); Y: AND(OR(s21, s22), s23); Z: SEQ(s31, OR(s32, s33)).
    let mut x = AttackTree::new(node_id(10), gate(GateKind::Or))?;
    let g = x.add_child(x.root(), node_id(14), gate(GateKind::And))?;
    x.add_child(g, node_id(11), Node::step("s"))?;
    x.add_child(g, node_id(12), Node::step("s"))?;
    x.add_child(x.root(), node_id(13), Node::step("s"))?;
    let mut y = AttackTree::new(node_id(20), gate(GateKind::And))?;
    let g = y.add_child(y.root(), node_id(24), gate(GateKind::Or))?;
    y.add_child(g, node_id(21), Node::step("s"))?;
    y.add_child(g, node_id(22), Node::step("s"))?;
    y.add_child(y.root(), node_id(23), Node::step("s"))?;
    let mut z = AttackTree::new(node_id(30), gate(GateKind::SeqAnd))?;
    z.add_child(z.root(), node_id(31), Node::step("s"))?;
    let g = z.add_child(z.root(), node_id(34), gate(GateKind::Or))?;
    z.add_child(g, node_id(32), Node::step("s"))?;
    z.add_child(g, node_id(33), Node::step("s"))?;
    let (tx, ty, tz) = (
        add_tree(&mut p, 1, x)?,
        add_tree(&mut p, 2, y)?,
        add_tree(&mut p, 3, z)?,
    );
    for (tree, node, via) in [
        (tx, 11, ItemRef::Component(k1)),
        (ty, 22, ItemRef::Interface(i1)),
        (tz, 33, ItemRef::Component(k2)),
    ] {
        p.apply(Op::SetStepVia {
            tree,
            node: node_id(node),
            via: Some(via),
            provenance: prov(),
        })?;
    }
    for (n, asset, tree) in [(1, 1, tx), (2, 1, ty), (3, 2, tz), (4, 2, tx)] {
        add_threat(&mut p, n, asset, tree)?;
    }
    let conditions = (1..=6)
        .map(|n| add_condition(&mut p, n))
        .collect::<Res<Vec<_>>>()?;
    let c = &conditions;
    let toggles = vec![
        Toggle::Step(c[0], tx, node_id(13)),
        Toggle::Step(c[1], ty, node_id(23)),
        Toggle::Step(c[0], tz, node_id(31)),
        Toggle::Element(c[2], ElementRef::Component(k1)),
        Toggle::Element(c[3], ElementRef::Interface(i1)),
        Toggle::Element(c[4], ElementRef::Asset(asset_id(2))),
        Toggle::Element(c[5], ElementRef::Threat(threat_id(1))),
        Toggle::Element(c[5], ElementRef::Tree(tz)),
        Toggle::Element(c[1], ElementRef::Component(k2)),
    ];
    let mut f = Fixture {
        p,
        conditions,
        threats: (1..=4).map(threat_id).collect(),
        toggles,
    };
    // Start with every link in place, so toggles both add and remove links.
    for t in f.toggles.clone() {
        let (need, _) = toggle_ops(t);
        f.p.apply(need)?;
    }
    Ok(f)
}

fn toggle_ops(t: Toggle) -> (Op, Op) {
    match t {
        Toggle::Step(condition, tree, node) => (
            Op::NeedStepCondition {
                tree,
                node,
                condition,
                provenance: prov(),
            },
            Op::UnneedStepCondition {
                tree,
                node,
                condition,
                provenance: prov(),
            },
        ),
        Toggle::Element(condition, element) => (
            Op::NeedCondition {
                element,
                condition,
                provenance: prov(),
            },
            Op::UnneedCondition {
                element,
                condition,
                provenance: prov(),
            },
        ),
    }
}

#[derive(Clone, Copy, Debug)]
enum Switch {
    Present(usize, bool),
    Toggle(usize),
}

fn switches() -> impl Strategy<Value = Vec<Switch>> {
    (
        prop::collection::vec((0..6usize, any::<bool>()), 0..10),
        prop::collection::vec(0..9usize, 0..10),
    )
        .prop_flat_map(|(present, toggles)| {
            let all: Vec<Switch> = present
                .into_iter()
                .map(|(c, v)| Switch::Present(c, v))
                .chain(toggles.into_iter().map(Switch::Toggle))
                .collect();
            Just(all).prop_shuffle()
        })
}

/// Applies `s` to `f.p` through `cache`; returns the inverse op.
fn apply_switch(f: &mut Fixture, s: Switch, cache: &mut ProjectEvalCache) -> Result<Op, String> {
    let op = match s {
        Switch::Present(c, v) => switch(f.conditions[c], v),
        Switch::Toggle(i) => {
            let (need, unneed) = toggle_ops(f.toggles[i]);
            if f.p.validate(&need).is_ok() {
                need
            } else {
                unneed
            }
        }
    };
    f.p.apply_cached(op, cache).map_err(|e| e.to_string())
}

fn assert_full(f: &Fixture, cache: &ProjectEvalCache) -> Result<(), TestCaseError> {
    prop_assert_eq!(cache, &ProjectEvalCache::new(&f.p));
    for &t in &f.threats {
        prop_assert_eq!(cache.evaluate_risk(&f.p, t).cloned(), f.p.evaluate_risk(t));
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn shuffled_switches_match_full_evaluation(ops in switches()) {
        let mut f = fixture().map_err(|e| TestCaseError::fail(e.to_string()))?;
        let mut cache = ProjectEvalCache::new(&f.p);
        let start = cache.clone();
        let mut inverses = Vec::new();
        for s in ops {
            inverses.push(apply_switch(&mut f, s, &mut cache).map_err(TestCaseError::fail)?);
            assert_full(&f, &cache)?;
        }
        for inv in inverses.into_iter().rev() {
            f.p.apply_cached(inv, &mut cache).map_err(|e| TestCaseError::fail(e.to_string()))?;
            assert_full(&f, &cache)?;
        }
        prop_assert_eq!(&cache, &start);
    }

    #[test]
    fn switch_order_does_not_matter(
        (a, b) in prop::collection::vec(any::<bool>(), 6).prop_flat_map(|v| {
            let all: Vec<Switch> = v.into_iter().enumerate().map(|(c, x)| Switch::Present(c, x)).collect();
            (Just(all.clone()).prop_shuffle(), Just(all).prop_shuffle())
        })
    ) {
        let mut ends = Vec::new();
        for order in [a, b] {
            let mut f = fixture().map_err(|e| TestCaseError::fail(e.to_string()))?;
            let mut cache = ProjectEvalCache::new(&f.p);
            for s in order {
                apply_switch(&mut f, s, &mut cache).map_err(TestCaseError::fail)?;
            }
            ends.push((cache, f.p.applicability()));
        }
        prop_assert_eq!(&ends[0], &ends[1]);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn cache_applicability_follows_every_op(seeds in seeds()) {
        let m = builtin();
        let mut cache = ProjectEvalCache::new(&Project::new(m.clone()));
        let mut inverses = Vec::new();
        let mut p = accepted(&m, &seeds, |curr, op, inv| {
            cache.update(curr, op);
            prop_assert_eq!(cache.applicability(), &curr.applicability());
            inverses.push(inv.clone());
            Ok(())
        })?;
        for inv in inverses.into_iter().rev() {
            p.apply_cached(inv, &mut cache).map_err(|e| TestCaseError::fail(e.to_string()))?;
            prop_assert_eq!(cache.applicability(), &p.applicability());
        }
    }
}
