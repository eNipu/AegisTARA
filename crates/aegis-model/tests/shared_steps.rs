//! Tests of shared attack steps: lifecycle, linking, guards, resolved
//! evaluation/lint/diff/system reads, cache invalidation, and op-log round-trips.

use std::sync::Arc;

use aegis_model::{
    AttackFilter, AttackTree, CeilingSource, ComponentId, ControlId, ElementKind, ElementRef,
    EntityKind, FeasibilityLevel, GateKind, InterfaceId, ItemRef, LintCode, MAX_DESCRIPTION_CHARS,
    MAX_NAME_BYTES, MAX_SHARED_STEPS, ModelError, Node, NodeId, Op, OpCodec, OpLogReader,
    OpLogWriter, Project, ProjectDiff, ProjectEvalCache, Provenance, RiskSnapshot, SharedStepId,
    SharedStepIdx, StepTarget, StoreError, ThreatId, TreeId, VisualPrefix,
};
use serde_json::{Value, json};

mod common;

use common::{builtin, node_id, shared_step_id, tree_id};

fn project() -> Project {
    Project::new(builtin())
}

fn who(actor: &str, at: u64) -> Provenance {
    Provenance::human(actor, at)
}

fn add(p: &mut Project, id: SharedStepId, name: &str) {
    p.apply(Op::AddSharedStep {
        id,
        name: name.into(),
        provenance: who("alice", 10),
    })
    .expect("add shared step");
}

fn shared_with_items() -> (Project, SharedStepId, ComponentId, InterfaceId) {
    let mut p = project();
    let (shared, component, interface) = (
        SharedStepId::new_v7(),
        ComponentId::new_v7(),
        InterfaceId::new_v7(),
    );
    add(&mut p, shared, "Inject frame");
    let m = Arc::clone(p.methodology());
    let kind = m.interface_kind("debug").expect("interface kind");
    p.apply(Op::AddComponent {
        id: component,
        name: "Gateway".into(),
        parent: None,
        provenance: who("alice", 11),
    })
    .expect("add component");
    p.apply(Op::AddInterface {
        id: interface,
        name: "Debug port".into(),
        component,
        kind,
        provenance: who("alice", 12),
    })
    .expect("add interface");
    (p, shared, component, interface)
}

fn rating_ops(
    p: &Project,
    id: SharedStepId,
    via: Option<ItemRef>,
    provenance: Provenance,
) -> [Op; 4] {
    let m = p.methodology();
    let factor = m.feasibility_factors().next().expect("factor");
    let level = m.factor_levels(factor).next().expect("factor level");
    let target = StepTarget::Shared(id);
    [
        Op::SetStepFeasibility {
            target,
            level: Some(m.feasibility_level("high").expect("feasibility")),
            provenance: provenance.clone(),
        },
        Op::SetStepFactor {
            target,
            factor,
            level: Some(level),
            provenance: provenance.clone(),
        },
        Op::SetStepVia {
            target,
            via,
            provenance: provenance.clone(),
        },
        Op::SetStepAttackType {
            target,
            attack_type: m.attack_type("debug-access"),
            provenance,
        },
    ]
}

fn clear_rating_ops(id: SharedStepId, provenance: Provenance) -> [Op; 4] {
    let m = builtin();
    let factor = m.feasibility_factors().next().expect("factor");
    let target = StepTarget::Shared(id);
    [
        Op::SetStepFeasibility {
            target,
            level: None,
            provenance: provenance.clone(),
        },
        Op::SetStepFactor {
            target,
            factor,
            level: None,
            provenance: provenance.clone(),
        },
        Op::SetStepVia {
            target,
            via: None,
            provenance: provenance.clone(),
        },
        Op::SetStepAttackType {
            target,
            attack_type: None,
            provenance,
        },
    ]
}

fn add_tree(p: &mut Project, tree: TreeId, nodes: AttackTree) {
    p.apply(Op::AddAttackTree {
        id: tree,
        tree: nodes,
        step_feasibility: Vec::new(),
        provenance: who("alice", 30),
    })
    .expect("add tree");
}

fn one_step_tree(step: NodeId) -> AttackTree {
    AttackTree::new(step, Node::step("Injected step")).expect("tree")
}

fn use_shared(p: &mut Project, tree: TreeId, node: NodeId, shared: SharedStepId) {
    p.apply(Op::UseSharedStep {
        tree,
        node,
        step: shared,
        provenance: who("bob", 40),
    })
    .expect("link shared step");
}

fn rate_shared(p: &mut Project, shared: SharedStepId, level: FeasibilityLevel) {
    p.apply(Op::SetStepFeasibility {
        target: StepTarget::Shared(shared),
        level: Some(level),
        provenance: who("carol", 50),
    })
    .expect("rate shared step");
}

fn linked_fixture() -> (
    Project,
    SharedStepId,
    TreeId,
    NodeId,
    ComponentId,
    InterfaceId,
) {
    let (mut p, shared, component, interface) = shared_with_items();
    let (tree, node) = (TreeId::new_v7(), NodeId::new_v7());
    add_tree(&mut p, tree, one_step_tree(node));
    use_shared(&mut p, tree, node, shared);
    (p, shared, tree, node, component, interface)
}

fn has_lint(p: &Project, code: LintCode) -> bool {
    p.lint().iter().any(|finding| finding.code() == code)
}

fn add_threat_and_damage(
    p: &mut Project,
    tree: TreeId,
    component: ComponentId,
) -> (ThreatId, aegis_model::DamageId) {
    let m = Arc::clone(p.methodology());
    let asset = aegis_model::AssetId::new_v7();
    p.apply(Op::AddAsset {
        id: asset,
        name: "ECU".into(),
        properties: m.all_properties(),
        provenance: who("alice", 60),
    })
    .expect("asset");
    p.apply(Op::SetAssetComponent {
        asset,
        component: Some(component),
        provenance: who("alice", 61),
    })
    .expect("bind asset");
    let damage = aegis_model::DamageId::new_v7();
    let safety = m.impact_category("safety").expect("safety");
    let severe = m.level(safety, "severe").expect("severe");
    p.apply(Op::AddDamageScenario {
        id: damage,
        name: "Crash".into(),
        impacts: vec![(safety, severe)],
        provenance: who("alice", 62),
    })
    .expect("damage");
    let threat = ThreatId::new_v7();
    p.apply(Op::AddThreatScenario {
        id: threat,
        name: "Injection".into(),
        asset,
        category: m.threat_categories().next().expect("category"),
        catalog_refs: Vec::new(),
        damages: vec![damage],
        provenance: who("alice", 63),
    })
    .expect("threat");
    p.apply(Op::SetThreatAttackTree {
        threat,
        tree: Some(tree),
        provenance: who("alice", 64),
    })
    .expect("link threat tree");
    (threat, damage)
}

#[test]
fn link_and_unlink_reject_blank_actors_and_have_exact_inverses() {
    let (mut p, shared, tree, node, _, _) = linked_fixture();
    let before = p.clone();
    assert_eq!(
        p.apply(Op::UseSharedStep {
            tree,
            node,
            step: shared,
            provenance: who("   ", 1),
        }),
        Err(ModelError::EmptyActor)
    );
    assert_eq!(p, before);
    let undo = p
        .apply(Op::UnuseSharedStep {
            tree,
            node,
            provenance: who("   ", 2),
        })
        .unwrap_err();
    assert_eq!(undo, ModelError::EmptyActor);
    assert_eq!(p, before);
    let tree_idx = p.attack_tree_idx(&tree).expect("tree");
    let tree_provenance = p.attack_tree_provenance(tree_idx).cloned();
    let inverse = p
        .apply(Op::UnuseSharedStep {
            tree,
            node,
            provenance: who("dave", 70),
        })
        .expect("unlink");
    assert_eq!(
        inverse,
        Op::UseSharedStep {
            tree,
            node,
            step: shared,
            provenance: who("dave", 70),
        }
    );
    assert_eq!(
        p.attack_tree_provenance(tree_idx),
        Some(tree_provenance.as_ref().unwrap())
    );
    assert_eq!(
        p.shared_step_users_count(p.shared_step_idx(&shared).unwrap()),
        0
    );
    p.apply(inverse).expect("relink");
    assert_eq!(p, before);
    assert_eq!(
        p.shared_step_users_count(p.shared_step_idx(&shared).unwrap()),
        1
    );
}

