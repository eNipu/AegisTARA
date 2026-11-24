//! Tests of the `Assumption` entity (`APL-04`): IDs, visual prefix `AS`,
//! lifecycle ops, validation guards (L09, L15), descriptions with translations,
//! condition linking, `ProjectDiff::assumptions`, op-log round-trips, and L30
//! full-`Project` undo equality.

use std::sync::Arc;

use aegis_model::{
    AssumptionId, AssumptionIdx, ConditionId, ElementRef, EntityKind, MAX_ASSUMPTIONS,
    MAX_DESCRIPTION_CHARS, MAX_NAME_BYTES, MAX_NAME_CHARS, Methodology, ModelError, Op, OpCodec,
    OpLogReader, OpLogWriter, Project, ProjectDiff, ProjectEvalCache, Provenance, StoreError,
    VisualPrefix,
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
