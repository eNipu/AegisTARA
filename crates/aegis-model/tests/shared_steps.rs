//! Tests of shared attack steps: IDs, lifecycle ops, guards, descriptions,
//! diff/equality, and op-log round-trips.

use std::sync::Arc;

use aegis_model::{
    ElementRef, EntityKind, MAX_DESCRIPTION_CHARS, MAX_NAME_BYTES, MAX_SHARED_STEPS, ModelError,
    Op, OpCodec, OpLogReader, OpLogWriter, Project, ProjectDiff, Provenance, SharedStepId,
    SharedStepIdx, StoreError, VisualPrefix,
};

mod common;

use common::{builtin, shared_step_id};

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
