//! Tests of the `Assumption` entity (`APL-04`): IDs, visual prefix `AS`,
//! lifecycle ops, validation guards (L09, L15), descriptions with translations,
//! condition linking, `ProjectDiff::assumptions`, op-log round-trips, and L30
//! full-`Project` undo equality.

use std::sync::Arc;

use aegis_model::{
    AssetId, AssumptionId, AssumptionIdx, AttackTree, ClaimId, ComponentId, ConditionId, DamageId,
    ElementRef, EntityKind, FeasibilityLevel, GateKind, InterfaceId, ItemRef, Limit, LintCode,
    LintFinding, LintSeverity, MAX_ASSUMPTION_EXCLUDES, MAX_ASSUMPTIONS, MAX_CLAIM_ASSUMPTIONS,
    MAX_DESCRIPTION_CHARS, MAX_NAME_BYTES, MAX_NAME_CHARS, Methodology, ModelError, Node, NodeId,
    Op, OpCodec, OpLogReader, OpLogWriter, Project, ProjectDiff, ProjectEvalCache, Provenance,
    Reason, RiskLevel, StepRef, StepTarget, StoreError, ThreatId, TreeId, VisualPrefix,
};
use proptest::prelude::*;
use serde_json::{Value, json};

fn project() -> Project {
    Project::new(Arc::new(Methodology::iso21434_r155().expect("builtin")))
}

fn who(actor: &str, at: u64) -> Provenance {
    Provenance::human(actor, at)
}

fn add_assumption(p: &mut Project, id: AssumptionId, name: &str, holds: bool) {
    p.apply(Op::AddAssumption {
        id,
        name: name.into(),
        holds,
        provenance: who("alice", 1_700_000_000_000),
    })
    .expect("add assumption");
}

fn inverse_round_trip(p: &mut Project, op: Op, expected_inverse: Op) {
    let before = p.clone();
    let undo = p.apply(op).expect("apply");
    assert_eq!(undo, expected_inverse);
    p.apply(undo).expect("apply inverse");
    assert_eq!(p, &before);
}

#[test]
fn ids_and_visual_ids() {
    assert_eq!(std::mem::size_of::<AssumptionIdx>(), 4);
    assert_eq!(VisualPrefix::Assumption.as_str(), "AS");
    assert_eq!(VisualPrefix::Assumption.format(0), "AS00");
    assert_eq!(VisualPrefix::Assumption.format(42), "AS42");
    assert_eq!(VisualPrefix::Assumption.format(100), "AS100");
    assert_eq!(VisualPrefix::Assumption.parse_num("AS00"), Some(0));
    assert_eq!(VisualPrefix::Assumption.parse_num("AS42"), Some(42));
    assert_eq!(VisualPrefix::Asset.parse_num("AS00"), None);

    assert_eq!(EntityKind::Assumption.as_str(), "assumption");
    assert_eq!(
        ElementRef::Assumption(AssumptionId::new_v7()).max_chars(),
        MAX_DESCRIPTION_CHARS
    );
    assert!(ProjectDiff::default().assumptions().is_empty());

    let mut p = project();
    let a = AssumptionId::new_v7();
    let b = AssumptionId::new_v7();
    add_assumption(&mut p, b, "Bootloader is locked", true);
    add_assumption(&mut p, a, "ECU is enclosed", false);

    let ai = p.assumption_idx(&a).expect("a idx");
    let bi = p.assumption_idx(&b).expect("b idx");
    assert_eq!(p.assumption_id(ai), Some(a));
    assert_eq!(p.assumption_visual_num(bi), Some(0));
    assert_eq!(p.assumption_visual_id(bi).as_deref(), Some("AS00"));
    assert_eq!(p.assumption_visual_id(ai).as_deref(), Some("AS01"));
    assert_eq!(
        p.visual_id(ElementRef::Assumption(b)).as_deref(),
        Some("AS00")
    );
    assert_eq!(
        p.visual_id(ElementRef::Assumption(a)).as_deref(),
        Some("AS01")
    );

    let got = p.assumption(ai).expect("a");
    assert_eq!(got.name(), "ECU is enclosed");
    assert!(!got.holds());
    assert!(got.excludes_trees().is_empty());
    assert!(got.excludes_steps().is_empty());
    assert_eq!(got.provenance(), &who("alice", 1_700_000_000_000));

    let walked: Vec<_> = p.assumptions().map(|(_, id, _)| id).collect();
    let mut expected = vec![a, b];
    expected.sort();
    assert_eq!(walked, expected);

    // Visual numbers stay monotonic across removals.
    p.apply(Op::RemoveAssumption { id: b }).expect("remove b");
    let c = AssumptionId::new_v7();
    add_assumption(&mut p, c, "Debug port fused", true);
    let ci = p.assumption_idx(&c).expect("c idx");
    assert_eq!(p.assumption_visual_id(ci).as_deref(), Some("AS02"));
    assert_eq!(p.assumption_idx(&b), None);
}

#[test]
fn lifecycle_ops_have_exact_inverses_and_l30_equality() {
    assert_eq!(EntityKind::Assumption.as_str(), "assumption");
    assert_eq!(
        ElementRef::Assumption(AssumptionId::new_v7()).max_chars(),
        MAX_DESCRIPTION_CHARS
    );
    assert!(ProjectDiff::default().assumptions().is_empty());

    let mut p = project();
    let id = AssumptionId::new_v7();
    let orig_prov = who("alice", 10);
    let add_op = Op::AddAssumption {
        id,
        name: "ECU physically secured".into(),
        holds: true,
        provenance: orig_prov.clone(),
    };
    inverse_round_trip(&mut p, add_op.clone(), Op::RemoveAssumption { id });

    p.apply(add_op).expect("add");
    let rename_prov = who("bob", 20);
    inverse_round_trip(
        &mut p,
        Op::RenameAssumption {
            id,
            name: "ECU inside locked cabin".into(),
            provenance: rename_prov.clone(),
        },
        Op::RenameAssumption {
            id,
            name: "ECU physically secured".into(),
            provenance: orig_prov.clone(),
        },
    );

    let toggle_prov = who("carol", 30);
    inverse_round_trip(
        &mut p,
        Op::SetAssumptionHolds {
            id,
            holds: false,
            provenance: toggle_prov.clone(),
        },
        Op::SetAssumptionHolds {
            id,
            holds: true,
            provenance: orig_prov.clone(),
        },
    );

    // Mutate name and holds, then remove and undo the whole sequence back to original.
    let after_add = p.clone();
    let u1 = p
        .apply(Op::RenameAssumption {
            id,
            name: "ECU locked".into(),
            provenance: rename_prov,
        })
        .expect("rename");
    let u2 = p
        .apply(Op::SetAssumptionHolds {
            id,
            holds: false,
            provenance: toggle_prov.clone(),
        })
        .expect("toggle");
    let mut cache = ProjectEvalCache::new(&p);
    let u_cache = p
        .apply(
            Op::SetAssumptionHolds {
                id,
                holds: true,
                provenance: Provenance::default(),
            }
            .with_provenance(toggle_prov),
        )
        .expect("toggle with_provenance");
    cache.update(&p, &u_cache);
    p.apply(u_cache).expect("undo toggle");
    let before_remove = p.clone();
    let u3 = p.apply(Op::RemoveAssumption { id }).expect("remove");
    assert_eq!(p.assumptions().count(), 0);

    p.apply(u3).expect("undo remove");
    assert_eq!(p, before_remove);
    p.apply(u2).expect("undo toggle");
    p.apply(u1).expect("undo rename");
    assert_eq!(p, after_add);

    // Also verify AS100 visual number survives remove + undo.
    let mut many = project();
    let mut ids = Vec::new();
    for _ in 0..101 {
        let mid = AssumptionId::new_v7();
        ids.push(mid);
        add_assumption(&mut many, mid, "Assumption", true);
    }
    let last_id = ids[100];
    let last_idx = many.assumption_idx(&last_id).expect("last");
    assert_eq!(
        many.assumption_visual_id(last_idx).as_deref(),
        Some("AS100")
    );
    let before = many.clone();
    let undo_rm = many
        .apply(Op::RemoveAssumption { id: last_id })
        .expect("remove");
    many.apply(undo_rm).expect("undo");
    assert_eq!(many, before);
}

#[test]
fn validation_guards_and_l15_blank_actor() {
    assert_eq!(EntityKind::Assumption.as_str(), "assumption");
    assert_eq!(
        ElementRef::Assumption(AssumptionId::new_v7()).max_chars(),
        MAX_DESCRIPTION_CHARS
    );
    assert!(ProjectDiff::default().assumptions().is_empty());

    let mut p = project();
    let id = AssumptionId::new_v7();
    add_assumption(&mut p, id, "ECU physically secured", true);

    assert_eq!(
        p.apply(Op::AddAssumption {
            id,
            name: "Other".into(),
            holds: false,
            provenance: who("alice", 1),
        }),
        Err(ModelError::DuplicateAssumptionId(id))
    );

    let missing = AssumptionId::new_v7();
    assert_eq!(
        p.apply(Op::RenameAssumption {
            id: missing,
            name: "Valid".into(),
            provenance: who("alice", 1),
        }),
        Err(ModelError::UnknownAssumption(missing))
    );
    assert_eq!(
        p.apply(Op::SetAssumptionHolds {
            id: missing,
            holds: false,
            provenance: who("alice", 1),
        }),
        Err(ModelError::UnknownAssumption(missing))
    );
    assert_eq!(
        p.apply(Op::RemoveAssumption { id: missing }),
        Err(ModelError::UnknownAssumption(missing))
    );

    // Blank names on Add and Rename.
    let blank_add = p.apply(Op::AddAssumption {
        id: AssumptionId::new_v7(),
        name: "   ".into(),
        holds: true,
        provenance: who("alice", 1),
    });
    assert_eq!(
        blank_add,
        Err(ModelError::EmptyName {
            kind: EntityKind::Assumption
        })
    );
    assert!(blank_add.unwrap_err().to_string().contains("assumption"));

    assert_eq!(
        p.apply(Op::RenameAssumption {
            id,
            name: " \t ".into(),
            provenance: who("alice", 1),
        }),
        Err(ModelError::EmptyName {
            kind: EntityKind::Assumption
        })
    );

    // Over-long names on Add and Rename.
    let long = "x".repeat(MAX_NAME_CHARS + 1);
    let long_add = p.apply(Op::AddAssumption {
        id: AssumptionId::new_v7(),
        name: long.as_str().into(),
        holds: true,
        provenance: who("alice", 1),
    });
    assert_eq!(
        long_add,
        Err(ModelError::NameTooLong {
            kind: EntityKind::Assumption,
            max: MAX_NAME_CHARS
        })
    );
    assert!(!long_add.unwrap_err().to_string().contains(&long));

    assert_eq!(
        p.apply(Op::RenameAssumption {
            id,
            name: long.as_str().into(),
            provenance: who("alice", 1),
        }),
        Err(ModelError::NameTooLong {
            kind: EntityKind::Assumption,
            max: MAX_NAME_CHARS
        })
    );

    // L15: blank actor provenance rejected for ALL THREE provenance-carrying ops.
    assert_eq!(
        p.apply(Op::AddAssumption {
            id: AssumptionId::new_v7(),
            name: "Valid".into(),
            holds: true,
            provenance: Provenance::human("   ", 1),
        }),
        Err(ModelError::EmptyActor)
    );
    assert_eq!(
        p.apply(Op::RenameAssumption {
            id,
            name: "Valid rename".into(),
            provenance: Provenance::human("\t ", 2),
        }),
        Err(ModelError::EmptyActor)
    );
    assert_eq!(
        p.apply(Op::SetAssumptionHolds {
            id,
            holds: false,
            provenance: Provenance::human(" ", 3),
        }),
        Err(ModelError::EmptyActor)
    );

    // Cap at MAX_ASSUMPTIONS = 256.
    for _ in 1..MAX_ASSUMPTIONS {
        add_assumption(&mut p, AssumptionId::new_v7(), "Assumption", true);
    }
    assert_eq!(p.assumptions().count(), MAX_ASSUMPTIONS);
    assert_eq!(
        p.apply(Op::AddAssumption {
            id: AssumptionId::new_v7(),
            name: "Too many".into(),
            holds: true,
            provenance: who("alice", 1),
        }),
        Err(ModelError::TooManyAssumptions {
            max: MAX_ASSUMPTIONS
        })
    );
}

