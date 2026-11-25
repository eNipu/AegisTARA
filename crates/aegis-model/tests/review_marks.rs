//! "Rating needs review" marks (job 057, LIB-03): a step whose rating came
//! from a library item is marked until a person reviews it, and the linter
//! reports every marked step.

use std::sync::Arc;

use aegis_model::{
    AssetId, AttackTree, DamageId, GateKind, LintCode, Methodology, ModelError, Node, NodeId, Op,
    OpCodec, OpLogReader, OpLogWriter, Project, Provenance, StepTarget, ThreatId, TreeId,
};

struct Fx {
    project: Project,
    tree: TreeId,
    step: NodeId,
    gate: NodeId,
}

fn methodology() -> Arc<Methodology> {
    Arc::new(Methodology::iso21434_r155().expect("builtin"))
}

fn fixture() -> Fx {
    let m = methodology();
    let integrity = m.property("integrity").expect("property");
    let tampering = m.threat_category("tampering").expect("category");
    let safety = m.impact_category("safety").expect("category");
    let severe = m.level(safety, "severe").expect("level");
    let low = m.feasibility_level("low").expect("level");
    let mut project = Project::new(Arc::clone(&m));
    let (asset, damage, threat, tree, root, gate, step) = (
        AssetId::new_v7(),
        DamageId::new_v7(),
        ThreatId::new_v7(),
        TreeId::new_v7(),
        NodeId::new_v7(),
        NodeId::new_v7(),
        NodeId::new_v7(),
    );
    let mut t = AttackTree::new(root, Node::gate(GateKind::Or, "Goal")).expect("tree");
    let g = t
        .add_child(t.root(), gate, Node::gate(GateKind::And, "Gate"))
        .expect("gate");
    t.add_child(g, step, Node::step("Step")).expect("step");
    for op in [
        Op::AddAsset {
            id: asset,
            name: "ECU".into(),
            properties: [integrity].into_iter().collect(),
            provenance: Provenance::default(),
        },
        Op::AddDamageScenario {
            id: damage,
            name: "D".into(),
            impacts: vec![(safety, severe)],
            provenance: Provenance::default(),
        },
        Op::AddThreatScenario {
            id: threat,
            name: "T".into(),
            asset,
            category: tampering,
            catalog_refs: vec![],
            damages: vec![damage],
            provenance: Provenance::default(),
        },
        Op::AddAttackTree {
            id: tree,
            tree: t,
            step_feasibility: vec![(step, low)],
            provenance: Provenance::default(),
        },
        Op::SetThreatAttackTree {
            threat,
            tree: Some(tree),
            provenance: Provenance::default(),
        },
    ] {
        project.apply(op).expect("applies");
    }
    Fx {
        project,
        tree,
        step,
        gate,
    }
}

fn mark(fx: &Fx, node: NodeId, marked: bool) -> Op {
    Op::SetReviewMark {
        tree: fx.tree,
        node,
        marked,
        provenance: Provenance::human("alice", 3),
    }
}

#[test]
fn a_step_is_marked_and_unmarked_and_undo_restores_everything() {
    let mut fx = fixture();
    let before = fx.project.clone();
    assert!(!fx.project.review_marked(fx.tree, fx.step));
    let undo = fx.project.apply(mark(&fx, fx.step, true)).expect("mark");
    assert!(fx.project.review_marked(fx.tree, fx.step));
    assert_eq!(
        fx.project.review_marks().collect::<Vec<_>>(),
        vec![(fx.tree, fx.step)]
    );
    assert_ne!(fx.project, before, "a mark is part of the project");
    let undo_unmark = fx.project.apply(mark(&fx, fx.step, false)).expect("unmark");
    assert!(!fx.project.review_marked(fx.tree, fx.step));
    fx.project.apply(undo_unmark).expect("redo the mark");
    fx.project.apply(undo).expect("undo the mark");
    assert_eq!(fx.project, before);
}

