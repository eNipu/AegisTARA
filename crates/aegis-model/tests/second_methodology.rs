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

use aegis_model::{
    Methodology, MethodologyDef, ModelError, Op, Project, PropertySet, Provenance, StepTarget,
};
use common::{
    asset_id, builtin, cat, cref, damage_id, prop, props, scheme, second, second_def, threat_cat,
    threat_id,
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
    assert_eq!(m.feasibility_levels().len(), 5);
    let fkeys: Vec<_> = m
        .feasibility_levels()
        .map(|(_, key, _)| key.to_string())
        .collect();
    assert_eq!(
        fkeys,
        ["improbable", "remote", "occasional", "probable", "frequent"]
    );
    let factor_scales: Vec<_> = m
        .feasibility_factors()
        .map(|f| m.factor_level_count(f))
        .collect();
    assert_eq!(factor_scales, [3, 4, 3]);
    assert_eq!(m.potential_bands().len(), 5);
    // The same definition gives an equal methodology.
    assert_eq!(Methodology::new(def), Ok(m));
}

#[test]
fn second_methodology_every_op() {
    common::second_every_op(second());
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
            provenance: Provenance::default(),
        },
    );
    ok(
        &mut p,
        Op::AddDamageScenario {
            id: dmg,
            name: "Harm".into(),
            impacts: vec![],
            provenance: Provenance::default(),
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
            provenance: Provenance::default(),
        },
    );

    // Property index 5 (non-repudiation in the built-in methodology).
    let p5 = first.properties().nth(5).expect("property 5");
    rejects(
        &mut p,
        Op::SetAssetProperties {
            id: pump,
            properties: PropertySet::empty().with(p5),
            provenance: Provenance::default(),
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
            provenance: Provenance::default(),
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
            provenance: Provenance::default(),
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
            provenance: Provenance::default(),
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
            provenance: Provenance::default(),
        },
        &ModelError::UnknownThreatCategory(t5),
    );
    // A two-segment ref from the built-in scheme does not fit the
    // one-segment CAPEC syntax that has the same index here.
    let annex5 = scheme(&first, "r155-annex5");
    let foreign = cref(&first, annex5, "32.99");
    let capec = scheme(&m, "capec");
    rejects(
        &mut p,
        Op::AddCatalogRef {
            threat,
            catalog_ref: foreign,
            provenance: Provenance::default(),
        },
        &ModelError::InvalidCatalogId {
            scheme: capec,
            syntax: m.catalog_syntax(capec).expect("capec syntax").clone(),
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
            provenance: Provenance::default(),
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
            provenance: Provenance::default(),
        },
        &ModelError::PropertyNotOnAsset {
            asset: c_only,
            property: prop(&m, "i"),
        },
    );
    // Feasibility level index 5 is out of range in the 5-level medical
    // feasibility scale.
    let tree = common::tree_id(12);
    let step = common::node_id(13);
    ok(
        &mut p,
        Op::AddAttackTree {
            id: tree,
            tree: aegis_model::AttackTree::new(step, aegis_model::Node::step("Step"))
                .expect("step"),
            step_feasibility: vec![],
            provenance: Provenance::default(),
        },
    );
    let f5 = common::wide()
        .feasibility_levels()
        .nth(5)
        .expect("feasibility 5")
        .0;
    rejects(
        &mut p,
        Op::SetStepFeasibility {
            target: StepTarget::Step(tree, step),
            level: Some(f5),
            provenance: Provenance::default(),
        },
        &ModelError::UnknownFeasibilityLevel(f5),
    );
    // Feasibility factor index 4 ("equipment" in the 5-factor built-in
    // methodology) is out of range in the 3-factor medical methodology.
    let factor4 = first.feasibility_factors().nth(4).expect("factor 4");
    rejects(
        &mut p,
        Op::SetStepFactor {
            target: StepTarget::Step(tree, step),
            factor: factor4,
            level: None,
            provenance: Provenance::default(),
        },
        &ModelError::UnknownFeasibilityFactor(factor4),
    );
    // Factor level index 3 is out of range in the 3-level "exposure" factor,
    // though valid in the 4-level "skill" factor.
    let exp = common::factor(&m, "exposure");
    let skill = common::factor(&m, "skill");
    let flvl3 = m.factor_levels(skill).nth(3).expect("level 3");
    rejects(
        &mut p,
        Op::SetStepFactor {
            target: StepTarget::Step(tree, step),
            factor: exp,
            level: Some(flvl3),
            provenance: Provenance::default(),
        },
        &ModelError::UnknownFactorLevel {
            factor: exp,
            level: flvl3,
        },
    );
    ok(
        &mut p,
        Op::SetStepFactor {
            target: StepTarget::Step(tree, step),
            factor: skill,
            level: Some(flvl3),
            provenance: Provenance::default(),
        },
    );
    rejects(
        &mut p,
        Op::RenameAsset {
            id: pump,
            name: "Pump".into(),
            provenance: Provenance::human("   ", 1),
        },
        &ModelError::EmptyActor,
    );
    rejects(
        &mut p,
        Op::RenameAsset {
            id: pump,
            name: "Pump".into(),
            provenance: Provenance::human("a".repeat(257), 1),
        },
        &ModelError::ActorTooLong { max: 256 },
    );
}