#[test]
fn remove_assumption_l09_single_guard_isolation_and_descriptions() {
    assert_eq!(EntityKind::Assumption.as_str(), "assumption");
    assert_eq!(
        ElementRef::Assumption(AssumptionId::new_v7()).max_chars(),
        MAX_DESCRIPTION_CHARS
    );
    assert!(ProjectDiff::default().assumptions().is_empty());

    let mut p = project();
    let id = AssumptionId::new_v7();
    let cond = ConditionId::new_v7();
    let target = ElementRef::Assumption(id);

    add_assumption(&mut p, id, "ECU physically secured", true);
    p.apply(Op::AddCondition {
        id: cond,
        name: "Production mode".into(),
        description: "".into(),
        present: true,
        provenance: who("alice", 1),
    })
    .expect("add condition");
    let baseline = p.clone();

    // L09 guard 1: ONLY a linked condition blocks removal.
    let u_need = p
        .apply(Op::NeedCondition {
            element: target,
            condition: cond,
            provenance: who("alice", 2),
        })
        .expect("need condition");
    assert_eq!(
        p.apply(Op::RemoveAssumption { id }),
        Err(ModelError::ElementNeedsConditions {
            element: target,
            conditions: 1
        })
    );
    p.apply(u_need).expect("unneed");
    assert_eq!(p, baseline);

    // L09 guard 2: ONLY a description (and with translation) blocks removal.
    let u_langs = p
        .apply(Op::SetProjectLanguages {
            langs: Arc::from([Arc::from("ja")]),
            provenance: who("alice", 3),
        })
        .expect("set langs");
    let u_desc = p
        .apply(Op::SetDescription {
            target,
            text: "Mounted inside locked enclosure.".into(),
            provenance: who("alice", 4),
        })
        .expect("set description");
    assert_eq!(
        p.apply(Op::RemoveAssumption { id }),
        Err(ModelError::DescriptionNotCleared {
            kind: EntityKind::Assumption
        })
    );

    let u_trans = p
        .apply(Op::SetDescriptionTranslation {
            target,
            lang: "ja".into(),
            text: "ロックされた筐体内に設置。".into(),
            provenance: who("alice", 5),
        })
        .expect("set translation");
    assert_eq!(
        p.description_translation(target, "ja"),
        Some("ロックされた筐体内に設置。")
    );
    assert_eq!(
        p.apply(Op::RemoveAssumption { id }),
        Err(ModelError::DescriptionNotCleared {
            kind: EntityKind::Assumption
        })
    );

    // Clear translation and description, remove, then undo step-by-step back to baseline.
    let u_clear_trans = p
        .apply(Op::SetDescriptionTranslation {
            target,
            lang: "ja".into(),
            text: "".into(),
            provenance: who("alice", 6),
        })
        .expect("clear translation");
    let u_clear_desc = p
        .apply(Op::SetDescription {
            target,
            text: "".into(),
            provenance: who("alice", 7),
        })
        .expect("clear description");
    let u_remove = p.apply(Op::RemoveAssumption { id }).expect("remove");
    assert_eq!(p.assumptions().count(), 0);

    for undo in [
        u_remove,
        u_clear_desc,
        u_clear_trans,
        u_trans,
        u_desc,
        u_langs,
    ] {
        p.apply(undo).expect("undo");
    }
    assert_eq!(p, baseline);
}

#[test]
fn project_diff_assumptions_tracks_lifecycle_descriptions_and_condition_links() {
    let mut base = project();
    let (a, b, gone) = (
        AssumptionId::new_v7(),
        AssumptionId::new_v7(),
        AssumptionId::new_v7(),
    );
    let cond = ConditionId::new_v7();
    add_assumption(&mut base, a, "A", true);
    add_assumption(&mut base, b, "B", true);
    add_assumption(&mut base, gone, "Gone", false);
    base.apply(Op::AddCondition {
        id: cond,
        name: "Feature enabled".into(),
        description: "".into(),
        present: true,
        provenance: who("alice", 1),
    })
    .expect("add condition");

    assert!(base.diff(&base).expect("empty diff").is_empty());

    let mut curr = base.clone();
    let added = AssumptionId::new_v7();
    add_assumption(&mut curr, added, "Added", true);
    curr.apply(Op::RemoveAssumption { id: gone })
        .expect("remove gone");
    curr.apply(Op::SetAssumptionHolds {
        id: a,
        holds: false,
        provenance: who("alice", 2),
    })
    .expect("toggle a");
    curr.apply(Op::SetDescription {
        target: ElementRef::Assumption(b),
        text: "Explained".into(),
        provenance: who("alice", 3),
    })
    .expect("describe b");

    let diff = base.diff(&curr).expect("diff");
    assert!(!diff.is_empty());
    assert_eq!(diff.assumptions().added(), [added]);
    assert_eq!(diff.assumptions().removed(), [gone]);
    let mut expected_mod = vec![a, b];
    expected_mod.sort();
    assert_eq!(diff.assumptions().modified(), expected_mod.as_slice());

    // Condition link changes on ElementRef::Assumption merge into diff.assumptions.
    let mut linked = base.clone();
    linked
        .apply(Op::NeedCondition {
            element: ElementRef::Assumption(a),
            condition: cond,
            provenance: who("alice", 4),
        })
        .expect("need condition");
    let link_diff = base.diff(&linked).expect("link diff");
    assert!(!link_diff.is_empty());
    assert_eq!(link_diff.assumptions().modified(), [a]);
}

#[test]
fn op_log_round_trips_every_assumption_op_and_target() {
    let m = Arc::new(Methodology::iso21434_r155().expect("builtin"));
    let id = AssumptionId::new_v7();
    let cond = ConditionId::new_v7();
    let target = ElementRef::Assumption(id);
    let prov = who("alice", 42);

    let ops = vec![
        Op::AddCondition {
            id: cond,
            name: "C".into(),
            description: "".into(),
            present: true,
            provenance: prov.clone(),
        },
        Op::AddAssumption {
            id,
            name: "ECU physically secured".into(),
            holds: true,
            provenance: prov.clone(),
        },
        Op::RenameAssumption {
            id,
            name: "ECU inside locked cabin".into(),
            provenance: prov.clone(),
        },
        Op::SetAssumptionHolds {
            id,
            holds: false,
            provenance: prov.clone(),
        },
        Op::SetProjectLanguages {
            langs: Arc::from([Arc::from("ja")]),
            provenance: prov.clone(),
        },
        Op::SetDescription {
            target,
            text: "Locked cabinet".into(),
            provenance: prov.clone(),
        },
        Op::SetDescriptionTranslation {
            target,
            lang: "ja".into(),
            text: "施錠されたキャビネット".into(),
            provenance: prov.clone(),
        },
        Op::NeedCondition {
            element: target,
            condition: cond,
            provenance: prov.clone(),
        },
        Op::UnneedCondition {
            element: target,
            condition: cond,
            provenance: prov.clone(),
        },
        Op::SetDescriptionTranslation {
            target,
            lang: "ja".into(),
            text: "".into(),
            provenance: prov.clone(),
        },
        Op::SetDescription {
            target,
            text: "".into(),
            provenance: prov,
        },
        Op::RemoveAssumption { id },
        Op::RemoveCondition { id: cond },
    ];

    let codec = OpCodec::new(Arc::clone(&m));
    for op in &ops {
        assert_eq!(
            codec
                .decode(&codec.encode(op).expect("encode"))
                .expect("decode"),
            *op
        );
    }

    let mut log = Vec::new();
    let mut w = OpLogWriter::create(&mut log, Arc::clone(&m)).expect("writer");
    for op in &ops {
        w.append(op).expect("append");
    }
    drop(w);
    let decoded: Vec<_> = OpLogReader::open(log.as_slice(), Arc::clone(&m))
        .expect("open")
        .map(|r| r.expect("record").op)
        .collect();
    assert_eq!(decoded, ops);

    // Over-cap name is rejected by within_caps before writing.
    let mut capped_log = Vec::new();
    let mut cw = OpLogWriter::create(&mut capped_log, m).expect("writer");
    let over_name: Arc<str> = "x".repeat(MAX_NAME_BYTES + 1).into();
    assert!(matches!(
        cw.append(&Op::AddAssumption {
            id,
            name: Arc::clone(&over_name),
            holds: true,
            provenance: Provenance::default(),
        }),
        Err(StoreError::UnencodableOp)
    ));
    assert!(matches!(
        cw.append(&Op::RenameAssumption {
            id,
            name: over_name,
            provenance: Provenance::default(),
        }),
        Err(StoreError::UnencodableOp)
    ));
}

fn add_claim(p: &mut Project, id: ClaimId, name: &str) {
    p.apply(Op::AddCybersecurityClaim {
        id,
        name: name.into(),
        provenance: who("alice", 1),
    })
    .expect("add claim");
}

