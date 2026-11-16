mod common;

use std::collections::BTreeMap;
use std::sync::Arc;

use aegis_model::{
    ComponentId, ControlId, ControlKind, InterfaceId, ItemRef, LintCode, Methodology,
    MethodologyDef, Op, Project, Provenance,
};

fn methodology() -> Arc<Methodology> {
    let mut def =
        MethodologyDef::read_pack(Methodology::iso21434_r155_pack().as_bytes()).expect("pack");
    let low = Arc::clone(&def.feasibility_levels[0].key);
    // Need debug-access attack type present (it is). Use low level.
    let rule = |_: &str| -> BTreeMap<Arc<str>, Arc<str>> {
        let mut m = BTreeMap::new();
        m.insert("debug-access".into(), Arc::clone(&low));
        m
    };
    let mut presets = Vec::new();
    // fused-a, open-b for components
    for key in ["fused-a", "open-b"] {
        presets.push(aegis_model::ControlKindDef {
            key: key.into(),
            label: key.into(),
            interface_kinds: Vec::new(),
            rules: rule(key),
            conflicts_with: Vec::new(),
        });
    }
    // fused-c, open-d for debug interface
    for key in ["fused-c", "open-d"] {
        presets.push(aegis_model::ControlKindDef {
            key: key.into(),
            label: key.into(),
            interface_kinds: vec!["debug".into()],
            rules: rule(key),
            conflicts_with: Vec::new(),
        });
    }
    // set conflicts
    presets[0].conflicts_with = vec!["open-b".into()]; // fused-a lists open-b
    presets[3].conflicts_with = vec!["fused-c".into()]; // open-d lists fused-c
    def.element_names.control_kinds = presets;
    Arc::new(Methodology::new(def).expect("valid"))
}

// helper to add a preset setting
fn add_setting(p: &mut Project, id: ControlId, kind_idx: usize, target: ItemRef) {
    let m = p.methodology();
    let kinds: Vec<ControlKind> = m.control_kinds().collect();
    let kind = kinds[kind_idx];
    p.apply(Op::AddSecurityControl {
        id,
        name: "s".into(),
        provenance: Provenance::default(),
    })
    .expect("add ctrl");
    p.apply(Op::SetControlKind {
        control: id,
        kind: Some(kind),
        provenance: Provenance::default(),
    })
    .expect("set kind");
    p.apply(Op::ApplyControl {
        control: id,
        target,
        provenance: Provenance::default(),
    })
    .expect("apply");
}

#[test]
fn basic() {
    let m = methodology();
    let mut p = Project::new(m);
    let ecu = ComponentId::new_v7();
    p.apply(Op::AddComponent {
        id: ecu,
        name: "Gateway ECU".into(),
        parent: None,
        provenance: Provenance::default(),
    })
    .expect("comp");
    // interface
    let iface_kind = p.methodology().interface_kind("debug").expect("debug");
    let jtag = InterfaceId::new_v7();
    p.apply(Op::AddInterface {
        id: jtag,
        name: "JTAG".into(),
        component: ecu,
        kind: iface_kind,
        provenance: Provenance::default(),
    })
    .expect("iface");

    // fused-a idx 0, open-b idx 1, fused-c idx 2, open-d idx 3
    let c1 = ControlId::new_v7();
    let c2 = ControlId::new_v7();
    add_setting(&mut p, c1, 0, ItemRef::Component(ecu));
    add_setting(&mut p, c2, 1, ItemRef::Component(ecu));

    let findings: Vec<_> = p
        .lint()
        .into_iter()
        .filter(|f| matches!(f.code(), LintCode::ConflictingSettings { .. }))
        .collect();
    assert_eq!(findings.len(), 1);
    if let LintCode::ConflictingSettings {
        item,
        first,
        second,
    } = findings[0].code()
    {
        assert_eq!(item, ItemRef::Component(ecu));
        assert!(first.index() < second.index());
    }
    assert!(findings[0].is_warning());
    assert!(!findings[0].is_error());
}

