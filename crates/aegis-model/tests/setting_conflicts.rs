//! Conflicting setting presets on one component or interface (CFG-09).

use std::sync::Arc;

use aegis_model::{
    ComponentId, ControlId, ControlKind, ControlKindDef, InterfaceId, ItemRef, LintCode,
    Methodology, MethodologyDef, Op, Project, Provenance,
};

/// Key, interface kinds (empty = for components), conflicts_with.
type Preset = (
    &'static str,
    &'static [&'static str],
    &'static [&'static str],
);

/// Index 0 lists 1 (declared on the lower side), 3 lists 2 (declared on
/// the higher side), 4 lists 1, so a target with 0, 1 and 4 has two pairs.
const PRESETS: &[Preset] = &[
    ("fused-a", &[], &["open-b"]),
    ("open-b", &[], &[]),
    ("fused-c", &["debug"], &[]),
    ("open-d", &["debug"], &["fused-c"]),
    ("sealed-e", &[], &["open-b"]),
];

struct Fx {
    p: Project,
    k: Vec<ControlKind>,
    ecu: ComponentId,
    ecu2: ComponentId,
    jtag: InterfaceId,
}

/// Two components, a debug interface on the first, and the given presets.
fn fixture(presets: &[Preset]) -> Fx {
    let mut def =
        MethodologyDef::read_pack(Methodology::iso21434_r155_pack().as_bytes()).expect("pack");
    let low = Arc::clone(&def.feasibility_levels[0].key);
    def.element_names.control_kinds = presets
        .iter()
        .map(|&(key, interfaces, conflicts)| ControlKindDef {
            key: key.into(),
            label: key.into(),
            interface_kinds: interfaces.iter().map(|&i| i.into()).collect(),
            rules: [("debug-access".into(), Arc::clone(&low))].into(),
            conflicts_with: conflicts.iter().map(|&c| c.into()).collect(),
        })
        .collect();
    let m = Arc::new(Methodology::new(def).expect("valid"));
    let k = m.control_kinds().collect();
    let debug = m.interface_kind("debug").expect("debug");
    let mut p = Project::new(m);
    let (ecu, ecu2, jtag) = (
        ComponentId::new_v7(),
        ComponentId::new_v7(),
        InterfaceId::new_v7(),
    );
    for (id, name) in [(ecu, "Gateway ECU"), (ecu2, "Telematics ECU")] {
        p.apply(Op::AddComponent {
            id,
            name: name.into(),
            parent: None,
            provenance: Provenance::default(),
        })
        .expect("component");
    }
    p.apply(Op::AddInterface {
        id: jtag,
        name: "JTAG".into(),
        component: ecu,
        kind: debug,
        provenance: Provenance::default(),
    })
    .expect("interface");
    Fx {
        p,
        k,
        ecu,
        ecu2,
        jtag,
    }
}

/// Applies a new setting to `target`; `kind` is `None` for a custom one.
fn add_setting(p: &mut Project, kind: Option<ControlKind>, target: ItemRef) {
    let id = ControlId::new_v7();
    p.apply(Op::AddSecurityControl {
        id,
        name: "s".into(),
        provenance: Provenance::default(),
    })
    .expect("add");
    if kind.is_some() {
        p.apply(Op::SetControlKind {
            control: id,
            kind,
            provenance: Provenance::default(),
        })
        .expect("kind");
    }
    p.apply(Op::ApplyControl {
        control: id,
        target,
        provenance: Provenance::default(),
    })
    .expect("apply");
}

/// Every conflicting-settings finding, in lint order (sorted).
fn conflicts(p: &Project) -> Vec<LintCode> {
    p.lint()
        .into_iter()
        .filter(|f| matches!(f.code(), LintCode::ConflictingSettings { .. }))
        .inspect(|f| assert!(f.is_warning() && !f.is_error(), "never a block"))
        .map(|f| f.code())
        .collect()
}

fn pair(item: ItemRef, first: ControlKind, second: ControlKind) -> LintCode {
    LintCode::ConflictingSettings {
        item,
        first,
        second,
    }
}