fn add_tree_with_steps(
    p: &mut Project,
    tree_id: TreeId,
    step_count: usize,
) -> (NodeId, Vec<NodeId>) {
    let root = NodeId::new_v7();
    let mut tree = AttackTree::new(root, Node::gate(GateKind::Or, "Root")).expect("tree");
    let mut steps = Vec::with_capacity(step_count);
    for i in 0..step_count {
        let s = NodeId::new_v7();
        tree.add_child(tree.root(), s, Node::step(format!("Step {i}")))
            .expect("step");
        steps.push(s);
    }
    p.apply(Op::AddAttackTree {
        role: Default::default(),
        id: tree_id,
        tree,
        step_feasibility: vec![],
        provenance: who("alice", 1),
    })
    .expect("add tree");
    (root, steps)
}

#[test]
fn set_assumption_excludes_sorts_dedups_and_keeps_step_ref_stable() {
    let mut p = project();
    let a = AssumptionId::new_v7();
    add_assumption(&mut p, a, "ECU locked", true);

    let t1 = TreeId::new_v7();
    let t2 = TreeId::new_v7();
    let (root1, steps1) = add_tree_with_steps(&mut p, t1, 2);
    let (_root2, _steps2) = add_tree_with_steps(&mut p, t2, 1);
    let (s1, s2) = (steps1[0], steps1[1]);

    // Gate target fails with NotAStep.
    assert_eq!(
        p.apply(Op::SetAssumptionExcludes {
            id: a,
            trees: vec![],
            steps: vec![StepRef {
                tree: t1,
                node: root1,
            }],
            provenance: who("alice", 2),
        }),
        Err(ModelError::NotAStep {
            tree: t1,
            node: root1,
        })
    );

    // Unknown tree / unknown node fail cleanly.
    let missing_tree = TreeId::new_v7();
    assert_eq!(
        p.apply(Op::SetAssumptionExcludes {
            id: a,
            trees: vec![missing_tree],
            steps: vec![],
            provenance: who("alice", 2),
        }),
        Err(ModelError::UnknownAttackTree(missing_tree))
    );

    // Unsorted + duplicate trees and steps are sorted and deduplicated on store and inverse.
    let sr2 = StepRef { tree: t1, node: s2 };
    let mut sorted_trees = vec![t1, t2];
    sorted_trees.sort();
    let mut cache = ProjectEvalCache::new(&p);
    let p_first = who("alice", 10);
    let inv1 = p
        .apply(Op::SetAssumptionExcludes {
            id: a,
            trees: vec![t2, t1, t2],
            steps: vec![sr2, sr2],
            provenance: p_first.clone(),
        })
        .expect("set excludes");
    cache.update(&p, &inv1);
    assert_eq!(
        inv1,
        Op::SetAssumptionExcludes {
            id: a,
            trees: vec![],
            steps: vec![],
            provenance: who("alice", 1_700_000_000_000),
        }
    );

    let ai = p.assumption_idx(&a).expect("idx");
    let got = p.assumption(ai).expect("assumption");
    let expected_tree_idxs: Vec<_> = sorted_trees
        .iter()
        .map(|t| p.attack_tree_idx(t).expect("tree idx"))
        .collect();
    assert_eq!(got.excludes_trees(), expected_tree_idxs.as_slice());
    assert_eq!(got.excludes_steps(), &[sr2]);

    // Removing sibling step s1 compacts NodeIdx in t1; StepRef for s2 remains pinned to s2.
    let before_rm_s1 = p.clone();
    let inv_rm_s1 = p
        .apply(Op::RemoveTreeNode {
            tree: t1,
            id: s1,
            provenance: who("bob", 11),
        })
        .expect("remove sibling s1");
    assert_eq!(p.assumption(ai).expect("a").excludes_steps(), &[sr2]);
    assert_eq!(
        p.apply(Op::RemoveTreeNode {
            tree: t1,
            id: s2,
            provenance: who("bob", 12),
        }),
        Err(ModelError::NodeIsRated(s2))
    );
    p.apply(inv_rm_s1).expect("undo remove s1");
    assert_eq!(p, before_rm_s1);

    // ProjectDiff tracks SetAssumptionExcludes under diff.assumptions.
    let diff = before_rm_s1.diff(&p).expect("diff");
    assert!(diff.is_empty());
    let mut after = p.clone();
    after
        .apply(Op::SetAssumptionExcludes {
            id: a,
            trees: vec![],
            steps: vec![],
            provenance: who("carol", 20),
        })
        .expect("clear");
    let diff2 = p.diff(&after).expect("diff2");
    assert_eq!(diff2.assumptions().modified(), &[a]);

    // Undo SetAssumptionExcludes restores exact prior state & provenance.
    let before_replace = p.clone();
    let inv2 = p
        .apply(Op::SetAssumptionExcludes {
            id: a,
            trees: vec![],
            steps: vec![],
            provenance: who("carol", 20),
        })
        .expect("clear");
    assert_eq!(
        inv2,
        Op::SetAssumptionExcludes {
            id: a,
            trees: sorted_trees,
            steps: vec![sr2],
            provenance: p_first,
        }
    );
    p.apply(inv2).expect("undo clear");
    assert_eq!(p, before_replace);
}

#[test]
fn combined_excludes_cap_enforced_in_model_and_codec() {
    let mut p = project();
    let a = AssumptionId::new_v7();
    add_assumption(&mut p, a, "Combined cap", true);

    // 130 live trees, each with 1 live step (staying well under MAX_TREE_OP_NODES = 32 per tree).
    let mut trees = Vec::with_capacity(130);
    let mut steps = Vec::with_capacity(127);
    for i in 0..130 {
        let tid = TreeId::new_v7();
        let (_, tree_steps) = add_tree_with_steps(&mut p, tid, 1);
        trees.push(tid);
        if i < 127 {
            steps.push(StepRef {
                tree: tid,
                node: tree_steps[0],
            });
        }
    }

    // Exactly 130 + 126 = 256 succeeds in model and op log.
    let ok_op = Op::SetAssumptionExcludes {
        id: a,
        trees: trees.clone(),
        steps: steps[..126].to_vec(),
        provenance: who("alice", 2),
    };
    let before = p.clone();
    let inv = p.apply(ok_op.clone()).expect("256 excludes allowed");
    p.apply(inv).expect("undo 256");
    assert_eq!(p, before);
    let ok_codec = OpCodec::new(Arc::clone(p.methodology()));
    let ok_bytes = ok_codec.encode(&ok_op).expect("encode 256");
    assert_eq!(ok_codec.decode(&ok_bytes).expect("decode 256"), ok_op);

    // 130 + 127 = 257 (each list individually <= 256) fails ONLY due to combined cap.
    let over_op = Op::SetAssumptionExcludes {
        id: a,
        trees: trees.clone(),
        steps: steps.clone(),
        provenance: who("alice", 3),
    };
    assert_eq!(
        p.apply(over_op.clone()),
        Err(ModelError::TooManyAssumptionExcludes {
            max: MAX_ASSUMPTION_EXCLUDES
        })
    );

    // OpLogWriter::append refuses it via within_caps.
    let m = Arc::clone(p.methodology());
    let mut buf = Vec::new();
    let mut w = OpLogWriter::create(&mut buf, Arc::clone(&m)).expect("writer");
    assert!(matches!(w.append(&over_op), Err(StoreError::UnencodableOp)));
    drop(w);

    // Craft a valid-JSON op-log line with 130 trees + 127 steps and verify OpLogReader + OpCodec reject it.
    let tree_json: Vec<String> = trees.iter().map(|t| format!("\"{t}\"")).collect();
    let step_json: Vec<String> = steps
        .iter()
        .map(|s| format!("{{\"tree\":\"{}\",\"node\":\"{}\"}}", s.tree, s.node))
        .collect();
    let op_json = format!(
        "{{\"op\":\"set_assumption_excludes\",\"id\":\"{a}\",\"trees\":[{}],\"steps\":[{}]}}",
        tree_json.join(","),
        step_json.join(",")
    );
    let codec = OpCodec::new(Arc::clone(&m));
    assert!(matches!(
        codec.decode(op_json.as_bytes()),
        Err(StoreError::LimitExceeded(Limit::AssumptionExcludes {
            max: MAX_ASSUMPTION_EXCLUDES
        }))
    ));

    let mut raw_log = Vec::new();
    let lw = OpLogWriter::create(&mut raw_log, Arc::clone(&m)).expect("header");
    drop(lw);
    raw_log.extend_from_slice(format!("{{\"seq\":1,\"op\":{op_json}}}\n").as_bytes());
    let mut reader = OpLogReader::open(raw_log.as_slice(), m).expect("open");
    let first = reader.next().expect("record 1");
    assert!(matches!(
        first,
        Err(StoreError::LimitExceeded(Limit::AssumptionExcludes {
            max: MAX_ASSUMPTION_EXCLUDES
        }))
    ));
}

