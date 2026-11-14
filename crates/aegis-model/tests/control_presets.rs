//! Control presets: `Op::SetControlKind` (job 124, CFG-06; decision D26).
//!
//! Covers exact inverses, the unknown-control and unknown-kind refusals, the
//! blank-actor refusal, the removal guard, and op log and snapshot round
//! trips with untrusted input.

mod common;

use std::sync::Arc;

use aegis_model::{
    ComponentId, ControlId, ControlKind, InterfaceId, ItemRef, LintCode, Methodology, ModelError,
    Op, OpCodec, Project, Provenance,
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

fn rule(p: &mut Project, control: ControlId, attack: &str, level: Option<&str>) {
    let m = p.methodology().clone();
    p.apply(Op::SetControlRule {
        control,
        attack_type: m.attack_type(attack).expect("attack type"),
        ceiling: level.map(|l| m.feasibility_level(l).expect("level")),
        provenance: Provenance::default(),
    })
    .expect("rule");
}

fn preset_control() -> (Project, ControlId) {
    let m = methodology();
    let (mut p, id) = project_with_control();
    p.apply(set(id, Some(boot(&m)), Provenance::default()))
        .expect("set");
    rule(&mut p, id, "firmware-modification", Some("low"));
    rule(&mut p, id, "bootloader-exploit", Some("low"));
    (p, id)
}

fn differing(p: &Project) -> Vec<ControlId> {
    p.lint()
        .into_iter()
        .filter_map(|f| match f.code() {
            LintCode::ControlRulesDifferFromKind { control } => Some(control),
            _ => None,
        })
        .collect()
}

#[test]
fn warning_appears_exactly_when_the_rules_differ() {
    let (mut p, id) = preset_control();
    assert!(differing(&p).is_empty());

    rule(&mut p, id, "bootloader-exploit", Some("medium"));
    assert_eq!(differing(&p), [id]);
    rule(&mut p, id, "bootloader-exploit", Some("low"));
    assert!(differing(&p).is_empty());

    rule(&mut p, id, "debug-access", Some("low"));
    assert_eq!(differing(&p), [id]);
    rule(&mut p, id, "debug-access", None);
    assert!(differing(&p).is_empty());

    rule(&mut p, id, "bootloader-exploit", None);
    assert_eq!(differing(&p), [id]);
}

#[test]
fn a_control_without_a_kind_never_differs() {
    let (mut p, id) = project_with_control();
    rule(&mut p, id, "debug-access", Some("low"));
    assert!(differing(&p).is_empty());
    let idx = p.security_control_idx(&id).expect("idx");
    assert!(!p.control_differs_from_kind(idx));
}

#[test]
fn fit_follows_the_preset_target_kinds() {
    let m = methodology();
    let mut p = Project::new(Arc::clone(&m));
    let comp = ComponentId::new_v7();
    p.apply(Op::AddComponent {
        id: comp,
        name: "ECU".into(),
        parent: None,
        provenance: Provenance::default(),
    })
    .expect("component");
    let mut iface = |key: &str| {
        let id = InterfaceId::new_v7();
        p.apply(Op::AddInterface {
            id,
            name: key.into(),
            component: comp,
            kind: m.interface_kind(key).expect("interface kind"),
            provenance: Provenance::default(),
        })
        .expect("interface");
        ItemRef::Interface(id)
    };
    let (debug, can, eth) = (iface("debug"), iface("can"), iface("ethernet"));
    let c = ItemRef::Component(comp);
    let kind = |k: &str| m.control_kind(k).expect("kind");
    let (sb, dp, sc) = (
        kind("secure-boot"),
        kind("debug-port-locked"),
        kind("secoc"),
    );

    assert!(p.control_kind_fits(sb, c));
    assert!(!p.control_kind_fits(sb, debug));
    assert!(!p.control_kind_fits(sb, can));

    assert!(p.control_kind_fits(dp, debug));
    assert!(!p.control_kind_fits(dp, can));
    assert!(!p.control_kind_fits(dp, c));

    assert!(p.control_kind_fits(sc, can));
    assert!(p.control_kind_fits(sc, eth));
    assert!(!p.control_kind_fits(sc, debug));
    assert!(!p.control_kind_fits(sc, c));

    let ghost_c = ItemRef::Component(ComponentId::new_v7());
    let ghost_i = ItemRef::Interface(InterfaceId::new_v7());
    assert!(!p.control_kind_fits(sb, ghost_c));
    assert!(!p.control_kind_fits(dp, ghost_i));
    assert!(!p.control_kind_fits(sc, ghost_i));
}
