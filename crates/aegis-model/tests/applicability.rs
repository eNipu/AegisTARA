//! Tests of `Project::applicability`: one hand-built test per rule, and a
//! property test against an oracle that walks owner chains upward.

mod common;

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

use aegis_model::{
    AssetId, AttackTree, ClaimId, ComponentId, ConditionId, ElementRef, GateKind, GoalId,
    InterfaceId, ItemRef, Methodology, Node, NodeId, NodeIdx, Op, Project, Provenance, Reason,
    StepTarget, ThreatId, TreeId, TreeIdx,
};
use proptest::prelude::*;

use common::{accepted, builtin, seeds};

fn who() -> Provenance {
    Provenance::human("alice", 1_700_000_000_000)
}

/// A project with helpers that add one element each and return its ID.
struct Fx {
    m: Arc<Methodology>,
    p: Project,
}

impl Fx {
    fn new() -> Self {
        let m = builtin();
        let p = Project::new(Arc::clone(&m));
        Self { m, p }
    }

    fn apply(&mut self, op: Op) {
        self.p.apply(op).expect("apply");
    }

    fn condition(&mut self, present: bool) -> ConditionId {
        let id = ConditionId::new_v7();
        self.apply(Op::AddCondition {
            id,
            name: "C".into(),
            description: "".into(),
            present,
            provenance: who(),
        });
        id
    }

    fn set(&mut self, id: ConditionId, present: bool) {
        self.apply(Op::SetConditionPresent {
            id,
            present,
            provenance: who(),
        });
    }

    fn need(&mut self, element: ElementRef, condition: ConditionId) {
        self.apply(Op::NeedCondition {
            element,
            condition,
            provenance: who(),
        });
    }

    fn need_step(&mut self, tree: TreeId, node: NodeId, condition: ConditionId) {
        self.apply(Op::NeedStepCondition {
            tree,
            node,
            condition,
            provenance: who(),
        });
    }

    fn component(&mut self, parent: Option<ComponentId>) -> ComponentId {
        let id = ComponentId::new_v7();
        self.apply(Op::AddComponent {
            id,
            name: "Comp".into(),
            parent,
            provenance: who(),
        });
        id
    }

    fn interface(&mut self, component: ComponentId) -> InterfaceId {
        let id = InterfaceId::new_v7();
        let kind = self.m.interface_kinds().next().expect("kind");
        self.apply(Op::AddInterface {
            id,
            name: "If".into(),
            component,
            kind,
            provenance: who(),
        });
        id
    }

    fn asset(&mut self, component: Option<ComponentId>) -> AssetId {
        let id = AssetId::new_v7();
        self.apply(Op::AddAsset {
            id,
            name: "Asset".into(),
            properties: self.m.all_properties(),
            provenance: who(),
        });
        if component.is_some() {
            self.apply(Op::SetAssetComponent {
                asset: id,
                component,
                provenance: who(),
            });
        }
        id
    }

    fn threat(&mut self, asset: AssetId, tree: Option<TreeId>) -> ThreatId {
        let id = ThreatId::new_v7();
        let category = self.m.threat_categories().next().expect("category");
        self.apply(Op::AddThreatScenario {
            id,
            name: "Threat".into(),
            asset,
            category,
            catalog_refs: vec![],
            damages: vec![],
            provenance: who(),
        });
        if tree.is_some() {
            self.apply(Op::SetThreatAttackTree {
                threat: id,
                tree,
                provenance: who(),
            });
        }
        id
    }

    /// A tree whose root gate of `kind` has `steps` step children.
    fn tree(&mut self, kind: GateKind, steps: usize) -> (TreeId, NodeId, Vec<NodeId>) {
        let (id, root) = (TreeId::new_v7(), NodeId::new_v7());
        let mut t = AttackTree::new(root, Node::gate(kind, "root")).expect("tree");
        let ids: Vec<_> = (0..steps).map(|_| NodeId::new_v7()).collect();
        for s in &ids {
            t.add_child(t.root(), *s, Node::step("s")).expect("step");
        }
        self.apply(Op::AddAttackTree {
            role: Default::default(),
            id,
            tree: t,
            step_feasibility: vec![],
            provenance: who(),
        });
        (id, root, ids)
    }

