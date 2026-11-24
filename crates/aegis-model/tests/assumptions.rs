//! Tests of the `Assumption` entity (`APL-04`): IDs, visual prefix `AS`,
//! lifecycle ops, validation guards (L09, L15), descriptions with translations,
//! condition linking, `ProjectDiff::assumptions`, op-log round-trips, and L30
//! full-`Project` undo equality.

use std::sync::Arc;

use aegis_model::{
    AssumptionId, AssumptionIdx, AttackTree, ClaimId, ConditionId, ElementRef, EntityKind,
    GateKind, Limit, MAX_ASSUMPTION_EXCLUDES, MAX_ASSUMPTIONS, MAX_CLAIM_ASSUMPTIONS,
    MAX_DESCRIPTION_CHARS, MAX_NAME_BYTES, MAX_NAME_CHARS, Methodology, ModelError, Node, NodeId,
    Op, OpCodec, OpLogReader, OpLogWriter, Project, ProjectDiff, ProjectEvalCache, Provenance,
    StepRef, StoreError, TreeId, VisualPrefix,
};

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