#[test]
fn claim_assumption_links_caps_equality_and_diff() {
    let mut p = project();
    let claim = ClaimId::new_v7();
    add_claim(&mut p, claim, "Locked enclosure");

    let a1 = AssumptionId::new_v7();
    add_assumption(&mut p, a1, "A1", true);

    assert_eq!(
        p.apply(Op::UnlinkClaimAssumption {
            claim,
            assumption: a1,
            provenance: who("alice", 2),
        }),
        Err(ModelError::ClaimAssumptionNotLinked {
            claim,
            assumption: a1
        })
    );

    let before_link = p.clone();
    let inv_link = p
        .apply(Op::LinkClaimAssumption {
            claim,
            assumption: a1,
            provenance: who("alice", 3),
        })
        .expect("link");
    assert_eq!(
        inv_link,
        Op::UnlinkClaimAssumption {
            claim,
            assumption: a1,
            provenance: who("alice", 1),
        }
    );
    assert_eq!(
        p.apply(Op::LinkClaimAssumption {
            claim,
            assumption: a1,
            provenance: who("alice", 4),
        }),
        Err(ModelError::ClaimAssumptionAlreadyLinked {
            claim,
            assumption: a1
        })
    );
    let diff_linked = before_link.diff(&p).expect("diff");
    assert_eq!(diff_linked.cybersecurity_claims().modified(), &[claim]);

    let after_link = p.clone();
    let inv_unlink = p
        .apply(Op::UnlinkClaimAssumption {
            claim,
            assumption: a1,
            provenance: who("bob", 5),
        })
        .expect("unlink");
    assert_eq!(
        inv_unlink,
        Op::LinkClaimAssumption {
            claim,
            assumption: a1,
            provenance: who("alice", 3),
        }
    );
    assert_eq!(
        after_link
            .diff(&p)
            .expect("diff unlink")
            .cybersecurity_claims()
            .modified(),
        &[claim]
    );
    p.apply(inv_unlink).expect("undo unlink");
    assert_eq!(p, after_link);
    p.apply(inv_link).expect("undo link");
    assert_eq!(p, before_link);

    // L29: Project::eq compares claim.assumptions by public ID across distinct slot layouts.
    let mut p2 = project();
    let filler = AssumptionId::new_v7();
    add_assumption(&mut p2, filler, "Filler", false);
    add_assumption(&mut p2, a1, "A1", true);
    p2.apply(Op::RemoveAssumption { id: filler })
        .expect("remove filler");
    add_claim(&mut p2, claim, "Locked enclosure");
    let mut p1 = project();
    add_assumption(&mut p1, a1, "A1", true);
    add_claim(&mut p1, claim, "Locked enclosure");
    p1.apply(Op::LinkClaimAssumption {
        claim,
        assumption: a1,
        provenance: who("alice", 10),
    })
    .expect("link p1");
    assert_ne!(p1, p2);
    p2.apply(Op::LinkClaimAssumption {
        claim,
        assumption: a1,
        provenance: who("alice", 10),
    })
    .expect("link p2");
    assert_eq!(p1, p2);

    // Cap at MAX_CLAIM_ASSUMPTIONS = 64.
    let mut p_cap = project();
    add_claim(&mut p_cap, claim, "Cap claim");
    let mut added = Vec::new();
    for _ in 0..MAX_CLAIM_ASSUMPTIONS {
        let aid = AssumptionId::new_v7();
        add_assumption(&mut p_cap, aid, "A", true);
        p_cap
            .apply(Op::LinkClaimAssumption {
                claim,
                assumption: aid,
                provenance: who("alice", 1),
            })
            .expect("link within cap");
        added.push(aid);
    }
    let ci = p_cap.cybersecurity_claim_idx(&claim).expect("claim idx");
    let stored_ids: Vec<_> = p_cap
        .cybersecurity_claim(ci)
        .expect("claim")
        .assumptions()
        .iter()
        .map(|&idx| p_cap.assumption_id(idx).expect("id"))
        .collect();
    added.sort();
    assert_eq!(stored_ids, added);

    let extra = AssumptionId::new_v7();
    add_assumption(&mut p_cap, extra, "Extra", true);
    assert_eq!(
        p_cap.apply(Op::LinkClaimAssumption {
            claim,
            assumption: extra,
            provenance: who("alice", 2),
        }),
        Err(ModelError::TooManyClaimAssumptions {
            max: MAX_CLAIM_ASSUMPTIONS
        })
    );
}

#[test]
fn l15_blank_actor_rejected_for_all_three_new_ops() {
    let mut p = project();
    let a = AssumptionId::new_v7();
    let claim = ClaimId::new_v7();
    let tree = TreeId::new_v7();
    add_assumption(&mut p, a, "A", true);
    add_claim(&mut p, claim, "C");
    let (_, steps) = add_tree_with_steps(&mut p, tree, 1);

    assert_eq!(
        p.apply(Op::SetAssumptionExcludes {
            id: a,
            trees: vec![tree],
            steps: vec![StepRef {
                tree,
                node: steps[0]
            }],
            provenance: Provenance::human("   ", 1),
        }),
        Err(ModelError::EmptyActor)
    );
    assert_eq!(
        p.apply(Op::LinkClaimAssumption {
            claim,
            assumption: a,
            provenance: Provenance::human("\t", 2),
        }),
        Err(ModelError::EmptyActor)
    );

    // Link validly first so UnlinkClaimAssumption fails ONLY due to blank actor (L09 + L15).
    p.apply(Op::LinkClaimAssumption {
        claim,
        assumption: a,
        provenance: who("alice", 3),
    })
    .expect("link");
    assert_eq!(
        p.apply(Op::UnlinkClaimAssumption {
            claim,
            assumption: a,
            provenance: Provenance::human(" \n ", 4),
        }),
        Err(ModelError::EmptyActor)
    );
}

#[test]
fn l09_single_guard_isolation_and_l30_undo_round_trip() {
    let mut p = project();
    let a = AssumptionId::new_v7();
    let claim = ClaimId::new_v7();
    let tree_only = TreeId::new_v7();
    let tree_step = TreeId::new_v7();
    add_assumption(&mut p, a, "ECU locked", true);
    add_claim(&mut p, claim, "Locked enclosure");
    let (_, steps_only) = add_tree_with_steps(&mut p, tree_only, 1);
    let (_, steps_step) = add_tree_with_steps(&mut p, tree_step, 1);
    let step = steps_step[0];
    let baseline = p.clone();

    // 1. RemoveAssumption with ONLY a claim link -> AssumptionInUse.
    let u_link = p
        .apply(Op::LinkClaimAssumption {
            claim,
            assumption: a,
            provenance: who("alice", 2),
        })
        .expect("link");
    assert_eq!(
        p.apply(Op::RemoveAssumption { id: a }),
        Err(ModelError::AssumptionInUse {
            assumption: a,
            claims: 1
        })
    );
    // 2. RemoveCybersecurityClaim with ONLY an assumption link -> CybersecurityClaimHasAssumptions.
    assert_eq!(
        p.apply(Op::RemoveCybersecurityClaim { id: claim }),
        Err(ModelError::CybersecurityClaimHasAssumptions {
            claim,
            assumptions: 1
        })
    );
    p.apply(u_link).expect("unlink");
    assert_eq!(p, baseline);

    // 3. RemoveAssumption with ONLY excludes -> AssumptionHasExclusions.
    //    RemoveAttackTree with ONLY a tree exclude -> AttackTreeExcludedByAssumption.
    let u_excl_tree = p
        .apply(Op::SetAssumptionExcludes {
            id: a,
            trees: vec![tree_only],
            steps: vec![],
            provenance: who("alice", 3),
        })
        .expect("excl tree");
    assert_eq!(
        p.apply(Op::RemoveAssumption { id: a }),
        Err(ModelError::AssumptionHasExclusions {
            assumption: a,
            excludes: 1
        })
    );
    assert_eq!(
        p.apply(Op::RemoveAttackTree { id: tree_only }),
        Err(ModelError::AttackTreeExcludedByAssumption {
            tree: tree_only,
            assumptions: 1
        })
    );
    p.apply(u_excl_tree).expect("undo excl tree");
    assert_eq!(p, baseline);

    // 4. RemoveAttackTree with ONLY a step exclude -> AttackTreeExcludedByAssumption.
    //    RemoveTreeNode on unrated, unlinked, undescribed step whose ONLY blocker is excludes_steps -> NodeIsRated.
    let u_excl_step = p
        .apply(Op::SetAssumptionExcludes {
            id: a,
            trees: vec![],
            steps: vec![StepRef {
                tree: tree_step,
                node: step,
            }],
            provenance: who("alice", 4),
        })
        .expect("excl step");
    assert_eq!(
        p.apply(Op::RemoveAttackTree { id: tree_step }),
        Err(ModelError::AttackTreeExcludedByAssumption {
            tree: tree_step,
            assumptions: 1
        })
    );
    assert_eq!(
        p.apply(Op::RemoveTreeNode {
            tree: tree_step,
            id: step,
            provenance: who("alice", 5),
        }),
        Err(ModelError::NodeIsRated(step))
    );
    p.apply(u_excl_step).expect("undo excl step");
    assert_eq!(p, baseline);

    // L30 full-Project undo equality across linking, excluding, clearing, removing all entities, and undoing step-by-step.
    let s_only = steps_only[0];
    let mut undos = Vec::new();
    undos.push(
        p.apply(Op::SetAssumptionExcludes {
            id: a,
            trees: vec![tree_only],
            steps: vec![StepRef {
                tree: tree_step,
                node: step,
            }],
            provenance: who("alice", 10),
        })
        .expect("set excl"),
    );
    undos.push(
        p.apply(Op::LinkClaimAssumption {
            claim,
            assumption: a,
            provenance: who("alice", 11),
        })
        .expect("link"),
    );
    let armed = p.clone();
    undos.push(
        p.apply(Op::UnlinkClaimAssumption {
            claim,
            assumption: a,
            provenance: who("bob", 12),
        })
        .expect("unlink"),
    );
    undos.push(
        p.apply(Op::SetAssumptionExcludes {
            id: a,
            trees: vec![],
            steps: vec![],
            provenance: who("bob", 13),
        })
        .expect("clear excl"),
    );
    undos.push(
        p.apply(Op::RemoveAssumption { id: a })
            .expect("remove assumption"),
    );
    undos.push(
        p.apply(Op::RemoveCybersecurityClaim { id: claim })
            .expect("remove claim"),
    );
    undos.push(
        p.apply(Op::RemoveTreeNode {
            tree: tree_step,
            id: step,
            provenance: who("bob", 14),
        })
        .expect("remove step"),
    );
    undos.push(
        p.apply(Op::RemoveTreeNode {
            tree: tree_only,
            id: s_only,
            provenance: who("bob", 15),
        })
        .expect("remove s_only"),
    );
    undos.push(
        p.apply(Op::RemoveAttackTree { id: tree_step })
            .expect("remove tree_step"),
    );
    undos.push(
        p.apply(Op::RemoveAttackTree { id: tree_only })
            .expect("remove tree_only"),
    );

    // Undo the removals + clears back to `armed`, then undo the initial link + excludes back to `baseline`.
    for idx in (2..undos.len()).rev() {
        p.apply(undos[idx].clone()).expect("undo back to armed");
    }
    assert_eq!(p, armed);
    p.apply(undos[1].clone()).expect("undo link");
    p.apply(undos[0].clone()).expect("undo excl");
    assert_eq!(p, baseline);
}

#[test]
fn op_log_round_trips_excludes_and_claim_assumption_ops() {
    let m = Arc::new(Methodology::iso21434_r155().expect("builtin"));
    let (a, claim, t1, t2, n1) = (
        AssumptionId::new_v7(),
        ClaimId::new_v7(),
        TreeId::new_v7(),
        TreeId::new_v7(),
        NodeId::new_v7(),
    );
    let prov = who("alice", 99);
    let ops = vec![
        Op::SetAssumptionExcludes {
            id: a,
            trees: vec![t1, t2],
            steps: vec![StepRef { tree: t1, node: n1 }],
            provenance: prov.clone(),
        },
        Op::LinkClaimAssumption {
            claim,
            assumption: a,
            provenance: prov.clone(),
        },
        Op::UnlinkClaimAssumption {
            claim,
            assumption: a,
            provenance: prov,
        },
    ];
    let codec = OpCodec::new(Arc::clone(&m));
    for op in &ops {
        assert_eq!(
            codec
                .decode(&codec.encode(op).expect("encode"))
                .expect("decode"),
            *op
        );
    }
    let mut log = Vec::new();
    let mut w = OpLogWriter::create(&mut log, Arc::clone(&m)).expect("writer");
    for op in &ops {
        w.append(op).expect("append");
    }
    drop(w);
    let decoded: Vec<_> = OpLogReader::open(log.as_slice(), m)
        .expect("open")
        .map(|r| r.expect("record").op)
        .collect();
    assert_eq!(decoded, ops);
}