#[test]
fn each_link_guard_fails_in_isolation() {
    let (base, shared, _, _, component, _) = linked_fixture();
    let unknown = SharedStepId::new_v7();
    let tree = TreeId::new_v7();
    let root = node_id(900);
    let gate = node_id(901);
    let mut nodes = AttackTree::new(root, Node::gate(GateKind::Or, "Root")).expect("root");
    nodes
        .add_child(nodes.root(), gate, Node::step("Step"))
        .expect("child");
    let mut gated = base.clone();
    add_tree(&mut gated, tree, nodes);
    assert_eq!(
        gated.apply(Op::UseSharedStep {
            tree,
            node: root,
            step: shared,
            provenance: who("alice", 1),
        }),
        Err(ModelError::NotAStep { tree, node: root })
    );
    assert_eq!(
        gated.apply(Op::UseSharedStep {
            tree,
            node: gate,
            step: unknown,
            provenance: who("alice", 1),
        }),
        Err(ModelError::UnknownSharedStep(unknown))
    );
    assert_eq!(
        gated.apply(Op::UnuseSharedStep {
            tree,
            node: gate,
            provenance: who("alice", 1),
        }),
        Err(ModelError::NodeNotShared { tree, node: gate })
    );
    assert_eq!(
        gated.apply(Op::UnuseSharedStep {
            tree,
            node: root,
            provenance: who("alice", 1),
        }),
        Err(ModelError::NotAStep { tree, node: root })
    );

    let m = base.methodology();
    let factor = m.feasibility_factors().next().expect("factor");
    let level = m.factor_levels(factor).next().expect("factor level");
    let own_ops = [
        Op::SetStepFeasibility {
            target: StepTarget::Step(tree, gate),
            level: Some(m.feasibility_level("high").expect("high")),
            provenance: who("alice", 2),
        },
        Op::SetStepFactor {
            target: StepTarget::Step(tree, gate),
            factor,
            level: Some(level),
            provenance: who("alice", 2),
        },
        Op::SetStepVia {
            target: StepTarget::Step(tree, gate),
            via: Some(ItemRef::Component(component)),
            provenance: who("alice", 2),
        },
        Op::SetStepAttackType {
            target: StepTarget::Step(tree, gate),
            attack_type: m.attack_type("debug-access"),
            provenance: who("alice", 2),
        },
    ];
    for op in own_ops {
        let mut p = gated.clone();
        p.apply(op).expect("set own field");
        assert_eq!(
            p.apply(Op::UseSharedStep {
                tree,
                node: gate,
                step: shared,
                provenance: who("alice", 3),
            }),
            Err(ModelError::StepIsRated { tree, node: gate })
        );
    }
    let (mut linked, shared, tree, node, _, _) = linked_fixture();
    assert_eq!(
        linked.apply(Op::UseSharedStep {
            tree,
            node,
            step: shared,
            provenance: who("alice", 4),
        }),
        Err(ModelError::NodeUsesSharedStep {
            tree,
            node,
            shared_step: shared,
        })
    );
}

#[test]
fn linked_steps_reject_own_edits_and_removal_with_exact_users() {
    let (mut p, shared, tree, node, component, _) = linked_fixture();
    let m = Arc::clone(p.methodology());
    let factor = m.feasibility_factors().next().expect("factor");
    for op in [
        Op::SetStepFeasibility {
            target: StepTarget::Step(tree, node),
            level: None,
            provenance: who("alice", 1),
        },
        Op::SetStepFactor {
            target: StepTarget::Step(tree, node),
            factor,
            level: None,
            provenance: who("alice", 1),
        },
        Op::SetStepVia {
            target: StepTarget::Step(tree, node),
            via: Some(ItemRef::Component(component)),
            provenance: who("alice", 1),
        },
        Op::SetStepAttackType {
            target: StepTarget::Step(tree, node),
            attack_type: None,
            provenance: who("alice", 1),
        },
    ] {
        assert_eq!(
            p.apply(op),
            Err(ModelError::NodeUsesSharedStep {
                tree,
                node,
                shared_step: shared,
            })
        );
    }
    let other_tree = TreeId::new_v7();
    let other_node = NodeId::new_v7();
    add_tree(&mut p, other_tree, one_step_tree(other_node));
    use_shared(&mut p, other_tree, other_node, shared);
    assert_eq!(
        p.shared_step_users_count(p.shared_step_idx(&shared).unwrap()),
        2
    );
    assert_eq!(p.description(ElementRef::SharedStep(shared)), None);
    assert_eq!(
        p.apply(Op::RemoveSharedStep { id: shared }),
        Err(ModelError::SharedStepInUse {
            shared_step: shared,
            users: 2,
        })
    );

    let root = NodeId::new_v7();
    let child = NodeId::new_v7();
    let mut nodes = AttackTree::new(root, Node::gate(GateKind::Or, "Root")).expect("root");
    nodes
        .add_child(nodes.root(), child, Node::step("Child"))
        .expect("child");
    let removable = TreeId::new_v7();
    add_tree(&mut p, removable, nodes);
    use_shared(&mut p, removable, child, shared);
    assert_eq!(
        p.apply(Op::RemoveTreeNode {
            tree: removable,
            id: child,
            provenance: who("alice", 5),
        }),
        Err(ModelError::NodeUsesSharedStep {
            tree: removable,
            node: child,
            shared_step: shared,
        })
    );
    let low = node_id(20);
    let high = node_id(30);
    let root = node_id(10);
    let mut nodes = AttackTree::new(root, Node::gate(GateKind::Or, "Root")).expect("root");
    nodes
        .add_child(nodes.root(), high, Node::step("High"))
        .expect("high");
    nodes
        .add_child(nodes.root(), low, Node::step("Low"))
        .expect("low");
    let deterministic = TreeId::new_v7();
    add_tree(&mut p, deterministic, nodes);
    use_shared(&mut p, deterministic, high, shared);
    use_shared(&mut p, deterministic, low, shared);
    assert_eq!(
        p.apply(Op::RemoveAttackTree { id: deterministic }),
        Err(ModelError::NodeUsesSharedStep {
            tree: deterministic,
            node: low,
            shared_step: shared,
        })
    );
}