    fn via(&mut self, tree: TreeId, node: NodeId, item: ItemRef) {
        self.apply(Op::SetStepVia {
            target: StepTarget::Step(tree, node),
            via: Some(item),
            provenance: who(),
        });
    }

    fn goal(&mut self) -> GoalId {
        let id = GoalId::new_v7();
        self.apply(Op::AddCybersecurityGoal {
            id,
            name: "Goal".into(),
            controls: vec![],
            provenance: who(),
        });
        id
    }

    fn claim(&mut self) -> ClaimId {
        let id = ClaimId::new_v7();
        self.apply(Op::AddCybersecurityClaim {
            id,
            name: "Claim".into(),
            provenance: who(),
        });
        id
    }
}

fn owner(e: ElementRef) -> Reason {
    Reason::OwnerExcluded(e)
}

#[test]
fn condition_off_on_each_element_kind_and_on_a_step() {
    let mut f = Fx::new();
    let cf = f.condition(true);
    let comp = f.component(None);
    let other = f.component(None);
    let iface = f.interface(other);
    let asset = f.asset(None);
    let threat_asset = f.asset(None);
    let threat = f.threat(threat_asset, None);
    let (tree, _, _) = f.tree(GateKind::Or, 1);
    let (step_tree, step_root, steps) = f.tree(GateKind::Or, 2);
    let (goal, claim) = (f.goal(), f.claim());
    let elements = [
        ElementRef::Component(comp),
        ElementRef::Interface(iface),
        ElementRef::Asset(asset),
        ElementRef::Threat(threat),
        ElementRef::Tree(tree),
        ElementRef::Goal(goal),
        ElementRef::Claim(claim),
    ];
    for e in elements {
        f.need(e, cf);
    }
    f.need_step(step_tree, steps[0], cf);
    assert!(f.p.applicability().is_empty(), "present condition");

    f.set(cf, false);
    let a = f.p.applicability();
    for e in elements {
        assert_eq!(a.reasons(e), &[Reason::ConditionOff(cf)], "{e:?}");
    }
    assert_eq!(a.excluded_elements().count(), elements.len());
    assert_eq!(
        a.node_reasons(step_tree, steps[0]),
        &[Reason::ConditionOff(cf)]
    );
    assert!(!a.is_node_excluded(step_tree, steps[1]));
    assert!(
        !a.is_node_excluded(step_tree, step_root),
        "OR has a live child"
    );
    assert_eq!(a.excluded_nodes().count(), 1);

    f.set(cf, true);
    assert!(f.p.applicability().is_empty(), "switched back on");
}

#[test]
fn owner_edges_propagate_downward() {
    let mut f = Fx::new();
    let cf = f.condition(false);
    let top = f.component(None);
    let mid = f.component(Some(top));
    let leaf = f.component(Some(mid));
    let iface = f.interface(top);
    let asset = f.asset(Some(top));
    let (tree, _, _) = f.tree(GateKind::Or, 1);
    let threat = f.threat(asset, Some(tree));
    f.need(ElementRef::Component(top), cf);

    let a = f.p.applicability();
    let c = ElementRef::Component;
    assert_eq!(a.reasons(c(top)), &[Reason::ConditionOff(cf)]);
    assert_eq!(a.reasons(c(mid)), &[owner(c(top))]);
    assert_eq!(a.reasons(c(leaf)), &[owner(c(mid))]);
    assert_eq!(a.reasons(ElementRef::Interface(iface)), &[owner(c(top))]);
    assert_eq!(a.reasons(ElementRef::Asset(asset)), &[owner(c(top))]);
    let asset_ref = ElementRef::Asset(asset);
    assert_eq!(a.reasons(ElementRef::Threat(threat)), &[owner(asset_ref)]);
    let threat_ref = ElementRef::Threat(threat);
    assert_eq!(a.reasons(ElementRef::Tree(tree)), &[owner(threat_ref)]);
    assert_eq!(
        a.excluded_nodes().count(),
        0,
        "owner reasons do not reach nodes"
    );
}

