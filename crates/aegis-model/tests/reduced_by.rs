//! The deciding step of a tree and what caps it (job 125, CFG-07).
//!
//! `Project::deciding_step` names the attack step that decides a tree's
//! residual feasibility and lists the ceilings below its rating, lowest first,
//! so the client, the report and the Excel export share one definition.

use std::sync::Arc;

use aegis_model::{
    AttackTree, CeilingSource, ComponentId, ControlId, FeasibilityLevel, GateKind, InterfaceId,
    ItemRef, Methodology, Node, NodeId, Op, Project, Provenance, StepTarget, TreeId,
};

fn methodology() -> Arc<Methodology> {
    Arc::new(Methodology::iso21434_r155().expect("builtin"))
}

fn level(m: &Methodology, key: &str) -> FeasibilityLevel {
    m.feasibility_level(key).expect("level")
}

fn ap(project: &mut Project, op: Op) {
    project.apply(op).expect("apply");
}

fn add_ecu(project: &mut Project) -> ComponentId {
    let ecu = ComponentId::new_v7();
    ap(
        project,
        Op::AddComponent {
            id: ecu,
            name: "ECU".into(),
            parent: None,
            provenance: Provenance::default(),
        },
    );
    ecu
}

fn add_control(project: &mut Project) -> ControlId {
    let id = ControlId::new_v7();
    ap(
        project,
        Op::AddSecurityControl {
            id,
            name: "Control".into(),
            provenance: Provenance::default(),
        },
    );
    id
}

fn apply_control(project: &mut Project, control: ControlId, target: ItemRef) {
    ap(
        project,
        Op::ApplyControl {
            control,
            target,
            provenance: Provenance::default(),
        },
    );
}

fn rule(project: &mut Project, control: ControlId, m: &Methodology, ceiling: FeasibilityLevel) {
    ap(
        project,
        Op::SetControlRule {
            control,
            attack_type: m.attack_type("debug-access").expect("debug-access"),
            ceiling: Some(ceiling),
            provenance: Provenance::default(),
        },
    );
}

fn route(project: &mut Project, tree: TreeId, node: NodeId, via: ItemRef, m: &Methodology) {
    ap(
        project,
        Op::SetStepVia {
            target: StepTarget::Step(tree, node),
            via: Some(via),
            provenance: Provenance::default(),
        },
    );
    ap(
        project,
        Op::SetStepAttackType {
            target: StepTarget::Step(tree, node),
            attack_type: m.attack_type("debug-access"),
            provenance: Provenance::default(),
        },
    );
}

fn mitigate(
    project: &mut Project,
    tree: TreeId,
    node: NodeId,
    control: ControlId,
    ceiling: FeasibilityLevel,
) {
    ap(
        project,
        Op::SetStepMitigation {
            tree,
            node,
            control,
            feasibility: Some(ceiling),
            provenance: Provenance::default(),
        },
    );
}

/// An OR tree with steps A and B; returns (tree, a, b).
fn or_tree(
    project: &mut Project,
    a_level: FeasibilityLevel,
    b_level: FeasibilityLevel,
) -> (TreeId, NodeId, NodeId) {
    let (tree_id, root, a, b) = (
        TreeId::new_v7(),
        NodeId::new_v7(),
        NodeId::new_v7(),
        NodeId::new_v7(),
    );
    let mut tree = AttackTree::new(root, Node::gate(GateKind::Or, "Goal")).expect("tree");
    tree.add_child(tree.root(), a, Node::step("A")).expect("a");
    tree.add_child(tree.root(), b, Node::step("B")).expect("b");
    ap(
        project,
        Op::AddAttackTree {
            role: Default::default(),
            id: tree_id,
            tree,
            step_feasibility: vec![(a, a_level), (b, b_level)],
            provenance: Provenance::default(),
        },
    );
    (tree_id, a, b)
}

