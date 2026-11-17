//! Tests of the typed `Party` element: ids, arena, ops, guards, diff and
//! op-log round-trips.

mod common;

use std::sync::Arc;

use aegis_model::{
    ClaimId, GoalId, MAX_NAME_CHARS, MAX_PARTIES, MAX_RESPONSIBLE, Methodology, ModelError, Op,
    OpCodec, OpLogReader, OpLogWriter, PartyId, PartyIdx, PartyKind, Project, Provenance,
    VisualPrefix,
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

fn add(p: &mut Project, id: PartyId, name: &str, kind: PartyKind) {
    p.apply(Op::AddParty {
        id,
        name: name.into(),
        kind,
        provenance: who(),
    })
    .expect("add party");
}

fn add_goal(p: &mut Project, id: GoalId) {
    p.apply(Op::AddCybersecurityGoal {
        id,
        name: "Protect the asset".into(),
        controls: vec![],
        provenance: who(),
    })
    .expect("add goal");
}

fn add_claim(p: &mut Project, id: ClaimId) {
    p.apply(Op::AddCybersecurityClaim {
        id,
        name: "Retain with rationale".into(),
        provenance: who(),
    })
    .expect("add claim");
}

/// Applies `op`, checks its inverse, applies the inverse and checks that the
/// saved bytes match (visual ids and order included).
fn inverse_round_trip(p: &mut Project, op: Op, expected_inverse: Op) {
    let before = saved(p);
    let undo = p.apply(op).expect("apply");
    assert_eq!(undo, expected_inverse);
    p.apply(undo).expect("apply inverse");
    assert_eq!(saved(p), before);
}

/// Resolves a goal's stored responsible indices to their public IDs.
fn responsible_ids(p: &Project, goal: GoalId) -> Vec<PartyId> {
    let idx = p.cybersecurity_goal_idx(&goal).expect("goal idx");
    let by_idx: Vec<(PartyIdx, PartyId)> = p.parties().map(|(i, id, _)| (i, id)).collect();
    p.cybersecurity_goal(idx)
        .expect("goal")
        .responsible()
        .iter()
        .map(|&i| {
            by_idx
                .iter()
                .find(|(j, _)| *j == i)
                .map(|(_, id)| *id)
                .expect("live responsible party")
        })
        .collect()
}

/// Resolves a claim's stored responsible indices to their public IDs.
fn claim_responsible_ids(p: &Project, claim: ClaimId) -> Vec<PartyId> {
    let idx = p.cybersecurity_claim_idx(&claim).expect("claim idx");
    let by_idx: Vec<(PartyIdx, PartyId)> = p.parties().map(|(i, id, _)| (i, id)).collect();
    p.cybersecurity_claim(idx)
        .expect("claim")
        .responsible()
        .iter()
        .map(|&i| {
            by_idx
                .iter()
                .find(|(j, _)| *j == i)
                .map(|(_, id)| *id)
                .expect("live responsible party")
        })
        .collect()
}

#[test]
fn ids_and_visual_ids() {
    assert_eq!(VisualPrefix::Party.as_str(), "P");
    assert_eq!(VisualPrefix::Party.format(0), "P00");
    assert_eq!(VisualPrefix::Party.format(100), "P100");

    let mut p = project();
    let first = PartyId::new_v7();
    add(&mut p, first, "Us", PartyKind::Us);
    let idx = p.party_idx(&first).expect("party idx");
    assert_eq!(idx.index(), 0);
    let party = p.party(idx).expect("party");
    assert_eq!(party.name(), "Us");
    assert_eq!(party.kind(), PartyKind::Us);
    assert_eq!(party.provenance(), &who());
    assert_eq!(p.parties().count(), 1);
    assert_eq!(p.party_visual_id(idx).as_deref(), Some("P00"));
    assert_eq!(p.party_idx(&PartyId::new_v7()), None);

    // More than 100 parties in one project: the 101st visual id is P100.
    let mut last = None;
    for _ in 0..100 {
        let id = PartyId::new_v7();
        add(&mut p, id, "Supplier", PartyKind::Supplier);
        last = Some(id);
    }
    let last = last.expect("101 parties");
    let last_idx = p.party_idx(&last).expect("last idx");
    assert_eq!(p.party_visual_id(last_idx).as_deref(), Some("P100"));
    assert_eq!(p.parties().count(), 101);
}

#[test]
fn every_party_op_has_an_exact_inverse() {
    let id = PartyId::new_v7();
    let original = who();

    // AddParty: empty -> party -> empty.
    let mut p = project();
    let before = saved(&p);
    let undo = p
        .apply(Op::AddParty {
            id,
            name: "Bosch".into(),
            kind: PartyKind::Supplier,
            provenance: original.clone(),
        })
        .expect("add");
    assert_eq!(undo, Op::RemoveParty { id });
    p.apply(undo).expect("undo add");
    assert_eq!(saved(&p), before);
    assert!(p.parties().next().is_none());

    // RenameParty: the inverse carries the old name and provenance.
    let mut p = project();
    add(&mut p, id, "Bosch", PartyKind::Supplier);
    let idx = p.party_idx(&id).expect("idx");
    let before = saved(&p);
    let undo = p
        .apply(Op::RenameParty {
            id,
            name: "Continental".into(),
            provenance: who(),
        })
        .expect("rename");
    assert!(matches!(undo, Op::RenameParty { .. }));
    p.apply(undo).expect("undo rename");
    assert_eq!(saved(&p), before);
    assert_eq!(p.party(idx).map(|x| x.name()), Some("Bosch"));
    assert_eq!(p.party(idx).map(|x| x.provenance()), Some(&original));

    // SetPartyKind: the inverse carries the old kind and provenance.
    let before = saved(&p);
    let undo = p
        .apply(Op::SetPartyKind {
            id,
            kind: PartyKind::Customer,
            provenance: who(),
        })
        .expect("set kind");
    assert!(matches!(undo, Op::SetPartyKind { .. }));
    p.apply(undo).expect("undo kind");
    assert_eq!(saved(&p), before);
    assert_eq!(p.party(idx).map(|x| x.kind()), Some(PartyKind::Supplier));
    assert_eq!(p.party(idx).map(|x| x.provenance()), Some(&original));

    // RemoveParty: the inverse restores name, kind and provenance exactly.
    let before = saved(&p);
    let undo = p.apply(Op::RemoveParty { id }).expect("remove");
    assert_eq!(
        undo,
        Op::AddParty {
            id,
            name: "Bosch".into(),
            kind: PartyKind::Supplier,
            provenance: original.clone(),
        }
    );
    p.apply(undo).expect("undo remove");
    assert_eq!(saved(&p), before);
    let restored_idx = p.party_idx(&id).expect("restored idx");
    assert_eq!(p.party(restored_idx).map(|x| x.name()), Some("Bosch"));
    assert_eq!(
        p.party(restored_idx).map(|x| x.provenance()),
        Some(&original)
    );
    assert_eq!(p.party_visual_id(restored_idx).as_deref(), Some("P00"));
}

#[test]
fn guards_reject_only_their_input() {
    let mut p = project();
    let id = PartyId::new_v7();
    add(&mut p, id, "Bosch", PartyKind::Supplier);

    // Duplicate id: a fresh name does not avoid the id guard.
    assert_eq!(
        p.apply(Op::AddParty {
            id,
            name: "Other".into(),
            kind: PartyKind::Other,
            provenance: who(),
        }),
        Err(ModelError::DuplicatePartyId(id))
    );

    // Unknown id: rename, kind change and removal all name a missing party.
    let missing = PartyId::new_v7();
    assert_eq!(
        p.apply(Op::RenameParty {
            id: missing,
            name: "X".into(),
            provenance: who(),
        }),
        Err(ModelError::UnknownParty(missing))
    );
    assert_eq!(
        p.apply(Op::SetPartyKind {
            id: missing,
            kind: PartyKind::Us,
            provenance: who(),
        }),
        Err(ModelError::UnknownParty(missing))
    );
    assert_eq!(
        p.apply(Op::RemoveParty { id: missing }),
        Err(ModelError::UnknownParty(missing))
    );

    // Blank names: only the name guard refuses whitespace.
    let blank = p.apply(Op::AddParty {
        id: PartyId::new_v7(),
        name: "   ".into(),
        kind: PartyKind::Us,
        provenance: who(),
    });
    assert_eq!(blank, Err(ModelError::PartyNameEmpty));
    assert!(!blank.unwrap_err().to_string().contains("   "));
    let blank_rename = p.apply(Op::RenameParty {
        id,
        name: " ".into(),
        provenance: who(),
    });
    assert_eq!(blank_rename, Err(ModelError::PartyNameEmpty));
    assert_eq!(
        p.party(p.party_idx(&id).expect("idx")).map(|x| x.name()),
        Some("Bosch")
    );

    // Over-long name: 257 chars is refused, the error carries no name.
    let long = "x".repeat(MAX_NAME_CHARS + 1);
    let too_long = p.apply(Op::AddParty {
        id: PartyId::new_v7(),
        name: long.as_str().into(),
        kind: PartyKind::Us,
        provenance: who(),
    });
    assert_eq!(
        too_long,
        Err(ModelError::PartyNameTooLong {
            max: MAX_NAME_CHARS
        })
    );
    assert!(!too_long.unwrap_err().to_string().contains(&long));

    // A blank actor is refused even with an otherwise valid party.
    assert_eq!(
        p.apply(Op::AddParty {
            id: PartyId::new_v7(),
            name: "Valid".into(),
            kind: PartyKind::Us,
            provenance: Provenance::human("  ", 1),
        }),
        Err(ModelError::EmptyActor)
    );
}

#[test]
fn cap_is_256_parties() {
    let mut p = project();
    for _ in 0..MAX_PARTIES {
        add(&mut p, PartyId::new_v7(), "Party", PartyKind::Other);
    }
    assert_eq!(p.parties().count(), MAX_PARTIES);
    let overflow = p.apply(Op::AddParty {
        id: PartyId::new_v7(),
        name: "One too many".into(),
        kind: PartyKind::Other,
        provenance: who(),
    });
    assert_eq!(
        overflow,
        Err(ModelError::TooManyParties { max: MAX_PARTIES })
    );
    assert_eq!(p.parties().count(), MAX_PARTIES);
}

#[test]
fn op_log_round_trips_every_party_op() {
    let m = Arc::new(Methodology::iso21434_r155().expect("builtin"));
    let id = PartyId::new_v7();
    let id2 = PartyId::new_v7();
    let ops = vec![
        Op::AddParty {
            id,
            name: "Bosch".into(),
            kind: PartyKind::Supplier,
            provenance: who(),
        },
        Op::RenameParty {
            id,
            name: "Bosch Mobility".into(),
            provenance: who(),
        },
        Op::SetPartyKind {
            id,
            kind: PartyKind::Customer,
            provenance: who(),
        },
        Op::RemoveParty { id },
        Op::AddParty {
            id: id2,
            name: "Us".into(),
            kind: PartyKind::Us,
            provenance: who(),
        },
    ];

    // Codec round-trip, plus the line-oriented op log.
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

    // More than 100 parties through one op-log round trip, including P100.
    let mut many = Vec::new();
    let mut ids = Vec::new();
    for _ in 0..101 {
        let id = PartyId::new_v7();
        ids.push(id);
        many.push(Op::AddParty {
            id,
            name: "Supplier".into(),
            kind: PartyKind::Supplier,
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
        p.apply(op).expect("add 101 parties");
    }
    let last_idx = p.party_idx(&ids[100]).expect("last idx");
    assert_eq!(p.party_visual_id(last_idx).as_deref(), Some("P100"));
}

#[test]
fn project_diff_lists_parties_in_canonical_order() {
    let base = project();
    let mut curr = base.clone();
    let (a, b) = (PartyId::new_v7(), PartyId::new_v7());
    add(&mut curr, b, "B", PartyKind::Customer);
    add(&mut curr, a, "A", PartyKind::Us);

    let removed = PartyId::new_v7();
    let mut with_removed = base.clone();
    add(&mut with_removed, removed, "Gone", PartyKind::Other);
    let mut curr_without = with_removed.clone();
    let idx = curr_without.party_idx(&removed).expect("removed idx");
    curr_without
        .apply(Op::RemoveParty { id: removed })
        .expect("remove");
    assert_eq!(curr_without.party(idx), None);

    let diff = with_removed.diff(&curr_without).expect("diff");
    assert_eq!(diff.parties().removed(), [removed]);
    assert!(diff.parties().added().is_empty());
    assert!(!diff.is_empty());

    let diff = base.diff(&curr).expect("diff");
    let mut expected = [a, b];
    expected.sort_unstable();
    assert_eq!(diff.parties().added(), expected);
    assert!(diff.parties().modified().is_empty());
    assert!(!diff.is_empty());

    // Renaming a party marks it modified, never added or removed.
    let mut renamed = curr.clone();
    renamed
        .apply(Op::RenameParty {
            id: a,
            name: "Renamed".into(),
            provenance: who(),
        })
        .expect("rename");
    let diff = curr.diff(&renamed).expect("diff");
    assert_eq!(diff.parties().modified(), [a]);
    assert!(diff.parties().added().is_empty());
    assert!(diff.parties().removed().is_empty());

    assert!(project().diff(&project()).expect("diff").is_empty());
    assert!(
        project()
            .diff(&project())
            .expect("diff")
            .parties()
            .is_empty()
    );
}

#[test]
fn every_responsible_op_has_an_exact_inverse() {
    let (party, goal, claim) = (PartyId::new_v7(), GoalId::new_v7(), ClaimId::new_v7());
    let old = Provenance::human("bob", 42);
    let mut p = project();
    add(&mut p, party, "Bosch", PartyKind::Supplier);
    add_goal(&mut p, goal);
    add_claim(&mut p, claim);

    // LinkGoalParty: empty -> linked -> empty, with saved bytes restored.
    inverse_round_trip(
        &mut p,
        Op::LinkGoalParty {
            goal,
            party,
            provenance: who(),
        },
        Op::UnlinkGoalParty {
            goal,
            party,
            provenance: who(),
        },
    );

    // UnlinkGoalParty: the inverse must carry the OLD provenance, not the new
    // one supplied by the unlink op.
    p.apply(Op::LinkGoalParty {
        goal,
        party,
        provenance: old.clone(),
    })
    .expect("set up goal link");
    inverse_round_trip(
        &mut p,
        Op::UnlinkGoalParty {
            goal,
            party,
            provenance: who(),
        },
        Op::LinkGoalParty {
            goal,
            party,
            provenance: old.clone(),
        },
    );
    p.apply(Op::UnlinkGoalParty {
        goal,
        party,
        provenance: who(),
    })
    .expect("tear down goal link");

    // LinkClaimParty: empty -> linked -> empty.
    inverse_round_trip(
        &mut p,
        Op::LinkClaimParty {
            claim,
            party,
            provenance: who(),
        },
        Op::UnlinkClaimParty {
            claim,
            party,
            provenance: who(),
        },
    );

    // UnlinkClaimParty: again, the inverse carries the old provenance.
    p.apply(Op::LinkClaimParty {
        claim,
        party,
        provenance: old.clone(),
    })
    .expect("set up claim link");
    inverse_round_trip(
        &mut p,
        Op::UnlinkClaimParty {
            claim,
            party,
            provenance: who(),
        },
        Op::LinkClaimParty {
            claim,
            party,
            provenance: old.clone(),
        },
    );
    p.apply(Op::UnlinkClaimParty {
        claim,
        party,
        provenance: who(),
    })
    .expect("tear down claim link");
}

#[test]
fn responsible_guards_reject_only_their_input() {
    let (party, goal, claim) = (PartyId::new_v7(), GoalId::new_v7(), ClaimId::new_v7());
    let missing = PartyId::new_v7();
    let mut p = project();
    add(&mut p, party, "Bosch", PartyKind::Supplier);
    add_goal(&mut p, goal);
    add_claim(&mut p, claim);

    // Unknown party: both link ops refuse it, before any other guard applies.
    assert_eq!(
        p.apply(Op::LinkGoalParty {
            goal,
            party: missing,
            provenance: who(),
        }),
        Err(ModelError::UnknownParty(missing))
    );
    assert_eq!(
        p.apply(Op::LinkClaimParty {
            claim,
            party: missing,
            provenance: who(),
        }),
        Err(ModelError::UnknownParty(missing))
    );

    // Unknown goal/claim: a valid party does not get around the entity guard.
    let unknown_goal = GoalId::new_v7();
    assert_eq!(
        p.apply(Op::LinkGoalParty {
            goal: unknown_goal,
            party,
            provenance: who(),
        }),
        Err(ModelError::UnknownCybersecurityGoal(unknown_goal))
    );
    let unknown_claim = ClaimId::new_v7();
    assert_eq!(
        p.apply(Op::LinkClaimParty {
            claim: unknown_claim,
            party,
            provenance: who(),
        }),
        Err(ModelError::UnknownCybersecurityClaim(unknown_claim))
    );

    // Blank actor on the link ops: no links exist yet, so only the provenance
    // guard may refuse them (L15: no validate arm may return before it).
    for op in [
        Op::LinkGoalParty {
            goal,
            party,
            provenance: Provenance::human("  ", 1),
        },
        Op::LinkClaimParty {
            claim,
            party,
            provenance: Provenance::human("  ", 1),
        },
    ] {
        assert_eq!(p.apply(op), Err(ModelError::EmptyActor));
    }

    // Goal link/unlink guards, with a second party that is deliberately
    // unlinked so only the duplicate/not-linked guard can fire.
    p.apply(Op::LinkGoalParty {
        goal,
        party,
        provenance: who(),
    })
    .expect("link goal party");
    assert_eq!(
        p.apply(Op::LinkGoalParty {
            goal,
            party,
            provenance: who(),
        }),
        Err(ModelError::DuplicateGoalParty { goal, party })
    );
    // Blank actor on unlink: the link exists, so only provenance may refuse it.
    assert_eq!(
        p.apply(Op::UnlinkGoalParty {
            goal,
            party,
            provenance: Provenance::human("  ", 1),
        }),
        Err(ModelError::EmptyActor)
    );
    let other = PartyId::new_v7();
    add(&mut p, other, "Continental", PartyKind::Supplier);
    assert_eq!(
        p.apply(Op::UnlinkGoalParty {
            goal,
            party: other,
            provenance: who(),
        }),
        Err(ModelError::GoalPartyNotLinked { goal, party: other })
    );

    // Claim link/unlink guards.
    p.apply(Op::LinkClaimParty {
        claim,
        party,
        provenance: who(),
    })
    .expect("link claim party");
    assert_eq!(
        p.apply(Op::LinkClaimParty {
            claim,
            party,
            provenance: who(),
        }),
        Err(ModelError::DuplicateClaimParty { claim, party })
    );
    assert_eq!(
        p.apply(Op::UnlinkClaimParty {
            claim,
            party: other,
            provenance: who(),
        }),
        Err(ModelError::ClaimPartyNotLinked {
            claim,
            party: other
        })
    );
    // Blank actor on claim unlink: the link exists, so only provenance refuses.
    assert_eq!(
        p.apply(Op::UnlinkClaimParty {
            claim,
            party,
            provenance: Provenance::human("  ", 1),
        }),
        Err(ModelError::EmptyActor)
    );
}

#[test]
fn responsible_links_are_sorted_and_unique() {
    let (goal, claim) = (GoalId::new_v7(), ClaimId::new_v7());
    let mut p = project();
    add_goal(&mut p, goal);
    add_claim(&mut p, claim);

    let mut ids: Vec<PartyId> = (0..8).map(|_| PartyId::new_v7()).collect();
    for id in &ids {
        add(&mut p, *id, "Party", PartyKind::Other);
    }
    ids.sort_unstable();
    let mut expected = ids.clone();

    // Link in an order unrelated to PartyId ordering, and twice for the goal
    // to prove the second link is refused rather than duplicated.
    for id in ids.iter().rev() {
        p.apply(Op::LinkGoalParty {
            goal,
            party: *id,
            provenance: who(),
        })
        .expect("link goal party");
        p.apply(Op::LinkClaimParty {
            claim,
            party: *id,
            provenance: who(),
        })
        .expect("link claim party");
    }
    assert_eq!(
        p.apply(Op::LinkGoalParty {
            goal,
            party: ids[0],
            provenance: who(),
        }),
        Err(ModelError::DuplicateGoalParty {
            goal,
            party: ids[0]
        })
    );
    assert_eq!(
        p.apply(Op::LinkClaimParty {
            claim,
            party: ids[0],
            provenance: who(),
        }),
        Err(ModelError::DuplicateClaimParty {
            claim,
            party: ids[0]
        })
    );

    assert_eq!(responsible_ids(&p, goal), expected);
    assert_eq!(claim_responsible_ids(&p, claim), expected);
    expected.dedup();
    assert_eq!(expected.len(), 8);
}

#[test]
fn responsible_cap_is_16() {
    let (goal, claim) = (GoalId::new_v7(), ClaimId::new_v7());
    let mut p = project();
    add_goal(&mut p, goal);
    add_claim(&mut p, claim);

    for _ in 0..MAX_RESPONSIBLE {
        let id = PartyId::new_v7();
        add(&mut p, id, "Party", PartyKind::Other);
        p.apply(Op::LinkGoalParty {
            goal,
            party: id,
            provenance: who(),
        })
        .expect("link goal party");
        p.apply(Op::LinkClaimParty {
            claim,
            party: id,
            provenance: who(),
        })
        .expect("link claim party");
    }
    assert_eq!(responsible_ids(&p, goal).len(), MAX_RESPONSIBLE);
    assert_eq!(claim_responsible_ids(&p, claim).len(), MAX_RESPONSIBLE);

    let extra = PartyId::new_v7();
    add(&mut p, extra, "Extra", PartyKind::Other);
    assert_eq!(
        p.apply(Op::LinkGoalParty {
            goal,
            party: extra,
            provenance: who(),
        }),
        Err(ModelError::TooManyGoalResponsible {
            max: MAX_RESPONSIBLE
        })
    );
    assert_eq!(
        p.apply(Op::LinkClaimParty {
            claim,
            party: extra,
            provenance: who(),
        }),
        Err(ModelError::TooManyClaimResponsible {
            max: MAX_RESPONSIBLE
        })
    );
    assert_eq!(responsible_ids(&p, goal).len(), MAX_RESPONSIBLE);
    assert_eq!(claim_responsible_ids(&p, claim).len(), MAX_RESPONSIBLE);
}

#[test]
fn remove_party_is_guarded_by_responsible_links() {
    let (party, goal, claim) = (PartyId::new_v7(), GoalId::new_v7(), ClaimId::new_v7());
    let mut p = project();
    add(&mut p, party, "Bosch", PartyKind::Supplier);
    add_goal(&mut p, goal);
    add_claim(&mut p, claim);

    // Only a linking goal refuses removal.
    p.apply(Op::LinkGoalParty {
        goal,
        party,
        provenance: who(),
    })
    .expect("link goal party");
    assert_eq!(
        p.apply(Op::RemoveParty { id: party }),
        Err(ModelError::PartyInUse {
            party,
            goals: 1,
            claims: 0
        })
    );

    // A linking claim refuses it too, and both are reported.
    p.apply(Op::LinkClaimParty {
        claim,
        party,
        provenance: who(),
    })
    .expect("link claim party");
    assert_eq!(
        p.apply(Op::RemoveParty { id: party }),
        Err(ModelError::PartyInUse {
            party,
            goals: 1,
            claims: 1
        })
    );

    // Unlink the goal: only the claim still blocks removal.
    p.apply(Op::UnlinkGoalParty {
        goal,
        party,
        provenance: who(),
    })
    .expect("unlink goal party");
    assert_eq!(
        p.apply(Op::RemoveParty { id: party }),
        Err(ModelError::PartyInUse {
            party,
            goals: 0,
            claims: 1
        })
    );

    // After unlinking the claim, removal succeeds and its inverse restores
    // the party and the saved bytes exactly.
    p.apply(Op::UnlinkClaimParty {
        claim,
        party,
        provenance: who(),
    })
    .expect("unlink claim party");
    let before = saved(&p);
    let undo = p.apply(Op::RemoveParty { id: party }).expect("remove");
    assert_eq!(
        undo,
        Op::AddParty {
            id: party,
            name: "Bosch".into(),
            kind: PartyKind::Supplier,
            provenance: who(),
        }
    );
    p.apply(undo).expect("undo remove");
    assert_eq!(saved(&p), before);
    assert!(p.party_idx(&party).is_some());
}

#[test]
fn responsible_ops_add_no_risk_delta() {
    let (party, goal, claim) = (PartyId::new_v7(), GoalId::new_v7(), ClaimId::new_v7());
    let mut p = project();
    add(&mut p, party, "Bosch", PartyKind::Supplier);
    add_goal(&mut p, goal);
    add_claim(&mut p, claim);
    let base = p.clone();

    p.apply(Op::LinkGoalParty {
        goal,
        party,
        provenance: who(),
    })
    .expect("link goal party");
    p.apply(Op::LinkClaimParty {
        claim,
        party,
        provenance: who(),
    })
    .expect("link claim party");
    p.apply(Op::UnlinkGoalParty {
        goal,
        party,
        provenance: who(),
    })
    .expect("unlink goal party");
    p.apply(Op::UnlinkClaimParty {
        claim,
        party,
        provenance: who(),
    })
    .expect("unlink claim party");

    let diff = base.diff(&p).expect("diff");
    assert!(diff.risk_deltas().is_empty());
}

#[test]
fn op_log_round_trips_every_responsible_op() {
    let m = Arc::new(Methodology::iso21434_r155().expect("builtin"));
    let (goal, claim, party) = (GoalId::new_v7(), ClaimId::new_v7(), PartyId::new_v7());
    let ops = vec![
        Op::LinkGoalParty {
            goal,
            party,
            provenance: who(),
        },
        Op::UnlinkGoalParty {
            goal,
            party,
            provenance: who(),
        },
        Op::LinkClaimParty {
            claim,
            party,
            provenance: who(),
        },
        Op::UnlinkClaimParty {
            claim,
            party,
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

#[test]
fn project_eq_sees_goal_responsible_difference() {
    let (party, goal) = (PartyId::new_v7(), GoalId::new_v7());
    let mut without = project();
    add(&mut without, party, "Bosch", PartyKind::Supplier);
    add_goal(&mut without, goal);

    let mut with = without.clone();
    // Linking with the same provenance `add_goal` used leaves provenance
    // unchanged, so the responsible list is the only difference.
    with.apply(Op::LinkGoalParty {
        goal,
        party,
        provenance: who(),
    })
    .expect("link goal party");

    assert!(without != with);
}

#[test]
fn project_eq_ignores_party_insertion_order_for_claims() {
    let (p1, p2, claim) = (PartyId::new_v7(), PartyId::new_v7(), ClaimId::new_v7());

    // A: parties enter the arena as p1 then p2; claim links p1 then p2.
    let mut a = project();
    add(&mut a, p1, "First", PartyKind::Supplier);
    add(&mut a, p2, "Second", PartyKind::Customer);
    add_claim(&mut a, claim);
    a.apply(Op::LinkClaimParty {
        claim,
        party: p1,
        provenance: who(),
    })
    .expect("link p1");
    a.apply(Op::LinkClaimParty {
        claim,
        party: p2,
        provenance: who(),
    })
    .expect("link p2");

    // B: same public parties and claim, but parties enter the arena in the
    // opposite order (so the stored PartyIdx slots differ) and the links are
    // applied in the opposite order too.
    let mut b = project();
    add(&mut b, p2, "Second", PartyKind::Customer);
    add(&mut b, p1, "First", PartyKind::Supplier);
    add_claim(&mut b, claim);
    b.apply(Op::LinkClaimParty {
        claim,
        party: p2,
        provenance: who(),
    })
    .expect("link p2");
    b.apply(Op::LinkClaimParty {
        claim,
        party: p1,
        provenance: who(),
    })
    .expect("link p1");

    assert!(a == b);
    assert!(a.diff(&b).expect("diff").cybersecurity_claims().is_empty());
}