#[test]
fn ids_visual_ids_and_unrated_defaults() {
    assert_eq!(std::mem::size_of::<SharedStepIdx>(), 4);
    assert_eq!(VisualPrefix::SharedStep.as_str(), "L");
    assert_eq!(VisualPrefix::SharedStep.format(0), "L00");
    assert_eq!(VisualPrefix::SharedStep.format(1), "L01");
    assert_eq!(VisualPrefix::SharedStep.format(100), "L100");
    assert_eq!(VisualPrefix::SharedStep.parse_num("L00"), Some(0));
    assert_eq!(VisualPrefix::SharedStep.parse_num("L1"), None);
    assert_eq!(VisualPrefix::Asset.parse_num("L00"), None);
    assert_eq!(EntityKind::SharedStep.as_str(), "shared step");
    assert_eq!(
        ElementRef::SharedStep(SharedStepId::new_v7()).max_chars(),
        MAX_DESCRIPTION_CHARS
    );
    assert!(ProjectDiff::default().shared_steps().is_empty());

    let mut p = project();
    let first = SharedStepId::new_v7();
    let second = SharedStepId::new_v7();
    add(&mut p, second, "Second");
    add(&mut p, first, "First");
    let first_idx = p.shared_step_idx(&first).expect("first index");
    let second_idx = p.shared_step_idx(&second).expect("second index");
    assert_eq!(p.shared_step_id(first_idx), Some(first));
    assert_eq!(p.shared_step_count(), 2);
    assert_eq!(p.shared_step_visual_id(second_idx).as_deref(), Some("L00"));
    assert_eq!(p.shared_step_visual_id(first_idx).as_deref(), Some("L01"));
    assert_eq!(
        p.visual_id(ElementRef::SharedStep(second)).as_deref(),
        Some("L00")
    );
    assert_eq!(
        p.visual_id(ElementRef::SharedStep(first)).as_deref(),
        Some("L01")
    );
    let step = p.shared_step(first_idx).expect("shared step");
    assert_eq!(step.name(), "First");
    assert_eq!(step.feasibility(), None);
    assert!(step.factor_slice().is_empty());
    assert_eq!(step.attack_type(), None);
    assert_eq!(p.shared_step_via(first_idx), None);
    assert!(!step.is_rated());
    assert_eq!(step.provenance(), &who("alice", 10));
    let walked: Vec<_> = p.shared_steps().map(|(_, id, _)| id).collect();
    let mut expected = vec![first, second];
    expected.sort();
    assert_eq!(walked, expected);

    p.apply(Op::RemoveSharedStep { id: second })
        .expect("remove second");
    let third = SharedStepId::new_v7();
    add(&mut p, third, "Third");
    let third_idx = p.shared_step_idx(&third).expect("third index");
    assert_eq!(p.shared_step_visual_id(third_idx).as_deref(), Some("L02"));
    assert_eq!(p.shared_step_idx(&second), None);
    assert_eq!(p.shared_step(second_idx), None);
    assert_eq!(p.shared_step_id(second_idx), None);
    assert_eq!(p.shared_step_visual_id(second_idx), None);
}

#[test]
fn whitespace_actors_are_rejected_but_empty_actors_remain_compatible() {
    let mut p = project();
    let id = SharedStepId::new_v7();
    assert_eq!(
        p.apply(Op::AddSharedStep {
            id,
            name: "Inject frame".into(),
            provenance: who("   ", 1),
        }),
        Err(ModelError::EmptyActor)
    );
    // Empty actors are intentionally accepted by `check_provenance`, matching
    // every other entity and preserving imported/unrecorded provenance.
    p.apply(Op::AddSharedStep {
        id,
        name: "Inject frame".into(),
        provenance: who("", 1),
    })
    .expect("empty actor remains compatible");
    assert_eq!(
        p.apply(Op::RenameSharedStep {
            id,
            name: "Spoof frame".into(),
            provenance: who("   ", 2),
        }),
        Err(ModelError::EmptyActor)
    );
    p.apply(Op::RenameSharedStep {
        id,
        name: "Spoof frame".into(),
        provenance: who("", 2),
    })
    .expect("empty actor remains compatible");
}

#[test]
fn each_add_rename_and_remove_guard_fails_in_isolation() {
    let mut p = project();
    let id = SharedStepId::new_v7();
    for name in ["", " \t "] {
        assert_eq!(
            p.apply(Op::AddSharedStep {
                id,
                name: name.into(),
                provenance: who("alice", 1),
            }),
            Err(ModelError::EmptyName {
                kind: EntityKind::SharedStep,
            })
        );
    }
    add(&mut p, id, "Inject frame");
    for name in ["", "   "] {
        assert_eq!(
            p.apply(Op::RenameSharedStep {
                id,
                name: name.into(),
                provenance: who("alice", 2),
            }),
            Err(ModelError::EmptyName {
                kind: EntityKind::SharedStep,
            })
        );
    }
    assert_eq!(
        p.apply(Op::AddSharedStep {
            id,
            name: "Duplicate".into(),
            provenance: who("alice", 3),
        }),
        Err(ModelError::DuplicateSharedStep(id))
    );

    let unknown = SharedStepId::new_v7();
    assert_eq!(
        p.apply(Op::RenameSharedStep {
            id: unknown,
            name: "Missing".into(),
            provenance: who("alice", 4),
        }),
        Err(ModelError::UnknownSharedStep(unknown))
    );
    assert_eq!(
        p.apply(Op::RemoveSharedStep { id: unknown }),
        Err(ModelError::UnknownSharedStep(unknown))
    );

    let target = ElementRef::SharedStep(id);
    p.apply(Op::SetDescription {
        target,
        text: "A reusable frame injection".into(),
        provenance: who("alice", 5),
    })
    .expect("describe shared step");
    assert_eq!(p.description(target), Some("A reusable frame injection"));
    assert_eq!(
        p.apply(Op::RemoveSharedStep { id }),
        Err(ModelError::DescriptionNotCleared {
            kind: EntityKind::SharedStep,
        })
    );

    let mut full = project();
    for n in 0..MAX_SHARED_STEPS {
        add(
            &mut full,
            shared_step_id(1_000_000 + n as u64),
            "Shared step",
        );
    }
    assert_eq!(full.shared_step_count(), MAX_SHARED_STEPS);
    assert_eq!(
        full.apply(Op::AddSharedStep {
            id: SharedStepId::new_v7(),
            name: "One too many".into(),
            provenance: who("alice", 6),
        }),
        Err(ModelError::TooManySharedSteps {
            max: MAX_SHARED_STEPS,
        })
    );
}

#[test]
fn add_respects_the_project_slot_limit_in_validate_and_apply() {
    let mut p = Project::with_slot_limit(builtin(), 0);
    let id = SharedStepId::new_v7();
    let op = Op::AddSharedStep {
        id,
        name: "Inject frame".into(),
        provenance: who("alice", 1),
    };
    let expected = ModelError::TooManySharedSteps {
        max: MAX_SHARED_STEPS,
    };
    assert_eq!(p.validate(&op).unwrap_err(), expected);
    assert_eq!(p.apply(op).unwrap_err(), expected);
    assert_eq!(p.shared_step_count(), 0);
}

#[test]
fn removal_undo_redo_equality_diff_and_descriptions() {
    let mut p = project();
    let id = SharedStepId::new_v7();
    let original = who("alice", 10);
    assert_eq!(
        p.apply(Op::AddSharedStep {
            id,
            name: "Inject frame".into(),
            provenance: original.clone(),
        }),
        Ok(Op::RemoveSharedStep { id })
    );
    let base = p.clone();
    assert!(base.diff(&base).expect("empty diff").is_empty());

    let mut renamed = base.clone();
    let rename_inverse = renamed
        .apply(Op::RenameSharedStep {
            id,
            name: "Spoof frame".into(),
            provenance: who("bob", 20),
        })
        .expect("rename");
    let diff = base.diff(&renamed).expect("rename diff");
    assert_eq!(diff.shared_steps().modified(), [id]);
    assert_ne!(base, renamed);

    let target = ElementRef::SharedStep(id);
    let set_description = renamed
        .apply(Op::SetDescription {
            target,
            text: "Reusable attack".into(),
            provenance: who("carol", 30),
        })
        .expect("describe");
    let described = renamed.clone();
    let diff = base.diff(&described).expect("description diff");
    assert_eq!(diff.shared_steps().modified(), [id]);
    assert_eq!(
        renamed.apply(Op::RemoveSharedStep { id }),
        Err(ModelError::DescriptionNotCleared {
            kind: EntityKind::SharedStep,
        })
    );
    let clear_description = renamed
        .apply(Op::SetDescription {
            target,
            text: "".into(),
            provenance: who("dave", 40),
        })
        .expect("clear description");
    assert_eq!(renamed.description(target), None);

    let before_remove = renamed.clone();
    let remove = Op::RemoveSharedStep { id };
    let remove_inverse = renamed.apply(remove.clone()).expect("remove");
    assert_eq!(
        remove_inverse,
        Op::AddSharedStep {
            id,
            name: "Spoof frame".into(),
            provenance: who("bob", 20),
        }
    );
    assert_eq!(renamed.shared_step_count(), 0);
    let removed = renamed.clone();
    let diff = before_remove.diff(&removed).expect("remove diff");
    assert_eq!(diff.shared_steps().removed(), [id]);
    assert!(!diff.is_empty());

    let undo = renamed.apply(remove_inverse.clone()).expect("undo remove");
    assert_eq!(undo, remove);
    assert_eq!(renamed, before_remove);
    assert!(before_remove.diff(&renamed).expect("undo diff").is_empty());
    renamed.apply(remove.clone()).expect("redo remove");
    assert_eq!(renamed, removed);
    renamed.apply(remove_inverse).expect("undo remove again");
    renamed.apply(clear_description).expect("undo clear");
    assert_eq!(renamed, described);
    renamed.apply(set_description).expect("undo description");
    renamed.apply(rename_inverse).expect("undo rename");
    assert_eq!(renamed, base);
}