#[test]
fn both_presets_on_a_component_warn_once() {
    let mut fx = fixture(PRESETS);
    let ecu = ItemRef::Component(fx.ecu);
    // Applied higher index first: the finding is still lowest first.
    add_setting(&mut fx.p, Some(fx.k[1]), ecu);
    add_setting(&mut fx.p, Some(fx.k[0]), ecu);
    assert_eq!(conflicts(&fx.p), [pair(ecu, fx.k[0], fx.k[1])]);
}

#[test]
fn the_same_pair_on_two_targets_warns_once_per_target() {
    let mut fx = fixture(PRESETS);
    let (ecu, ecu2) = (ItemRef::Component(fx.ecu), ItemRef::Component(fx.ecu2));
    for target in [ecu, ecu2] {
        add_setting(&mut fx.p, Some(fx.k[0]), target);
        add_setting(&mut fx.p, Some(fx.k[1]), target);
    }
    let mut want = [pair(ecu, fx.k[0], fx.k[1]), pair(ecu2, fx.k[0], fx.k[1])];
    want.sort_unstable();
    assert_eq!(conflicts(&fx.p), want);
}

#[test]
fn no_warning_without_both_presets_on_one_target() {
    let mut fx = fixture(PRESETS);
    let (ecu, ecu2) = (ItemRef::Component(fx.ecu), ItemRef::Component(fx.ecu2));

    add_setting(&mut fx.p, Some(fx.k[0]), ecu);
    assert_eq!(conflicts(&fx.p), [], "only one of the pair");

    add_setting(&mut fx.p, Some(fx.k[0]), ecu);
    assert_eq!(conflicts(&fx.p), [], "two settings of the same preset");

    add_setting(&mut fx.p, Some(fx.k[1]), ecu2);
    assert_eq!(conflicts(&fx.p), [], "the pair on different targets");

    add_setting(&mut fx.p, None, ecu);
    add_setting(&mut fx.p, None, ecu2);
    assert_eq!(conflicts(&fx.p), [], "a custom setting has no preset");

    add_setting(&mut fx.p, Some(fx.k[4]), ecu);
    assert_eq!(conflicts(&fx.p), [], "presets that do not conflict");

    // The fixture can fire: completing the pair on one target warns.
    add_setting(&mut fx.p, Some(fx.k[0]), ecu2);
    assert_eq!(conflicts(&fx.p), [pair(ecu2, fx.k[0], fx.k[1])]);
}

#[test]
fn declared_on_the_higher_side_warns_on_an_interface() {
    let mut fx = fixture(PRESETS);
    let jtag = ItemRef::Interface(fx.jtag);
    add_setting(&mut fx.p, Some(fx.k[2]), jtag);
    add_setting(&mut fx.p, Some(fx.k[3]), jtag);
    assert_eq!(conflicts(&fx.p), [pair(jtag, fx.k[2], fx.k[3])]);
}

#[test]
fn declared_on_both_sides_warns_once() {
    let mut fx = fixture(&[("fused-a", &[], &["open-b"]), ("open-b", &[], &["fused-a"])]);
    let ecu = ItemRef::Component(fx.ecu);
    add_setting(&mut fx.p, Some(fx.k[0]), ecu);
    add_setting(&mut fx.p, Some(fx.k[1]), ecu);
    assert_eq!(conflicts(&fx.p), [pair(ecu, fx.k[0], fx.k[1])]);
}

#[test]
fn two_conflicting_pairs_on_one_target_warn_once_per_pair() {
    let mut fx = fixture(PRESETS);
    let ecu = ItemRef::Component(fx.ecu);
    for i in [0, 1, 4] {
        add_setting(&mut fx.p, Some(fx.k[i]), ecu);
    }
    assert_eq!(
        conflicts(&fx.p),
        [pair(ecu, fx.k[0], fx.k[1]), pair(ecu, fx.k[1], fx.k[4])]
    );
}
