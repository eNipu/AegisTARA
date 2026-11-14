//! Control presets: `Op::SetControlKind` (job 124, CFG-06; decision D26).
//!
//! Covers exact inverses, the unknown-control and unknown-kind refusals, the
//! blank-actor refusal, the removal guard, and op log and snapshot round
//! trips with untrusted input.

mod common;

use std::sync::Arc;

use aegis_model::{
    ControlId, ControlKind, Methodology, ModelError, Op, OpCodec, Project, Provenance,
};

fn methodology() -> Arc<Methodology> {
    common::builtin()
}

fn boot(m: &Methodology) -> ControlKind {
    m.control_kind("secure-boot").expect("secure-boot")
}

fn project_with_control() -> (Project, ControlId) {
    let mut p = Project::new(methodology());
    let id = ControlId::new_v7();
    p.apply(Op::AddSecurityControl {
        id,
        name: "Secure boot".into(),
        provenance: Provenance::human("alice", 1),
    })
    .expect("add");
    (p, id)
}

fn kind_of(p: &Project, id: ControlId) -> Option<ControlKind> {
    let idx = p.security_control_idx(&id).expect("idx");
    p.security_control(idx).expect("control").kind()
}

fn set(control: ControlId, kind: Option<ControlKind>, provenance: Provenance) -> Op {
    Op::SetControlKind {
        control,
        kind,
        provenance,
    }
}

#[test]
fn set_and_clear_invert_exactly() {
    let m = methodology();
    let (mut p, id) = project_with_control();
    assert_eq!(kind_of(&p, id), None);
    let before = p.clone();

    let undo = p
        .apply(set(id, Some(boot(&m)), Provenance::human("bob", 2)))
        .expect("set");
    assert_eq!(kind_of(&p, id), Some(boot(&m)));
    let after_set = p.clone();
    assert_ne!(p, before);

    p.apply(undo).expect("undo");
    assert_eq!(p, before);

    // Clear after set.
    p.apply(set(id, Some(boot(&m)), Provenance::human("bob", 2)))
        .expect("set again");
    let undo_clear = p
        .apply(set(id, None, Provenance::human("carol", 3)))
        .expect("clear");
    assert_eq!(kind_of(&p, id), None);
    p.apply(undo_clear).expect("undo clear");
    assert_eq!(p, after_set);
}

#[test]
fn unknown_control_and_unknown_kind_are_refused() {
    let m = methodology();
    let (mut p, _) = project_with_control();
    let err = p
        .apply(set(
            ControlId::new_v7(),
            Some(boot(&m)),
            Provenance::default(),
        ))
        .err();
    assert!(matches!(err, Some(ModelError::UnknownSecurityControl(_))));

    let second = common::second();
    let mut q = Project::new(Arc::clone(&second));
    let id = ControlId::new_v7();
    q.apply(Op::AddSecurityControl {
        id,
        name: "Control".into(),
        provenance: Provenance::default(),
    })
    .expect("add");
    let before = q.clone();
    let err = q
        .apply(set(id, Some(boot(&m)), Provenance::default()))
        .err();
    assert!(matches!(err, Some(ModelError::UnknownControlKind(_))));
    assert_eq!(q, before);
}

#[test]
fn blank_actor_is_refused() {
    let m = methodology();
    let (mut p, id) = project_with_control();
    let before = p.clone();
    let err = p
        .apply(set(id, Some(boot(&m)), Provenance::human("  ", 1)))
        .err();
    assert_eq!(err, Some(ModelError::EmptyActor));
    assert_eq!(p, before);
}

#[test]
fn removal_is_blocked_only_by_the_kind() {
    let m = methodology();
    let (mut p, id) = project_with_control();
    p.apply(set(id, Some(boot(&m)), Provenance::default()))
        .expect("set");
    let err = p.apply(Op::RemoveSecurityControl { id }).err();
    assert_eq!(
        err,
        Some(ModelError::SecurityControlHasKind { control: id })
    );
    p.apply(set(id, None, Provenance::default()))
        .expect("clear");
    p.apply(Op::RemoveSecurityControl { id }).expect("remove");
    assert!(p.security_control_idx(&id).is_none());
}

#[test]
fn op_log_round_trips_some_and_none() {
    let m = methodology();
    let codec = OpCodec::new(Arc::clone(&m));
    let id = ControlId::new_v7();
    for (kind, needle) in [
        (Some(boot(&m)), "\"kind\":\"secure-boot\""),
        (None, "\"kind\":null"),
    ] {
        let op = set(id, kind, Provenance::human("alice", 1));
        let bytes = codec.encode(&op).expect("encode");
        let text = String::from_utf8(bytes.clone()).expect("utf8");
        assert!(text.contains("\"op\":\"set_control_kind\""), "{text}");
        assert!(text.contains(needle), "{text}");
        assert_eq!(codec.decode(&bytes).expect("decode"), op);
    }

    // The kind field is required, even when null.
    let op = set(id, None, Provenance::default());
    let text = String::from_utf8(codec.encode(&op).expect("encode")).expect("utf8");
    let without = text
        .replace("\"kind\":null,", "")
        .replace(",\"kind\":null", "");
    assert!(!without.contains("\"kind\""), "{without}");
    assert!(codec.decode(without.as_bytes()).is_err());

    // Also through a whole log.
    let bytes = common::write_log(&m, [&op]);
    assert_eq!(common::read_log(&m, &bytes).len(), 1);
}

#[test]
fn snapshot_round_trips_and_omits_an_absent_kind() {
    let m = methodology();
    let (mut p, id) = project_with_control();
    let bare = String::from_utf8(common::save(&p)).expect("utf8");
    assert!(!bare.contains("\"kind\""), "{bare}");

    p.apply(set(id, Some(boot(&m)), Provenance::human("bob", 2)))
        .expect("set");
    let bytes = common::save(&p);
    let text = String::from_utf8(bytes.clone()).expect("utf8");
    assert!(text.contains("\"kind\":\"secure-boot\""), "{text}");
    let loaded = Project::load(bytes.as_slice(), Arc::clone(&m)).expect("load");
    assert_eq!(loaded, p);
    assert_eq!(kind_of(&loaded, id), Some(boot(&m)));

    // An unknown kind key fails to load.
    let bad = text.replace("\"kind\":\"secure-boot\"", "\"kind\":\"no-such-preset\"");
    assert_ne!(bad, text);
    assert!(Project::load(bad.as_bytes(), m).is_err());
}