#[test]
fn lifecycle_and_description_ops_round_trip_through_the_op_log() {
    let m = builtin();
    let id = SharedStepId::new_v7();
    let target = ElementRef::SharedStep(id);
    let prov = who("alice", 42);
    let ops = vec![
        Op::AddSharedStep {
            id,
            name: "Inject frame".into(),
            provenance: prov.clone(),
        },
        Op::RenameSharedStep {
            id,
            name: "Spoof frame".into(),
            provenance: prov.clone(),
        },
        Op::SetDescription {
            target,
            text: "Reusable attack".into(),
            provenance: prov.clone(),
        },
        Op::SetDescription {
            target,
            text: "".into(),
            provenance: prov.clone(),
        },
        Op::RemoveSharedStep { id },
    ];
    assert_eq!(ops[4].provenance(), None);
    assert_eq!(ops[4].clone().with_provenance(who("bob", 43)), ops[4]);

    let codec = OpCodec::new(Arc::clone(&m));
    for op in &ops {
        let bytes = codec.encode(op).expect("encode");
        assert_eq!(codec.decode(&bytes).expect("decode"), *op);
        let value: serde_json::Value = serde_json::from_slice(&bytes).expect("JSON op");
        if let Op::SetDescription { .. } = op {
            assert_eq!(value["target"]["kind"], "shared_step");
        }
    }
    let value: serde_json::Value =
        serde_json::from_slice(&codec.encode(&ops[0]).expect("encode")).expect("JSON op");
    assert_eq!(value["op"], "add_shared_step");

    let mut log = Vec::new();
    let mut writer = OpLogWriter::create(&mut log, Arc::clone(&m)).expect("writer");
    for op in &ops {
        writer.append(op).expect("append");
    }
    drop(writer);
    let decoded: Vec<_> = OpLogReader::open(log.as_slice(), m)
        .expect("reader")
        .map(|record| record.expect("record").op)
        .collect();
    assert_eq!(decoded, ops);

    let over_name: Arc<str> = "x".repeat(MAX_NAME_BYTES + 1).into();
    let mut log = Vec::new();
    let mut writer = OpLogWriter::create(&mut log, builtin()).expect("writer");
    for op in [
        Op::AddSharedStep {
            id,
            name: Arc::clone(&over_name),
            provenance: prov.clone(),
        },
        Op::RenameSharedStep {
            id,
            name: over_name,
            provenance: prov,
        },
    ] {
        assert!(matches!(writer.append(&op), Err(StoreError::UnencodableOp)));
    }
}

#[test]
fn shared_rating_ops_have_exact_inverses_and_compact_factors() {
    let (mut p, id, component, _) = shared_with_items();
    let ops = rating_ops(&p, id, Some(ItemRef::Component(component)), who("bob", 20));
    let clears = clear_rating_ops(id, who("alice", 10));
    for (op, expected_inverse) in ops.into_iter().zip(clears) {
        let is_factor = matches!(op, Op::SetStepFactor { .. });
        let before = p.clone();
        let inverse = p.apply(op).expect("shared rating");
        assert_eq!(inverse, expected_inverse);
        if is_factor {
            let idx = p.shared_step_idx(&id).expect("shared step");
            let step = p.shared_step(idx).expect("shared step");
            assert_eq!(
                step.factor_slice().len(),
                p.methodology().feasibility_factor_count()
            );
        }
        p.apply(inverse).expect("undo shared rating");
        assert_eq!(p, before);
        if is_factor {
            let idx = p.shared_step_idx(&id).expect("shared step");
            assert!(
                p.shared_step(idx)
                    .expect("shared step")
                    .factor_slice()
                    .is_empty()
            );
        }
    }
    p.apply(Op::RemoveSharedStep { id })
        .expect("all ratings cleared");
}

#[test]
fn shared_rating_ops_reject_blank_actors_and_unknown_ids() {
    let (mut p, id, component, _) = shared_with_items();
    let unknown = SharedStepId::new_v7();
    let known = rating_ops(&p, id, Some(ItemRef::Component(component)), who("bob", 20));
    let missing = rating_ops(
        &p,
        unknown,
        Some(ItemRef::Component(component)),
        who("bob", 20),
    );
    for (op, missing) in known.into_iter().zip(missing) {
        assert_eq!(
            p.apply(op.with_provenance(who("   ", 1))),
            Err(ModelError::EmptyActor)
        );
        assert_eq!(
            p.apply(missing),
            Err(ModelError::UnknownSharedStep(unknown))
        );
    }
}

#[test]
fn each_shared_rating_field_blocks_removal_in_isolation() {
    let (base, id, component, _) = shared_with_items();
    for op in rating_ops(
        &base,
        id,
        Some(ItemRef::Component(component)),
        who("bob", 20),
    ) {
        let mut p = base.clone();
        let before = p.clone();
        let inverse = p.apply(op).expect("shared rating");
        assert_eq!(
            p.apply(Op::RemoveSharedStep { id }),
            Err(ModelError::SharedStepIsRated(id))
        );
        p.apply(inverse).expect("clear shared rating");
        assert_eq!(p, before);
        let idx = p.shared_step_idx(&id).expect("shared step");
        assert!(
            p.shared_step(idx)
                .expect("shared step")
                .factor_slice()
                .is_empty()
        );
        p.apply(Op::RemoveSharedStep { id })
            .expect("unrated shared step removes");
    }
}

#[test]
fn shared_via_blocks_component_and_interface_removal() {
    let (mut p, id, component, interface) = shared_with_items();
    let target = StepTarget::Shared(id);
    let undo = p
        .apply(Op::SetStepVia {
            target,
            via: Some(ItemRef::Interface(interface)),
            provenance: who("bob", 21),
        })
        .expect("interface via");
    assert_eq!(
        p.apply(Op::RemoveInterface { id: interface }),
        Err(ModelError::InterfaceHasSteps {
            interface,
            steps: 1,
        })
    );
    p.apply(undo).expect("clear interface via");
    p.apply(Op::RemoveInterface { id: interface })
        .expect("remove unreferenced interface");
    let undo = p
        .apply(Op::SetStepVia {
            target,
            via: Some(ItemRef::Component(component)),
            provenance: who("bob", 20),
        })
        .expect("component via");
    assert_eq!(
        p.apply(Op::RemoveComponent { id: component }),
        Err(ModelError::ComponentHasSteps {
            component,
            steps: 1,
        })
    );
    p.apply(undo).expect("clear component via");
}