fn feas(m: &Methodology, key: &str) -> FeasibilityLevel {
    m.feasibility_level(key).expect("feasibility level")
}

fn risk(m: &Methodology, key: &str) -> RiskLevel {
    m.risk_level(key).expect("risk level")
}

fn saved_bytes(p: &Project) -> Vec<u8> {
    let mut out = Vec::new();
    p.save(&mut out).expect("save");
    out
}

fn setup_asset_and_severe_damage(p: &mut Project) -> (AssetId, DamageId) {
    let m = Arc::clone(p.methodology());
    let asset = AssetId::new_v7();
    let damage = DamageId::new_v7();
    let safety = m.impact_category("safety").expect("safety");
    let severe = m.level(safety, "severe").expect("severe");
    p.apply(Op::AddAsset {
        id: asset,
        name: "Brake ECU".into(),
        properties: m.all_properties(),
        provenance: who("alice", 1),
    })
    .expect("asset");
    p.apply(Op::AddDamageScenario {
        id: damage,
        name: "Unintended braking".into(),
        impacts: vec![(safety, severe)],
        provenance: who("alice", 1),
    })
    .expect("damage");
    (asset, damage)
}

fn add_threat(p: &mut Project, asset: AssetId, damage: DamageId, tree: TreeId) -> ThreatId {
    let category = p
        .methodology()
        .threat_categories()
        .next()
        .expect("threat category");
    let threat = ThreatId::new_v7();
    p.apply(Op::AddThreatScenario {
        id: threat,
        name: "Inject brake command".into(),
        asset,
        category,
        catalog_refs: vec![],
        damages: vec![damage],
        provenance: who("alice", 2),
    })
    .expect("threat");
    p.apply(Op::SetThreatAttackTree {
        threat,
        tree: Some(tree),
        provenance: who("alice", 2),
    })
    .expect("link tree");
    threat
}

fn add_rated_tree(
    p: &mut Project,
    kind: GateKind,
    step_ratings: &[(&str, &str)],
) -> (TreeId, NodeId, Vec<NodeId>) {
    let m = Arc::clone(p.methodology());
    let tid = TreeId::new_v7();
    let root = NodeId::new_v7();
    let mut tree = AttackTree::new(root, Node::gate(kind, "Root")).expect("root");
    let mut step_ids = Vec::with_capacity(step_ratings.len());
    let mut rated = Vec::with_capacity(step_ratings.len());
    for &(label, level) in step_ratings {
        let sid = NodeId::new_v7();
        tree.add_child(tree.root(), sid, Node::step(label))
            .expect("step");
        step_ids.push(sid);
        rated.push((sid, feas(&m, level)));
    }
    p.apply(Op::AddAttackTree {
        role: Default::default(),
        id: tid,
        tree,
        step_feasibility: rated,
        provenance: who("alice", 2),
    })
    .expect("add tree");
    (tid, root, step_ids)
}

#[test]
fn holds_toggles_exclude_steps_and_trees_and_lower_or_eliminate_risk() {
    let mut p = project();
    let m = Arc::clone(p.methodology());
    let (asset, damage) = setup_asset_and_severe_damage(&mut p);

    // Part (a): OR(cheap=high, hard=low) over safety=severe.
    // Unexcluded: high feasibility + severe safety => r5.
    // Cheap excluded: low feasibility + severe safety => r3.
    let (t_or, _root_or, steps_or) = add_rated_tree(
        &mut p,
        GateKind::Or,
        &[("Easy port", "high"), ("Hard chip", "low")],
    );
    let (s_cheap, s_hard) = (steps_or[0], steps_or[1]);
    let th_or = add_threat(&mut p, asset, damage, t_or);

    let a_step = AssumptionId::new_v7();
    add_assumption(&mut p, a_step, "Debug port fused", false);
    p.apply(Op::SetAssumptionExcludes {
        id: a_step,
        trees: vec![],
        steps: vec![StepRef {
            tree: t_or,
            node: s_cheap,
        }],
        provenance: who("alice", 3),
    })
    .expect("excludes");

    let mut cache = ProjectEvalCache::new(&p);
    let check_or =
        |p: &Project, cache: &ProjectEvalCache, want_f: &str, want_r: &str, excluded: bool| {
            let app = p.applicability();
            assert_eq!(app.is_node_excluded(t_or, s_cheap), excluded);
            assert!(!app.is_node_excluded(t_or, s_hard));
            assert!(!app.is_excluded(ElementRef::Tree(t_or)));
            if excluded {
                assert_eq!(
                    app.node_reasons(t_or, s_cheap),
                    &[Reason::AssumptionHolds(a_step)]
                );
            } else {
                assert!(app.is_empty());
            }
            let wf = feas(&m, want_f);
            let wr = risk(&m, want_r);
            let eval = p.evaluate_tree(t_or).expect("tree eval");
            assert_eq!(eval.root_value(), &wf);
            assert_eq!(
                eval.get(
                    p.attack_tree(p.attack_tree_idx(&t_or).expect("t"))
                        .expect("e")
                        .idx_of(&s_cheap)
                        .expect("s")
                )
                .is_some(),
                !excluded
            );
            let r = p.evaluate_risk(th_or).expect("risk");
            assert_eq!(r.feasibility(), wf);
            assert_eq!(r.max_risk(), Some(wr));
            assert_eq!(cache, &ProjectEvalCache::new(p));
            assert_eq!(cache.evaluate_risk(p, th_or).expect("cached"), &r);
        };

    check_or(&p, &cache, "high", "r5", false);

    p.apply_cached(
        Op::SetAssumptionHolds {
            id: a_step,
            holds: true,
            provenance: who("alice", 4),
        },
        &mut cache,
    )
    .expect("enable");
    check_or(&p, &cache, "low", "r3", true);

    p.apply_cached(
        Op::SetAssumptionHolds {
            id: a_step,
            holds: false,
            provenance: who("alice", 5),
        },
        &mut cache,
    )
    .expect("disable");
    check_or(&p, &cache, "high", "r5", false);

    p.apply_cached(
        Op::SetAssumptionHolds {
            id: a_step,
            holds: true,
            provenance: who("alice", 6),
        },
        &mut cache,
    )
    .expect("re-enable");
    check_or(&p, &cache, "low", "r3", true);

    // Part (b1): AND gate where excluding one step collapses the sole path -> AllPathsExcluded.
    let (t_and, root_and, steps_and) = add_rated_tree(
        &mut p,
        GateKind::And,
        &[("Physical tap", "high"), ("Decode key", "medium")],
    );
    let th_and = add_threat(&mut p, asset, damage, t_and);
    let a_and = AssumptionId::new_v7();
    add_assumption(&mut p, a_and, "Locked harness", true);
    p.apply_cached(
        Op::SetAssumptionExcludes {
            id: a_and,
            trees: vec![],
            steps: vec![StepRef {
                tree: t_and,
                node: steps_and[0],
            }],
            provenance: who("alice", 7),
        },
        &mut cache,
    )
    .expect("and exclude");

    let app = p.applicability();
    assert_eq!(
        app.node_reasons(t_and, steps_and[0]),
        &[Reason::AssumptionHolds(a_and)]
    );
    assert_eq!(
        app.node_reasons(t_and, root_and),
        &[Reason::AllPathsExcluded]
    );
    assert_eq!(
        app.reasons(ElementRef::Tree(t_and)),
        &[Reason::AllPathsExcluded]
    );
    assert!(!app.is_excluded(ElementRef::Threat(th_and)));
    assert_eq!(
        p.evaluate_tree(t_and),
        Err(ModelError::TreeNotApplicable(t_and))
    );
    assert_eq!(
        p.evaluate_threat(th_and),
        Err(ModelError::TreeNotApplicable(t_and))
    );
    assert_eq!(
        p.evaluate_risk(th_and),
        Err(ModelError::ThreatNotApplicable(th_and))
    );
    assert_eq!(
        p.evaluate_residual_risk(th_and),
        Err(ModelError::ThreatNotApplicable(th_and))
    );
    assert_eq!(cache, ProjectEvalCache::new(&p));
    assert_eq!(
        cache.evaluate_risk(&p, th_and).cloned(),
        Err(ModelError::ThreatNotApplicable(th_and))
    );

    // Part (b2): Whole-tree exclusion via excludes_trees (plus an overlapping step exclusion).
    let (t_whole, _root_whole, steps_whole) =
        add_rated_tree(&mut p, GateKind::Or, &[("Bypass", "medium")]);
    let th_whole = add_threat(&mut p, asset, damage, t_whole);
    let a_tree = AssumptionId::new_v7();
    add_assumption(&mut p, a_tree, "Module absent", true);
    p.apply_cached(
        Op::SetAssumptionExcludes {
            id: a_tree,
            trees: vec![t_whole],
            steps: vec![StepRef {
                tree: t_whole,
                node: steps_whole[0],
            }],
            provenance: who("alice", 8),
        },
        &mut cache,
    )
    .expect("whole tree exclude");
    let app = p.applicability();
    assert_eq!(
        app.node_reasons(t_whole, steps_whole[0]),
        &[Reason::AssumptionHolds(a_tree)]
    );
    assert_eq!(
        app.reasons(ElementRef::Tree(t_whole)),
        &[Reason::AssumptionHolds(a_tree), Reason::AllPathsExcluded]
    );
    assert!(!app.is_excluded(ElementRef::Threat(th_whole)));
    assert_eq!(
        p.evaluate_tree(t_whole),
        Err(ModelError::TreeNotApplicable(t_whole))
    );
    assert_eq!(
        p.evaluate_risk(th_whole),
        Err(ModelError::ThreatNotApplicable(th_whole))
    );
    assert_eq!(cache, ProjectEvalCache::new(&p));

    p.apply_cached(
        Op::SetAssumptionHolds {
            id: a_tree,
            holds: false,
            provenance: who("alice", 9),
        },
        &mut cache,
    )
    .expect("turn off whole tree assumption");
    assert_eq!(
        p.evaluate_tree(t_whole).expect("ok").root_value(),
        &feas(&m, "medium")
    );
    assert_eq!(
        p.evaluate_risk(th_whole).expect("ok").max_risk(),
        Some(risk(&m, "r4"))
    );
    assert_eq!(cache, ProjectEvalCache::new(&p));
}