#[test]
fn a_shared_tree_stays_live_while_one_threat_is_live() {
    let mut f = Fx::new();
    let (cf1, cf2) = (f.condition(false), f.condition(true));
    let (a1, a2) = (f.asset(None), f.asset(None));
    let (tree, _, _) = f.tree(GateKind::Or, 1);
    let t1 = f.threat(a1, Some(tree));
    let t2 = f.threat(a2, Some(tree));
    f.need(ElementRef::Asset(a1), cf1);
    f.need(ElementRef::Asset(a2), cf2);

    let a = f.p.applicability();
    assert!(a.is_excluded(ElementRef::Threat(t1)));
    assert!(!a.is_excluded(ElementRef::Threat(t2)));
    assert!(!a.is_excluded(ElementRef::Tree(tree)));

    f.set(cf2, false);
    let a = f.p.applicability();
    let mut want = vec![owner(ElementRef::Threat(t1)), owner(ElementRef::Threat(t2))];
    want.sort();
    assert_eq!(a.reasons(ElementRef::Tree(tree)), want.as_slice());
}

#[test]
fn steps_through_excluded_items() {
    let mut f = Fx::new();
    let cf = f.condition(false);
    let comp = f.component(None);
    let iface = f.interface(comp);
    let live = f.component(None);
    let (tree, root, steps) = f.tree(GateKind::Or, 3);
    f.via(tree, steps[0], ItemRef::Component(comp));
    f.via(tree, steps[1], ItemRef::Interface(iface));
    f.via(tree, steps[2], ItemRef::Component(live));
    f.need(ElementRef::Component(comp), cf);

    let a = f.p.applicability();
    let via_comp = Reason::ViaExcluded(ItemRef::Component(comp));
    assert_eq!(a.node_reasons(tree, steps[0]), &[via_comp]);
    let via_iface = Reason::ViaExcluded(ItemRef::Interface(iface));
    assert_eq!(a.node_reasons(tree, steps[1]), &[via_iface]);
    assert!(!a.is_node_excluded(tree, steps[2]));
    assert!(!a.is_node_excluded(tree, root));
}

#[test]
fn goals_and_claims_are_not_excluded_by_their_threat() {
    let mut f = Fx::new();
    let cf = f.condition(false);
    let asset = f.asset(None);
    let threat = f.threat(asset, None);
    let (goal, claim) = (f.goal(), f.claim());
    f.apply(Op::SetThreatGoal {
        threat,
        goal: Some(goal),
        provenance: who(),
    });
    let other = f.threat(asset, None);
    f.apply(Op::SetThreatClaim {
        threat: other,
        claim: Some(claim),
        provenance: who(),
    });
    f.need(ElementRef::Threat(threat), cf);
    f.need(ElementRef::Threat(other), cf);

    let a = f.p.applicability();
    assert!(a.is_excluded(ElementRef::Threat(threat)));
    assert!(!a.is_excluded(ElementRef::Goal(goal)));
    assert!(!a.is_excluded(ElementRef::Claim(claim)));
}