#[test]
fn rating_wire_preserves_v1_steps_and_round_trips_shared_targets() {
    let m = builtin();
    let codec = OpCodec::new(Arc::clone(&m));
    let tree = tree_id(1);
    let node = node_id(2);
    let prov = who("alice", 42);
    let step_op = Op::SetStepFeasibility {
        target: StepTarget::Step(tree, node),
        level: Some(m.feasibility_level("high").expect("feasibility")),
        provenance: prov.clone(),
    };
    let v1 = format!(
        r#"{{"op":"set_step_feasibility","tree":"{tree}","node":"{node}","level":"high","provenance":{{"actor":"alice","timestamp_ms":42,"source":"human"}}}}"#
    );
    assert_eq!(codec.decode(v1.as_bytes()).expect("v1 decode"), step_op);
    assert_eq!(codec.encode(&step_op).expect("v1 encode"), v1.as_bytes());

    let (p, id, component, _) = shared_with_items();
    let ops = rating_ops(&p, id, Some(ItemRef::Component(component)), prov);
    for op in &ops {
        let bytes = codec.encode(op).expect("shared encode");
        let value: Value = serde_json::from_slice(&bytes).expect("shared JSON");
        assert_eq!(value["shared_step"], id.to_string());
        assert!(value.get("tree").is_none());
        assert!(value.get("node").is_none());
        assert_eq!(codec.decode(&bytes).expect("shared decode"), *op);
    }

    let mut log = Vec::new();
    let mut writer = OpLogWriter::create(&mut log, Arc::clone(&m)).expect("writer");
    for op in &ops {
        writer.append(op).expect("append shared rating");
    }
    drop(writer);
    let decoded: Vec<_> = OpLogReader::open(log.as_slice(), Arc::clone(&m))
        .expect("reader")
        .map(|record| record.expect("record").op)
        .collect();
    assert_eq!(decoded, ops);

    let invalid_targets = [
        [false, false, false],
        [true, false, false],
        [false, true, false],
        [true, false, true],
        [false, true, true],
        [true, true, true],
    ];
    for op in &ops {
        let valid: Value =
            serde_json::from_slice(&codec.encode(op).expect("encode")).expect("valid JSON");
        for [has_tree, has_node, has_shared] in invalid_targets {
            let mut bad = valid.clone();
            bad.as_object_mut()
                .expect("op object")
                .remove("shared_step");
            if has_tree {
                bad["tree"] = json!(tree.to_string());
            }
            if has_node {
                bad["node"] = json!(node.to_string());
            }
            if has_shared {
                bad["shared_step"] = json!(id.to_string());
            }
            let bytes = serde_json::to_vec(&bad).expect("bad JSON");
            assert!(matches!(
                codec.decode(&bytes),
                Err(StoreError::Decode { line: 1, column: 0 })
            ));
        }
    }

    let bad = json!({"seq": 1, "op": {
        "op": "set_step_feasibility", "level": "high"
    }});
    let mut bytes = OpLogWriter::create(Vec::new(), Arc::clone(&m))
        .expect("header")
        .into_inner()
        .expect("log");
    bytes.extend(serde_json::to_vec(&bad).expect("record"));
    bytes.push(b'\n');
    let mut reader = OpLogReader::open(bytes.as_slice(), m).expect("reader");
    let error = reader.next().expect("record").expect_err("invalid target");
    assert!(matches!(error, StoreError::Decode { line: 2, column: 0 }));
}

#[test]
fn shared_link_vectors_allocate_compact_and_survive_node_shifts() {
    let (mut p, shared, tree, node, component, _) = linked_fixture();
    let tree_idx = p.attack_tree_idx(&tree).unwrap();
    assert_eq!(
        p.step_shared(
            tree_idx,
            p.attack_tree(tree_idx).unwrap().idx_of(&node).unwrap()
        ),
        Some(p.shared_step_idx(&shared).unwrap())
    );
    assert_eq!(
        p.step_shared_id(
            tree_idx,
            p.attack_tree(tree_idx).unwrap().idx_of(&node).unwrap()
        ),
        Some(shared)
    );
    p.apply(Op::UnuseSharedStep {
        tree,
        node,
        provenance: who("alice", 1),
    })
    .expect("unlink");
    p.apply(Op::RemoveAttackTree { id: tree })
        .expect("last shared link compacts the block");

    let condition = aegis_model::ConditionId::new_v7();
    p.apply(Op::AddCondition {
        id: condition,
        name: "Bench access".into(),
        description: "".into(),
        present: true,
        provenance: who("alice", 2),
    })
    .expect("condition");
    let tree = TreeId::new_v7();
    let root = node_id(100);
    let first = node_id(101);
    let second = node_id(102);
    let mut nodes = AttackTree::new(root, Node::gate(GateKind::Or, "Root")).expect("root");
    nodes
        .add_child(nodes.root(), first, Node::step("First"))
        .expect("first");
    nodes
        .add_child(nodes.root(), second, Node::step("Second"))
        .expect("second");
    add_tree(&mut p, tree, nodes);
    p.apply(Op::SetStepVia {
        target: StepTarget::Step(tree, second),
        via: Some(ItemRef::Component(component)),
        provenance: who("alice", 3),
    })
    .expect("own via");
    p.apply(Op::NeedStepCondition {
        tree,
        node: second,
        condition,
        provenance: who("alice", 4),
    })
    .expect("own condition");
    use_shared(&mut p, tree, first, shared);
    p.apply(Op::UnuseSharedStep {
        tree,
        node: first,
        provenance: who("alice", 5),
    })
    .expect("unlink with other links present");
    assert_eq!(
        p.apply(Op::RemoveAttackTree { id: tree }),
        Err(ModelError::AttackTreeHasStepLinks { tree, steps: 1 })
    );
    p.apply(Op::SetStepVia {
        target: StepTarget::Step(tree, second),
        via: None,
        provenance: who("alice", 6),
    })
    .expect("clear via");
    p.apply(Op::UnneedStepCondition {
        tree,
        node: second,
        condition,
        provenance: who("alice", 7),
    })
    .expect("clear condition");
    p.apply(Op::RemoveAttackTree { id: tree })
        .expect("all links compact");

    let tree = TreeId::new_v7();
    let root = node_id(200);
    let linked = node_id(201);
    let inserted = node_id(202);
    let mut nodes = AttackTree::new(root, Node::gate(GateKind::Or, "Root")).expect("root");
    nodes
        .add_child(nodes.root(), linked, Node::step("Linked"))
        .expect("linked");
    add_tree(&mut p, tree, nodes);
    use_shared(&mut p, tree, linked, shared);
    p.apply(Op::AddTreeNode {
        tree,
        parent: root,
        id: inserted,
        node: Node::step("Inserted"),
        index: 1,
        position: 0,
        provenance: who("alice", 8),
    })
    .expect("insert before later slots");
    let tree_idx = p.attack_tree_idx(&tree).unwrap();
    let attack_tree = p.attack_tree(tree_idx).unwrap();
    let linked_idx = attack_tree.idx_of(&linked).unwrap();
    let inserted_idx = attack_tree.idx_of(&inserted).unwrap();
    assert_eq!(linked_idx.index(), 2);
    assert_eq!(
        p.step_shared(tree_idx, linked_idx),
        Some(p.shared_step_idx(&shared).unwrap())
    );
    assert_eq!(p.step_shared(tree_idx, inserted_idx), None);
    p.apply(Op::RemoveTreeNode {
        tree,
        id: inserted,
        provenance: who("alice", 9),
    })
    .expect("remove shifted slot");
    let attack_tree = p.attack_tree(tree_idx).unwrap();
    assert_eq!(
        p.step_shared(tree_idx, attack_tree.idx_of(&linked).unwrap()),
        Some(p.shared_step_idx(&shared).unwrap())
    );
}

