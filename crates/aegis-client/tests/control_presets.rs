//! Preset settings (job 124, CFG-06): created, refused and reset by commands,
//! one undo step each.

use std::sync::Arc;

use aegis_client::commands::{Edit, EditError};
use aegis_client::workspace::{Origin, Workspace};
use aegis_model::{ComponentId, ControlId, InterfaceId, ItemRef, Methodology, Op};

struct Fx {
    ws: Workspace,
    ecu: ComponentId,
    jtag: InterfaceId,
    can: InterfaceId,
}

fn run(ws: &mut Workspace, edit: Edit) {
    let ops = edit.to_ops(ws.project(), "alice", 1).expect("ops");
    ws.apply(ops, Origin::Edit).expect("applies");
}

fn fixture() -> Fx {
    let m = Arc::new(Methodology::iso21434_r155().expect("builtin"));
    let mut ws = Workspace::empty(m);
    let (ecu, jtag, can) = (
        ComponentId::new_v7(),
        InterfaceId::new_v7(),
        InterfaceId::new_v7(),
    );
    run(
        &mut ws,
        Edit::AddComponent {
            id: ecu,
            name: "ADAS ECU".into(),
            parent: None,
        },
    );
    for (id, name, kind) in [(jtag, "JTAG", 3), (can, "CAN-diag", 0)] {
        run(
            &mut ws,
            Edit::AddInterface {
                id,
                name: name.into(),
                component: ecu,
                kind,
            },
        );
    }
    Fx { ws, ecu, jtag, can }
}

fn preset(id: ControlId, kind: usize, name: &str, target: ItemRef) -> Edit {
    Edit::AddPresetSetting {
        id,
        kind,
        name: name.into(),
        target,
    }
}

fn err(fx: &Fx, edit: &Edit) -> EditError {
    edit.to_ops(fx.ws.project(), "alice", 1)
        .expect_err("refused")
}

#[test]
fn preset_setting_is_one_undo_step() {
    let mut fx = fixture();
    let id = ControlId::new_v7();
    let before = fx.ws.project().clone();
    run(
        &mut fx.ws,
        preset(id, 0, "Secure boot", ItemRef::Component(fx.ecu)),
    );
    let after = fx.ws.project().clone();
    let p = fx.ws.project();
    let idx = p.security_control_idx(&id).expect("control");
    let c = p.security_control(idx).expect("c");
    let m = p.methodology();
    let kind = m.control_kind("secure-boot").expect("kind");
    assert_eq!(c.name(), "Secure boot");
    assert_eq!(c.kind(), Some(kind));
    assert_eq!(c.rules(), m.control_kind_rules(kind));
    assert_eq!(
        p.control_targets(idx).collect::<Vec<_>>(),
        vec![ItemRef::Component(fx.ecu)]
    );
    assert!(!p.control_differs_from_kind(idx));
    fx.ws.undo().expect("undo");
    assert!(fx.ws.project() == &before);
    fx.ws.redo().expect("redo");
    assert!(fx.ws.project() == &after);
}

#[test]
fn name_comes_from_the_edit() {
    let mut fx = fixture();
    let id = ControlId::new_v7();
    run(
        &mut fx.ws,
        preset(id, 0, "セキュアブート", ItemRef::Component(fx.ecu)),
    );
    let p = fx.ws.project();
    let c = p
        .security_control_idx(&id)
        .and_then(|i| p.security_control(i))
        .expect("c");
    assert_eq!(c.name(), "セキュアブート");
}

#[test]
fn wrong_fits_are_refused_with_their_sentence() {
    let mut fx = fixture();
    let id = ControlId::new_v7();
    let before = fx.ws.project().clone();
    let e = err(&fx, &preset(id, 5, "x", ItemRef::Component(fx.ecu)));
    assert_eq!(
        e,
        EditError::PresetDoesNotFit(
            "\"Debug port locked\" is a setting for Debug interfaces; it cannot be applied to a component".into()
        )
    );
    assert_eq!(
        e.to_string(),
        "\"Debug port locked\" is a setting for Debug interfaces; it cannot be applied to a component"
    );
    let e = err(&fx, &preset(id, 5, "x", ItemRef::Interface(fx.can)));
    assert_eq!(
        e.to_string(),
        "\"Debug port locked\" is a setting for Debug interfaces; it cannot be applied to a CAN interface"
    );
    let e = err(&fx, &preset(id, 0, "x", ItemRef::Interface(fx.jtag)));
    assert_eq!(
        e.to_string(),
        "\"Secure boot\" is a setting for components; it cannot be applied to an interface"
    );
    assert_eq!(
        err(&fx, &preset(id, 99, "x", ItemRef::Component(fx.ecu))),
        EditError::UnknownChoice
    );
    assert_eq!(
        err(&fx, &preset(id, 0, "", ItemRef::Component(fx.ecu))),
        EditError::EmptyName
    );
    assert!(fx.ws.project() == &before);
    run(
        &mut fx.ws,
        preset(id, 5, "Debug port locked", ItemRef::Interface(fx.jtag)),
    );
    assert!(fx.ws.project().security_control_idx(&id).is_some());
}

