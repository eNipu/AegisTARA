//! Tests of the typed `Condition` element: ids, arena, ops, guards, op-log
//! round-trips and diff. Conditions have no snapshot fields in this slice, so
//! inverse checks compare `save()` bytes (L06) as well as the in-memory state.

mod common;

use std::sync::Arc;

use aegis_model::{
    ConditionId, ConditionIdx, MAX_CONDITIONS, MAX_DESCRIPTION_CHARS, MAX_NAME_CHARS, Methodology,
    ModelError, Op, OpCodec, OpLogReader, OpLogWriter, Project, Provenance, VisualPrefix,
};

fn project() -> Project {
    Project::new(Arc::new(Methodology::iso21434_r155().expect("builtin")))
}

fn who() -> Provenance {
    Provenance::human("alice", 1_700_000_000_000)
}

fn saved(p: &Project) -> Vec<u8> {
    let mut out = Vec::new();
    p.save(&mut out).expect("save");
    out
}

fn add(p: &mut Project, id: ConditionId, name: &str, description: &str, present: bool) {
    p.apply(Op::AddCondition {
        id,
        name: name.into(),
        description: description.into(),
        present,
        provenance: who(),
    })
    .expect("add condition");
}

/// Applies `op`, checks its inverse, applies the inverse and checks that the
/// saved bytes and the in-memory project match (visual ids and order included).
fn inverse_round_trip(p: &mut Project, op: Op, expected_inverse: Op) {
    let before = saved(p);
    let expected_project = p.clone();
    let undo = p.apply(op).expect("apply");
    assert_eq!(undo, expected_inverse);
    p.apply(undo).expect("apply inverse");
    assert_eq!(saved(p), before);
    assert_eq!(p, &expected_project);
}

#[test]
fn ids_and_visual_ids() {
    assert_eq!(std::mem::size_of::<ConditionIdx>(), 4);
    assert_eq!(VisualPrefix::Condition.as_str(), "CF");
    assert_eq!(VisualPrefix::Condition.format(0), "CF00");
    assert_eq!(VisualPrefix::Condition.format(99), "CF99");
    assert_eq!(VisualPrefix::Condition.format(100), "CF100");
    assert!(VisualPrefix::Condition.parse_num("CF07").is_some());
    assert!(VisualPrefix::Condition.parse_num("P07").is_none());

    let mut p = project();
    let a = ConditionId::new_v7();
    let b = ConditionId::new_v7();
    add(&mut p, a, "Airbag deploys", "Frontal crash", true);
    add(&mut p, b, "Brake fails", "Brake fault", false);
    let ai = p.condition_idx(&a).expect("a idx");
    let bi = p.condition_idx(&b).expect("b idx");
    assert_eq!(p.condition_visual_id(ai).as_deref(), Some("CF00"));
    assert_eq!(p.condition_visual_id(bi).as_deref(), Some("CF01"));
    let c = p.condition(ai).expect("a");
    assert_eq!(c.name(), "Airbag deploys");
    assert_eq!(c.description(), "Frontal crash");
    assert!(c.present());
    assert_eq!(c.provenance(), &who());
    assert_eq!(p.conditions().count(), 2);
}

#[test]
fn every_condition_op_has_an_exact_inverse() {
    let mut p = project();
    let (id, old_name, old_desc) = (
        ConditionId::new_v7(),
        "Airbag deploys".to_owned(),
        "Frontal crash".to_owned(),
    );
    let original = who();
    let add_op = Op::AddCondition {
        id,
        name: old_name.clone().into(),
        description: old_desc.clone().into(),
        present: true,
        provenance: original.clone(),
    };
    inverse_round_trip(&mut p, add_op, Op::RemoveCondition { id });

    add(&mut p, id, &old_name, &old_desc, true);
    let rename = Op::RenameCondition {
        id,
        name: "Airbag deploys in a crash".into(),
        provenance: who(),
    };
    inverse_round_trip(
        &mut p,
        rename,
        Op::RenameCondition {
            id,
            name: old_name.clone().into(),
            provenance: original.clone(),
        },
    );

    let set_absent = Op::SetConditionPresent {
        id,
        present: false,
        provenance: who(),
    };
    inverse_round_trip(
        &mut p,
        set_absent,
        Op::SetConditionPresent {
            id,
            present: true,
            provenance: original.clone(),
        },
    );

    let remove = Op::RemoveCondition { id };
    inverse_round_trip(
        &mut p,
        remove,
        Op::AddCondition {
            id,
            name: old_name.into(),
            description: old_desc.into(),
            present: true,
            provenance: original,
        },
    );
}

