//! Job 125 (CFG-07): which settings cap the step that decides a risk, and how
//! many risks a control lowers, built through `Edit` commands only.

use std::sync::Arc;

use aegis_client::commands::Edit;
use aegis_client::pages::{
    self, control_rows, lowers_label, reduced_by_cell, reduced_by_text, risk_rows,
};
use aegis_client::workspace::{Origin, Workspace};
use aegis_model::{
    AssetId, ComponentId, ControlId, DamageId, ItemRef, Methodology, NodeId, ThreatId, TreeId,
};

fn apply(ws: &mut Workspace, edit: &Edit) {
    let ops = edit
        .to_ops(ws.project(), "mira", 1_792_956_600_000)
        .expect("builds");
    ws.apply(ops, Origin::Edit).expect("applies");
}

struct Fx {
    ws: Workspace,
    threat: ThreatId,
    tree: TreeId,
    flash: NodeId,
    ecu: ComponentId,
    top: usize,
}

/// One threat with an OR tree: a flash step rated high, going via the ECU,
/// and an inject step rated `inject`.
fn fixture(inject: usize) -> Fx {
    let m = Arc::new(Methodology::iso21434_r155().expect("builtin"));
    let vocab = pages::vocabulary(&m);
    let mut ws = Workspace::empty(Arc::clone(&m));
    let (asset, damage, threat, tree, ecu) = (
        AssetId::new_v7(),
        DamageId::new_v7(),
        ThreatId::new_v7(),
        TreeId::new_v7(),
        ComponentId::new_v7(),
    );
    let top = vocab.feasibility.len() - 1;
    apply(
        &mut ws,
        &Edit::AddAsset {
            id: asset,
            name: "Firmware".into(),
            properties: (0..vocab.properties.len()).collect(),
        },
    );
    apply(
        &mut ws,
        &Edit::AddDamage {
            id: damage,
            name: "Vehicle misbehaves".into(),
        },
    );
    let safety = vocab
        .impact_categories
        .iter()
        .position(|c| c.label.eq_ignore_ascii_case("safety"))
        .expect("safety");
    apply(
        &mut ws,
        &Edit::SetImpact {
            damage,
            category: safety,
            level: Some(vocab.impact_categories[safety].levels.len() - 1),
        },
    );
    apply(
        &mut ws,
        &Edit::AddThreat {
            id: threat,
            name: "Firmware replaced".into(),
            asset,
            category: 0,
        },
    );
    apply(
        &mut ws,
        &Edit::LinkDamage {
            threat,
            damage,
            linked: true,
        },
    );
    apply(
        &mut ws,
        &Edit::NewTree {
            id: tree,
            root: "Goal".into(),
            threat: Some(threat),
        },
    );
    apply(
        &mut ws,
        &Edit::ReplaceTree {
            tree,
            outline: "OR Goal\n  - Flash firmware over USB\n  - Inject on CAN\n".into(),
        },
    );
    apply(
        &mut ws,
        &Edit::AddComponent {
            id: ecu,
            name: "ADAS ECU".into(),
            parent: None,
        },
    );
    let t = ws.project().attack_tree_idx(&tree).expect("tree");
    let nodes: Vec<(NodeId, String)> = ws
        .project()
        .attack_tree(t)
        .expect("tree")
        .iter()
        .map(|(_, id, n)| (id, n.label().to_owned()))
        .collect();
    let find = |s: &str| nodes.iter().find(|(_, l)| l == s).expect("step").0;
    let (flash, inj) = (find("Flash firmware over USB"), find("Inject on CAN"));
    for (node, level) in [(flash, top), (inj, inject)] {
        apply(
            &mut ws,
            &Edit::SetStepFeasibility {
                tree,
                node,
                level: Some(level),
            },
        );
    }
    apply(
        &mut ws,
        &Edit::SetStepVia {
            tree,
            node: flash,
            via: Some(ItemRef::Component(ecu)),
        },
    );
    apply(
        &mut ws,
        &Edit::SetStepAttackType {
            tree,
            node: flash,
            attack_type: Some(0),
        },
    );
    Fx {
        ws,
        threat,
        tree,
        flash,
        ecu,
        top,
    }
}

fn add_setting(f: &mut Fx, rules: Vec<(usize, usize)>) -> ControlId {
    let id = ControlId::new_v7();
    apply(
        &mut f.ws,
        &Edit::AddCustomSetting {
            id,
            name: "Secure boot".into(),
            target: ItemRef::Component(f.ecu),
            rules,
        },
    );
    id
}

fn add_signed(f: &mut Fx) -> ControlId {
    let id = ControlId::new_v7();
    apply(
        &mut f.ws,
        &Edit::AddControl {
            id,
            name: "Signed updates".into(),
        },
    );
    apply(
        &mut f.ws,
        &Edit::SetStepMitigation {
            tree: f.tree,
            node: f.flash,
            control: id,
            level: Some(2),
        },
    );
    id
}