#[test]
fn linked_steps_resolve_inherent_residual_and_both_ceiling_sources() {
    let (mut p, shared, tree, node, component, _) = linked_fixture();
    let m = Arc::clone(p.methodology());
    let (high, low, medium) = (
        m.feasibility_level("high").unwrap(),
        m.feasibility_level("low").unwrap(),
        m.feasibility_level("medium").unwrap(),
    );
    rate_shared(&mut p, shared, high);
    let attack_type = m.attack_type("debug-access").unwrap();
    p.apply(Op::SetStepVia {
        target: StepTarget::Shared(shared),
        via: Some(ItemRef::Component(component)),
        provenance: who("alice", 1),
    })
    .expect("shared via");
    p.apply(Op::SetStepAttackType {
        target: StepTarget::Shared(shared),
        attack_type: Some(attack_type),
        provenance: who("alice", 2),
    })
    .expect("shared attack type");
    let control = ControlId::new_v7();
    p.apply(Op::AddSecurityControl {
        id: control,
        name: "Debug lock".into(),
        provenance: who("alice", 3),
    })
    .expect("control");
    p.apply(Op::SetControlRule {
        control,
        attack_type,
        ceiling: Some(low),
        provenance: who("alice", 4),
    })
    .expect("rule");
    p.apply(Op::ApplyControl {
        control,
        target: ItemRef::Component(component),
        provenance: who("alice", 5),
    })
    .expect("apply control");
    p.apply(Op::SetStepMitigation {
        tree,
        node,
        control,
        feasibility: Some(medium),
        provenance: who("alice", 6),
    })
    .expect("manual mitigation");
    let tree_idx = p.attack_tree_idx(&tree).unwrap();
    let node_idx = p.attack_tree(tree_idx).unwrap().idx_of(&node).unwrap();
    assert_eq!(p.step_feasibility(tree_idx, node_idx), Some(high));
    assert_eq!(p.effective_step_feasibility(tree_idx, node_idx), Some(high));
    assert_eq!(
        p.step_via(tree_idx, node_idx),
        Some(ItemRef::Component(component))
    );
    assert_eq!(p.step_attack_type(tree_idx, node_idx), Some(attack_type));
    assert_eq!(p.evaluate_tree(tree).unwrap().root_value(), &high);
    let ceilings = p.step_ceilings(tree_idx, node_idx);
    assert_eq!(ceilings.len(), 2);
    assert_eq!(ceilings[0].feasibility(), low);
    assert_eq!(
        ceilings[0].source(),
        CeilingSource::ControlRule {
            target: ItemRef::Component(component),
            attack_type
        }
    );
    assert_eq!(ceilings[1].feasibility(), medium);
    assert_eq!(ceilings[1].source(), CeilingSource::StepMitigation);
    assert_eq!(p.residual_step_feasibility(tree_idx, node_idx), Some(low));
    assert_eq!(p.evaluate_residual_tree(tree).unwrap().root_value(), &low);
    assert_eq!(
        p.steps_via(ItemRef::Component(component))
            .collect::<Vec<_>>(),
        [(tree_idx, node_idx)]
    );
}

#[test]
fn shared_factors_drive_attack_potential_and_resolved_factor_reads() {
    let (mut p, shared, tree, node, _, _) = linked_fixture();
    let m = Arc::clone(p.methodology());
    let ratings = [
        ("elapsed-time", "le-1m"),
        ("expertise", "expert"),
        ("knowledge", "restricted"),
        ("window-of-opportunity", "moderate"),
        ("equipment", "specialized"),
    ];
    for (factor_key, level_key) in ratings {
        let factor = m.feasibility_factor(factor_key).unwrap();
        let level = m.factor_level(factor, level_key).unwrap();
        p.apply(Op::SetStepFactor {
            target: StepTarget::Shared(shared),
            factor,
            level: Some(level),
            provenance: who("alice", 1),
        })
        .expect("factor");
    }
    let tree_idx = p.attack_tree_idx(&tree).unwrap();
    let node_idx = p.attack_tree(tree_idx).unwrap().idx_of(&node).unwrap();
    let potential = p.step_attack_potential(tree_idx, node_idx).unwrap();
    assert_eq!(potential.score(), 21);
    assert_eq!(
        m.feasibility_level_key(potential.feasibility()),
        Some("low")
    );
    assert_eq!(
        p.effective_step_feasibility(tree_idx, node_idx),
        Some(potential.feasibility())
    );
    assert_eq!(
        p.evaluate_tree(tree).unwrap().root_value(),
        &potential.feasibility()
    );
    assert_eq!(p.step_factors(tree_idx, node_idx).count(), 5);
}

#[test]
fn applicability_excludes_linked_steps_through_shared_via_and_factors() {
    let (mut p, shared, tree, _, component, _) = linked_fixture();
    let m = Arc::clone(p.methodology());
    rate_shared(&mut p, shared, m.feasibility_level("high").unwrap());
    p.apply(Op::SetStepVia {
        target: StepTarget::Shared(shared),
        via: Some(ItemRef::Component(component)),
        provenance: who("alice", 1),
    })
    .expect("via");
    let condition = aegis_model::ConditionId::new_v7();
    p.apply(Op::AddCondition {
        id: condition,
        name: "Bench only".into(),
        description: "".into(),
        present: false,
        provenance: who("alice", 2),
    })
    .expect("condition");
    p.apply(Op::NeedCondition {
        element: ElementRef::Component(component),
        condition,
        provenance: who("alice", 3),
    })
    .expect("need condition");
    assert_eq!(
        p.evaluate_tree(tree),
        Err(ModelError::TreeNotApplicable(tree))
    );
    p.apply(Op::SetConditionPresent {
        id: condition,
        present: true,
        provenance: who("alice", 4),
    })
    .expect("condition present");
    assert!(p.evaluate_tree(tree).is_ok());

    let factor = m.feasibility_factors().next().unwrap();
    let excluded = m.factor_levels(factor).next().unwrap();
    p.apply(Op::SetStepFactor {
        target: StepTarget::Shared(shared),
        factor,
        level: Some(excluded),
        provenance: who("alice", 5),
    })
    .expect("factor");
    p.apply(Op::SetAttackFilter {
        filter: AttackFilter::default().without(factor, excluded),
        provenance: who("alice", 6),
    })
    .expect("filter");
    assert_eq!(
        p.evaluate_tree(tree),
        Err(ModelError::TreeNotApplicable(tree))
    );
}

#[test]
fn lint_reads_all_four_step_codes_through_the_shared_step() {
    let (mut base, shared, tree, node, component, _) = linked_fixture();
    let m = Arc::clone(base.methodology());
    let tree_idx = base.attack_tree_idx(&tree).unwrap();
    let node_idx = base.attack_tree(tree_idx).unwrap().idx_of(&node).unwrap();
    assert!(has_lint(
        &base,
        LintCode::AttackTreeStepUnrated { tree, node }
    ));

    let factor = m.feasibility_factors().next().unwrap();
    let level = m.factor_levels(factor).next().unwrap();
    base.apply(Op::SetStepFactor {
        target: StepTarget::Shared(shared),
        factor,
        level: Some(level),
        provenance: who("alice", 1),
    })
    .expect("one factor");
    assert!(has_lint(
        &base,
        LintCode::AttackTreeStepIncompleteFactors { tree, node }
    ));
    for factor in m.feasibility_factors().skip(1) {
        base.apply(Op::SetStepFactor {
            target: StepTarget::Shared(shared),
            factor,
            level: Some(m.factor_levels(factor).next().unwrap()),
            provenance: who("alice", 2),
        })
        .expect("remaining factor");
    }
    rate_shared(&mut base, shared, m.feasibility_level("high").unwrap());
    assert!(has_lint(
        &base,
        LintCode::RedundantStepFeasibility { tree, node }
    ));
    assert_eq!(
        base.step_factors(tree_idx, node_idx).count(),
        m.feasibility_factor_count()
    );

    let control = ControlId::new_v7();
    base.apply(Op::AddSecurityControl {
        id: control,
        name: "Rule".into(),
        provenance: who("alice", 3),
    })
    .expect("control");
    base.apply(Op::SetControlRule {
        control,
        attack_type: m.attack_type("debug-access").unwrap(),
        ceiling: Some(m.feasibility_level("low").unwrap()),
        provenance: who("alice", 4),
    })
    .expect("rule");
    base.apply(Op::SetStepVia {
        target: StepTarget::Shared(shared),
        via: Some(ItemRef::Component(component)),
        provenance: who("alice", 5),
    })
    .expect("via without attack type");
    assert!(has_lint(
        &base,
        LintCode::StepSettingsIncomplete { tree, node }
    ));
}

