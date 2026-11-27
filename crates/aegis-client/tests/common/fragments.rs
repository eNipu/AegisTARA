//! Loaded, non-default-provenance references shared by client regressions.
#![allow(dead_code)]

use std::sync::Arc;

use aegis_client::workspace::{Origin, Workspace};
use aegis_model::{
    AttackTree, ConditionId, ControlId, ElementRef, GateKind, Methodology, Node, NodeId, Op,
    Project, Provenance, SharedStepId, StepTarget, TreeId, TreeRole,
};

pub struct Ids {
    pub a: TreeId,
    pub b: TreeId,
    pub unused: TreeId,
    pub first: TreeId,
    pub second: TreeId,
    pub root: NodeId,
    pub ordinary: NodeId,
    pub shared: NodeId,
    pub shared_id: SharedStepId,
    pub user_a: NodeId,
    pub user_b: NodeId,
    pub user_c: NodeId,
    pub transitive: NodeId,
    pub condition: ConditionId,
    pub control: ControlId,
}

pub fn saved(project: &Project) -> Vec<u8> {
    let mut bytes = Vec::new();
    project.save(&mut bytes).expect("save");
    bytes
}

#[allow(clippy::too_many_lines)]
pub fn loaded() -> (Workspace, Ids) {
    let m = Arc::new(Methodology::iso21434_r155().expect("builtin"));
    let mut ws = Workspace::empty(Arc::clone(&m));
    let i = Ids {
        a: TreeId::new_v7(),
        b: TreeId::new_v7(),
        unused: TreeId::new_v7(),
        first: TreeId::new_v7(),
        second: TreeId::new_v7(),
        root: NodeId::new_v7(),
        ordinary: NodeId::new_v7(),
        shared: NodeId::new_v7(),
        shared_id: SharedStepId::new_v7(),
        user_a: NodeId::new_v7(),
        user_b: NodeId::new_v7(),
        user_c: NodeId::new_v7(),
        transitive: NodeId::new_v7(),
        condition: ConditionId::new_v7(),
        control: ControlId::new_v7(),
    };
    let p = || Provenance::human("loaded-fragment-author", 42);
    let rated = NodeId::new_v7();
    let mut a =
        AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::Or, "Fragment A")).expect("A");
    a.add_child(a.root(), rated, Node::step("Rated fragment leaf"))
        .expect("rated child");
    let mut b =
        AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::Or, "Fragment B")).expect("B");
    b.add_child(b.root(), i.user_c, Node::step("B directly uses A"))
        .expect("B child");
    let unused =
        AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::Or, "Unused")).expect("unused");
    let mut first = AttackTree::new(i.root, Node::gate(GateKind::Or, "First")).expect("first");
    for (id, label) in [
        (i.ordinary, "Ordinary"),
        (i.user_a, "First directly uses A"),
        (i.shared, "Shared"),
    ] {
        first
            .add_child(first.root(), id, Node::step(label))
            .expect("child");
    }
    let extra_gate = NodeId::new_v7();
    first
        .add_child(first.root(), extra_gate, Node::gate(GateKind::And, "Extra"))
        .expect("extra gate");
    let mut second =
        AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::Or, "Second")).expect("second");
    for (id, label) in [
        (i.user_b, "Second directly uses A"),
        (i.transitive, "Uses B"),
    ] {
        second
            .add_child(second.root(), id, Node::step(label))
            .expect("child");
    }
    let mut ops = Vec::new();
    for (id, tree, role) in [
        (i.a, a, TreeRole::Fragment),
        (i.b, b, TreeRole::Fragment),
        (i.unused, unused, TreeRole::Fragment),
        (i.first, first, TreeRole::Threat),
        (i.second, second, TreeRole::Threat),
    ] {
        ops.push(Op::AddAttackTree {
            role,
            id,
            tree,
            step_feasibility: Vec::new(),
            provenance: p(),
        });
    }
    ops.extend([
        Op::SetStepFeasibility {
            target: StepTarget::Step(i.a, rated),
            level: Some(m.feasibility_level("low").expect("low")),
            provenance: p(),
        },
        Op::AddSharedStep {
            id: i.shared_id,
            name: "Reusable shared leaf".into(),
            provenance: p(),
        },
        Op::UseSharedStep {
            tree: i.first,
            node: i.shared,
            step: i.shared_id,
            provenance: p(),
        },
        Op::AddCondition {
            id: i.condition,
            name: "Loaded condition".into(),
            description: "".into(),
            present: true,
            provenance: p(),
        },
        Op::NeedCondition {
            element: ElementRef::Tree(i.first),
            condition: i.condition,
            provenance: p(),
        },
        Op::AddSecurityControl {
            id: i.control,
            name: "Loaded mitigation".into(),
            provenance: p(),
        },
        Op::SetDescription {
            target: ElementRef::Tree(i.first),
            text: "Loaded tree description".into(),
            provenance: p(),
        },
        Op::LinkTreeNode {
            tree: i.first,
            parent: extra_gate,
            child: i.user_a,
            position: 0,
            slot: 0,
            provenance: p(),
        },
    ]);
    for (tree, node, fragment) in [
        (i.first, i.user_a, i.a),
        (i.second, i.user_b, i.a),
        (i.b, i.user_c, i.a),
        (i.second, i.transitive, i.b),
    ] {
        ops.push(Op::UseFragment {
            tree,
            node,
            fragment,
            provenance: p(),
        });
    }
    ops.extend([
        Op::NeedStepCondition {
            tree: i.first,
            node: i.user_a,
            condition: i.condition,
            provenance: p(),
        },
        Op::SetStepMitigation {
            tree: i.first,
            node: i.user_a,
            control: i.control,
            feasibility: Some(m.feasibility_level("low").expect("low")),
            provenance: p(),
        },
        Op::SetReviewMark {
            tree: i.first,
            node: i.user_a,
            marked: true,
            provenance: p(),
        },
    ]);
    ws.apply(ops, Origin::Edit).expect("fixture");
    let bytes = saved(ws.project());
    let project = Project::load(bytes.as_slice(), m).expect("loaded fixture");
    assert_eq!(saved(&project), bytes);
    (Workspace::new(project, 0), i)
}