#[test]
fn off_condition_on_assumption_switches_it_off_and_back_on() {
    let mut p = project();
    let m = Arc::clone(p.methodology());
    let (asset, damage) = setup_asset_and_severe_damage(&mut p);

    let (t_step, _, steps_a) =
        add_rated_tree(&mut p, GateKind::Or, &[("Easy", "high"), ("Hard", "low")]);
    let th_step = add_threat(&mut p, asset, damage, t_step);
    let (t_tree, _, _) = add_rated_tree(&mut p, GateKind::Or, &[("Direct", "medium")]);
    let th_tree = add_threat(&mut p, asset, damage, t_tree);

    let a = AssumptionId::new_v7();
    add_assumption(&mut p, a, "Active only in normal mode", true);
    p.apply(Op::SetAssumptionExcludes {
        id: a,
        trees: vec![t_tree],
        steps: vec![StepRef {
            tree: t_step,
            node: steps_a[0],
        }],
        provenance: who("alice", 3),
    })
    .expect("excludes");

    let cond = ConditionId::new_v7();
    p.apply(Op::AddCondition {
        id: cond,
        name: "Normal mode".into(),
        description: "".into(),
        present: false,
        provenance: who("alice", 4),
    })
    .expect("cond");
    p.apply(Op::NeedCondition {
        element: ElementRef::Assumption(a),
        condition: cond,
        provenance: who("alice", 5),
    })
    .expect("link cond to assumption");

    let mut cache = ProjectEvalCache::new(&p);

    // Condition is off -> assumption is switched off despite holds == true.
    let app = p.applicability();
    assert_eq!(
        app.reasons(ElementRef::Assumption(a)),
        &[Reason::ConditionOff(cond)]
    );
    assert!(!app.is_excluded(ElementRef::Tree(t_tree)));
    assert!(app.reasons(ElementRef::Tree(t_tree)).is_empty());
    assert!(!app.is_node_excluded(t_step, steps_a[0]));
    assert!(app.node_reasons(t_step, steps_a[0]).is_empty());
    assert_eq!(
        p.evaluate_risk(th_step).expect("r5").max_risk(),
        Some(risk(&m, "r5"))
    );
    assert_eq!(
        p.evaluate_risk(th_tree).expect("r4").max_risk(),
        Some(risk(&m, "r4"))
    );
    assert_eq!(cache, ProjectEvalCache::new(&p));

    // Turn condition on -> assumption re-activates Reason::AssumptionHolds.
    p.apply_cached(
        Op::SetConditionPresent {
            id: cond,
            present: true,
            provenance: who("alice", 6),
        },
        &mut cache,
    )
    .expect("cond on");
    let app = p.applicability();
    assert!(!app.is_excluded(ElementRef::Assumption(a)));
    assert_eq!(
        app.reasons(ElementRef::Tree(t_tree)),
        &[Reason::AssumptionHolds(a)]
    );
    assert_eq!(
        app.node_reasons(t_step, steps_a[0]),
        &[Reason::AssumptionHolds(a)]
    );
    assert_eq!(
        p.evaluate_risk(th_step).expect("r3").max_risk(),
        Some(risk(&m, "r3"))
    );
    assert_eq!(
        p.evaluate_risk(th_tree),
        Err(ModelError::ThreatNotApplicable(th_tree))
    );
    assert_eq!(cache, ProjectEvalCache::new(&p));
}

#[derive(Clone)]
struct CacheFixture {
    p: Project,
    trees: Vec<TreeId>,
    threats: Vec<ThreatId>,
    steps: Vec<(TreeId, NodeId)>,
    conditions: Vec<ConditionId>,
    assumptions: Vec<AssumptionId>,
    asset2: AssetId,
    ctrl: aegis_model::ControlId,
}

fn cache_fixture() -> CacheFixture {
    let mut p = project();
    let m = Arc::clone(p.methodology());
    let (k1, k2) = (ComponentId::new_v7(), ComponentId::new_v7());
    p.apply(Op::AddComponent {
        id: k1,
        name: "ECU".into(),
        parent: None,
        provenance: who("alice", 1),
    })
    .expect("k1");
    p.apply(Op::AddComponent {
        id: k2,
        name: "Sub".into(),
        parent: Some(k1),
        provenance: who("alice", 1),
    })
    .expect("k2");
    let iface = InterfaceId::new_v7();
    let ikind = m.interface_kinds().next().expect("ikind");
    p.apply(Op::AddInterface {
        id: iface,
        name: "CAN".into(),
        component: k2,
        kind: ikind,
        provenance: who("alice", 1),
    })
    .expect("iface");
    let (asset1, damage) = setup_asset_and_severe_damage(&mut p);
    p.apply(Op::SetAssetComponent {
        asset: asset1,
        component: Some(k2),
        provenance: who("alice", 1),
    })
    .expect("bind");
    let asset2 = AssetId::new_v7();
    p.apply(Op::AddAsset {
        id: asset2,
        name: "Key".into(),
        properties: m.all_properties(),
        provenance: who("alice", 1),
    })
    .expect("asset2");

    // t1: OR(AND(s11, s12), s13); t2: AND(OR(s21, s22), s23); t3: SeqAnd(s31, OR(s32, s33)).
    let (s11, s12, s13) = (NodeId::new_v7(), NodeId::new_v7(), NodeId::new_v7());
    let mut t1 = AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::Or, "r1")).expect("t1");
    let g1 = t1
        .add_child(t1.root(), NodeId::new_v7(), Node::gate(GateKind::And, "g1"))
        .expect("g1");
    t1.add_child(g1, s11, Node::step("s11")).expect("s11");
    t1.add_child(g1, s12, Node::step("s12")).expect("s12");
    t1.add_child(t1.root(), s13, Node::step("s13"))
        .expect("s13");

    let (s21, s22, s23) = (NodeId::new_v7(), NodeId::new_v7(), NodeId::new_v7());
    let mut t2 = AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::And, "r2")).expect("t2");
    let g2 = t2
        .add_child(t2.root(), NodeId::new_v7(), Node::gate(GateKind::Or, "g2"))
        .expect("g2");
    t2.add_child(g2, s21, Node::step("s21")).expect("s21");
    t2.add_child(g2, s22, Node::step("s22")).expect("s22");
    t2.add_child(t2.root(), s23, Node::step("s23"))
        .expect("s23");

    let (s31, s32, s33) = (NodeId::new_v7(), NodeId::new_v7(), NodeId::new_v7());
    let mut t3 = AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::SeqAnd, "r3")).expect("t3");
    t3.add_child(t3.root(), s31, Node::step("s31"))
        .expect("s31");
    let g3 = t3
        .add_child(t3.root(), NodeId::new_v7(), Node::gate(GateKind::Or, "g3"))
        .expect("g3");
    t3.add_child(g3, s32, Node::step("s32")).expect("s32");
    t3.add_child(g3, s33, Node::step("s33")).expect("s33");

    let (tid1, tid2, tid3) = (TreeId::new_v7(), TreeId::new_v7(), TreeId::new_v7());
    for (tid, tree, rated) in [
        (
            tid1,
            t1,
            vec![
                (s11, feas(&m, "high")),
                (s12, feas(&m, "medium")),
                (s13, feas(&m, "high")),
            ],
        ),
        (
            tid2,
            t2,
            vec![
                (s21, feas(&m, "medium")),
                (s22, feas(&m, "low")),
                (s23, feas(&m, "high")),
            ],
        ),
        (
            tid3,
            t3,
            vec![
                (s31, feas(&m, "high")),
                (s32, feas(&m, "low")),
                (s33, feas(&m, "very-low")),
            ],
        ),
    ] {
        p.apply(Op::AddAttackTree {
            role: Default::default(),
            id: tid,
            tree,
            step_feasibility: rated,
            provenance: who("alice", 2),
        })
        .expect("tree");
    }
    p.apply(Op::SetStepVia {
        target: StepTarget::Step(tid1, s11),
        via: Some(ItemRef::Component(k1)),
        provenance: who("alice", 2),
    })
    .expect("via1");
    p.apply(Op::SetStepVia {
        target: StepTarget::Step(tid2, s22),
        via: Some(ItemRef::Interface(iface)),
        provenance: who("alice", 2),
    })
    .expect("via2");
    p.apply(Op::SetStepVia {
        target: StepTarget::Step(tid3, s33),
        via: Some(ItemRef::Component(k2)),
        provenance: who("alice", 2),
    })
    .expect("via3");

    let ctrl = aegis_model::ControlId::new_v7();
    p.apply(Op::AddSecurityControl {
        id: ctrl,
        name: "HSM".into(),
        provenance: who("alice", 2),
    })
    .expect("ctrl");
    p.apply(Op::SetStepMitigation {
        tree: tid1,
        node: s13,
        control: ctrl,
        feasibility: Some(feas(&m, "very-low")),
        provenance: who("alice", 2),
    })
    .expect("mit");

    let threats = vec![
        add_threat(&mut p, asset1, damage, tid1),
        add_threat(&mut p, asset1, damage, tid2),
        add_threat(&mut p, asset2, damage, tid3),
        add_threat(&mut p, asset2, damage, tid1),
    ];

    let mut conditions = Vec::new();
    for i in 0..3 {
        let cid = ConditionId::new_v7();
        p.apply(Op::AddCondition {
            id: cid,
            name: format!("C{i}").into(),
            description: "".into(),
            present: i != 0,
            provenance: who("alice", 3),
        })
        .expect("cond");
        conditions.push(cid);
    }

    let mut assumptions = Vec::new();
    for i in 0..3 {
        let aid = AssumptionId::new_v7();
        add_assumption(&mut p, aid, &format!("A{i}"), i != 1);
        assumptions.push(aid);
    }
    p.apply(Op::NeedCondition {
        element: ElementRef::Assumption(assumptions[0]),
        condition: conditions[0],
        provenance: who("alice", 4),
    })
    .expect("need a0");
    p.apply(Op::NeedCondition {
        element: ElementRef::Component(k1),
        condition: conditions[1],
        provenance: who("alice", 4),
    })
    .expect("need k1");
    p.apply(Op::NeedStepCondition {
        tree: tid2,
        node: s23,
        condition: conditions[2],
        provenance: who("alice", 4),
    })
    .expect("need s23");
    p.apply(Op::SetAssumptionExcludes {
        id: assumptions[0],
        trees: vec![],
        steps: vec![StepRef {
            tree: tid1,
            node: s13,
        }],
        provenance: who("alice", 4),
    })
    .expect("excl a0");

    CacheFixture {
        p,
        trees: vec![tid1, tid2, tid3],
        threats,
        steps: vec![
            (tid1, s11),
            (tid1, s12),
            (tid1, s13),
            (tid2, s21),
            (tid2, s22),
            (tid2, s23),
            (tid3, s31),
            (tid3, s32),
            (tid3, s33),
        ],
        conditions,
        assumptions,
        asset2,
        ctrl,
    }
}