#[test]
fn gate_reasons_per_kind() {
    let mut f = Fx::new();
    let cf = f.condition(false);
    // OR: excluded only when every child is.
    let (or_tree, or_root, or_steps) = f.tree(GateKind::Or, 2);
    f.need_step(or_tree, or_steps[0], cf);
    assert!(!f.p.applicability().is_node_excluded(or_tree, or_root));
    f.need_step(or_tree, or_steps[1], cf);
    // AND and SAND: one excluded child is enough.
    let (and_tree, and_root, and_steps) = f.tree(GateKind::And, 2);
    f.need_step(and_tree, and_steps[1], cf);
    let (seq_tree, seq_root, seq_steps) = f.tree(GateKind::SeqAnd, 2);
    f.need_step(seq_tree, seq_steps[0], cf);

    let a = f.p.applicability();
    let all = [Reason::AllPathsExcluded];
    for (tree, root) in [
        (or_tree, or_root),
        (and_tree, and_root),
        (seq_tree, seq_root),
    ] {
        assert_eq!(a.node_reasons(tree, root), &all, "{tree:?}");
        assert_eq!(a.reasons(ElementRef::Tree(tree)), &all, "{tree:?}");
    }
    assert!(!a.is_node_excluded(and_tree, and_steps[0]));
}

#[test]
fn a_nested_gate_excludes_only_itself_when_the_root_stays_live() {
    let mut f = Fx::new();
    let cf = f.condition(false);
    let (tree, root, inner, step) = (
        TreeId::new_v7(),
        NodeId::new_v7(),
        NodeId::new_v7(),
        NodeId::new_v7(),
    );
    let mut t = AttackTree::new(root, Node::gate(GateKind::Or, "root")).expect("tree");
    let g = t
        .add_child(t.root(), inner, Node::gate(GateKind::And, "and"))
        .expect("gate");
    t.add_child(g, step, Node::step("s")).expect("step");
    t.add_child(g, NodeId::new_v7(), Node::step("s2"))
        .expect("step");
    t.add_child(t.root(), NodeId::new_v7(), Node::step("live"))
        .expect("step");
    f.apply(Op::AddAttackTree {
        role: Default::default(),
        id: tree,
        tree: t,
        step_feasibility: vec![],
        provenance: who(),
    });
    f.need_step(tree, step, cf);
    let a = f.p.applicability();
    assert_eq!(a.node_reasons(tree, inner), &[Reason::AllPathsExcluded]);
    assert!(!a.is_node_excluded(tree, root));
    assert!(!a.is_excluded(ElementRef::Tree(tree)));
}

#[test]
fn reasons_are_sorted_and_unique() {
    let mut f = Fx::new();
    let (c1, c2, c3) = (f.condition(false), f.condition(false), f.condition(false));
    let top = f.component(None);
    let child = f.component(Some(top));
    f.need(ElementRef::Component(top), c1);
    // Needed in reverse order, plus an owner reason.
    f.need(ElementRef::Component(child), c3);
    f.need(ElementRef::Component(child), c2);

    let a = f.p.applicability();
    let mut want = vec![
        Reason::ConditionOff(c2),
        Reason::ConditionOff(c3),
        owner(ElementRef::Component(top)),
    ];
    want.sort();
    assert_eq!(a.reasons(ElementRef::Component(child)), want.as_slice());
}

// -- Oracle --------------------------------------------------------------------

/// Whether `e` has an off condition of its own.
fn own_off(p: &Project, e: ElementRef) -> bool {
    p.condition_link_ids(e)
        .iter()
        .any(|&i| p.condition(i).is_some_and(|c| !c.present()))
}

/// A component is excluded when it or any ancestor has an off condition.
fn comp_out(p: &Project, id: ComponentId) -> bool {
    let mut at = p.component_idx(&id);
    while let Some(i) = at {
        let Some(cid) = p.component_id(i) else { break };
        if own_off(p, ElementRef::Component(cid)) {
            return true;
        }
        at = p.component(i).and_then(|c| c.parent());
    }
    false
}

fn iface_out(p: &Project, id: InterfaceId) -> bool {
    let owner = p
        .interface_idx(&id)
        .and_then(|i| p.interface(i))
        .and_then(|i| p.component_id(i.component()));
    own_off(p, ElementRef::Interface(id)) || owner.is_some_and(|c| comp_out(p, c))
}

