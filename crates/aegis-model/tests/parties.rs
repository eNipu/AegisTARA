//! Tests of the typed `Party` element: ids, arena, ops, guards, diff and
//! op-log round-trips.

mod common;

use std::sync::Arc;

use aegis_model::{
    MAX_NAME_CHARS, MAX_PARTIES, Methodology, ModelError, Op, OpCodec, OpLogReader, OpLogWriter,
    PartyId, PartyKind, Project, Provenance, VisualPrefix,
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