#[derive(Clone, Copy, Debug)]
enum CacheEdit {
    Holds(usize, bool),
    CondPresent(usize, bool),
    Excludes(usize, usize),
    ToggleAssumptionCond(usize, usize),
    ToggleAssetCond(usize),
    RateStep(usize, usize),
    MitigateStep(usize, bool),
    ThreatTree(usize, Option<usize>),
}

fn cache_edit_strategy() -> impl Strategy<Value = Vec<CacheEdit>> {
    let one = prop_oneof![
        (0..3usize, any::<bool>()).prop_map(|(a, h)| CacheEdit::Holds(a, h)),
        (0..3usize, any::<bool>()).prop_map(|(c, v)| CacheEdit::CondPresent(c, v)),
        (0..3usize, 0..5usize).prop_map(|(a, p)| CacheEdit::Excludes(a, p)),
        (0..3usize, 0..3usize).prop_map(|(a, c)| CacheEdit::ToggleAssumptionCond(a, c)),
        (0..3usize).prop_map(CacheEdit::ToggleAssetCond),
        (0..9usize, 0..4usize).prop_map(|(s, l)| CacheEdit::RateStep(s, l)),
        (0..9usize, any::<bool>()).prop_map(|(s, on)| CacheEdit::MitigateStep(s, on)),
        (0..4usize, prop::option::of(0..3usize)).prop_map(|(t, tr)| CacheEdit::ThreatTree(t, tr)),
    ];
    prop::collection::vec(one, 1..20)
}

fn build_cache_op(f: &CacheFixture, edit: CacheEdit) -> Op {
    let levels = ["very-low", "low", "medium", "high"];
    match edit {
        CacheEdit::Holds(a, holds) => Op::SetAssumptionHolds {
            id: f.assumptions[a],
            holds,
            provenance: who("bob", 10),
        },
        CacheEdit::CondPresent(c, present) => Op::SetConditionPresent {
            id: f.conditions[c],
            present,
            provenance: who("bob", 11),
        },
        CacheEdit::Excludes(a, preset) => {
            let sr = |i: usize| StepRef {
                tree: f.steps[i].0,
                node: f.steps[i].1,
            };
            let (trees, steps) = match preset {
                0 => (vec![], vec![]),
                1 => (vec![f.trees[1]], vec![]),
                2 => (vec![], vec![sr(2), sr(4)]),
                3 => (vec![f.trees[2]], vec![sr(0), sr(7)]),
                _ => (vec![f.trees[0]], vec![sr(3)]),
            };
            Op::SetAssumptionExcludes {
                id: f.assumptions[a],
                trees,
                steps,
                provenance: who("bob", 12),
            }
        }
        CacheEdit::ToggleAssumptionCond(a, c) => {
            let element = ElementRef::Assumption(f.assumptions[a]);
            let condition = f.conditions[c];
            let need = Op::NeedCondition {
                element,
                condition,
                provenance: who("bob", 13),
            };
            if f.p.validate(&need).is_ok() {
                need
            } else {
                Op::UnneedCondition {
                    element,
                    condition,
                    provenance: who("bob", 13),
                }
            }
        }
        CacheEdit::ToggleAssetCond(c) => {
            let element = ElementRef::Asset(f.asset2);
            let condition = f.conditions[c];
            let need = Op::NeedCondition {
                element,
                condition,
                provenance: who("bob", 14),
            };
            if f.p.validate(&need).is_ok() {
                need
            } else {
                Op::UnneedCondition {
                    element,
                    condition,
                    provenance: who("bob", 14),
                }
            }
        }
        CacheEdit::RateStep(s, l) => Op::SetStepFeasibility {
            target: StepTarget::Step(f.steps[s].0, f.steps[s].1),
            level: Some(feas(f.p.methodology(), levels[l])),
            provenance: who("bob", 15),
        },
        CacheEdit::MitigateStep(s, on) => Op::SetStepMitigation {
            tree: f.steps[s].0,
            node: f.steps[s].1,
            control: f.ctrl,
            feasibility: on.then(|| feas(f.p.methodology(), "very-low")),
            provenance: who("bob", 16),
        },
        CacheEdit::ThreatTree(t, tr) => Op::SetThreatAttackTree {
            threat: f.threats[t],
            tree: tr.map(|i| f.trees[i]),
            provenance: who("bob", 17),
        },
    }
}