#[test]
fn only_steps_of_a_live_tree_can_be_marked() {
    let fx = fixture();
    assert!(matches!(
        fx.project.validate(&mark(&fx, fx.gate, true)),
        Err(ModelError::NotAStep { .. })
    ));
    assert!(matches!(
        fx.project.validate(&mark(&fx, NodeId::new_v7(), true)),
        Err(ModelError::UnknownTreeNode { .. })
    ));
    let other = Op::SetReviewMark {
        tree: TreeId::new_v7(),
        node: fx.step,
        marked: true,
        provenance: Provenance::default(),
    };
    assert!(matches!(
        fx.project.validate(&other),
        Err(ModelError::UnknownAttackTree(_))
    ));
}

#[test]
fn a_marked_step_and_a_tree_with_marks_cannot_be_removed() {
    let mut fx = fixture();
    let m = methodology();
    let _ = m;
    fx.project.apply(mark(&fx, fx.step, true)).expect("mark");
    // Clear the rating first, as for any step that has one.
    fx.project
        .apply(Op::SetStepFeasibility {
            target: StepTarget::Step(fx.tree, fx.step),
            level: None,
            provenance: Provenance::default(),
        })
        .expect("clear rating");
    let remove = Op::RemoveTreeNode {
        tree: fx.tree,
        id: fx.step,
        provenance: Provenance::default(),
    };
    assert_eq!(
        fx.project.validate(&remove),
        Err(ModelError::NodeIsRated(fx.step))
    );
    assert!(matches!(
        fx.project.validate(&Op::RemoveAttackTree { id: fx.tree }),
        Err(ModelError::NodeIsRated(_))
    ));
    fx.project.apply(mark(&fx, fx.step, false)).expect("unmark");
    fx.project.validate(&remove).expect("now it can go");
}

#[test]
fn the_linter_reports_each_marked_step_until_it_is_reviewed() {
    let mut fx = fixture();
    let codes = |p: &Project| -> Vec<LintCode> {
        p.lint()
            .into_iter()
            .map(|f| f.code())
            .filter(|c| matches!(c, LintCode::LibraryRatingNotReviewed { .. }))
            .collect()
    };
    assert!(codes(&fx.project).is_empty());
    fx.project.apply(mark(&fx, fx.step, true)).expect("mark");
    assert_eq!(
        codes(&fx.project),
        vec![LintCode::LibraryRatingNotReviewed {
            tree: fx.tree,
            node: fx.step
        }]
    );
    let finding = fx
        .project
        .lint()
        .into_iter()
        .find(|f| matches!(f.code(), LintCode::LibraryRatingNotReviewed { .. }))
        .expect("finding");
    assert_eq!(finding.severity(), aegis_model::LintSeverity::Warning);
    fx.project.apply(mark(&fx, fx.step, false)).expect("review");
    assert!(codes(&fx.project).is_empty());
}

#[test]
fn marks_survive_save_load_and_old_files_are_unchanged() {
    let mut fx = fixture();
    let m = methodology();
    let mut plain = Vec::new();
    fx.project.save(&mut plain).expect("save");
    assert!(
        !String::from_utf8_lossy(&plain).contains("review"),
        "no marks, no field"
    );
    fx.project.apply(mark(&fx, fx.step, true)).expect("mark");
    let mut bytes = Vec::new();
    fx.project.save(&mut bytes).expect("save");
    let loaded = Project::load(bytes.as_slice(), Arc::clone(&m)).expect("load");
    assert_eq!(loaded, fx.project);
    assert!(loaded.review_marked(fx.tree, fx.step));
    let mut again = Vec::new();
    loaded.save(&mut again).expect("save");
    assert_eq!(again, bytes);
}

#[test]
fn the_op_log_carries_marks() {
    let fx = fixture();
    let m = methodology();
    let op = mark(&fx, fx.step, true);
    let mut log = Vec::new();
    let mut w = OpLogWriter::create(&mut log, Arc::clone(&m)).expect("writer");
    w.append(&op).expect("append");
    drop(w);
    let read: Vec<_> = OpLogReader::open(log.as_slice(), Arc::clone(&m))
        .expect("header")
        .map(|r| r.expect("record").op)
        .collect();
    assert_eq!(read, vec![op.clone()]);
    let codec = OpCodec::new(m);
    assert_eq!(codec.decode(&codec.encode(&op).expect("e")).expect("d"), op);
}