#[test]
fn second_component_two_findings() {
    let m = methodology();
    let mut p = Project::new(m);
    let ecu = ComponentId::new_v7();
    let ecu2 = ComponentId::new_v7();
    p.apply(Op::AddComponent {
        id: ecu,
        name: "G1".into(),
        parent: None,
        provenance: Provenance::default(),
    })
    .unwrap();
    p.apply(Op::AddComponent {
        id: ecu2,
        name: "G2".into(),
        parent: None,
        provenance: Provenance::default(),
    })
    .unwrap();

    for ecu in [ecu, ecu2] {
        let c1 = ControlId::new_v7();
        let c2 = ControlId::new_v7();
        // add settings
        let m = p.methodology();
        let kinds: Vec<ControlKind> = m.control_kinds().collect();
        p.apply(Op::AddSecurityControl {
            id: c1,
            name: "s".into(),
            provenance: Provenance::default(),
        })
        .unwrap();
        p.apply(Op::SetControlKind {
            control: c1,
            kind: Some(kinds[0]),
            provenance: Provenance::default(),
        })
        .unwrap();
        p.apply(Op::ApplyControl {
            control: c1,
            target: ItemRef::Component(ecu),
            provenance: Provenance::default(),
        })
        .unwrap();
        p.apply(Op::AddSecurityControl {
            id: c2,
            name: "s".into(),
            provenance: Provenance::default(),
        })
        .unwrap();
        p.apply(Op::SetControlKind {
            control: c2,
            kind: Some(kinds[1]),
            provenance: Provenance::default(),
        })
        .unwrap();
        p.apply(Op::ApplyControl {
            control: c2,
            target: ItemRef::Component(ecu),
            provenance: Provenance::default(),
        })
        .unwrap();
    }

    let findings: Vec<_> = p
        .lint()
        .into_iter()
        .filter(|f| matches!(f.code(), LintCode::ConflictingSettings { .. }))
        .collect();
    assert_eq!(findings.len(), 2);
}

#[test]
fn no_findings_cases() {
    let m = methodology();
    let mut p = Project::new(m);
    let ecu = ComponentId::new_v7();
    let ecu2 = ComponentId::new_v7();
    p.apply(Op::AddComponent {
        id: ecu,
        name: "G1".into(),
        parent: None,
        provenance: Provenance::default(),
    })
    .unwrap();
    p.apply(Op::AddComponent {
        id: ecu2,
        name: "G2".into(),
        parent: None,
        provenance: Provenance::default(),
    })
    .unwrap();

    // only one of two
    let c1 = ControlId::new_v7();
    p.apply(Op::AddSecurityControl {
        id: c1,
        name: "s".into(),
        provenance: Provenance::default(),
    })
    .unwrap();
    let kinds: Vec<ControlKind> = p.methodology().control_kinds().collect();
    p.apply(Op::SetControlKind {
        control: c1,
        kind: Some(kinds[0]),
        provenance: Provenance::default(),
    })
    .unwrap();
    p.apply(Op::ApplyControl {
        control: c1,
        target: ItemRef::Component(ecu),
        provenance: Provenance::default(),
    })
    .unwrap();
    let findings: Vec<_> = p
        .lint()
        .into_iter()
        .filter(|f| matches!(f.code(), LintCode::ConflictingSettings { .. }))
        .collect();
    assert!(findings.is_empty());

    // two same kind
    let c2 = ControlId::new_v7();
    let c3 = ControlId::new_v7();
    p.apply(Op::AddSecurityControl {
        id: c2,
        name: "s".into(),
        provenance: Provenance::default(),
    })
    .unwrap();
    p.apply(Op::SetControlKind {
        control: c2,
        kind: Some(kinds[0]),
        provenance: Provenance::default(),
    })
    .unwrap();
    p.apply(Op::ApplyControl {
        control: c2,
        target: ItemRef::Component(ecu),
        provenance: Provenance::default(),
    })
    .unwrap();
    p.apply(Op::AddSecurityControl {
        id: c3,
        name: "s".into(),
        provenance: Provenance::default(),
    })
    .unwrap();
    p.apply(Op::SetControlKind {
        control: c3,
        kind: Some(kinds[0]),
        provenance: Provenance::default(),
    })
    .unwrap();
    p.apply(Op::ApplyControl {
        control: c3,
        target: ItemRef::Component(ecu),
        provenance: Provenance::default(),
    })
    .unwrap();
    let findings: Vec<_> = p
        .lint()
        .into_iter()
        .filter(|f| matches!(f.code(), LintCode::ConflictingSettings { .. }))
        .collect();
    assert!(findings.is_empty());

    // different components
    // presets do not conflict: use fused-a (0) and fused-c (2) (not conflicting) on same target
    let c4 = ControlId::new_v7();
    let c5 = ControlId::new_v7();
    p.apply(Op::AddSecurityControl {
        id: c4,
        name: "s".into(),
        provenance: Provenance::default(),
    })
    .unwrap();
    p.apply(Op::SetControlKind {
        control: c4,
        kind: Some(kinds[0]),
        provenance: Provenance::default(),
    })
    .unwrap();
    p.apply(Op::ApplyControl {
        control: c4,
        target: ItemRef::Component(ecu),
        provenance: Provenance::default(),
    })
    .unwrap();
    p.apply(Op::AddSecurityControl {
        id: c5,
        name: "s".into(),
        provenance: Provenance::default(),
    })
    .unwrap();
    p.apply(Op::SetControlKind {
        control: c5,
        kind: Some(kinds[2]),
        provenance: Provenance::default(),
    })
    .unwrap();
    p.apply(Op::ApplyControl {
        control: c5,
        target: ItemRef::Component(ecu2),
        provenance: Provenance::default(),
    })
    .unwrap();
    let findings: Vec<_> = p
        .lint()
        .into_iter()
        .filter(|f| matches!(f.code(), LintCode::ConflictingSettings { .. }))
        .collect();
    assert!(findings.is_empty());
}