fn one_step_tree(project: &mut Project, rated: FeasibilityLevel) -> (TreeId, NodeId) {
    let (tree_id, step) = (TreeId::new_v7(), NodeId::new_v7());
    let tree = AttackTree::new(step, Node::step("Only")).expect("tree");
    ap(
        project,
        Op::AddAttackTree {
            role: Default::default(),
            id: tree_id,
            tree,
            step_feasibility: vec![(step, rated)],
            provenance: Provenance::default(),
        },
    );
    (tree_id, step)
}

#[test]
fn the_capped_deciding_step_lists_its_ceilings_lowest_first() {
    let m = methodology();
    let mut project = Project::new(Arc::clone(&m));
    let (vlow, low, med, high) = (
        level(&m, "very-low"),
        level(&m, "low"),
        level(&m, "medium"),
        level(&m, "high"),
    );
    let ecu = add_ecu(&mut project);
    let x = add_control(&mut project);
    let y = add_control(&mut project);
    apply_control(&mut project, x, ItemRef::Component(ecu));
    rule(&mut project, x, &m, low);
    let (tree_id, a, _b) = or_tree(&mut project, high, vlow);
    route(&mut project, tree_id, a, ItemRef::Component(ecu), &m);
    mitigate(&mut project, tree_id, a, y, med);

    let t_idx = project.attack_tree_idx(&tree_id).expect("idx");
    let eval = project.evaluate_residual_tree(tree_id).expect("eval");
    let d = project.deciding_step(t_idx, &eval).expect("deciding");
    assert_eq!(d.node_id(), a);
    assert_eq!(d.tree(), t_idx);
    assert_eq!(d.rated(), high);
    assert_eq!(d.residual(), low);
    assert!(d.is_capped());
    let c = d.ceilings();
    assert_eq!(c.len(), 2);
    assert_eq!(c[0].control_id(), x);
    assert_eq!(c[0].feasibility(), low);
    assert!(matches!(
        c[0].source(),
        CeilingSource::ControlRule { target, .. } if target == ItemRef::Component(ecu)
    ));
    assert_eq!(c[1].control_id(), y);
    assert_eq!(c[1].feasibility(), med);
    assert_eq!(c[1].source(), CeilingSource::StepMitigation);
}

#[test]
fn an_uncapped_deciding_step_is_returned_with_no_ceilings() {
    let m = methodology();
    let mut project = Project::new(Arc::clone(&m));
    let (low, med, high) = (level(&m, "low"), level(&m, "medium"), level(&m, "high"));
    let ecu = add_ecu(&mut project);
    let x = add_control(&mut project);
    let y = add_control(&mut project);
    apply_control(&mut project, x, ItemRef::Component(ecu));
    rule(&mut project, x, &m, low);
    let (tree_id, a, b) = or_tree(&mut project, high, med);
    route(&mut project, tree_id, a, ItemRef::Component(ecu), &m);
    mitigate(&mut project, tree_id, a, y, med);

    let t_idx = project.attack_tree_idx(&tree_id).expect("idx");
    let eval = project.evaluate_residual_tree(tree_id).expect("eval");
    assert_eq!(eval.root_value(), &med);
    let d = project.deciding_step(t_idx, &eval).expect("deciding");
    assert_eq!(d.node_id(), b);
    assert_eq!(d.rated(), med);
    assert_eq!(d.residual(), med);
    assert!(d.ceilings().is_empty());
    assert!(!d.is_capped());
}

#[test]
fn a_ceiling_at_or_above_the_rating_is_not_listed() {
    let m = methodology();
    let mut project = Project::new(Arc::clone(&m));
    let (low, med) = (level(&m, "low"), level(&m, "medium"));
    let ecu = add_ecu(&mut project);
    let x = add_control(&mut project);
    let y = add_control(&mut project);
    apply_control(&mut project, x, ItemRef::Component(ecu));
    rule(&mut project, x, &m, med);
    let (tree_id, step) = one_step_tree(&mut project, low);
    route(&mut project, tree_id, step, ItemRef::Component(ecu), &m);
    mitigate(&mut project, tree_id, step, y, low);

    let t_idx = project.attack_tree_idx(&tree_id).expect("idx");
    let eval = project.evaluate_residual_tree(tree_id).expect("eval");
    let d = project.deciding_step(t_idx, &eval).expect("deciding");
    assert!(d.ceilings().is_empty());
    assert_eq!(d.residual(), low);
}