#[test]
fn each_guard_refuses_only_its_own_input() {
    let mut p = project();
    let id = ConditionId::new_v7();
    add(&mut p, id, "Airbag deploys", "Frontal crash", true);

    // Duplicate ID: every other field is valid.
    let duplicate = p.apply(Op::AddCondition {
        id,
        name: "Other".into(),
        description: "Other".into(),
        present: false,
        provenance: who(),
    });
    assert_eq!(duplicate, Err(ModelError::DuplicateConditionId(id)));

    // Unknown ID: same op on an absent ID for all three mutating ops.
    let missing = ConditionId::new_v7();
    assert_eq!(
        p.apply(Op::RenameCondition {
            id: missing,
            name: "Other".into(),
            provenance: who(),
        }),
        Err(ModelError::UnknownCondition(missing))
    );
    assert_eq!(
        p.apply(Op::SetConditionPresent {
            id: missing,
            present: false,
            provenance: who(),
        }),
        Err(ModelError::UnknownCondition(missing))
    );
    assert_eq!(
        p.apply(Op::RemoveCondition { id: missing }),
        Err(ModelError::UnknownCondition(missing))
    );

    // Blank name: only the name guard refuses it, on add and on rename.
    let blank = p.apply(Op::AddCondition {
        id: ConditionId::new_v7(),
        name: "   ".into(),
        description: "Valid".into(),
        present: true,
        provenance: who(),
    });
    assert_eq!(blank, Err(ModelError::ConditionNameEmpty));
    assert!(!blank.unwrap_err().to_string().contains("   "));
    let blank_rename = p.apply(Op::RenameCondition {
        id,
        name: " ".into(),
        provenance: who(),
    });
    assert_eq!(blank_rename, Err(ModelError::ConditionNameEmpty));
    assert_eq!(
        p.condition(p.condition_idx(&id).expect("idx"))
            .map(|c| c.name()),
        Some("Airbag deploys")
    );

    // Over-long name: 257 chars is refused, the error carries no name.
    let long = "x".repeat(MAX_NAME_CHARS + 1);
    let too_long = p.apply(Op::AddCondition {
        id: ConditionId::new_v7(),
        name: long.as_str().into(),
        description: "Valid".into(),
        present: true,
        provenance: who(),
    });
    assert_eq!(
        too_long,
        Err(ModelError::ConditionNameTooLong {
            max: MAX_NAME_CHARS
        })
    );
    assert!(!too_long.unwrap_err().to_string().contains(&long));

    // Over-long description reuses the shared description error.
    let long_desc = "d".repeat(MAX_DESCRIPTION_CHARS + 1);
    let too_long_desc = p.apply(Op::AddCondition {
        id: ConditionId::new_v7(),
        name: "Valid".into(),
        description: long_desc.as_str().into(),
        present: true,
        provenance: who(),
    });
    assert_eq!(
        too_long_desc,
        Err(ModelError::DescriptionTooLong {
            max: MAX_DESCRIPTION_CHARS
        })
    );

    // A blank actor is refused even when every other field is valid (L15).
    assert_eq!(
        p.apply(Op::AddCondition {
            id: ConditionId::new_v7(),
            name: "Valid".into(),
            description: "Valid".into(),
            present: true,
            provenance: Provenance::human("  ", 1),
        }),
        Err(ModelError::EmptyActor)
    );
}

#[test]
fn cap_is_256_conditions() {
    let mut p = project();
    for _ in 0..MAX_CONDITIONS {
        add(
            &mut p,
            ConditionId::new_v7(),
            "Condition",
            "Description",
            true,
        );
    }
    assert_eq!(p.conditions().count(), MAX_CONDITIONS);
    let overflow = p.apply(Op::AddCondition {
        id: ConditionId::new_v7(),
        name: "One too many".into(),
        description: "One too many".into(),
        present: true,
        provenance: who(),
    });
    assert_eq!(
        overflow,
        Err(ModelError::TooManyConditions {
            max: MAX_CONDITIONS
        })
    );
    assert_eq!(p.conditions().count(), MAX_CONDITIONS);
}