fn assert_cache_matches_project(
    f: &CacheFixture,
    cache: &ProjectEvalCache,
) -> Result<(), TestCaseError> {
    prop_assert_eq!(cache, &ProjectEvalCache::new(&f.p));
    prop_assert_eq!(cache.applicability(), &f.p.applicability());
    for &t in &f.trees {
        prop_assert_eq!(cache.evaluate_tree(&f.p, t).cloned(), f.p.evaluate_tree(t));
        prop_assert_eq!(
            cache.evaluate_residual_tree(&f.p, t).cloned(),
            f.p.evaluate_residual_tree(t)
        );
    }
    for &th in &f.threats {
        prop_assert_eq!(
            cache.evaluate_risk(&f.p, th).cloned(),
            f.p.evaluate_risk(th)
        );
        prop_assert_eq!(
            cache.evaluate_residual_risk(&f.p, th).cloned(),
            f.p.evaluate_residual_risk(th)
        );
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn shuffled_assumption_edits_match_fresh_eval(edits in cache_edit_strategy()) {
        let mut f = cache_fixture();
        let start_p = f.p.clone();
        let mut cache = ProjectEvalCache::new(&f.p);
        let start_cache = cache.clone();
        let mut inverses = Vec::with_capacity(edits.len());
        for edit in edits {
            let op = build_cache_op(&f, edit);
            let inv = f
                .p
                .apply_cached(op, &mut cache)
                .map_err(|e| TestCaseError::fail(e.to_string()))?;
            inverses.push(inv);
            assert_cache_matches_project(&f, &cache)?;
        }
        for inv in inverses.into_iter().rev() {
            f.p.apply_cached(inv, &mut cache)
                .map_err(|e| TestCaseError::fail(e.to_string()))?;
            assert_cache_matches_project(&f, &cache)?;
        }
        prop_assert_eq!(&f.p, &start_p);
        prop_assert_eq!(&cache, &start_cache);
    }

    #[test]
    fn assumption_and_condition_switch_order_does_not_matter(
        (seq_a, seq_b) in (
            prop::collection::vec(any::<bool>(), 3),
            prop::collection::vec(any::<bool>(), 3),
            prop::collection::vec(0..5usize, 3),
        )
            .prop_flat_map(|(conds, holds, excludes)| {
                let ops: Vec<CacheEdit> = conds
                    .into_iter()
                    .enumerate()
                    .map(|(i, v)| CacheEdit::CondPresent(i, v))
                    .chain(holds.into_iter().enumerate().map(|(i, h)| CacheEdit::Holds(i, h)))
                    .chain(excludes.into_iter().enumerate().map(|(i, p)| CacheEdit::Excludes(i, p)))
                    .collect();
                (Just(ops.clone()).prop_shuffle(), Just(ops).prop_shuffle())
            })
    ) {
        let base = cache_fixture();
        let mut results = Vec::new();
        for seq in [seq_a, seq_b] {
            let mut f = base.clone();
            let mut cache = ProjectEvalCache::new(&f.p);
            for edit in seq {
                let op = build_cache_op(&f, edit);
                f.p.apply_cached(op, &mut cache)
                    .map_err(|e| TestCaseError::fail(e.to_string()))?;
            }
            assert_cache_matches_project(&f, &cache)?;
            results.push((cache, f.p.applicability()));
        }
        prop_assert_eq!(&results[0], &results[1]);
    }
}

#[test]
#[allow(clippy::too_many_lines)]
fn snapshot_and_workspace_round_trip_and_decode_limits() {
    let mut p = project();
    let m = Arc::clone(p.methodology());
    let (t1, _, steps1) = add_rated_tree(&mut p, GateKind::Or, &[("S1", "high"), ("S2", "low")]);
    let (t2, _, steps2) = add_rated_tree(&mut p, GateKind::Or, &[("S3", "medium")]);
    let claim = ClaimId::new_v7();
    add_claim(&mut p, claim, "Enclosure claim");
    let cond = ConditionId::new_v7();
    p.apply(Op::AddCondition {
        id: cond,
        name: "Production".into(),
        description: "".into(),
        present: true,
        provenance: who("alice", 1),
    })
    .expect("cond");
    p.apply(Op::SetProjectLanguages {
        langs: Arc::from([Arc::from("ja")]),
        provenance: who("alice", 1),
    })
    .expect("langs");

    // 102 assumptions so AS101 tests visual numbers past 100 and removing AS00 tests gaps.
    let mut assumptions = Vec::with_capacity(102);
    for i in 0..102 {
        let id = AssumptionId::new_v7();
        p.apply(Op::AddAssumption {
            id,
            name: format!("Assumption {i}").into(),
            holds: i % 2 == 0,
            provenance: who("alice", 10 + i as u64),
        })
        .expect("add");
        assumptions.push(id);
    }
    p.apply(Op::RemoveAssumption { id: assumptions[0] })
        .expect("gap");

    // Unsorted trees/steps and descending claim links must sort canonically.
    let a_first = assumptions[1];
    let a_last = assumptions[101];
    p.apply(Op::SetAssumptionExcludes {
        id: a_first,
        trees: vec![t2, t1],
        steps: vec![
            StepRef {
                tree: t2,
                node: steps2[0],
            },
            StepRef {
                tree: t1,
                node: steps1[1],
            },
            StepRef {
                tree: t1,
                node: steps1[0],
            },
        ],
        provenance: who("bob", 200),
    })
    .expect("excludes");
    for aid in [a_last, a_first] {
        p.apply(Op::LinkClaimAssumption {
            claim,
            assumption: aid,
            provenance: who("bob", 201),
        })
        .expect("link claim");
    }
    p.apply(Op::NeedCondition {
        element: ElementRef::Assumption(a_last),
        condition: cond,
        provenance: who("bob", 202),
    })
    .expect("need cond");
    p.apply(Op::SetDescription {
        target: ElementRef::Assumption(a_last),
        text: "Requires seal".into(),
        provenance: who("bob", 203),
    })
    .expect("desc");
    p.apply(Op::SetDescriptionTranslation {
        target: ElementRef::Assumption(a_last),
        lang: "ja".into(),
        text: "封印が必要".into(),
        provenance: who("bob", 204),
    })
    .expect("trans");

    // Snapshot round-trip byte-for-byte.
    let bytes = saved_bytes(&p);
    let snap_text = std::str::from_utf8(&bytes).expect("utf8");
    assert!(snap_text.contains("\"assumptions\""));
    assert!(snap_text.contains("\"holds\":true"));
    assert!(snap_text.contains("\"holds\":false"));
    let loaded = Project::load(bytes.as_slice(), Arc::clone(&m)).expect("load");
    assert_eq!(loaded, p);
    assert_eq!(saved_bytes(&loaded), bytes);
    assert_eq!(
        loaded
            .assumption_visual_id(loaded.assumption_idx(&a_last).expect("idx"))
            .as_deref(),
        Some("AS101")
    );

    // Workspace round-trip in forward and reversed file order.
    let mut files = p.to_workspace_files().expect("ws files");
    let ws_assumptions = files
        .iter()
        .find(|f| f.path == "systems/main/assumptions.json")
        .expect("systems/main/assumptions.json");
    let ws_assumptions_json: Value =
        serde_json::from_slice(&ws_assumptions.bytes).expect("assumptions json");
    assert_eq!(ws_assumptions_json["kind"], "assumptions");
    let arr = ws_assumptions_json["assumptions"]
        .as_array()
        .expect("array");
    assert_eq!(arr.len(), 101);
    assert!(arr.iter().any(|entry| entry["holds"] == json!(true)));
    assert!(arr.iter().any(|entry| entry["holds"] == json!(false)));
    let ws_claims = files
        .iter()
        .find(|f| f.path == "shared/claims.json")
        .expect("claims");
    let ws_claims_json: Value = serde_json::from_slice(&ws_claims.bytes).expect("claims json");
    assert_eq!(
        ws_claims_json["cybersecurity_claims"][0]["assumptions"]
            .as_array()
            .map(Vec::len),
        Some(2)
    );
    files.reverse();
    let ws_back = Project::from_workspace_files(
        files.iter().map(|f| (f.path.as_str(), f.bytes.as_slice())),
        Arc::clone(&m),
    )
    .expect("ws load");
    assert_eq!(ws_back, p);
    assert_eq!(
        ws_back.to_workspace_files().expect("ws"),
        p.to_workspace_files().expect("ws2")
    );

    // #[serde(default)] omitting "holds" decodes as false, then saves explicitly as false.
    let default_id = AssumptionId::new_v7();
    let omitted_holds = json!({
        "format": "aegis-project",
        "version": 1,
        "methodology": {
            "key": m.key(),
            "version": m.version(),
            "digest": m.pack_digest(),
        },
        "assets": [],
        "damage_scenarios": [],
        "threat_scenarios": [],
        "assumptions": [{
            "id": default_id.to_string(),
            "name": "Omitted holds"
        }]
    });
    let omitted_bytes = serde_json::to_vec(&omitted_holds).expect("json");
    let omitted_proj =
        Project::load(omitted_bytes.as_slice(), Arc::clone(&m)).expect("load omitted holds");
    let idx = omitted_proj.assumption_idx(&default_id).expect("idx");
    assert!(!omitted_proj.assumption(idx).expect("a").holds());
    assert!(omitted_proj.applicability().is_empty());
    assert!(
        std::str::from_utf8(&saved_bytes(&omitted_proj))
            .expect("utf8")
            .contains("\"holds\":false")
    );

    // L09 bounded decode limits (at cap succeeds, cap + 1 returns exact StoreError::LimitExceeded).
    let base_snap = || {
        json!({
            "format": "aegis-project",
            "version": 1,
            "methodology": {"key": m.key(), "version": m.version()},
            "assets": [],
            "damage_scenarios": [],
            "threat_scenarios": [],
        })
    };
    let make_assumptions = |count: usize| -> Vec<Value> {
        (0..count)
            .map(|i| {
                json!({
                    "id": AssumptionId::new_v7().to_string(),
                    "name": format!("A{i}"),
                })
            })
            .collect()
    };

    // 1. 256 assumptions passes; 257 -> Limit::Assumptions.
    let mut s = base_snap();
    s["assumptions"] = json!(make_assumptions(MAX_ASSUMPTIONS));
    Project::load(s.to_string().as_bytes(), Arc::clone(&m)).expect("256 assumptions ok");
    s["assumptions"] = json!(make_assumptions(MAX_ASSUMPTIONS + 1));
    assert!(matches!(
        Project::load(s.to_string().as_bytes(), Arc::clone(&m)),
        Err(StoreError::LimitExceeded(Limit::Assumptions {
            max: MAX_ASSUMPTIONS
        }))
    ));

    // 2. 64 claim assumptions passes; 65 -> Limit::ClaimAssumptions.
    let mut s = base_snap();
    let max_a = make_assumptions(MAX_CLAIM_ASSUMPTIONS);
    let max_a_ids: Vec<Value> = max_a.iter().map(|v| v["id"].clone()).collect();
    s["assumptions"] = json!(max_a);
    s["cybersecurity_claims"] = json!([{
        "id": ClaimId::new_v7().to_string(),
        "name": "C",
        "assumptions": max_a_ids,
    }]);
    Project::load(s.to_string().as_bytes(), Arc::clone(&m)).expect("64 claim assumptions ok");
    let over_a = make_assumptions(MAX_CLAIM_ASSUMPTIONS + 1);
    let over_a_ids: Vec<Value> = over_a.iter().map(|v| v["id"].clone()).collect();
    s["assumptions"] = json!(over_a);
    s["cybersecurity_claims"] = json!([{
        "id": ClaimId::new_v7().to_string(),
        "name": "C",
        "assumptions": over_a_ids,
    }]);
    assert!(matches!(
        Project::load(s.to_string().as_bytes(), Arc::clone(&m)),
        Err(StoreError::LimitExceeded(Limit::ClaimAssumptions {
            max: MAX_CLAIM_ASSUMPTIONS
        }))
    ));

    // 3. 130 live trees + 126 live steps (= 256) passes; 130 + 127 (= 257 > 256 combined) -> Limit::AssumptionExcludes.
    let mut tree_ids = Vec::with_capacity(130);
    let mut tree_wires = Vec::with_capacity(130);
    let mut step_wires = Vec::with_capacity(127);
    for i in 0..130 {
        let tid = TreeId::new_v7();
        let rid = NodeId::new_v7();
        let sid = NodeId::new_v7();
        tree_ids.push(json!(tid.to_string()));
        tree_wires.push(json!({
            "id": tid.to_string(),
            "nodes": [
                {"id": rid.to_string(), "label": "R", "gate": "or", "children": [sid.to_string()]},
                {"id": sid.to_string(), "label": "S"}
            ],
            "step_feasibility": {}
        }));
        if i < 127 {
            step_wires.push(json!({"tree": tid.to_string(), "node": sid.to_string()}));
        }
    }
    let mut s = base_snap();
    s["attack_trees"] = json!(tree_wires);
    s["assumptions"] = json!([{
        "id": AssumptionId::new_v7().to_string(),
        "name": "Combined",
        "holds": true,
        "excludes_trees": tree_ids,
        "excludes_steps": step_wires[..126],
    }]);
    Project::load(s.to_string().as_bytes(), Arc::clone(&m)).expect("130 + 126 = 256 ok");
    s["assumptions"] = json!([{
        "id": AssumptionId::new_v7().to_string(),
        "name": "Combined",
        "holds": true,
        "excludes_trees": tree_ids,
        "excludes_steps": step_wires,
    }]);
    assert!(matches!(
        Project::load(s.to_string().as_bytes(), Arc::clone(&m)),
        Err(StoreError::LimitExceeded(Limit::AssumptionExcludes {
            max: MAX_ASSUMPTION_EXCLUDES
        }))
    ));

    // Confirm legacy project-v1.json fixture loads cleanly with 0 assumptions and emits no assumptions workspace file.
    let v1_bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/project-v1.json"
    ))
    .expect("v1 fixture");
    let v1_proj = Project::load(v1_bytes.as_slice(), m).expect("v1 load");
    assert_eq!(v1_proj.assumptions().count(), 0);
    assert!(
        v1_proj
            .to_workspace_files()
            .expect("v1 ws")
            .iter()
            .all(|f| f.path != "systems/main/assumptions.json")
    );
}

#[test]
fn lint_assumption_without_claim_and_suppresses_unlinked_claim() {
    let mut p = project();
    let claim = ClaimId::new_v7();
    let a1 = AssumptionId::new_v7();
    let a2 = AssumptionId::new_v7();
    let prov = who("alice", 1_700_000_000_000);

    p.apply(Op::AddCybersecurityClaim {
        id: claim,
        name: "ECU physically secured".into(),
        provenance: prov.clone(),
    })
    .expect("add claim");
    add_assumption(&mut p, a1, "Locked housing", true);
    add_assumption(&mut p, a2, "Tamper-evident seal", false);

    let findings = p.lint();
    let unlinked_claim = LintFinding::new(LintCode::CybersecurityClaimUnlinked { claim });
    let unlinked_a1 = LintFinding::new(LintCode::AssumptionWithoutClaim { assumption: a1 });
    let unlinked_a2 = LintFinding::new(LintCode::AssumptionWithoutClaim { assumption: a2 });
    assert!(findings.contains(&unlinked_claim));
    assert!(findings.contains(&unlinked_a1));
    assert!(findings.contains(&unlinked_a2));
    assert_eq!(unlinked_a1.severity(), LintSeverity::Warning);
    assert!(
        unlinked_a1
            .code()
            .to_string()
            .contains("has no cybersecurity claim")
    );

    // Linking a1 clears both AssumptionWithoutClaim(a1) and CybersecurityClaimUnlinked(claim),
    // while a2 still warns until linked.
    p.apply(Op::LinkClaimAssumption {
        claim,
        assumption: a1,
        provenance: prov.clone(),
    })
    .expect("link a1");
    let after_link = p.lint();
    assert!(!after_link.contains(&unlinked_claim));
    assert!(!after_link.contains(&unlinked_a1));
    assert!(after_link.contains(&unlinked_a2));

    // Unlinking restores both warnings.
    p.apply(Op::UnlinkClaimAssumption {
        claim,
        assumption: a1,
        provenance: prov,
    })
    .expect("unlink a1");
    let after_unlink = p.lint();
    assert!(after_unlink.contains(&unlinked_claim));
    assert!(after_unlink.contains(&unlinked_a1));
}
