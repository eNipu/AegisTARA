//! Tests of the typed `Condition` element: ids, arena, ops, guards, op-log
//! round-trips and diff. Conditions have no snapshot fields in this slice, so
//! inverse checks compare `save()` bytes (L06) as well as the in-memory state.

mod common;

use std::sync::Arc;

use aegis_model::{
    AssetId, AttackTree, ConditionId, ConditionIdx, ElementRef, GateKind, MAX_CONDITIONS,
    MAX_DESCRIPTION_CHARS, MAX_NAME_CHARS, Methodology, ModelError, Node, NodeId, Op, OpCodec,
    OpLogReader, OpLogWriter, Project, Provenance, TreeId, VisualPrefix,
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

// -- Condition links (133b) --------------------------------------------------------

/// A project with `count` conditions, one asset, and one tree with a step.
fn linked_project(
    m: &Arc<Methodology>,
    count: usize,
) -> (Project, Vec<ConditionId>, AssetId, TreeId, NodeId) {
    let mut p = Project::new(Arc::clone(m));
    let conditions: Vec<_> = (0..count).map(|_| ConditionId::new_v7()).collect();
    for (i, id) in conditions.iter().enumerate() {
        add(&mut p, *id, &format!("C{i}"), "desc", true);
    }
    let asset = AssetId::new_v7();
    p.apply(Op::AddAsset {
        id: asset,
        name: "ECU".into(),
        properties: m.all_properties(),
        provenance: who(),
    })
    .expect("asset");
    let (tree, root, step) = (TreeId::new_v7(), NodeId::new_v7(), NodeId::new_v7());
    let mut at = AttackTree::new(root, Node::gate(GateKind::Or, "Goal")).expect("tree");
    at.add_child(at.root(), step, Node::step("Step"))
        .expect("step");
    p.apply(Op::AddAttackTree {
        id: tree,
        tree: at,
        step_feasibility: vec![],
        provenance: who(),
    })
    .expect("tree");
    (p, conditions, asset, tree, step)
}

#[test]
fn every_condition_link_op_has_an_exact_inverse() {
    let m = Arc::new(Methodology::iso21434_r155().expect("builtin"));
    let (mut p, conditions, asset, tree, step) = linked_project(&m, 1);
    let c = conditions[0];
    let element = ElementRef::Asset(asset);

    let before_link = saved(&p);
    inverse_round_trip(
        &mut p,
        Op::NeedCondition {
            element,
            condition: c,
            provenance: who(),
        },
        Op::UnneedCondition {
            element,
            condition: c,
            provenance: who(),
        },
    );
    // Links are not snapshot fields yet (133c): applying one changes no byte.
    assert_eq!(saved(&p), before_link);
    assert_eq!(p.condition_link_ids(element), &[]);

    inverse_round_trip(
        &mut p,
        Op::NeedStepCondition {
            tree,
            node: step,
            condition: c,
            provenance: who(),
        },
        Op::UnneedStepCondition {
            tree,
            node: step,
            condition: c,
            provenance: who(),
        },
    );
    assert_eq!(p.step_condition_link_ids(tree, step), &[]);

    // Seed both links, then check the unlink ops invert back to need ops.
    p.apply(Op::NeedCondition {
        element,
        condition: c,
        provenance: who(),
    })
    .expect("need");
    p.apply(Op::NeedStepCondition {
        tree,
        node: step,
        condition: c,
        provenance: who(),
    })
    .expect("need step");
    inverse_round_trip(
        &mut p,
        Op::UnneedCondition {
            element,
            condition: c,
            provenance: who(),
        },
        Op::NeedCondition {
            element,
            condition: c,
            provenance: who(),
        },
    );
    inverse_round_trip(
        &mut p,
        Op::UnneedStepCondition {
            tree,
            node: step,
            condition: c,
            provenance: who(),
        },
        Op::NeedStepCondition {
            tree,
            node: step,
            condition: c,
            provenance: who(),
        },
    );
    // Undoing an unlink restores its link.
    assert_eq!(p.condition_usages(c), (1, 1));
}

#[test]
fn each_link_guard_refuses_only_its_own_input() {
    let m = Arc::new(Methodology::iso21434_r155().expect("builtin"));
    let (mut p, conditions, asset, tree, step) = linked_project(&m, 17);
    let c = conditions[0];
    let element = ElementRef::Asset(asset);
    let missing = ConditionId::new_v7();

    // Unknown condition, on both element and step ops.
    assert_eq!(
        p.apply(Op::NeedCondition {
            element,
            condition: missing,
            provenance: who(),
        }),
        Err(ModelError::UnknownCondition(missing))
    );
    assert_eq!(
        p.apply(Op::NeedStepCondition {
            tree,
            node: step,
            condition: missing,
            provenance: who(),
        }),
        Err(ModelError::UnknownCondition(missing))
    );

    // Unsupported target: everything else is valid.
    assert_eq!(
        p.apply(Op::NeedCondition {
            element: ElementRef::Project,
            condition: c,
            provenance: who(),
        }),
        Err(ModelError::UnsupportedConditionTarget {
            element: ElementRef::Project,
        })
    );
    let absent_asset = AssetId::new_v7();
    assert_eq!(
        p.apply(Op::NeedCondition {
            element: ElementRef::Asset(absent_asset),
            condition: c,
            provenance: who(),
        }),
        Err(ModelError::UnknownAsset(absent_asset))
    );

    // Not a step: the tree root is a gate.
    let root = p
        .attack_tree_idx(&tree)
        .and_then(|t| p.attack_tree(t))
        .and_then(|t| t.id_of(t.root()))
        .expect("root");
    assert_eq!(
        p.apply(Op::NeedStepCondition {
            tree,
            node: root,
            condition: c,
            provenance: who(),
        }),
        Err(ModelError::NotAStep { tree, node: root })
    );

    // A blank actor is refused even when the link itself is valid (L15).
    assert_eq!(
        p.apply(Op::NeedCondition {
            element,
            condition: c,
            provenance: Provenance::human("  ", 1),
        }),
        Err(ModelError::EmptyActor)
    );

    // Duplicate need and unlink of a missing link, on both element and step.
    p.apply(Op::NeedCondition {
        element,
        condition: c,
        provenance: who(),
    })
    .expect("need");
    assert_eq!(
        p.apply(Op::NeedCondition {
            element,
            condition: c,
            provenance: who(),
        }),
        Err(ModelError::ConditionAlreadyNeeded {
            element,
            condition: c,
        })
    );
    p.apply(Op::UnneedCondition {
        element,
        condition: c,
        provenance: who(),
    })
    .expect("unneed");
    assert_eq!(
        p.apply(Op::UnneedCondition {
            element,
            condition: c,
            provenance: who(),
        }),
        Err(ModelError::ConditionNotLinked {
            element,
            condition: c,
        })
    );

    p.apply(Op::NeedStepCondition {
        tree,
        node: step,
        condition: c,
        provenance: who(),
    })
    .expect("need step");
    assert_eq!(
        p.apply(Op::NeedStepCondition {
            tree,
            node: step,
            condition: c,
            provenance: who(),
        }),
        Err(ModelError::StepConditionAlreadyNeeded {
            tree,
            node: step,
            condition: c,
        })
    );
    p.apply(Op::UnneedStepCondition {
        tree,
        node: step,
        condition: c,
        provenance: who(),
    })
    .expect("unneed step");
    assert_eq!(
        p.apply(Op::UnneedStepCondition {
            tree,
            node: step,
            condition: c,
            provenance: who(),
        }),
        Err(ModelError::StepConditionNotLinked {
            tree,
            node: step,
            condition: c,
        })
    );

    // A 17th link is refused on both an element and a step.
    for id in &conditions[..16] {
        p.apply(Op::NeedCondition {
            element,
            condition: *id,
            provenance: who(),
        })
        .expect("need up to cap");
        p.apply(Op::NeedStepCondition {
            tree,
            node: step,
            condition: *id,
            provenance: who(),
        })
        .expect("need step up to cap");
    }
    let overflow = conditions[16];
    assert_eq!(
        p.apply(Op::NeedCondition {
            element,
            condition: overflow,
            provenance: who(),
        }),
        Err(ModelError::TooManyConditionLinks { max: 16 })
    );
    assert_eq!(
        p.apply(Op::NeedStepCondition {
            tree,
            node: step,
            condition: overflow,
            provenance: who(),
        }),
        Err(ModelError::TooManyConditionLinks { max: 16 })
    );
}

#[test]
fn condition_in_use_refuses_removal_until_links_clear() {
    let m = Arc::new(Methodology::iso21434_r155().expect("builtin"));
    let (mut p, conditions, asset, tree, step) = linked_project(&m, 1);
    let c = conditions[0];
    let element = ElementRef::Asset(asset);

    p.apply(Op::NeedCondition {
        element,
        condition: c,
        provenance: who(),
    })
    .expect("need");
    p.apply(Op::NeedStepCondition {
        tree,
        node: step,
        condition: c,
        provenance: who(),
    })
    .expect("need step");
    assert_eq!(p.condition_usages(c), (1, 1));
    assert_eq!(
        p.apply(Op::RemoveCondition { id: c }),
        Err(ModelError::ConditionInUse {
            condition: c,
            elements: 1,
            steps: 1,
        })
    );

    // One step link alone still blocks removal.
    p.apply(Op::UnneedCondition {
        element,
        condition: c,
        provenance: who(),
    })
    .expect("unneed");
    assert_eq!(
        p.apply(Op::RemoveCondition { id: c }),
        Err(ModelError::ConditionInUse {
            condition: c,
            elements: 0,
            steps: 1,
        })
    );

    // One element link alone still blocks removal.
    p.apply(Op::NeedCondition {
        element,
        condition: c,
        provenance: who(),
    })
    .expect("need");
    p.apply(Op::UnneedStepCondition {
        tree,
        node: step,
        condition: c,
        provenance: who(),
    })
    .expect("unneed step");
    assert_eq!(
        p.apply(Op::RemoveCondition { id: c }),
        Err(ModelError::ConditionInUse {
            condition: c,
            elements: 1,
            steps: 0,
        })
    );

    // Once both links are cleared, removal succeeds.
    p.apply(Op::UnneedCondition {
        element,
        condition: c,
        provenance: who(),
    })
    .expect("unneed");
    assert_eq!(p.condition_usages(c), (0, 0));
    p.apply(Op::RemoveCondition { id: c }).expect("remove");
    assert_eq!(p.conditions().count(), 0);
}

#[test]
fn links_are_sorted_unique_and_compared_by_public_id() {
    let m = Arc::new(Methodology::iso21434_r155().expect("builtin"));
    let conditions: Vec<_> = (0..4).map(|_| ConditionId::new_v7()).collect();
    let mut a = linked_project(&m, 0).0;
    let asset = AssetId::new_v7();
    let (tree, root, step) = (TreeId::new_v7(), NodeId::new_v7(), NodeId::new_v7());
    let mut at = AttackTree::new(root, Node::gate(GateKind::Or, "Goal")).expect("tree");
    at.add_child(at.root(), step, Node::step("Step"))
        .expect("step");
    for p in [&mut a] {
        for id in &conditions {
            add(p, *id, "C", "desc", true);
        }
        p.apply(Op::AddAsset {
            id: asset,
            name: "ECU".into(),
            properties: m.all_properties(),
            provenance: who(),
        })
        .expect("asset");
        p.apply(Op::AddAttackTree {
            id: tree,
            tree: at.clone(),
            step_feasibility: vec![],
            provenance: who(),
        })
        .expect("tree");
    }
    let mut b = a.clone();
    let element = ElementRef::Asset(asset);

    // Same three links in opposite insertion orders compare equal.
    for id in conditions[..3].iter() {
        a.apply(Op::NeedCondition {
            element,
            condition: *id,
            provenance: who(),
        })
        .expect("need a");
    }
    for id in conditions[..3].iter().rev() {
        b.apply(Op::NeedCondition {
            element,
            condition: *id,
            provenance: who(),
        })
        .expect("need b");
    }
    assert_eq!(a, b);
    assert!(a.diff(&b).expect("diff").is_empty());
    assert_eq!(a.condition_link_ids(element).len(), 3);

    // The stored list is sorted by public ConditionId, not arena index.
    let listed: Vec<_> = b
        .condition_link_ids(element)
        .iter()
        .map(|&idx| {
            b.conditions()
                .find(|(i, _, _)| *i == idx)
                .map(|(_, id, _)| id)
                .expect("resolved id")
        })
        .collect();
    let mut sorted = listed.clone();
    sorted.sort_unstable();
    assert_eq!(listed, sorted);

    // Adding a link to only one project marks the owning element modified.
    b.apply(Op::NeedCondition {
        element,
        condition: conditions[3],
        provenance: who(),
    })
    .expect("need b extra");
    assert_ne!(a, b);
    let diff = a.diff(&b).expect("diff");
    assert_eq!(diff.assets().modified(), [asset]);
    assert!(diff.attack_trees().modified().is_empty());

    // A step link marks its tree modified and changes no risk number.
    let before = saved(&b);
    b.apply(Op::NeedStepCondition {
        tree,
        node: step,
        condition: conditions[3],
        provenance: who(),
    })
    .expect("need step");
    assert_ne!(saved(&b), before, "the step link is saved (133c)");
    assert_ne!(a, b);
    let diff = a.diff(&b).expect("diff");
    assert_eq!(diff.attack_trees().modified(), [tree]);
    assert_eq!(b.step_condition_link_ids(tree, step).len(), 1);
}

#[test]
fn link_ops_round_trip_through_the_op_log() {
    let m = Arc::new(Methodology::iso21434_r155().expect("builtin"));
    let (_, conditions, asset, tree, step) = linked_project(&m, 1);
    let c = conditions[0];
    let element = ElementRef::Asset(asset);
    let ops = vec![
        Op::NeedCondition {
            element,
            condition: c,
            provenance: who(),
        },
        Op::UnneedCondition {
            element,
            condition: c,
            provenance: who(),
        },
        Op::NeedStepCondition {
            tree,
            node: step,
            condition: c,
            provenance: who(),
        },
        Op::UnneedStepCondition {
            tree,
            node: step,
            condition: c,
            provenance: who(),
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
    let read: Vec<_> = OpLogReader::open(log.as_slice(), Arc::clone(&m))
        .expect("header")
        .map(|r| r.expect("record").op)
        .collect();
    assert_eq!(read, ops);
}

/// An element or step that needs a condition cannot be removed, nor its tree:
/// the removal inverses (`AddAsset`, `AddTreeNode`, `AddAttackTree`) cannot
/// carry the links, so undo would silently lose them. Once unneeded, each
/// removal goes through.
#[test]
fn removing_an_element_or_step_that_needs_a_condition_is_refused() {
    let m = Arc::new(Methodology::iso21434_r155().expect("builtin"));
    let (mut p, conditions, asset, tree, step) = linked_project(&m, 1);
    let condition = conditions[0];
    let element = ElementRef::Asset(asset);
    p.apply(Op::NeedCondition {
        element,
        condition,
        provenance: who(),
    })
    .expect("need");
    p.apply(Op::NeedStepCondition {
        tree,
        node: step,
        condition,
        provenance: who(),
    })
    .expect("need step");

    assert_eq!(
        p.apply(Op::RemoveAsset { id: asset }).map(|_| ()),
        Err(ModelError::ElementNeedsConditions {
            element,
            conditions: 1
        })
    );
    assert_eq!(
        p.apply(Op::RemoveTreeNode {
            tree,
            id: step,
            provenance: who()
        })
        .map(|_| ()),
        Err(ModelError::NodeIsRated(step))
    );
    assert!(
        matches!(
            p.apply(Op::RemoveAttackTree { id: tree }),
            Err(ModelError::AttackTreeHasStepLinks { steps: 1, .. })
        ),
        "a step condition counts as a step link"
    );

    p.apply(Op::UnneedCondition {
        element,
        condition,
        provenance: who(),
    })
    .expect("unneed");
    p.apply(Op::UnneedStepCondition {
        tree,
        node: step,
        condition,
        provenance: who(),
    })
    .expect("unneed step");
    p.apply(Op::RemoveTreeNode {
        tree,
        id: step,
        provenance: who(),
    })
    .expect("node removes");
    p.apply(Op::RemoveAttackTree { id: tree })
        .expect("tree removes");
    p.apply(Op::RemoveAsset { id: asset })
        .expect("asset removes");
}