#[test]
fn apply_control_checks_the_preset_but_not_custom_controls() {
    let mut fx = fixture();
    let (locked, custom) = (ControlId::new_v7(), ControlId::new_v7());
    run(
        &mut fx.ws,
        preset(locked, 5, "Debug port locked", ItemRef::Interface(fx.jtag)),
    );
    let e = err(
        &fx,
        &Edit::ApplyControl {
            control: locked,
            target: ItemRef::Component(fx.ecu),
        },
    );
    assert_eq!(
        e.to_string(),
        "\"Debug port locked\" is a setting for Debug interfaces; it cannot be applied to a component"
    );
    run(
        &mut fx.ws,
        Edit::AddCustomSetting {
            id: custom,
            name: "Custom".into(),
            target: ItemRef::Interface(fx.can),
            rules: vec![(0, 0)],
        },
    );
    run(
        &mut fx.ws,
        Edit::ApplyControl {
            control: custom,
            target: ItemRef::Component(fx.ecu),
        },
    );
}

fn rules_of(
    ws: &Workspace,
    id: ControlId,
) -> Vec<(aegis_model::AttackType, aegis_model::FeasibilityLevel)> {
    let p = ws.project();
    let c = p
        .security_control_idx(&id)
        .and_then(|i| p.security_control(i))
        .expect("c");
    c.rules()
        .iter()
        .map(|r| (r.attack_type(), r.ceiling()))
        .collect()
}

#[test]
fn use_pack_values_restores_the_preset_in_one_step() {
    let mut fx = fixture();
    let id = ControlId::new_v7();
    run(
        &mut fx.ws,
        preset(id, 0, "Secure boot", ItemRef::Component(fx.ecu)),
    );
    let pristine = Edit::UsePackValues { control: id }
        .to_ops(fx.ws.project(), "alice", 1)
        .expect("ops");
    assert!(pristine.is_empty());
    let m = fx.ws.project().methodology().clone();
    let kind = m.control_kind("secure-boot").expect("kind");
    let preset_rules = m.control_kind_rules(kind).to_vec();
    let pos = |t| m.attack_types().position(|x| x == t).expect("pos");
    let first = preset_rules[0].attack_type();
    let second = preset_rules[1].attack_type();
    let third = m
        .attack_types()
        .find(|t| preset_rules.iter().all(|r| r.attack_type() != *t))
        .expect("third");
    let other = m
        .feasibility_levels()
        .position(|(l, _, _)| l != preset_rules[0].ceiling())
        .expect("level");
    for e in [
        Edit::SetControlRule {
            control: id,
            attack_type: pos(first),
            level: Some(other),
        },
        Edit::SetControlRule {
            control: id,
            attack_type: pos(third),
            level: Some(0),
        },
        Edit::SetControlRule {
            control: id,
            attack_type: pos(second),
            level: None,
        },
    ] {
        run(&mut fx.ws, e);
    }
    let idx = fx.ws.project().security_control_idx(&id).expect("idx");
    assert!(fx.ws.project().control_differs_from_kind(idx));
    let changed = fx.ws.project().clone();
    run(&mut fx.ws, Edit::UsePackValues { control: id });
    let p = fx.ws.project();
    let c = p.security_control(idx).expect("c");
    assert_eq!(c.rules(), m.control_kind_rules(kind));
    assert!(!p.control_differs_from_kind(idx));
    fx.ws.undo().expect("undo");
    assert!(fx.ws.project() == &changed);

    // Custom control: nothing to do.
    let custom = ControlId::new_v7();
    run(
        &mut fx.ws,
        Edit::AddCustomSetting {
            id: custom,
            name: "Custom".into(),
            target: ItemRef::Component(fx.ecu),
            rules: vec![(0, 0)],
        },
    );
    assert!(
        Edit::UsePackValues { control: custom }
            .to_ops(fx.ws.project(), "alice", 1)
            .expect("ops")
            .is_empty()
    );
}

