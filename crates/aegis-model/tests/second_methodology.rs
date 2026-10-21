//! A second, non-automotive methodology declared purely as data.
//!
//! The definition is `common::second_def()` (a medical-device-style
//! methodology: three impact categories with scales of 3, 5, and 2 levels;
//! only the properties C, I, and A; a threat category that violates two
//! properties; and a CAPEC-style catalog scheme). It lives in the shared
//! test kit because the property tests replay random ops over it too. No
//! library code knows about it: supporting it needed test code only.
//!
//! This file applies every `Op` variant over it and checks that indices
//! handed out by the built-in methodology, which are out of range here, are
//! rejected.

mod common;

use std::sync::Arc;

use aegis_model::{Methodology, MethodologyDef, ModelError, Op, Project, PropertySet};
use common::{
    asset_id, builtin, cat, cref, damage_id, lvl, prop, props, scheme, second, second_def,
    threat_cat, threat_id,
};

fn ok(p: &mut Project, op: Op) -> Op {
    match p.apply(op.clone()) {
        Ok(inverse) => inverse,
        Err(e) => panic!("{op:?} failed: {e}"),
    }
}

fn rejects(p: &mut Project, op: Op, want: &ModelError) {
    let before = p.clone();
    assert_eq!(p.validate(&op).as_ref(), Err(want), "validate {op:?}");
    assert_eq!(p.apply(op.clone()).as_ref(), Err(want), "apply {op:?}");
    assert_eq!(*p, before);
}

#[test]
fn second_methodology_is_valid_data() {
    let def: MethodologyDef = second_def();
    let m = Methodology::new(def.clone()).expect("valid");
    assert_eq!(m.key(), "med-device-demo");
    let scales: Vec<_> = m.impact_categories().map(|c| m.level_count(c)).collect();
    assert_eq!(scales, [3, 5, 2]);
    let keys: Vec<_> = m.properties().filter_map(|p| m.property_key(p)).collect();
    assert_eq!(keys, ["c", "i", "a"]);
    let ransomware = threat_cat(&m, "ransomware");
    assert_eq!(m.violates(ransomware), props(&m, &["i", "a"]));
    assert_eq!(m.catalog_scheme_count(), 1);
    let capec = scheme(&m, "capec");
    assert_eq!(
        m.display_catalog_ref(&cref(&m, capec, "CAPEC-123"))
            .to_string(),
        "CAPEC-123"
    );
    assert!(m.parse_catalog_ref(capec, "4.1").is_err());
    // The same definition gives an equal methodology.
    assert_eq!(Methodology::new(def), Ok(m));
}

#[test]
fn second_methodology_every_op() {
    let m = second();
    let mut p = Project::new(Arc::clone(&m));
    let capec = scheme(&m, "capec");
    let harm = cat(&m, "patient-harm");
    let exposure = cat(&m, "data-exposure");
    let loss = cat(&m, "service-loss");
    let (pump, monitor) = (asset_id(1), asset_id(2));
    let (overdose, leak) = (damage_id(3), damage_id(4));
    let (lock, sniff) = (threat_id(5), threat_id(6));

    // Assets.
    ok(
        &mut p,
        Op::AddAsset {
            id: pump,
            name: "Infusion pump".into(),
            properties: props(&m, &["i", "a"]),
        },
    );
    ok(
        &mut p,
        Op::AddAsset {
            id: monitor,
            name: "Monitor".into(),
            properties: props(&m, &["c"]),
        },
    );
    ok(
        &mut p,
        Op::RenameAsset {
            id: monitor,
            name: "Bedside monitor".into(),
        },
    );
    ok(
        &mut p,
        Op::SetAssetProperties {
            id: monitor,
            properties: m.all_properties(),
        },
    );

    // Damage scenarios, rated on the 5-level and 2-level scales.
    ok(
        &mut p,
        Op::AddDamageScenario {
            id: overdose,
            name: "Overdose".into(),
            impacts: vec![
                (harm, lvl(&m, harm, "critical")),
                (loss, lvl(&m, loss, "intolerable")),
            ],
        },
    );
    ok(
        &mut p,
        Op::AddDamageScenario {
            id: leak,
            name: "Record leak".into(),
            impacts: vec![],
        },
    );
    ok(
        &mut p,
        Op::RenameDamageScenario {
            id: leak,
            name: "Patient record leak".into(),
        },
    );
    ok(
        &mut p,
        Op::SetImpact {
            id: leak,
            category: exposure,
            level: Some(lvl(&m, exposure, "extreme")),
        },
    );

    // Threats; ransomware violates two properties.
    ok(
        &mut p,
        Op::AddThreatScenario {
            id: lock,
            name: "Encrypt pump firmware".into(),
            asset: pump,
            category: threat_cat(&m, "ransomware"),
            catalog_refs: vec![cref(&m, capec, "CAPEC-549")],
            damages: vec![overdose],
        },
    );
    ok(
        &mut p,
        Op::AddThreatScenario {
            id: sniff,
            name: "Sniff telemetry".into(),
            asset: monitor,
            category: threat_cat(&m, "corruption"),
            catalog_refs: vec![],
            damages: vec![],
        },
    );
    ok(
        &mut p,
        Op::RenameThreatScenario {
            id: sniff,
            name: "Sniff patient telemetry".into(),
        },
    );
    ok(
        &mut p,
        Op::SetThreatCategory {
            id: sniff,
            category: threat_cat(&m, "data-leak"),
        },
    );
    ok(
        &mut p,
        Op::AddCatalogRef {
            threat: sniff,
            catalog_ref: cref(&m, capec, "CAPEC-157"),
        },
    );
    ok(
        &mut p,
        Op::LinkDamage {
            threat: sniff,
            damage: leak,
        },
    );

    // Check the state.
    let lock_idx = p.threat_scenario_idx(&lock).expect("lock");
    assert_eq!(p.targets(lock_idx), Some(props(&m, &["i", "a"])));
    let sniff_idx = p.threat_scenario_idx(&sniff).expect("sniff");
    assert_eq!(p.targets(sniff_idx), Some(props(&m, &["c"])));
    let d = p
        .damage_scenario(p.damage_scenario_idx(&leak).expect("leak"))
        .expect("live");
    assert_eq!(d.name(), "Patient record leak");
    assert_eq!(d.impacts().count(), 3);
    assert_eq!(d.impact(exposure), Some(lvl(&m, exposure, "extreme")));
    assert_eq!(d.impact(harm), None);
    let t = p.threat_scenario(sniff_idx).expect("live");
    assert_eq!(t.name(), "Sniff patient telemetry");
    assert_eq!(t.catalog_refs(), [cref(&m, capec, "CAPEC-157")]);
    assert_eq!(t.damages(), [p.damage_scenario_idx(&leak).expect("leak")]);

    // Ransomware relies on both I and A of the pump.
    rejects(
        &mut p,
        Op::SetAssetProperties {
            id: pump,
            properties: props(&m, &["i"]),
        },
        &ModelError::PropertyInUse {
            asset: pump,
            property: prop(&m, "a"),
            threats: 1,
        },
    );

    // Tear down with the remaining ops.
    ok(
        &mut p,
        Op::RemoveCatalogRef {
            threat: sniff,
            catalog_ref: cref(&m, capec, "CAPEC-157"),
        },
    );
    ok(
        &mut p,
        Op::UnlinkDamage {
            threat: sniff,
            damage: leak,
        },
    );
    ok(&mut p, Op::RemoveDamageScenario { id: leak });
    ok(&mut p, Op::RemoveThreatScenario { id: sniff });
    ok(&mut p, Op::RemoveThreatScenario { id: lock });
    ok(&mut p, Op::RemoveDamageScenario { id: overdose });
    ok(&mut p, Op::RemoveAsset { id: pump });
    ok(&mut p, Op::RemoveAsset { id: monitor });
    assert_eq!(p, Project::new(m));
}