#[test]
fn a_setting_on_the_owning_component_names_the_component() {
    let m = methodology();
    let mut project = Project::new(Arc::clone(&m));
    let (low, high) = (level(&m, "low"), level(&m, "high"));
    let ecu = add_ecu(&mut project);
    let jtag = InterfaceId::new_v7();
    ap(
        &mut project,
        Op::AddInterface {
            id: jtag,
            name: "JTAG".into(),
            component: ecu,
            kind: m.interface_kind("debug").expect("debug"),
            provenance: Provenance::default(),
        },
    );
    let x = add_control(&mut project);
    apply_control(&mut project, x, ItemRef::Component(ecu));
    rule(&mut project, x, &m, low);
    let (tree_id, step) = one_step_tree(&mut project, high);
    route(&mut project, tree_id, step, ItemRef::Interface(jtag), &m);

    let t_idx = project.attack_tree_idx(&tree_id).expect("idx");
    let eval = project.evaluate_residual_tree(tree_id).expect("eval");
    let d = project.deciding_step(t_idx, &eval).expect("deciding");
    assert_eq!(d.ceilings().len(), 1);
    assert!(matches!(
        d.ceilings()[0].source(),
        CeilingSource::ControlRule { target, .. } if target == ItemRef::Component(ecu)
    ));
}

#[test]
fn equal_ceilings_are_ordered_by_control_id() {
    let m = methodology();
    let (low, high) = (level(&m, "low"), level(&m, "high"));
    let (c1, c2) = (ControlId::new_v7(), ControlId::new_v7());
    let (first, second) = if c1 < c2 { (c1, c2) } else { (c2, c1) };

    for order in [[c1, c2], [c2, c1]] {
        let mut project = Project::new(Arc::clone(&m));
        let ecu = add_ecu(&mut project);
        for id in [c1, c2] {
            ap(
                &mut project,
                Op::AddSecurityControl {
                    id,
                    name: "Control".into(),
                    provenance: Provenance::default(),
                },
            );
            rule(&mut project, id, &m, low);
        }
        for id in order {
            apply_control(&mut project, id, ItemRef::Component(ecu));
        }
        let (tree_id, step) = one_step_tree(&mut project, high);
        route(&mut project, tree_id, step, ItemRef::Component(ecu), &m);

        let t_idx = project.attack_tree_idx(&tree_id).expect("idx");
        let eval = project.evaluate_residual_tree(tree_id).expect("eval");
        let d = project.deciding_step(t_idx, &eval).expect("deciding");
        let ids: Vec<ControlId> = d.ceilings().iter().map(|c| c.control_id()).collect();
        assert_eq!(ids, [first, second]);
    }
}

#[test]
fn the_wrong_tree_or_evaluation_gives_none() {
    let m = methodology();
    let high = level(&m, "high");
    let mut project = Project::new(Arc::clone(&m));
    let (small_id, _) = one_step_tree(&mut project, high);
    let (big_id, _, _) = or_tree(&mut project, high, high);
    let small_idx = project.attack_tree_idx(&small_id).expect("small");
    let big_idx = project.attack_tree_idx(&big_id).expect("big");
    let small_eval = project.evaluate_residual_tree(small_id).expect("eval");
    let big_eval = project.evaluate_residual_tree(big_id).expect("eval");
    assert!(project.deciding_step(small_idx, &small_eval).is_some());
    assert!(project.deciding_step(big_idx, &big_eval).is_some());

    // Three-node evaluation with the one-node tree.
    assert!(project.deciding_step(small_idx, &big_eval).is_none());

    // An index from a project with two trees, used on a project with one.
    let mut single = Project::new(Arc::clone(&m));
    let (single_id, _) = one_step_tree(&mut single, high);
    let single_eval = single.evaluate_residual_tree(single_id).expect("eval");
    assert!(single.deciding_step(big_idx, &single_eval).is_none());
}