#[test]
fn use_pack_values_clears_before_it_sets() {
    let mut fx = fixture();
    let id = ControlId::new_v7();
    run(
        &mut fx.ws,
        preset(id, 0, "Secure boot", ItemRef::Component(fx.ecu)),
    );
    let n = fx.ws.project().methodology().attack_types().count();
    assert_eq!(n, 10);
    for pos in 0..n {
        run(
            &mut fx.ws,
            Edit::SetControlRule {
                control: id,
                attack_type: pos,
                level: Some(3),
            },
        );
    }
    let ops = Edit::UsePackValues { control: id }
        .to_ops(fx.ws.project(), "alice", 1)
        .expect("ops");
    let cleared = |o: &Op| matches!(o, Op::SetControlRule { ceiling: None, .. });
    let first_set = ops
        .iter()
        .position(|o| {
            matches!(
                o,
                Op::SetControlRule {
                    ceiling: Some(_),
                    ..
                }
            )
        })
        .expect("a set");
    assert!(ops[..first_set].iter().all(cleared));
    assert!(ops[first_set..].iter().all(|o| !cleared(o)));
    assert!(ops.iter().any(cleared));
    fx.ws.apply(ops, Origin::Edit).expect("applies");
    let m = fx.ws.project().methodology().clone();
    let kind = m.control_kind("secure-boot").expect("kind");
    let want: Vec<_> = m
        .control_kind_rules(kind)
        .iter()
        .map(|r| (r.attack_type(), r.ceiling()))
        .collect();
    assert_eq!(rules_of(&fx.ws, id), want);
}

#[test]
fn duplicate_and_remove_keep_the_kind() {
    let mut fx = fixture();
    let (id, copy) = (ControlId::new_v7(), ControlId::new_v7());
    run(
        &mut fx.ws,
        preset(id, 0, "Secure boot", ItemRef::Component(fx.ecu)),
    );
    run(
        &mut fx.ws,
        Edit::DuplicateControl {
            id: copy,
            source: id,
            suffix: " copy".into(),
        },
    );
    let p = fx.ws.project();
    let get = |i: ControlId| {
        p.security_control_idx(&i)
            .and_then(|x| p.security_control(x))
            .expect("c")
    };
    assert_eq!(get(copy).kind(), get(id).kind());
    assert!(get(copy).kind().is_some());
    assert_eq!(get(copy).rules(), get(id).rules());
    let before = fx.ws.project().clone();
    run(&mut fx.ws, Edit::RemoveControl { control: id });
    assert!(fx.ws.project().security_control_idx(&id).is_none());
    fx.ws.undo().expect("undo");
    assert!(fx.ws.project() == &before);
}

fn list(fx: &Fx, target: ItemRef, filter: &str, lang: &str) -> aegis_client::pages::AddSettingList {
    aegis_client::pages::add_setting_list(&fx.ws, target, filter, lang).expect("list")
}

fn kinds(l: &aegis_client::pages::AddSettingList) -> Vec<usize> {
    l.presets.iter().map(|p| p.kind).collect()
}

fn names(l: &aegis_client::pages::AddSettingList) -> Vec<&str> {
    l.presets.iter().map(|p| p.name.as_str()).collect()
}

#[test]
fn list_for_a_component_and_interfaces() {
    let fx = fixture();
    let ecu = ItemRef::Component(fx.ecu);
    let l = list(&fx, ecu, "", "en");
    assert_eq!(kinds(&l), vec![0, 1, 2, 3, 4]);
    assert_eq!(
        names(&l)[..2],
        ["Secure boot", "Secure flash (signed updates)"]
    );
    assert!(l.existing.is_empty());
    assert_eq!(l.custom, "");
    assert_eq!(l.pack, fx.ws.project().methodology().label());
    assert!(l.presets.iter().all(|p| !p.rules.is_empty()));
    let j = list(&fx, ItemRef::Interface(fx.jtag), "", "en");
    assert_eq!(kinds(&j), vec![5, 6]);
    let c = list(&fx, ItemRef::Interface(fx.can), "", "en");
    assert_eq!(kinds(&c), vec![7, 8, 9]);
    assert!(c.presets.iter().any(|p| p.name.contains("SecOC")));
}

#[test]
fn filter_is_trimmed_and_case_free() {
    let fx = fixture();
    let l = list(&fx, ItemRef::Component(fx.ecu), "  SEC ", "en");
    assert_eq!(names(&l), ["Secure boot", "Secure flash (signed updates)"]);
    assert_eq!(l.custom, "SEC");
}