#[test]
fn declared_higher_side_interface() {
    let m = methodology();
    let mut p = Project::new(m);
    let ecu = ComponentId::new_v7();
    p.apply(Op::AddComponent {
        id: ecu,
        name: "Gateway ECU".into(),
        parent: None,
        provenance: Provenance::default(),
    })
    .unwrap();
    let iface_kind = p.methodology().interface_kind("debug").unwrap();
    let jtag = InterfaceId::new_v7();
    p.apply(Op::AddInterface {
        id: jtag,
        name: "JTAG".into(),
        component: ecu,
        kind: iface_kind,
        provenance: Provenance::default(),
    })
    .unwrap();

    let kinds: Vec<ControlKind> = p.methodology().control_kinds().collect();
    // fused-c idx2, open-d idx3; open-d lists fused-c
    let c1 = ControlId::new_v7();
    let c2 = ControlId::new_v7();
    p.apply(Op::AddSecurityControl {
        id: c1,
        name: "s".into(),
        provenance: Provenance::default(),
    })
    .unwrap();
    p.apply(Op::SetControlKind {
        control: c1,
        kind: Some(kinds[2]),
        provenance: Provenance::default(),
    })
    .unwrap();
    p.apply(Op::ApplyControl {
        control: c1,
        target: ItemRef::Interface(jtag),
        provenance: Provenance::default(),
    })
    .unwrap();
    p.apply(Op::AddSecurityControl {
        id: c2,
        name: "s".into(),
        provenance: Provenance::default(),
    })
    .unwrap();
    p.apply(Op::SetControlKind {
        control: c2,
        kind: Some(kinds[3]),
        provenance: Provenance::default(),
    })
    .unwrap();
    p.apply(Op::ApplyControl {
        control: c2,
        target: ItemRef::Interface(jtag),
        provenance: Provenance::default(),
    })
    .unwrap();

    let findings: Vec<_> = p
        .lint()
        .into_iter()
        .filter(|f| matches!(f.code(), LintCode::ConflictingSettings { .. }))
        .collect();
    assert_eq!(findings.len(), 1);
    if let LintCode::ConflictingSettings {
        item,
        first,
        second,
    } = findings[0].code()
    {
        assert_eq!(item, ItemRef::Interface(jtag));
        assert!(first.index() < second.index());
    }
}

#[test]
fn both_sides_one_finding() {
    // modify methodology so both list each other
    let mut def =
        MethodologyDef::read_pack(Methodology::iso21434_r155_pack().as_bytes()).expect("pack");
    let low = Arc::clone(&def.feasibility_levels[0].key);
    let mut presets = Vec::new();
    for key in ["fused-a", "open-b"] {
        presets.push(aegis_model::ControlKindDef {
            key: key.into(),
            label: key.into(),
            interface_kinds: Vec::new(),
            rules: {
                let mut m = BTreeMap::new();
                m.insert("debug-access".into(), Arc::clone(&low));
                m
            },
            conflicts_with: Vec::new(),
        });
    }
    presets[0].conflicts_with = vec!["open-b".into()];
    presets[1].conflicts_with = vec!["fused-a".into()];
    def.element_names.control_kinds = presets;
    let m = Arc::new(Methodology::new(def).expect("valid"));

    let mut p = Project::new(m);
    let ecu = ComponentId::new_v7();
    p.apply(Op::AddComponent {
        id: ecu,
        name: "G".into(),
        parent: None,
        provenance: Provenance::default(),
    })
    .unwrap();
    let kinds: Vec<ControlKind> = p.methodology().control_kinds().collect();
    let c1 = ControlId::new_v7();
    let c2 = ControlId::new_v7();
    p.apply(Op::AddSecurityControl {
        id: c1,
        name: "s".into(),
        provenance: Provenance::default(),
    })
    .unwrap();
    p.apply(Op::SetControlKind {
        control: c1,
        kind: Some(kinds[0]),
        provenance: Provenance::default(),
    })
    .unwrap();
    p.apply(Op::ApplyControl {
        control: c1,
        target: ItemRef::Component(ecu),
        provenance: Provenance::default(),
    })
    .unwrap();
    p.apply(Op::AddSecurityControl {
        id: c2,
        name: "s".into(),
        provenance: Provenance::default(),
    })
    .unwrap();
    p.apply(Op::SetControlKind {
        control: c2,
        kind: Some(kinds[1]),
        provenance: Provenance::default(),
    })
    .unwrap();
    p.apply(Op::ApplyControl {
        control: c2,
        target: ItemRef::Component(ecu),
        provenance: Provenance::default(),
    })
    .unwrap();

    let findings: Vec<_> = p
        .lint()
        .into_iter()
        .filter(|f| matches!(f.code(), LintCode::ConflictingSettings { .. }))
        .collect();
    assert_eq!(findings.len(), 1);
}
