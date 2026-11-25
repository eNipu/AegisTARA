//! Attack step "through" item and attack type (jobs 122, CFG-01 and CFG-02):
//! the rules of the two ops, what blocks removals, and the store round trip.

mod common;

use std::sync::Arc;

use aegis_model::{
    AttackTree, ComponentId, GateKind, InterfaceId, ItemRef, Methodology, ModelError, Node, NodeId,
    Op, OpCodec, OpLogReader, OpLogWriter, Project, Provenance, StepTarget, TreeId,
};

struct Fx {
    project: Project,
    tree: TreeId,
    gate: NodeId,
    step: NodeId,
    ecu: ComponentId,
    jtag: InterfaceId,
}

fn methodology() -> Arc<Methodology> {
    Arc::new(Methodology::iso21434_r155().expect("builtin"))
}

fn fixture() -> Fx {
    let m = methodology();
    let mut project = Project::new(Arc::clone(&m));
    let (ecu, jtag, tree, root, step) = (
        ComponentId::new_v7(),
        InterfaceId::new_v7(),
        TreeId::new_v7(),
        NodeId::new_v7(),
        NodeId::new_v7(),
    );
    let mut t = AttackTree::new(root, Node::gate(GateKind::Or, "Goal")).expect("tree");
    t.add_child(t.root(), step, Node::step("Open the debug port"))
        .expect("step");
    let kind = m.interface_kind("debug").expect("kind");
    for op in [
        Op::AddComponent {
            id: ecu,
            name: "ADAS ECU".into(),
            parent: None,
            provenance: Provenance::default(),
        },
        Op::AddInterface {
            id: jtag,
            name: "JTAG".into(),
            component: ecu,
            kind,
            provenance: Provenance::default(),
        },
        Op::AddAttackTree {
            id: tree,
            tree: t,
            step_feasibility: vec![],
            provenance: Provenance::default(),
        },
    ] {
        project.apply(op).expect("setup");
    }
    Fx {
        project,
        tree,
        gate: root,
        step,
        ecu,
        jtag,
    }
}

fn set_via(fx: &mut Fx, via: Option<ItemRef>) -> Result<Op, ModelError> {
    fx.project.apply(Op::SetStepVia {
        target: StepTarget::Step(fx.tree, fx.step),
        via,
        provenance: Provenance::default(),
    })
}

fn set_type(fx: &mut Fx, key: Option<&str>) -> Result<Op, ModelError> {
    let attack_type = key.map(|k| fx.project.methodology().attack_type(k).expect("type"));
    fx.project.apply(Op::SetStepAttackType {
        target: StepTarget::Step(fx.tree, fx.step),
        attack_type,
        provenance: Provenance::default(),
    })
}

#[test]
fn via_and_attack_type_are_set_read_and_undone_exactly() {
    let mut fx = fixture();
    let jtag = fx.jtag;
    let before = fx.project.clone();
    let undo_via = set_via(&mut fx, Some(ItemRef::Interface(jtag))).expect("via");
    let undo_type = set_type(&mut fx, Some("debug-access")).expect("type");
    let tree_idx = fx.project.attack_tree_idx(&fx.tree).expect("tree");
    let step_idx = fx
        .project
        .attack_tree(tree_idx)
        .and_then(|t| t.idx_of(&fx.step))
        .expect("step");
    assert_eq!(
        fx.project.step_via(tree_idx, step_idx),
        Some(ItemRef::Interface(fx.jtag))
    );
    let m = fx.project.methodology();
    let t = fx.project.step_attack_type(tree_idx, step_idx).expect("t");
    assert_eq!(m.attack_type_key(t), Some("debug-access"));
    assert_eq!(
        fx.project
            .steps_via(ItemRef::Interface(fx.jtag))
            .collect::<Vec<_>>(),
        vec![(tree_idx, step_idx)]
    );
    assert_eq!(fx.project.steps_via(ItemRef::Component(fx.ecu)).count(), 0);
    fx.project.apply(undo_type).expect("undo type");
    fx.project.apply(undo_via).expect("undo via");
    assert_eq!(fx.project, before);
}

#[test]
fn a_gate_is_not_a_step() {
    let mut fx = fixture();
    let via = Op::SetStepVia {
        target: StepTarget::Step(fx.tree, fx.gate),
        via: Some(ItemRef::Component(fx.ecu)),
        provenance: Provenance::default(),
    };
    let want = ModelError::NotAStep {
        tree: fx.tree,
        node: fx.gate,
    };
    assert_eq!(fx.project.apply(via), Err(want.clone()));
    let attack_type = fx.project.methodology().attack_type("debug-access");
    let kind = Op::SetStepAttackType {
        target: StepTarget::Step(fx.tree, fx.gate),
        attack_type,
        provenance: Provenance::default(),
    };
    assert_eq!(fx.project.apply(kind), Err(want));
}

#[test]
fn unknown_items_and_attack_types_are_rejected() {
    let mut fx = fixture();
    let ghost = ComponentId::new_v7();
    assert_eq!(
        set_via(&mut fx, Some(ItemRef::Component(ghost))),
        Err(ModelError::UnknownComponent(ghost))
    );
    let ghost = InterfaceId::new_v7();
    assert_eq!(
        set_via(&mut fx, Some(ItemRef::Interface(ghost))),
        Err(ModelError::UnknownInterface(ghost))
    );
}