#[test]
fn diff_marks_threats_for_shared_rating_and_shared_via_rule_edits() {
    let (mut base, shared, tree, _, component, _) = linked_fixture();
    let m = Arc::clone(base.methodology());
    let (high, low) = (
        m.feasibility_level("high").unwrap(),
        m.feasibility_level("low").unwrap(),
    );
    rate_shared(&mut base, shared, high);
    let (threat, _) = add_threat_and_damage(&mut base, tree, component);
    let before = RiskSnapshot::new(high, Some(m.risk_level("r5").unwrap()));
    let after = RiskSnapshot::new(low, Some(m.risk_level("r3").unwrap()));

    let mut rated = base.clone();
    rate_shared(&mut rated, shared, low);
    let diff = base.diff(&rated).unwrap();
    assert_eq!(diff.shared_steps().modified(), [shared]);
    assert!(diff.attack_trees().modified().is_empty());
    assert_eq!(diff.risk_deltas().len(), 1);
    assert_eq!(diff.risk_deltas()[0].threat(), threat);
    assert_eq!(diff.risk_deltas()[0].inherent().before(), Some(before));
    assert_eq!(diff.risk_deltas()[0].inherent().after(), Some(after));

    let attack_type = m.attack_type("debug-access").unwrap();
    base.apply(Op::SetStepVia {
        target: StepTarget::Shared(shared),
        via: Some(ItemRef::Component(component)),
        provenance: who("alice", 1),
    })
    .expect("via");
    base.apply(Op::SetStepAttackType {
        target: StepTarget::Shared(shared),
        attack_type: Some(attack_type),
        provenance: who("alice", 2),
    })
    .expect("type");
    let control = ControlId::new_v7();
    base.apply(Op::AddSecurityControl {
        id: control,
        name: "Debug lock".into(),
        provenance: who("alice", 3),
    })
    .expect("control");
    base.apply(Op::ApplyControl {
        control,
        target: ItemRef::Component(component),
        provenance: who("alice", 4),
    })
    .expect("apply");
    let mut ruled = base.clone();
    ruled
        .apply(Op::SetControlRule {
            control,
            attack_type,
            ceiling: Some(low),
            provenance: who("alice", 5),
        })
        .expect("rule");
    let diff = base.diff(&ruled).unwrap();
    assert_eq!(diff.security_controls().modified(), [control]);
    assert!(diff.attack_trees().modified().is_empty());
    assert_eq!(diff.risk_deltas().len(), 1);
    assert_eq!(diff.risk_deltas()[0].inherent().before(), Some(before));
    assert_eq!(diff.risk_deltas()[0].inherent().after(), Some(before));
    assert_eq!(diff.risk_deltas()[0].residual().before(), Some(before));
    assert_eq!(diff.risk_deltas()[0].residual().after(), Some(after));
}

#[test]
fn system_use_for_a_shared_step_unions_both_using_trees_and_its_via() {
    let mut p = project();
    let m = Arc::clone(p.methodology());
    let system_a = aegis_model::ElementId::new_v7();
    let system_b = aegis_model::ElementId::new_v7();
    for (id, name) in [(system_a, "Brake"), (system_b, "ADAS")] {
        p.apply(Op::AddElement {
            id,
            kind: ElementKind::System,
            name: name.into(),
            provenance: who("alice", 1),
        })
        .expect("system");
    }
    let component_a = ComponentId::new_v7();
    let component_b = ComponentId::new_v7();
    for (id, system) in [(component_a, system_a), (component_b, system_b)] {
        p.apply(Op::AddComponent {
            id,
            name: "ECU".into(),
            parent: None,
            provenance: who("alice", 2),
        })
        .expect("component");
        p.apply(Op::SetComponentSystem {
            id,
            system: Some(system),
            provenance: who("alice", 3),
        })
        .expect("component system");
    }
    let shared = SharedStepId::new_v7();
    add(&mut p, shared, "Shared injection");
    p.apply(Op::SetStepVia {
        target: StepTarget::Shared(shared),
        via: Some(ItemRef::Component(component_a)),
        provenance: who("alice", 4),
    })
    .expect("shared via");
    for component in [component_a, component_b] {
        let tree = TreeId::new_v7();
        let node = NodeId::new_v7();
        add_tree(&mut p, tree, one_step_tree(node));
        use_shared(&mut p, tree, node, shared);
        add_threat_and_damage(&mut p, tree, component);
    }
    let usage = p.system_use();
    let shared_idx = p.shared_step_idx(&shared).unwrap();
    let expected = usage.set_of([system_a, system_b]);
    assert_eq!(usage.shared_step(shared_idx), expected);
    assert_eq!(usage.shared_step(shared_idx).len(), 2);
    let mut ids = usage.ids(expected).collect::<Vec<_>>();
    ids.sort();
    let mut expected_ids = vec![system_a, system_b];
    expected_ids.sort();
    assert_eq!(ids, expected_ids);
    assert_eq!(m.key(), "iso21434-r155");
}

fn cache_fixture() -> (Project, TreeId, NodeId, TreeId, NodeId, ComponentId) {
    let (mut p, _, component, _) = shared_with_items();
    let first = (TreeId::new_v7(), NodeId::new_v7());
    let second = (TreeId::new_v7(), NodeId::new_v7());
    for (tree, node) in [first, second] {
        add_tree(&mut p, tree, one_step_tree(node));
        add_threat_and_damage(&mut p, tree, component);
    }
    (p, first.0, first.1, second.0, second.1, component)
}

fn assert_cache_matches(p: &Project, cache: &ProjectEvalCache) {
    for (_, tree, _) in p.attack_trees() {
        assert_eq!(cache.evaluate_tree(p, tree).cloned(), p.evaluate_tree(tree));
        assert_eq!(
            cache.evaluate_residual_tree(p, tree).cloned(),
            p.evaluate_residual_tree(tree)
        );
    }
    for (_, threat, _) in p.threat_scenarios() {
        assert_eq!(
            cache.evaluate_risk(p, threat).cloned(),
            p.evaluate_risk(threat)
        );
        assert_eq!(
            cache.evaluate_residual_risk(p, threat).cloned(),
            p.evaluate_residual_risk(threat)
        );
    }
}

#[test]
fn eval_cache_grows_shared_slots_without_rebuild_and_invalidates_every_user() {
    let (mut p, first_tree, first_node, second_tree, second_node, component) = cache_fixture();
    let rated_tree = TreeId::new_v7();
    let rated_node = NodeId::new_v7();
    add_tree(&mut p, rated_tree, one_step_tree(rated_node));
    let high = p.methodology().feasibility_level("high").unwrap();
    p.apply(Op::SetStepFeasibility {
        target: StepTarget::Step(rated_tree, rated_node),
        level: Some(high),
        provenance: who("alice", 1),
    })
    .expect("rated control tree");
    let mut cache = ProjectEvalCache::new(&p);
    p.apply_cached(
        Op::SetStepFeasibility {
            target: StepTarget::Step(rated_tree, rated_node),
            level: Some(high),
            provenance: who("alice", 2),
        },
        &mut cache,
    )
    .expect("warm cache");
    assert!(cache.recomputed_nodes() > 0);
    let warmed = cache.recomputed_nodes();
    let shared = SharedStepId::new_v7();
    p.apply_cached(
        Op::AddSharedStep {
            id: shared,
            name: "Shared".into(),
            provenance: who("alice", 3),
        },
        &mut cache,
    )
    .expect("grow shared slots");
    assert_eq!(
        cache.recomputed_nodes(),
        warmed,
        "AddSharedStep rebuilt the cache"
    );
    rate_shared(&mut p, shared, high);
    cache.update(
        &p,
        &Op::SetStepFeasibility {
            target: StepTarget::Shared(shared),
            level: Some(high),
            provenance: who("carol", 50),
        },
    );
    for (tree, node) in [(first_tree, first_node), (second_tree, second_node)] {
        p.apply_cached(
            Op::UseSharedStep {
                tree,
                node,
                step: shared,
                provenance: who("bob", 4),
            },
            &mut cache,
        )
        .expect("link cached step");
    }
    let low = p.methodology().feasibility_level("low").unwrap();
    let before = cache.recomputed_nodes();
    rate_shared(&mut p, shared, low);
    cache.update(
        &p,
        &Op::SetStepFeasibility {
            target: StepTarget::Shared(shared),
            level: Some(low),
            provenance: who("carol", 51),
        },
    );
    assert!(cache.recomputed_nodes() > before);
    for tree in [first_tree, second_tree] {
        assert_eq!(cache.evaluate_tree(&p, tree).unwrap().root_value(), &low);
    }
    assert_eq!(
        p.shared_step_users_count(p.shared_step_idx(&shared).unwrap()),
        2
    );
    assert_eq!(p.steps_via(ItemRef::Component(component)).count(), 0);
    assert_cache_matches(&p, &cache);
}