#[test]
fn labels_follow_the_language() {
    let fx = fixture();
    let ecu = ItemRef::Component(fx.ecu);
    let ja = list(&fx, ecu, "セキュア", "ja");
    assert!(!ja.presets.is_empty());
    assert_eq!(ja.presets[0].name, "セキュアブート");
    assert!(ja.presets.iter().all(|p| p.name.contains("セキュア")));
    let fr = list(&fx, ecu, "secure boot", "fr");
    assert_eq!(names(&fr), ["Secure boot"]);
}

#[test]
fn created_presets_move_to_existing() {
    let mut fx = fixture();
    let ecu = ItemRef::Component(fx.ecu);
    let id = ControlId::new_v7();
    run(&mut fx.ws, preset(id, 0, "Secure boot", ecu));
    let same = list(&fx, ecu, "", "en");
    assert!(same.existing.is_empty());
    assert!(!kinds(&same).contains(&0));
    let other = ComponentId::new_v7();
    run(
        &mut fx.ws,
        Edit::AddComponent {
            id: other,
            name: "Gateway".into(),
            parent: None,
        },
    );
    let l = list(&fx, ItemRef::Component(other), "", "en");
    assert_eq!(l.existing.len(), 1);
    assert_eq!(l.existing[0].id, id);
    assert_eq!(l.existing[0].name, "Secure boot");
    assert_eq!(l.existing[0].applied_to, 1);
    assert!(!kinds(&l).contains(&0));
}

#[test]
fn existing_respects_the_fit() {
    let mut fx = fixture();
    let (locked, custom) = (ControlId::new_v7(), ControlId::new_v7());
    run(
        &mut fx.ws,
        preset(locked, 5, "Debug port locked", ItemRef::Interface(fx.jtag)),
    );
    run(
        &mut fx.ws,
        Edit::AddCustomSetting {
            id: custom,
            name: "Custom".into(),
            target: ItemRef::Interface(fx.jtag),
            rules: vec![(0, 0)],
        },
    );
    let l = list(&fx, ItemRef::Component(fx.ecu), "", "en");
    let ids: Vec<_> = l.existing.iter().map(|e| e.id).collect();
    assert_eq!(ids, vec![custom]);
}

#[test]
fn every_listed_preset_is_accepted() {
    let fx = fixture();
    for target in [
        ItemRef::Component(fx.ecu),
        ItemRef::Interface(fx.jtag),
        ItemRef::Interface(fx.can),
    ] {
        for p in list(&fx, target, "", "en").presets {
            let e = preset(ControlId::new_v7(), p.kind, &p.name, target);
            e.to_ops(fx.ws.project(), "alice", 1).expect("accepted");
        }
    }
}

#[test]
fn unknown_target_gives_none() {
    let fx = fixture();
    let gone = ItemRef::Component(ComponentId::new_v7());
    assert!(aegis_client::pages::add_setting_list(&fx.ws, gone, "", "en").is_none());
}

#[test]
fn control_detail_names_the_preset_and_the_difference() {
    let mut fx = fixture();
    let (id, custom) = (ControlId::new_v7(), ControlId::new_v7());
    run(
        &mut fx.ws,
        preset(id, 0, "Secure boot", ItemRef::Component(fx.ecu)),
    );
    let d = aegis_client::pages::control_detail(&fx.ws, id).expect("d");
    assert_eq!(d.preset.as_deref(), Some("Secure boot"));
    assert!(!d.differs_from_preset);
    let m = fx.ws.project().methodology().clone();
    let kind = m.control_kind("secure-boot").expect("kind");
    let first = m.control_kind_rules(kind)[0];
    let at = m
        .attack_types()
        .position(|t| t == first.attack_type())
        .expect("pos");
    let other = m
        .feasibility_levels()
        .position(|(l, _, _)| l != first.ceiling())
        .expect("level");
    run(
        &mut fx.ws,
        Edit::SetControlRule {
            control: id,
            attack_type: at,
            level: Some(other),
        },
    );
    let d = aegis_client::pages::control_detail(&fx.ws, id).expect("d");
    assert!(d.differs_from_preset);
    run(&mut fx.ws, Edit::UsePackValues { control: id });
    let d = aegis_client::pages::control_detail(&fx.ws, id).expect("d");
    assert!(!d.differs_from_preset);
    run(
        &mut fx.ws,
        Edit::AddCustomSetting {
            id: custom,
            name: "Mine".into(),
            target: ItemRef::Component(fx.ecu),
            rules: vec![(0, 0)],
        },
    );
    let d = aegis_client::pages::control_detail(&fx.ws, custom).expect("d");
    assert_eq!(d.preset, None);
    assert!(!d.differs_from_preset);
}