#[test]
fn second_methodology_rejects_foreign_indices() {
    let m = second();
    let first = builtin();
    let mut p = Project::new(Arc::clone(&m));
    let pump = asset_id(1);
    let dmg = damage_id(2);
    let threat = threat_id(3);
    ok(
        &mut p,
        Op::AddAsset {
            id: pump,
            name: "Pump".into(),
            properties: m.all_properties(),
        },
    );
    ok(
        &mut p,
        Op::AddDamageScenario {
            id: dmg,
            name: "Harm".into(),
            impacts: vec![],
        },
    );
    ok(
        &mut p,
        Op::AddThreatScenario {
            id: threat,
            name: "Corrupt dose table".into(),
            asset: pump,
            category: threat_cat(&m, "corruption"),
            catalog_refs: vec![],
            damages: vec![],
        },
    );

    // Property index 5 (non-repudiation in the built-in methodology).
    let p5 = first.properties().nth(5).expect("property 5");
    rejects(
        &mut p,
        Op::SetAssetProperties {
            id: pump,
            properties: PropertySet::empty().with(p5),
        },
        &ModelError::UnknownProperty(p5),
    );
    // Level 3 ("severe") in the 3-level patient-harm scale.
    let harm = cat(&m, "patient-harm");
    let first_cat = first.impact_categories().next().expect("category");
    let level3 = first.levels(first_cat).nth(3).expect("level 3");
    rejects(
        &mut p,
        Op::SetImpact {
            id: dmg,
            category: harm,
            level: Some(level3),
        },
        &ModelError::UnknownLevel {
            category: harm,
            level: level3,
        },
    );
    // Level 3 is fine in the 5-level scale.
    ok(
        &mut p,
        Op::SetImpact {
            id: dmg,
            category: cat(&m, "data-exposure"),
            level: Some(level3),
        },
    );
    // Impact category index 3 (the fourth built-in category).
    let cat3 = first.impact_categories().nth(3).expect("category 3");
    rejects(
        &mut p,
        Op::AddDamageScenario {
            id: damage_id(9),
            name: "x".into(),
            impacts: vec![(cat3, level3)],
        },
        &ModelError::UnknownImpactCategory(cat3),
    );
    // Threat category index 5 (the sixth STRIDE category).
    let t5 = first.threat_categories().nth(5).expect("threat category 5");
    rejects(
        &mut p,
        Op::SetThreatCategory {
            id: threat,
            category: t5,
        },
        &ModelError::UnknownThreatCategory(t5),
    );
    // A two-segment ref from the built-in scheme does not fit the
    // one-segment CAPEC syntax that has the same index here.
    let annex5 = scheme(&first, "r155-annex5");
    let foreign = cref(&first, annex5, "32.99");
    rejects(
        &mut p,
        Op::AddCatalogRef {
            threat,
            catalog_ref: foreign,
        },
        &ModelError::InvalidCatalogId {
            scheme: scheme(&m, "capec"),
        },
    );
    // Ransomware needs I and A; an asset with only C cannot carry it.
    let c_only = asset_id(10);
    ok(
        &mut p,
        Op::AddAsset {
            id: c_only,
            name: "Display".into(),
            properties: props(&m, &["c"]),
        },
    );
    rejects(
        &mut p,
        Op::AddThreatScenario {
            id: threat_id(11),
            name: "x".into(),
            asset: c_only,
            category: threat_cat(&m, "ransomware"),
            catalog_refs: vec![],
            damages: vec![],
        },
        &ModelError::PropertyNotOnAsset {
            asset: c_only,
            property: prop(&m, "i"),
        },
    );
}