fn asset_out(p: &Project, id: AssetId) -> bool {
    let owner = p
        .asset_idx(&id)
        .and_then(|i| p.asset(i))
        .and_then(|a| a.component())
        .and_then(|c| p.component_id(c));
    own_off(p, ElementRef::Asset(id)) || owner.is_some_and(|c| comp_out(p, c))
}

fn threat_out(p: &Project, id: ThreatId) -> bool {
    let info = p
        .threat_scenarios()
        .find(|(_, t, _)| *t == id)
        .map(|(_, _, t)| (p.asset_id(t.asset()), t.scales()));
    let asset = info.and_then(|(a, _)| a);
    let scales_out = info.is_some_and(|(_, s)| !p.attack_filter().allows_scalability(s));
    own_off(p, ElementRef::Threat(id)) || scales_out || asset.is_some_and(|a| asset_out(p, a))
}

fn step_out(p: &Project, t: TreeIdx, tid: TreeId, n: NodeIdx, nid: NodeId) -> bool {
    let own = p
        .step_condition_link_ids(tid, nid)
        .iter()
        .any(|&i| p.condition(i).is_some_and(|c| !c.present()));
    let via = p.step_via(t, n).is_some_and(|v| match v {
        ItemRef::Component(c) => comp_out(p, c),
        ItemRef::Interface(i) => iface_out(p, i),
    });
    let profile = p.attack_filter().has_excluded_levels()
        && p.step_factors(t, n)
            .any(|(f, l)| p.attack_filter().is_excluded(f, l));
    own || via || profile
}

/// Naive recursion over gates; `true` means excluded.
fn node_out(
    p: &Project,
    t: TreeIdx,
    tree: &AttackTree,
    n: NodeIdx,
    memo: &mut HashMap<NodeIdx, bool>,
) -> bool {
    if let Some(v) = memo.get(&n) {
        return *v;
    }
    let tid = p.attack_tree_id(t).expect("tree id");
    let v = match tree.get(n).and_then(Node::gate_kind) {
        None => step_out(p, t, tid, n, tree.id_of(n).expect("node id")),
        Some(kind) => {
            let kids: Vec<_> = tree
                .children(n)
                .iter()
                .map(|&c| node_out(p, t, tree, c, memo))
                .collect();
            if kind == GateKind::Or {
                kids.iter().all(|k| *k)
            } else {
                kids.iter().any(|k| *k)
            }
        }
    };
    memo.insert(n, v);
    v
}

/// The excluded elements and nodes, computed by walking upward.
fn oracle(p: &Project) -> (BTreeSet<ElementRef>, BTreeSet<(TreeId, NodeId)>) {
    let mut els = BTreeSet::new();
    let mut nodes = BTreeSet::new();
    let mut add = |e: ElementRef, out: bool| {
        if out {
            els.insert(e);
        }
    };
    for (_, id, _) in p.components() {
        add(ElementRef::Component(id), comp_out(p, id));
    }
    for (_, id, _) in p.interfaces() {
        add(ElementRef::Interface(id), iface_out(p, id));
    }
    for (_, id, _) in p.assets() {
        add(ElementRef::Asset(id), asset_out(p, id));
    }
    for (_, id, _) in p.threat_scenarios() {
        add(ElementRef::Threat(id), threat_out(p, id));
    }
    for (_, id, _) in p.cybersecurity_goals() {
        add(ElementRef::Goal(id), own_off(p, ElementRef::Goal(id)));
    }
    for (_, id, _) in p.cybersecurity_claims() {
        add(ElementRef::Claim(id), own_off(p, ElementRef::Claim(id)));
    }
    for (t, id, tree) in p.attack_trees() {
        let users: Vec<_> = p
            .threat_scenarios()
            .filter(|(_, _, th)| th.attack_tree() == Some(t))
            .map(|(_, th, _)| th)
            .collect();
        let by_threats = !users.is_empty() && users.iter().all(|th| threat_out(p, *th));
        let mut memo = HashMap::new();
        let any_step = tree
            .iter()
            .any(|(n, _, node)| node.gate_kind().is_none() && node_out(p, t, tree, n, &mut memo));
        let mut root_out = false;
        // `evaluate` refuses a tree with an empty gate: then only steps.
        let empty_gate = tree
            .iter()
            .any(|(n, _, node)| node.gate_kind().is_some() && tree.children(n).is_empty());
        if any_step && !empty_gate {
            for (n, nid, _) in tree.iter() {
                if node_out(p, t, tree, n, &mut memo) {
                    nodes.insert((id, nid));
                }
            }
            root_out = node_out(p, t, tree, tree.root(), &mut memo);
        } else if any_step {
            for (n, nid, node) in tree.iter() {
                if node.gate_kind().is_none() && node_out(p, t, tree, n, &mut memo) {
                    nodes.insert((id, nid));
                }
            }
        }
        add(
            ElementRef::Tree(id),
            own_off(p, ElementRef::Tree(id)) || by_threats || root_out,
        );
    }
    (els, nodes)
}