#[test]
fn eval_cache_matches_fresh_evaluation_for_shuffled_shared_edits() {
    let (base, first_tree, first_node, second_tree, second_node, component) = cache_fixture();
    let shared = SharedStepId::new_v7();
    let m = base.methodology();
    let high = m.feasibility_level("high").unwrap();
    let factor = m.feasibility_factors().next().unwrap();
    let level = m.factor_levels(factor).next().unwrap();
    let attack_type = m.attack_type("debug-access").unwrap();
    let edit_provenance = who("alice", 2);
    let setup = |id: SharedStepId, name: &str| Op::AddSharedStep {
        id,
        name: name.into(),
        provenance: who("alice", 1),
    };
    let rate = || Op::SetStepFeasibility {
        target: StepTarget::Shared(shared),
        level: Some(high),
        provenance: edit_provenance.clone(),
    };
    let factor_op = || Op::SetStepFactor {
        target: StepTarget::Shared(shared),
        factor,
        level: Some(level),
        provenance: edit_provenance.clone(),
    };
    let via = || Op::SetStepVia {
        target: StepTarget::Shared(shared),
        via: Some(ItemRef::Component(component)),
        provenance: edit_provenance.clone(),
    };
    let attack = || Op::SetStepAttackType {
        target: StepTarget::Shared(shared),
        attack_type: Some(attack_type),
        provenance: edit_provenance.clone(),
    };
    let link = |tree, node| Op::UseSharedStep {
        tree,
        node,
        step: shared,
        provenance: who("bob", 6),
    };
    let orders = [
        vec![
            setup(shared, "Shared"),
            rate(),
            factor_op(),
            via(),
            attack(),
            link(first_tree, first_node),
            link(second_tree, second_node),
        ],
        vec![
            setup(shared, "Shared"),
            factor_op(),
            attack(),
            via(),
            rate(),
            link(second_tree, second_node),
            link(first_tree, first_node),
        ],
    ];
    let mut results = Vec::new();
    for ops in orders {
        let mut p = base.clone();
        let mut cache = ProjectEvalCache::new(&p);
        for op in ops {
            p.apply_cached(op, &mut cache).expect("shuffled op");
            assert_cache_matches(&p, &cache);
        }
        results.push((p, cache));
    }
    assert_eq!(results[0].0, results[1].0);
    assert_eq!(results[0].1, results[1].1);
}

#[test]
fn unuse_undo_restores_project_refcounts_and_exact_cache_state() {
    let (mut p, first_tree, first_node, second_tree, second_node, _) = cache_fixture();
    let shared = SharedStepId::new_v7();
    add(&mut p, shared, "Shared");
    let high = p.methodology().feasibility_level("high").unwrap();
    rate_shared(&mut p, shared, high);
    for (tree, node) in [(first_tree, first_node), (second_tree, second_node)] {
        use_shared(&mut p, tree, node, shared);
    }
    let before = p.clone();
    let cache_before = ProjectEvalCache::new(&p);
    let mut cache = cache_before.clone();
    let undo = p
        .apply_cached(
            Op::UnuseSharedStep {
                tree: first_tree,
                node: first_node,
                provenance: who("dave", 99),
            },
            &mut cache,
        )
        .expect("unlink");
    assert_eq!(
        p.shared_step_users_count(p.shared_step_idx(&shared).unwrap()),
        1
    );
    assert!(p.evaluate_tree(first_tree).is_err());
    p.apply_cached(undo, &mut cache).expect("undo unlink");
    assert_eq!(p, before);
    assert_eq!(
        p.shared_step_users_count(p.shared_step_idx(&shared).unwrap()),
        2
    );
    assert_eq!(cache, cache_before);
    assert_cache_matches(&p, &cache);
}

#[test]
fn link_ops_round_trip_through_wire_and_op_log() {
    let m = builtin();
    let codec = OpCodec::new(Arc::clone(&m));
    let tree = tree_id(1);
    let node = node_id(2);
    let shared = shared_step_id(3);
    let provenance = who("alice", 42);
    let ops = [
        Op::UseSharedStep {
            tree,
            node,
            step: shared,
            provenance: provenance.clone(),
        },
        Op::UnuseSharedStep {
            tree,
            node,
            provenance: provenance.clone(),
        },
    ];
    for op in &ops {
        assert_eq!(op.provenance(), Some(&provenance));
        assert_eq!(
            op.clone().with_provenance(who("bob", 43)).provenance(),
            Some(&who("bob", 43))
        );
        let bytes = codec.encode(op).expect("encode");
        assert_eq!(codec.decode(&bytes).expect("decode"), *op);
    }
    let value: Value = serde_json::from_slice(&codec.encode(&ops[0]).unwrap()).unwrap();
    assert_eq!(value["op"], "use_shared_step");
    assert_eq!(value["shared_step"], shared.to_string());
    let value: Value = serde_json::from_slice(&codec.encode(&ops[1]).unwrap()).unwrap();
    assert_eq!(value["op"], "unuse_shared_step");
    assert!(value.get("shared_step").is_none());
    let mut log = Vec::new();
    let mut writer = OpLogWriter::create(&mut log, Arc::clone(&m)).unwrap();
    for op in &ops {
        writer.append(op).expect("append");
    }
    drop(writer);
    let decoded: Vec<_> = OpLogReader::open(log.as_slice(), Arc::clone(&m))
        .unwrap()
        .map(|record| record.unwrap().op)
        .collect();
    assert_eq!(decoded, ops);
}

#[test]
fn tree_equality_resolves_shared_links_by_public_id_not_slot_layout() {
    let shared_a = shared_step_id(10);
    let shared_b = shared_step_id(20);
    let tree = tree_id(30);
    let node = node_id(31);
    let mut left = project();
    add(&mut left, shared_a, "Same name");
    add(&mut left, shared_b, "Same name");
    add_tree(&mut left, tree, one_step_tree(node));
    use_shared(&mut left, tree, node, shared_a);
    let mut right = project();
    add(&mut right, shared_b, "Same name");
    add(&mut right, shared_a, "Same name");
    add_tree(&mut right, tree, one_step_tree(node));
    use_shared(&mut right, tree, node, shared_a);
    assert_ne!(
        left.shared_step_idx(&shared_a).unwrap().index(),
        right.shared_step_idx(&shared_a).unwrap().index()
    );
    assert_eq!(left, right);
    assert!(left.diff(&right).unwrap().is_empty());
    let mut different = right.clone();
    different
        .apply(Op::UnuseSharedStep {
            tree,
            node,
            provenance: who("alice", 1),
        })
        .unwrap();
    use_shared(&mut different, tree, node, shared_b);
    assert_ne!(left, different);
    assert_eq!(
        left.diff(&different).unwrap().attack_trees().modified(),
        [tree]
    );
}