#[test]
fn project_diff_lists_conditions_in_canonical_order() {
    let base = project();
    let mut curr = base.clone();
    let (a, b) = (ConditionId::new_v7(), ConditionId::new_v7());
    add(&mut curr, b, "B", "desc", true);
    add(&mut curr, a, "A", "desc", false);

    let removed = ConditionId::new_v7();
    let mut with_removed = base.clone();
    add(&mut with_removed, removed, "Gone", "desc", true);
    let mut curr_without = with_removed.clone();
    curr_without
        .apply(Op::RemoveCondition { id: removed })
        .expect("remove");

    let diff = with_removed.diff(&curr_without).expect("diff");
    assert_eq!(diff.conditions().removed(), [removed]);
    assert!(diff.conditions().added().is_empty());
    assert!(!diff.is_empty());

    let diff = base.diff(&curr).expect("diff");
    let mut expected = [a, b];
    expected.sort_unstable();
    assert_eq!(diff.conditions().added(), expected);
    assert!(diff.conditions().modified().is_empty());

    let mut renamed = curr.clone();
    renamed
        .apply(Op::RenameCondition {
            id: a,
            name: "A renamed".into(),
            provenance: who(),
        })
        .expect("rename");
    let diff = curr.diff(&renamed).expect("diff");
    assert_eq!(diff.conditions().modified(), [a]);
    assert_eq!(
        base.diff(&base).expect("empty diff").conditions().added(),
        []
    );
    assert!(base.diff(&base).expect("empty diff").is_empty());
}

#[test]
fn op_log_round_trips_every_condition_op() {
    let m = Arc::new(Methodology::iso21434_r155().expect("builtin"));
    let id = ConditionId::new_v7();
    let ops = vec![
        Op::AddCondition {
            id,
            name: "Airbag deploys".into(),
            description: "Frontal crash".into(),
            present: true,
            provenance: who(),
        },
        Op::RenameCondition {
            id,
            name: "Airbag deploys in a crash".into(),
            provenance: who(),
        },
        Op::SetConditionPresent {
            id,
            present: false,
            provenance: who(),
        },
        Op::RemoveCondition { id },
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
    let read: Vec<_> = OpLogReader::open(log.as_slice(), Arc::clone(&m))
        .expect("header")
        .map(|r| r.expect("record").op)
        .collect();
    assert_eq!(read, ops);

    // More than 100 conditions through one log round trip, including CF100.
    let mut many = Vec::new();
    let mut ids = Vec::new();
    for _ in 0..101 {
        let id = ConditionId::new_v7();
        ids.push(id);
        many.push(Op::AddCondition {
            id,
            name: "Condition".into(),
            description: "Description".into(),
            present: true,
            provenance: who(),
        });
    }
    let mut log = Vec::new();
    let mut w = OpLogWriter::create(&mut log, Arc::clone(&m)).expect("writer");
    for op in &many {
        w.append(op).expect("append");
    }
    drop(w);
    let read: Vec<_> = OpLogReader::open(log.as_slice(), Arc::clone(&m))
        .expect("header")
        .map(|r| r.expect("record").op)
        .collect();
    assert_eq!(read, many);

    let mut p = project();
    for op in many {
        p.apply(op).expect("add 101 conditions");
    }
    let last_idx = p.condition_idx(&ids[100]).expect("last idx");
    assert_eq!(p.condition_visual_id(last_idx).as_deref(), Some("CF100"));
    let before = saved(&p);
    let before_project = p.clone();
    let undo = p
        .apply(Op::RemoveCondition { id: ids[100] })
        .expect("remove");
    assert_eq!(
        undo,
        Op::AddCondition {
            id: ids[100],
            name: "Condition".into(),
            description: "Description".into(),
            present: true,
            provenance: who(),
        }
    );
    p.apply(undo).expect("restore");
    assert_eq!(saved(&p), before);
    assert_eq!(&p, &before_project);
    // Removal tombstones the old slot; the restored condition has a new index
    // but keeps its visual number (and order in the arena) by public ID.
    let restored = p.condition_idx(&ids[100]).expect("restored idx");
    assert_ne!(restored, last_idx);
    assert_eq!(p.condition_visual_id(restored).as_deref(), Some("CF100"));
}