fn row(f: &Fx) -> pages::RiskRow {
    risk_rows(&f.ws)
        .into_iter()
        .find(|r| r.id == f.threat)
        .expect("row")
}

#[test]
fn settings_on_the_deciding_step_come_deciding_first() {
    let mut f = fixture(0);
    add_setting(&mut f, vec![(0, 1)]);
    add_signed(&mut f);
    let r = row(&f);
    let names: Vec<&str> = r.reduced_by.iter().map(|e| e.name.as_str()).collect();
    let on: Vec<&str> = r.reduced_by.iter().map(|e| e.on.as_str()).collect();
    let ceil: Vec<&str> = r.reduced_by.iter().map(|e| e.ceiling.as_str()).collect();
    assert_eq!(names, ["Secure boot", "Signed updates"]);
    assert_eq!(on, ["ADAS ECU", ""]);
    assert_eq!(ceil, ["Low", "Medium"]);
    let d = r.deciding_step.as_ref().expect("deciding");
    assert_eq!(d.label, "Flash firmware over USB");
    assert_eq!((d.tree, d.node), (f.tree, f.flash));
    assert!(r.lowered);
    assert_eq!(reduced_by_cell(&r), "Secure boot +1");
    assert_eq!(
        reduced_by_text(&r).as_deref(),
        Some("Secure boot (ADAS ECU); Signed updates")
    );
}

#[test]
fn lowers_counts_the_risks_a_control_is_named_on() {
    let mut f = fixture(0);
    add_setting(&mut f, vec![(0, 1)]);
    add_signed(&mut f);
    apply(
        &mut f.ws,
        &Edit::AddControl {
            id: ControlId::new_v7(),
            name: "Unused".into(),
        },
    );
    let lowers = |name: &str| {
        control_rows(&f.ws)
            .into_iter()
            .find(|c| c.name == name)
            .expect("control")
            .lowers
    };
    assert_eq!(lowers("Secure boot"), 1);
    assert_eq!(lowers("Signed updates"), 1);
    assert_eq!(lowers("Unused"), 0);
}

#[test]
fn an_uncapped_deciding_step_is_named() {
    let mut f = fixture(2);
    add_setting(&mut f, vec![(0, 1)]);
    let r = row(&f);
    assert!(r.reduced_by.is_empty());
    assert_eq!(
        r.deciding_step.as_ref().expect("step").label,
        "Inject on CAN"
    );
    assert_eq!(reduced_by_cell(&r), "\u{2014} (Inject on CAN)");
    assert_eq!(reduced_by_text(&r), None);
    let sb = control_rows(&f.ws)
        .into_iter()
        .find(|c| c.name == "Secure boot")
        .expect("control");
    assert_eq!(sb.lowers, 0);
}

#[test]
fn a_ceiling_that_caps_nothing_is_not_listed() {
    let mut f = fixture(0);
    let top = f.top;
    add_setting(&mut f, vec![(0, top)]);
    let r = row(&f);
    assert!(r.reduced_by.is_empty());
    assert!(!r.lowered);
    assert_eq!(
        r.deciding_step.as_ref().expect("step").label,
        "Flash firmware over USB"
    );
}

#[test]
fn a_control_that_reaches_a_step_twice_is_listed_once() {
    let mut f = fixture(0);
    let usb = aegis_model::InterfaceId::new_v7();
    apply(
        &mut f.ws,
        &Edit::AddInterface {
            id: usb,
            name: "USB".into(),
            component: f.ecu,
            kind: 0,
        },
    );
    apply(
        &mut f.ws,
        &Edit::SetStepVia {
            tree: f.tree,
            node: f.flash,
            via: Some(ItemRef::Interface(usb)),
        },
    );
    let c = add_setting(&mut f, vec![(0, 1)]);
    apply(
        &mut f.ws,
        &Edit::ApplyControl {
            control: c,
            target: ItemRef::Interface(usb),
        },
    );
    assert_eq!(row(&f).reduced_by.len(), 1);
}

#[test]
fn a_threat_without_a_tree_has_no_reduced_by() {
    let mut f = fixture(0);
    let second = ThreatId::new_v7();
    let asset = f.ws.project().assets().next().map(|(_, id, _)| id);
    apply(
        &mut f.ws,
        &Edit::AddThreat {
            id: second,
            name: "Other".into(),
            asset: asset.expect("asset"),
            category: 0,
        },
    );
    apply(
        &mut f.ws,
        &Edit::SetThreatFeasibility {
            threat: second,
            level: Some(1),
        },
    );
    let r = risk_rows(&f.ws)
        .into_iter()
        .find(|r| r.id == second)
        .expect("row");
    assert!(r.reduced_by.is_empty());
    assert_eq!(r.deciding_step, None);
    assert_eq!(reduced_by_cell(&r), "\u{2014}");
}

#[test]
fn lowers_label_has_three_forms() {
    assert_eq!(lowers_label(0), "Lowers no risk");
    assert_eq!(lowers_label(1), "Lowers 1 risk");
    assert_eq!(lowers_label(3), "Lowers 3 risks");
}