fn check(p: &Project) -> Result<(), TestCaseError> {
    let a = p.applicability();
    let (want_els, want_nodes) = oracle(p);
    let got_els: BTreeSet<_> = a.excluded_elements().map(|(e, _)| e).collect();
    let got_nodes: BTreeSet<_> = a.excluded_nodes().map(|(t, n, _)| (t, n)).collect();
    prop_assert_eq!(&got_els, &want_els);
    prop_assert_eq!(&got_nodes, &want_nodes);
    prop_assert_eq!(a.is_empty(), want_els.is_empty() && want_nodes.is_empty());
    let lists = a
        .excluded_elements()
        .map(|(_, r)| r)
        .chain(a.excluded_nodes().map(|(_, _, r)| r));
    for r in lists {
        prop_assert!(!r.is_empty());
        prop_assert!(r.windows(2).all(|w| w[0] < w[1]), "sorted, unique: {:?}", r);
    }
    // Own off conditions appear as `ConditionOff` reasons, exactly.
    for (e, reasons) in a.excluded_elements() {
        let own: BTreeSet<_> = p
            .condition_link_ids(e)
            .iter()
            .filter(|&&i| p.condition(i).is_some_and(|c| !c.present()))
            .filter_map(|&i| p.condition_id(i))
            .collect();
        let got: BTreeSet<_> = reasons
            .iter()
            .filter_map(|r| match r {
                Reason::ConditionOff(c) => Some(*c),
                _ => None,
            })
            .collect();
        prop_assert_eq!(got, own);
    }
    Ok(())
}

/// Item `n` modulo the length, or `None` when empty.
fn pick<T: Copy>(v: &[T], n: usize) -> Option<T> {
    (!v.is_empty()).then(|| v[n % v.len()])
}

/// Random structure the generator builds rarely: child components, bound
/// assets, threats with trees. Refused ops are skipped.
fn shape(p: &mut Project, choices: &[(usize, usize, u8)]) {
    for &(a, b, k) in choices {
        let comps: Vec<_> = p.components().map(|(_, id, _)| id).collect();
        let assets: Vec<_> = p.assets().map(|(_, id, _)| id).collect();
        let threats: Vec<_> = p.threat_scenarios().map(|(_, id, _)| id).collect();
        let trees: Vec<_> = p.attack_trees().map(|(_, id, _)| id).collect();
        let op = match k % 3 {
            0 => Op::AddComponent {
                id: ComponentId::new_v7(),
                name: "Shape".into(),
                parent: pick(&comps, a),
                provenance: who(),
            },
            1 => match pick(&assets, a) {
                Some(asset) => Op::SetAssetComponent {
                    asset,
                    component: pick(&comps, b),
                    provenance: who(),
                },
                None => continue,
            },
            _ => match pick(&threats, a) {
                Some(threat) => Op::SetThreatAttackTree {
                    threat,
                    tree: pick(&trees, b),
                    provenance: who(),
                },
                None => continue,
            },
        };
        let _ = p.apply(op);
    }
}