#[test]
fn an_attack_type_of_another_methodology_is_rejected() {
    // The ISO pack has attack types; the second pack has none.
    let far = methodology().attack_types().nth(9).expect("tenth type");
    let m = common::second();
    let mut project = Project::new(Arc::clone(&m));
    let (tree, root) = (TreeId::new_v7(), NodeId::new_v7());
    project
        .apply(Op::AddAttackTree {
            id: tree,
            tree: AttackTree::new(root, Node::step("Step")).expect("tree"),
            step_feasibility: vec![],
            provenance: Provenance::default(),
        })
        .expect("tree");
    let op = |attack_type| Op::SetStepAttackType {
        target: StepTarget::Step(tree, root),
        attack_type,
        provenance: Provenance::default(),
    };
    assert_eq!(
        project.apply(op(Some(far))),
        Err(ModelError::UnknownAttackType(far))
    );
    project.apply(op(None)).expect("clearing is always fine");
}

#[test]
fn removing_what_a_step_goes_through_is_rejected_until_it_is_cleared() {
    let mut fx = fixture();
    let (ecu, jtag) = (fx.ecu, fx.jtag);
    set_via(&mut fx, Some(ItemRef::Interface(jtag))).expect("via");
    assert_eq!(
        fx.project.apply(Op::RemoveInterface { id: fx.jtag }),
        Err(ModelError::InterfaceHasSteps {
            interface: fx.jtag,
            steps: 1
        })
    );
    set_via(&mut fx, Some(ItemRef::Component(ecu))).expect("via component");
    fx.project
        .apply(Op::RemoveInterface { id: fx.jtag })
        .expect("interface is free now");
    assert_eq!(
        fx.project.apply(Op::RemoveComponent { id: fx.ecu }),
        Err(ModelError::ComponentHasSteps {
            component: fx.ecu,
            steps: 1
        })
    );
    set_via(&mut fx, None).expect("clear");
    fx.project
        .apply(Op::RemoveComponent { id: fx.ecu })
        .expect("component is free now");
}

#[test]
fn a_step_with_a_via_or_type_cannot_be_removed_from_its_tree() {
    let mut fx = fixture();
    set_type(&mut fx, Some("debug-access")).expect("type");
    assert_eq!(
        fx.project.apply(Op::RemoveTreeNode {
            tree: fx.tree,
            id: fx.step,
            provenance: Provenance::default(),
        }),
        Err(ModelError::NodeIsRated(fx.step))
    );
    // The root is a gate here; a tree whose step has a type keeps it.
    let err = fx.project.apply(Op::RemoveAttackTree { id: fx.tree });
    assert!(matches!(
        err,
        Err(ModelError::AttackTreeHasStepLinks { steps: 1, .. })
    ));
    set_type(&mut fx, None).expect("clear");
    fx.project
        .apply(Op::RemoveTreeNode {
            tree: fx.tree,
            id: fx.step,
            provenance: Provenance::default(),
        })
        .expect("free now");
}

#[test]
fn snapshot_and_op_log_round_trip_and_stay_absent_when_empty() {
    let mut fx = fixture();
    let jtag = fx.jtag;
    let plain = {
        let mut out = Vec::new();
        fx.project.save(&mut out).expect("save");
        String::from_utf8(out).expect("utf8")
    };
    assert!(!plain.contains("step_via") && !plain.contains("step_attack_type"));

    let undo_via = set_via(&mut fx, Some(ItemRef::Interface(jtag))).expect("via");
    set_type(&mut fx, Some("debug-access")).expect("type");
    let mut out = Vec::new();
    fx.project.save(&mut out).expect("save");
    let text = String::from_utf8(out.clone()).expect("utf8");
    assert!(text.contains("\"step_via\"") && text.contains("\"interface\""));
    assert!(text.contains("\"step_attack_type\"") && text.contains("debug-access"));
    let loaded = Project::load(out.as_slice(), methodology()).expect("load");
    assert_eq!(loaded, fx.project);

    // Both ops and an inverse survive the op log.
    let mut bytes = Vec::new();
    let mut log = OpLogWriter::create(&mut bytes, methodology()).expect("header");
    for op in [
        Op::SetStepVia {
            target: StepTarget::Step(fx.tree, fx.step),
            via: Some(ItemRef::Component(fx.ecu)),
            provenance: Provenance::default(),
        },
        Op::SetStepAttackType {
            target: StepTarget::Step(fx.tree, fx.step),
            attack_type: fx.project.methodology().attack_type("side-channel"),
            provenance: Provenance::default(),
        },
        undo_via,
    ] {
        log.append(&op).expect("append");
    }
    drop(log);
    let ops: Vec<_> = OpLogReader::open(bytes.as_slice(), methodology())
        .expect("open")
        .map(|r| r.expect("record").op)
        .collect();
    assert_eq!(ops.len(), 3);
    assert!(matches!(
        ops[0],
        Op::SetStepVia {
            via: Some(ItemRef::Component(_)),
            ..
        }
    ));
    assert!(matches!(
        ops[1],
        Op::SetStepAttackType {
            attack_type: Some(_),
            ..
        }
    ));
    let codec = OpCodec::new(methodology());
    let op = &ops[0];
    assert_eq!(codec.decode(&codec.encode(op).expect("e")).expect("d"), *op);
}

#[test]
fn a_snapshot_with_a_bad_via_is_refused() {
    let mut fx = fixture();
    let ecu = fx.ecu;
    set_via(&mut fx, Some(ItemRef::Component(ecu))).expect("via");
    let mut out = Vec::new();
    fx.project.save(&mut out).expect("save");
    let text = String::from_utf8(out).expect("utf8");
    let bad = text.replace("\"component\":\"", "\"nowhere\":\"");
    assert_ne!(bad, text);
    assert!(Project::load(bad.as_bytes(), methodology()).is_err());
}