/// Adds three conditions and random links, so most cases exclude
/// something; refused ops are skipped (the generator links rarely).
fn salt(p: &mut Project, choices: &[(usize, usize, u8)]) {
    let conditions: Vec<_> = (0..3).map(|_| ConditionId::new_v7()).collect();
    for &id in &conditions {
        let _ = p.apply(Op::AddCondition {
            id,
            name: "Salt".into(),
            description: "".into(),
            present: true,
            provenance: who(),
        });
    }
    let mut elements: Vec<ElementRef> = Vec::new();
    elements.extend(p.components().map(|(_, id, _)| ElementRef::Component(id)));
    elements.extend(p.interfaces().map(|(_, id, _)| ElementRef::Interface(id)));
    elements.extend(p.assets().map(|(_, id, _)| ElementRef::Asset(id)));
    elements.extend(
        p.threat_scenarios()
            .map(|(_, id, _)| ElementRef::Threat(id)),
    );
    elements.extend(p.attack_trees().map(|(_, id, _)| ElementRef::Tree(id)));
    elements.extend(
        p.cybersecurity_goals()
            .map(|(_, id, _)| ElementRef::Goal(id)),
    );
    elements.extend(
        p.cybersecurity_claims()
            .map(|(_, id, _)| ElementRef::Claim(id)),
    );
    let steps: Vec<(TreeId, NodeId)> = p
        .attack_trees()
        .flat_map(|(_, t, tree)| {
            tree.iter()
                .filter(|(_, _, n)| n.gate_kind().is_none())
                .map(move |(_, n, _)| (t, n))
                .collect::<Vec<_>>()
        })
        .collect();
    let mut items: Vec<ItemRef> = p
        .components()
        .map(|(_, id, _)| ItemRef::Component(id))
        .collect();
    items.extend(p.interfaces().map(|(_, id, _)| ItemRef::Interface(id)));
    for &(a, b, k) in choices {
        let condition = conditions[b % conditions.len()];
        let op = match k % 3 {
            0 if !elements.is_empty() => Op::NeedCondition {
                element: elements[a % elements.len()],
                condition,
                provenance: who(),
            },
            1 if !steps.is_empty() => {
                let (tree, node) = steps[a % steps.len()];
                Op::NeedStepCondition {
                    tree,
                    node,
                    condition,
                    provenance: who(),
                }
            }
            2 if !steps.is_empty() && !items.is_empty() => {
                let (tree, node) = steps[a % steps.len()];
                let via = Some(items[b % items.len()]);
                Op::SetStepVia {
                    target: StepTarget::Step(tree, node),
                    via,
                    provenance: who(),
                }
            }
            _ => continue,
        };
        let _ = p.apply(op);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn matches_an_upward_walking_oracle(
        seeds in seeds(),
        choices in prop::collection::vec((any::<usize>(), any::<usize>(), any::<u8>()), 0..24),
        bits in any::<u64>(),
    ) {
        let mut p = accepted(&builtin(), &seeds, |_, _, _| Ok(()))?;
        check(&p)?;
        shape(&mut p, &choices);
        salt(&mut p, &choices);
        let toggles: BTreeMap<_, _> = p
            .conditions()
            .enumerate()
            .map(|(i, (_, id, c))| (id, (bits >> (i % 64)) & 1 == 1, c.present()))
            .map(|(id, want, now)| (id, (want, now)))
            .collect();
        for (id, (want, now)) in toggles {
            if want != now {
                p.apply(Op::SetConditionPresent { id, present: want, provenance: who() })
                    .expect("toggle");
            }
        }
        check(&p)?;
    }
}
