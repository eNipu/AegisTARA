//! `Project` through the public API, over the built-in ISO/SAE 21434 +
//! UN R155 methodology: a worked example, every rejection, referential
//! integrity, inverse round trips, deterministic order, and errors that
//! never contain names.

mod common;

use std::collections::BTreeSet;
use std::sync::Arc;

use aegis_model::{
    AssetId, CatalogRef, DamageId, Methodology, ModelError, Op, Project, PropertySet, ThreatId,
};
use common::{
    SENTINEL, asset_id, builtin, cat, cref, damage_id, lvl, prop, props, scheme, threat_cat,
    threat_id, variant, wide,
};

/// Applies `op` and expects success; returns the inverse.
fn ok(p: &mut Project, op: Op) -> Op {
    match p.apply(op.clone()) {
        Ok(inverse) => inverse,
        Err(e) => panic!("{op:?} failed: {e}"),
    }
}

/// Expects `op` to fail with `want` from both `validate` and `apply`, and
/// the project to stay equal to its clone from before.
fn rejects(p: &mut Project, op: Op, want: &ModelError) {
    let before = p.clone();
    assert_eq!(p.validate(&op).as_ref(), Err(want), "validate {op:?}");
    assert_eq!(p.apply(op.clone()).as_ref(), Err(want), "apply {op:?}");
    assert_eq!(*p, before, "project changed by failed {op:?}");
}

fn name(s: &str) -> Arc<str> {
    s.into()
}

// ---------------------------------------------------------------------------
// AC6: worked example.
// ---------------------------------------------------------------------------

#[test]
fn worked_example() {
    let m = builtin();
    let mut p = Project::new(Arc::clone(&m));
    let annex5 = scheme(&m, "r155-annex5");

    let gateway = asset_id(1);
    let brake = asset_id(2);
    ok(
        &mut p,
        Op::AddAsset {
            id: gateway,
            name: name("Central gateway"),
            properties: m.all_properties(),
        },
    );
    ok(
        &mut p,
        Op::AddAsset {
            id: brake,
            name: name("Brake ECU firmware"),
            properties: props(&m, &["integrity", "availability"]),
        },
    );

    let (safety, financial, operational, privacy) = (
        cat(&m, "safety"),
        cat(&m, "financial"),
        cat(&m, "operational"),
        cat(&m, "privacy"),
    );
    let no_brakes = damage_id(10);
    let data_leak = damage_id(11);
    ok(
        &mut p,
        Op::AddDamageScenario {
            id: no_brakes,
            name: name("Vehicle cannot brake"),
            impacts: vec![
                (safety, lvl(&m, safety, "severe")),
                (financial, lvl(&m, financial, "moderate")),
                (operational, lvl(&m, operational, "major")),
                (privacy, lvl(&m, privacy, "negligible")),
            ],
        },
    );
    // Partly unrated: only privacy is rated.
    ok(
        &mut p,
        Op::AddDamageScenario {
            id: data_leak,
            name: name("Driver location leaked"),
            impacts: vec![(privacy, lvl(&m, privacy, "major"))],
        },
    );

    let tamper = threat_id(20);
    let flood = threat_id(21);
    // Damages and refs listed out of order on purpose.
    ok(
        &mut p,
        Op::AddThreatScenario {
            id: tamper,
            name: name("Reflash brake firmware"),
            asset: brake,
            category: threat_cat(&m, "tampering"),
            catalog_refs: vec![cref(&m, annex5, "5"), cref(&m, annex5, "4.1")],
            damages: vec![data_leak, no_brakes],
        },
    );
    ok(
        &mut p,
        Op::AddThreatScenario {
            id: flood,
            name: name("Flood the gateway"),
            asset: gateway,
            category: threat_cat(&m, "denial-of-service"),
            catalog_refs: vec![],
            damages: vec![],
        },
    );
    // Link out of order too.
    ok(
        &mut p,
        Op::LinkDamage {
            threat: flood,
            damage: data_leak,
        },
    );
    ok(
        &mut p,
        Op::LinkDamage {
            threat: flood,
            damage: no_brakes,
        },
    );
    ok(
        &mut p,
        Op::AddCatalogRef {
            threat: flood,
            catalog_ref: cref(&m, annex5, "4"),
        },
    );

    // Counts and assets.
    assert_eq!(
        (
            p.asset_count(),
            p.damage_scenario_count(),
            p.threat_scenario_count()
        ),
        (2, 2, 2)
    );
    let b = p.asset(p.asset_idx(&brake).expect("brake")).expect("live");
    assert_eq!(b.name(), "Brake ECU firmware");
    assert_eq!(b.properties(), props(&m, &["integrity", "availability"]));
    let g_idx = p.asset_idx(&gateway).expect("gateway");
    assert_eq!(p.asset_id(g_idx), Some(gateway));
    assert_eq!(
        p.asset(g_idx).map(|a| a.properties()),
        Some(m.all_properties())
    );

    // Damage scenarios: unrated is None, not the lowest level.
    let d = p
        .damage_scenario(p.damage_scenario_idx(&data_leak).expect("leak"))
        .expect("live");
    assert_eq!(d.name(), "Driver location leaked");
    assert_eq!(d.impact(safety), None);
    assert_eq!(d.impact(financial), None);
    assert_eq!(d.impact(privacy), Some(lvl(&m, privacy, "major")));
    let low = lvl(&m, safety, "negligible");
    assert!(
        d.impacts()
            .filter(|&(c, _)| c != privacy)
            .all(|(_, l)| l.is_none() && l != Some(low))
    );
    let full = p
        .damage_scenario(p.damage_scenario_idx(&no_brakes).expect("brakes"))
        .expect("live");
    assert_eq!(full.impact(privacy), Some(lvl(&m, privacy, "negligible")));
    assert_eq!(full.impacts().filter(|(_, l)| l.is_some()).count(), 4);

    // Threats: derived targets equal the STRIDE mapping; sorted lists.
    let sorted_damages = {
        let mut v = vec![no_brakes, data_leak];
        v.sort();
        v
    };
    for (id, asset, category, refs) in [
        (tamper, brake, "tampering", vec!["4.1", "5"]),
        (flood, gateway, "denial-of-service", vec!["4"]),
    ] {
        let idx = p.threat_scenario_idx(&id).expect("threat");
        assert_eq!(p.threat_scenario_id(idx), Some(id));
        let t = p.threat_scenario(idx).expect("live");
        assert_eq!(p.asset_id(t.asset()), Some(asset));
        assert_eq!(t.category(), threat_cat(&m, category));
        let shown: Vec<_> = t
            .catalog_refs()
            .iter()
            .map(|r| m.display_catalog_ref(r).to_string())
            .collect();
        assert_eq!(shown, refs);
        let linked: Vec<_> = t
            .damages()
            .iter()
            .map(|&d| p.damage_scenario_id(d).expect("live"))
            .collect();
        assert_eq!(linked, sorted_damages);
    }
    let t_idx = p.threat_scenario_idx(&tamper).expect("tamper");
    assert_eq!(p.targets(t_idx), Some(props(&m, &["integrity"])));
    let f_idx = p.threat_scenario_idx(&flood).expect("flood");
    assert_eq!(p.targets(f_idx), Some(props(&m, &["availability"])));
    assert_eq!(
        p.threat_scenario(t_idx).map(|t| t.name()),
        Some("Reflash brake firmware")
    );
}

// ---------------------------------------------------------------------------
// Fixture shared by the rejection, inverse, and label tests.
// ---------------------------------------------------------------------------

/// IDs and vocabulary of the fixture project.
struct Fx {
    m: Arc<Methodology>,
    /// Carries every property; targeted by `t1` and `t_full`.
    a_all: AssetId,
    /// Carries only integrity; targeted by nothing.
    a_int: AssetId,
    /// Rated in two categories; linked by `t1`.
    d1: DamageId,
    /// Unrated; not linked.
    d2: DamageId,
    /// Tampering on `a_all`, one ref (4.1), linked to `d1`.
    t1: ThreatId,
    /// Spoofing on `a_all` with 16 refs.
    t_full: ThreatId,
    r41: CatalogRef,
    r5: CatalogRef,
    /// An unused ID of each kind.
    fresh_a: AssetId,
    fresh_d: DamageId,
    fresh_t: ThreatId,
}

/// Builds the fixture with every entity named `name`.
fn fixture(name: &str) -> (Project, Fx) {
    let m = builtin();
    let mut p = Project::new(Arc::clone(&m));
    let annex5 = scheme(&m, "r155-annex5");
    let n = || -> Arc<str> { name.into() };
    let fx = Fx {
        a_all: asset_id(1),
        a_int: asset_id(2),
        d1: damage_id(3),
        d2: damage_id(4),
        t1: threat_id(5),
        t_full: threat_id(6),
        r41: cref(&m, annex5, "4.1"),
        r5: cref(&m, annex5, "5"),
        fresh_a: asset_id(100),
        fresh_d: damage_id(101),
        fresh_t: threat_id(102),
        m: Arc::clone(&m),
    };
    ok(
        &mut p,
        Op::AddAsset {
            id: fx.a_all,
            name: n(),
            properties: m.all_properties(),
        },
    );
    ok(
        &mut p,
        Op::AddAsset {
            id: fx.a_int,
            name: n(),
            properties: props(&m, &["integrity"]),
        },
    );
    let safety = cat(&m, "safety");
    let privacy = cat(&m, "privacy");
    ok(
        &mut p,
        Op::AddDamageScenario {
            id: fx.d1,
            name: n(),
            impacts: vec![
                (safety, lvl(&m, safety, "major")),
                (privacy, lvl(&m, privacy, "negligible")),
            ],
        },
    );
    ok(
        &mut p,
        Op::AddDamageScenario {
            id: fx.d2,
            name: n(),
            impacts: vec![],
        },
    );
    ok(
        &mut p,
        Op::AddThreatScenario {
            id: fx.t1,
            name: n(),
            asset: fx.a_all,
            category: threat_cat(&m, "tampering"),
            catalog_refs: vec![fx.r41],
            damages: vec![fx.d1],
        },
    );
    let sixteen: Vec<_> = (1..=16)
        .map(|i| cref(&m, annex5, &format!("1.{i}")))
        .collect();
    ok(
        &mut p,
        Op::AddThreatScenario {
            id: fx.t_full,
            name: n(),
            asset: fx.a_all,
            category: threat_cat(&m, "spoofing"),
            catalog_refs: sixteen,
            damages: vec![],
        },
    );
    (p, fx)
}

/// Every new `ModelError` variant, each with a project and an op that
/// triggers it. Entities are named `name`.
fn cases(name: &str) -> Vec<(Project, Op, ModelError)> {
    let (p, fx) = fixture(name);
    let m = &fx.m;
    let w = wide();
    let n = || -> Arc<str> { name.into() };
    let annex5 = scheme(m, "r155-annex5");
    let safety = cat(m, "safety");
    let tampering = threat_cat(m, "tampering");
    // Out-of-range indices, drawn from a larger methodology.
    let bad_cat = w.impact_categories().nth(6).expect("wide category");
    let bad_level = w.levels(bad_cat).nth(9).expect("wide level");
    let bad_prop = w.properties().nth(9).expect("wide property");
    let bad_threat_cat = w.threat_categories().nth(10).expect("wide threat category");
    let wide_scheme_0 = w.catalog_schemes().next().expect("wide scheme");
    let wide_scheme_2 = w.catalog_schemes().nth(2).expect("wide scheme");
    let foreign_scheme_ref = w.parse_catalog_ref(wide_scheme_2, "1").expect("valid");
    // Index 0 is the R155 scheme here, but 50 is out of its range.
    let out_of_syntax_ref = w.parse_catalog_ref(wide_scheme_0, "50").expect("valid");
    let add_threat = |id, catalog_refs: Vec<CatalogRef>| Op::AddThreatScenario {
        id,
        name: n(),
        asset: fx.a_all,
        category: tampering,
        catalog_refs,
        damages: vec![],
    };
    let mut full = Project::with_slot_limit(Arc::clone(m), 1);
    ok(
        &mut full,
        Op::AddAsset {
            id: fx.a_all,
            name: n(),
            properties: m.all_properties(),
        },
    );

    vec![
        (
            p.clone(),
            Op::AddAsset {
                id: fx.a_all,
                name: n(),
                properties: m.all_properties(),
            },
            ModelError::DuplicateAsset(fx.a_all),
        ),
        (
            p.clone(),
            Op::AddDamageScenario {
                id: fx.d1,
                name: n(),
                impacts: vec![],
            },
            ModelError::DuplicateDamageScenario(fx.d1),
        ),
        (
            p.clone(),
            add_threat(fx.t1, vec![]),
            ModelError::DuplicateThreatScenario(fx.t1),
        ),
        (
            p.clone(),
            Op::RenameAsset {
                id: fx.fresh_a,
                name: n(),
            },
            ModelError::UnknownAsset(fx.fresh_a),
        ),
        (
            p.clone(),
            Op::SetImpact {
                id: fx.fresh_d,
                category: safety,
                level: None,
            },
            ModelError::UnknownDamageScenario(fx.fresh_d),
        ),
        (
            p.clone(),
            Op::RemoveThreatScenario { id: fx.fresh_t },
            ModelError::UnknownThreatScenario(fx.fresh_t),
        ),
        (
            p.clone(),
            Op::RenameThreatScenario {
                id: fx.t1,
                name: " \t ".into(),
            },
            ModelError::EmptyName,
        ),
        (
            p.clone(),
            Op::RenameDamageScenario {
                id: fx.d1,
                name: format!("{name}{}", "x".repeat(257)).into(),
            },
            ModelError::NameTooLong { max: 256 },
        ),
        (
            p.clone(),
            Op::SetImpact {
                id: fx.d1,
                category: bad_cat,
                level: None,
            },
            ModelError::UnknownImpactCategory(bad_cat),
        ),
        (
            p.clone(),
            Op::AddDamageScenario {
                id: fx.fresh_d,
                name: n(),
                impacts: vec![(safety, bad_level)],
            },
            ModelError::UnknownLevel {
                category: safety,
                level: bad_level,
            },
        ),
        (
            p.clone(),
            Op::SetAssetProperties {
                id: fx.a_int,
                properties: PropertySet::empty().with(bad_prop),
            },
            ModelError::UnknownProperty(bad_prop),
        ),
        (
            p.clone(),
            Op::SetThreatCategory {
                id: fx.t1,
                category: bad_threat_cat,
            },
            ModelError::UnknownThreatCategory(bad_threat_cat),
        ),
        (
            p.clone(),
            Op::AddCatalogRef {
                threat: fx.t1,
                catalog_ref: foreign_scheme_ref,
            },
            ModelError::UnknownCatalogScheme(wide_scheme_2),
        ),
        (
            p.clone(),
            add_threat(fx.fresh_t, vec![out_of_syntax_ref]),
            ModelError::InvalidCatalogId { scheme: annex5 },
        ),
        (
            p.clone(),
            Op::AddAsset {
                id: fx.fresh_a,
                name: n(),
                properties: PropertySet::empty(),
            },
            ModelError::NoSecurityProperties(fx.fresh_a),
        ),
        (
            p.clone(),
            Op::AddThreatScenario {
                id: fx.fresh_t,
                name: n(),
                asset: fx.a_int,
                category: threat_cat(m, "spoofing"),
                catalog_refs: vec![],
                damages: vec![],
            },
            ModelError::PropertyNotOnAsset {
                asset: fx.a_int,
                property: prop(m, "authenticity"),
            },
        ),
        (
            p.clone(),
            Op::SetAssetProperties {
                id: fx.a_all,
                properties: props(m, &["confidentiality", "availability"]),
            },
            ModelError::PropertyInUse {
                asset: fx.a_all,
                property: prop(m, "integrity"),
                threats: 1,
            },
        ),
        (
            p.clone(),
            Op::RemoveAsset { id: fx.a_all },
            ModelError::AssetInUse {
                asset: fx.a_all,
                threats: 2,
            },
        ),
        (
            p.clone(),
            Op::RemoveDamageScenario { id: fx.d1 },
            ModelError::DamageScenarioInUse {
                damage: fx.d1,
                threats: 1,
            },
        ),
        (
            p.clone(),
            Op::AddDamageScenario {
                id: fx.fresh_d,
                name: n(),
                impacts: vec![
                    (safety, lvl(m, safety, "major")),
                    (safety, lvl(m, safety, "severe")),
                ],
            },
            ModelError::DuplicateImpactCategory(safety),
        ),
        (
            p.clone(),
            Op::LinkDamage {
                threat: fx.t1,
                damage: fx.d1,
            },
            ModelError::DuplicateDamageLink {
                threat: fx.t1,
                damage: fx.d1,
            },
        ),
        (
            p.clone(),
            Op::UnlinkDamage {
                threat: fx.t1,
                damage: fx.d2,
            },
            ModelError::DamageNotLinked {
                threat: fx.t1,
                damage: fx.d2,
            },
        ),
        (
            p.clone(),
            Op::RemoveCatalogRef {
                threat: fx.t1,
                catalog_ref: fx.r5,
            },
            ModelError::CatalogRefNotFound { threat: fx.t1 },
        ),
        (
            p.clone(),
            Op::AddCatalogRef {
                threat: fx.t1,
                catalog_ref: fx.r41,
            },
            ModelError::DuplicateCatalogRef { threat: fx.t1 },
        ),
        (
            p.clone(),
            Op::AddCatalogRef {
                threat: fx.t_full,
                catalog_ref: fx.r5,
            },
            ModelError::TooManyCatalogRefs {
                threat: fx.t_full,
                max: 16,
            },
        ),
        (
            full,
            Op::AddAsset {
                id: fx.fresh_a,
                name: n(),
                properties: m.all_properties(),
            },
            ModelError::ProjectCapacityExceeded,
        ),
    ]
}

/// The new `ModelError` variants of this job.
const NEW_VARIANTS: [&str; 26] = [
    "DuplicateAsset",
    "DuplicateDamageScenario",
    "DuplicateThreatScenario",
    "UnknownAsset",
    "UnknownDamageScenario",
    "UnknownThreatScenario",
    "EmptyName",
    "NameTooLong",
    "UnknownImpactCategory",
    "UnknownLevel",
    "UnknownProperty",
    "UnknownThreatCategory",
    "UnknownCatalogScheme",
    "InvalidCatalogId",
    "NoSecurityProperties",
    "PropertyNotOnAsset",
    "PropertyInUse",
    "AssetInUse",
    "DamageScenarioInUse",
    "DuplicateImpactCategory",
    "DuplicateDamageLink",
    "DamageNotLinked",
    "CatalogRefNotFound",
    "DuplicateCatalogRef",
    "TooManyCatalogRefs",
    "ProjectCapacityExceeded",
];

// ---------------------------------------------------------------------------
// AC8: rejections.
// ---------------------------------------------------------------------------

#[test]
fn rejects_every_new_variant() {
    let mut seen = BTreeSet::new();
    for (mut p, op, want) in cases("fixture") {
        rejects(&mut p, op, &want);
        seen.insert(variant(&want));
    }
    let all: BTreeSet<String> = NEW_VARIANTS.iter().map(|s| (*s).to_string()).collect();
    assert_eq!(seen, all);
}

#[test]
fn rejects_duplicate_catalog_ref_and_damage_inside_one_op() {
    let (mut p, fx) = fixture("x");
    let op = Op::AddThreatScenario {
        id: fx.fresh_t,
        name: "x".into(),
        asset: fx.a_all,
        category: threat_cat(&fx.m, "tampering"),
        catalog_refs: vec![fx.r5, fx.r41, fx.r5],
        damages: vec![],
    };
    rejects(
        &mut p,
        op,
        &ModelError::DuplicateCatalogRef { threat: fx.fresh_t },
    );
    let op = Op::AddThreatScenario {
        id: fx.fresh_t,
        name: "x".into(),
        asset: fx.a_all,
        category: threat_cat(&fx.m, "tampering"),
        catalog_refs: vec![],
        damages: vec![fx.d2, fx.d1, fx.d2],
    };
    rejects(
        &mut p,
        op,
        &ModelError::DuplicateDamageLink {
            threat: fx.fresh_t,
            damage: fx.d2,
        },
    );
}

#[test]
fn rejects_too_many_refs_on_add_and_unknown_targets_on_add_threat() {
    let (mut p, fx) = fixture("x");
    let annex5 = scheme(&fx.m, "r155-annex5");
    let seventeen: Vec<_> = (1..=17)
        .map(|i| cref(&fx.m, annex5, &format!("2.{i}")))
        .collect();
    let base = |catalog_refs, damages, asset| Op::AddThreatScenario {
        id: fx.fresh_t,
        name: "x".into(),
        asset,
        category: threat_cat(&fx.m, "tampering"),
        catalog_refs,
        damages,
    };
    rejects(
        &mut p,
        base(seventeen, vec![], fx.a_all),
        &ModelError::TooManyCatalogRefs {
            threat: fx.fresh_t,
            max: 16,
        },
    );
    rejects(
        &mut p,
        base(vec![], vec![fx.fresh_d], fx.a_all),
        &ModelError::UnknownDamageScenario(fx.fresh_d),
    );
    rejects(
        &mut p,
        base(vec![], vec![], fx.fresh_a),
        &ModelError::UnknownAsset(fx.fresh_a),
    );
}

#[test]
fn rejects_names_on_every_kind() {
    let (mut p, fx) = fixture("x");
    let long: Arc<str> = "é".repeat(257).into();
    for name in [Arc::<str>::from(""), long.clone()] {
        let want = if name.is_empty() {
            ModelError::EmptyName
        } else {
            ModelError::NameTooLong { max: 256 }
        };
        rejects(
            &mut p,
            Op::AddAsset {
                id: fx.fresh_a,
                name: name.clone(),
                properties: fx.m.all_properties(),
            },
            &want,
        );
        rejects(
            &mut p,
            Op::AddDamageScenario {
                id: fx.fresh_d,
                name: name.clone(),
                impacts: vec![],
            },
            &want,
        );
        rejects(
            &mut p,
            Op::RenameAsset {
                id: fx.a_int,
                name: name.clone(),
            },
            &want,
        );
    }
    // 256 chars is the limit.
    ok(
        &mut p,
        Op::RenameAsset {
            id: fx.a_int,
            name: "é".repeat(256).into(),
        },
    );
}

// ---------------------------------------------------------------------------
// AC9: referential integrity.
// ---------------------------------------------------------------------------

#[test]
fn referential_integrity() {
    let (mut p, fx) = fixture("x");
    let m = Arc::clone(&fx.m);

    // While referenced: rejected.
    rejects(
        &mut p,
        Op::RemoveAsset { id: fx.a_all },
        &ModelError::AssetInUse {
            asset: fx.a_all,
            threats: 2,
        },
    );
    rejects(
        &mut p,
        Op::RemoveDamageScenario { id: fx.d1 },
        &ModelError::DamageScenarioInUse {
            damage: fx.d1,
            threats: 1,
        },
    );
    // Dropping integrity (targeted by t1, tampering) is rejected; dropping
    // an untargeted property is fine.
    rejects(
        &mut p,
        Op::SetAssetProperties {
            id: fx.a_all,
            properties: props(&m, &["authenticity"]),
        },
        &ModelError::PropertyInUse {
            asset: fx.a_all,
            property: prop(&m, "integrity"),
            threats: 1,
        },
    );
    let undo = ok(
        &mut p,
        Op::SetAssetProperties {
            id: fx.a_all,
            properties: props(&m, &["integrity", "authenticity"]),
        },
    );
    // Recategorising t1 to a category the asset cannot satisfy.
    rejects(
        &mut p,
        Op::SetThreatCategory {
            id: fx.t1,
            category: threat_cat(&m, "information-disclosure"),
        },
        &ModelError::PropertyNotOnAsset {
            asset: fx.a_all,
            property: prop(&m, "confidentiality"),
        },
    );
    ok(&mut p, undo);
    ok(
        &mut p,
        Op::SetThreatCategory {
            id: fx.t1,
            category: threat_cat(&m, "information-disclosure"),
        },
    );

    // After recategorising t1 to information disclosure, integrity is no
    // longer targeted, so it can be dropped.
    ok(
        &mut p,
        Op::SetAssetProperties {
            id: fx.a_all,
            properties: props(&m, &["confidentiality", "authenticity"]),
        },
    );
    // And confidentiality now is.
    rejects(
        &mut p,
        Op::SetAssetProperties {
            id: fx.a_all,
            properties: props(&m, &["authenticity"]),
        },
        &ModelError::PropertyInUse {
            asset: fx.a_all,
            property: prop(&m, "confidentiality"),
            threats: 1,
        },
    );

    // After removing the threats, the asset and damage can go.
    ok(
        &mut p,
        Op::UnlinkDamage {
            threat: fx.t1,
            damage: fx.d1,
        },
    );
    ok(&mut p, Op::RemoveDamageScenario { id: fx.d1 });
    ok(&mut p, Op::RemoveThreatScenario { id: fx.t1 });
    rejects(
        &mut p,
        Op::RemoveAsset { id: fx.a_all },
        &ModelError::AssetInUse {
            asset: fx.a_all,
            threats: 1,
        },
    );
    ok(&mut p, Op::RemoveThreatScenario { id: fx.t_full });
    ok(
        &mut p,
        Op::SetAssetProperties {
            id: fx.a_all,
            properties: props(&m, &["availability"]),
        },
    );
    ok(&mut p, Op::RemoveAsset { id: fx.a_all });
    assert_eq!(p.asset_count(), 1);
    assert_eq!(p.damage_scenario_count(), 1);
    assert_eq!(p.threat_scenario_count(), 0);
}

#[test]
fn referential_integrity_counts_threats_per_property() {
    let (mut p, fx) = fixture("x");
    let m = Arc::clone(&fx.m);
    let second = threat_id(200);
    ok(
        &mut p,
        Op::AddThreatScenario {
            id: second,
            name: "x".into(),
            asset: fx.a_all,
            category: threat_cat(&m, "tampering"),
            catalog_refs: vec![],
            damages: vec![fx.d1],
        },
    );
    let drop_integrity = Op::SetAssetProperties {
        id: fx.a_all,
        properties: props(&m, &["authenticity"]),
    };
    rejects(
        &mut p,
        drop_integrity.clone(),
        &ModelError::PropertyInUse {
            asset: fx.a_all,
            property: prop(&m, "integrity"),
            threats: 2,
        },
    );
    rejects(
        &mut p,
        Op::RemoveDamageScenario { id: fx.d1 },
        &ModelError::DamageScenarioInUse {
            damage: fx.d1,
            threats: 2,
        },
    );
    ok(&mut p, Op::RemoveThreatScenario { id: second });
    ok(&mut p, Op::RemoveThreatScenario { id: fx.t1 });
    ok(&mut p, drop_integrity);
}

// ---------------------------------------------------------------------------
// AC10: inverse round trips.
// ---------------------------------------------------------------------------

#[test]
fn inverse_round_trip() {
    let (base, fx) = fixture("x");
    let m = Arc::clone(&fx.m);
    let annex5 = scheme(&m, "r155-annex5");
    let safety = cat(&m, "safety");
    let financial = cat(&m, "financial");
    let mut sorted = vec![fx.d1, fx.d2];
    sorted.sort();
    // Ops in canonical form (sorted lists, impacts in category order), so
    // the inverse of the inverse is the op itself.
    let ops = vec![
        Op::AddAsset {
            id: fx.fresh_a,
            name: "new".into(),
            properties: props(&m, &["integrity", "availability"]),
        },
        Op::RenameAsset {
            id: fx.a_int,
            name: "renamed".into(),
        },
        Op::SetAssetProperties {
            id: fx.a_int,
            properties: props(&m, &["integrity", "authorization"]),
        },
        Op::RemoveAsset { id: fx.a_int },
        Op::AddDamageScenario {
            id: fx.fresh_d,
            name: "new".into(),
            impacts: vec![
                (safety, lvl(&m, safety, "negligible")),
                (financial, lvl(&m, financial, "severe")),
            ],
        },
        Op::RenameDamageScenario {
            id: fx.d1,
            name: "renamed".into(),
        },
        Op::SetImpact {
            id: fx.d1,
            category: safety,
            level: Some(lvl(&m, safety, "severe")),
        },
        Op::SetImpact {
            id: fx.d1,
            category: safety,
            level: None,
        },
        Op::SetImpact {
            id: fx.d2,
            category: financial,
            level: Some(lvl(&m, financial, "moderate")),
        },
        Op::RemoveDamageScenario { id: fx.d2 },
        Op::AddThreatScenario {
            id: fx.fresh_t,
            name: "new".into(),
            asset: fx.a_all,
            category: threat_cat(&m, "repudiation"),
            catalog_refs: vec![cref(&m, annex5, "3"), cref(&m, annex5, "3.7")],
            damages: sorted.clone(),
        },
        Op::RenameThreatScenario {
            id: fx.t1,
            name: "renamed".into(),
        },
        Op::SetThreatCategory {
            id: fx.t1,
            category: threat_cat(&m, "elevation-of-privilege"),
        },
        Op::AddCatalogRef {
            threat: fx.t1,
            catalog_ref: fx.r5,
        },
        Op::RemoveCatalogRef {
            threat: fx.t1,
            catalog_ref: fx.r41,
        },
        Op::LinkDamage {
            threat: fx.t1,
            damage: fx.d2,
        },
        Op::UnlinkDamage {
            threat: fx.t1,
            damage: fx.d1,
        },
        Op::RemoveThreatScenario { id: fx.t1 },
        Op::RemoveThreatScenario { id: fx.t_full },
        // Setting a value to what it already is succeeds with an equal
        // inverse.
        Op::RenameAsset {
            id: fx.a_all,
            name: "x".into(),
        },
        Op::SetThreatCategory {
            id: fx.t1,
            category: threat_cat(&m, "tampering"),
        },
    ];
    let mut kinds = BTreeSet::new();
    for op in ops {
        let mut p = base.clone();
        let inverse = ok(&mut p, op.clone());
        let after = p.clone();
        let inverse_of_inverse = ok(&mut p, inverse.clone());
        assert_eq!(p, base, "{op:?} then {inverse:?}");
        assert_eq!(inverse_of_inverse, op, "inverse of inverse of {op:?}");
        // Redo gives the same state again.
        ok(&mut p, inverse_of_inverse);
        assert_eq!(p, after);
        kinds.insert(variant(&op));
    }
    assert_eq!(kinds.len(), 16, "every Op variant: {kinds:?}");
}

#[test]
fn inverse_of_no_op_is_equal_op() {
    let (mut p, fx) = fixture("same");
    let before = p.clone();
    let op = Op::RenameDamageScenario {
        id: fx.d2,
        name: "same".into(),
    };
    assert_eq!(ok(&mut p, op.clone()), op);
    let op = Op::SetAssetProperties {
        id: fx.a_all,
        properties: fx.m.all_properties(),
    };
    assert_eq!(ok(&mut p, op.clone()), op);
    assert_eq!(p, before);
}

// ---------------------------------------------------------------------------
// AC13: determinism.
// ---------------------------------------------------------------------------

#[test]
fn deterministic_order() {
    let m = builtin();
    let annex5 = scheme(&m, "r155-annex5");
    let (a1, a2, a3) = (asset_id(30), asset_id(10), asset_id(20));
    let (d1, d2, d3) = (damage_id(31), damage_id(11), damage_id(21));
    let (t1, t2) = (threat_id(32), threat_id(12));
    let r = |t: &str| cref(&m, annex5, t);
    let all = m.all_properties();
    let add_asset = |id| Op::AddAsset {
        id,
        name: "asset".into(),
        properties: all,
    };
    let add_damage = |id| Op::AddDamageScenario {
        id,
        name: "damage".into(),
        impacts: vec![],
    };

    // Order one: everything in one go.
    let mut one = Project::new(Arc::clone(&m));
    for op in [add_asset(a1), add_asset(a2), add_asset(a3)] {
        ok(&mut one, op);
    }
    for op in [add_damage(d1), add_damage(d2), add_damage(d3)] {
        ok(&mut one, op);
    }
    ok(
        &mut one,
        Op::AddThreatScenario {
            id: t1,
            name: "threat".into(),
            asset: a1,
            category: threat_cat(&m, "spoofing"),
            catalog_refs: vec![r("7"), r("4.2"), r("4")],
            damages: vec![d1, d3, d2],
        },
    );
    ok(
        &mut one,
        Op::AddThreatScenario {
            id: t2,
            name: "threat".into(),
            asset: a3,
            category: threat_cat(&m, "tampering"),
            catalog_refs: vec![],
            damages: vec![d2],
        },
    );

    // Order two: reversed adds, links and refs one at a time, with a
    // removed and re-added entity (a tombstone) along the way.
    let mut two = Project::new(Arc::clone(&m));
    for op in [add_damage(d3), add_damage(d2), add_damage(d1)] {
        ok(&mut two, op);
    }
    for op in [add_asset(a3), add_asset(a1)] {
        ok(&mut two, op);
    }
    ok(&mut two, Op::RemoveDamageScenario { id: d2 });
    ok(&mut two, add_damage(d2));
    ok(&mut two, add_asset(a2));
    ok(
        &mut two,
        Op::AddThreatScenario {
            id: t2,
            name: "first name".into(),
            asset: a3,
            category: threat_cat(&m, "spoofing"),
            catalog_refs: vec![],
            damages: vec![d2],
        },
    );
    ok(
        &mut two,
        Op::AddThreatScenario {
            id: t1,
            name: "threat".into(),
            asset: a1,
            category: threat_cat(&m, "spoofing"),
            catalog_refs: vec![r("4")],
            damages: vec![d2],
        },
    );
    for d in [d3, d1] {
        ok(
            &mut two,
            Op::LinkDamage {
                threat: t1,
                damage: d,
            },
        );
    }
    for t in ["7", "4.2"] {
        ok(
            &mut two,
            Op::AddCatalogRef {
                threat: t1,
                catalog_ref: r(t),
            },
        );
    }
    ok(
        &mut two,
        Op::RenameThreatScenario {
            id: t2,
            name: "threat".into(),
        },
    );
    ok(
        &mut two,
        Op::SetThreatCategory {
            id: t2,
            category: threat_cat(&m, "tampering"),
        },
    );

    assert_eq!(one, two);
    let ids = |p: &Project| {
        (
            p.assets().map(|(_, id, _)| id).collect::<Vec<_>>(),
            p.damage_scenarios()
                .map(|(_, id, _)| id)
                .collect::<Vec<_>>(),
            p.threat_scenarios()
                .map(|(_, id, _)| id)
                .collect::<Vec<_>>(),
        )
    };
    let (assets, damages, threats) = ids(&one);
    assert_eq!(
        ids(&two),
        (assets.clone(), damages.clone(), threats.clone())
    );
    assert!(assets.windows(2).all(|w| w[0] < w[1]));
    assert!(damages.windows(2).all(|w| w[0] < w[1]));
    assert!(threats.windows(2).all(|w| w[0] < w[1]));
    assert_eq!(assets, [a2, a3, a1]);

    // The stored lists agree too, by public ID.
    for p in [&one, &two] {
        let t = p
            .threat_scenario(p.threat_scenario_idx(&t1).expect("t1"))
            .expect("live");
        let linked: Vec<_> = t
            .damages()
            .iter()
            .map(|&d| p.damage_scenario_id(d).expect("live"))
            .collect();
        assert_eq!(linked, [d2, d3, d1]);
        let shown: Vec<_> = t
            .catalog_refs()
            .iter()
            .map(|r| m.display_catalog_ref(r).to_string())
            .collect();
        assert_eq!(shown, ["4", "4.2", "7"]);
    }
}

#[test]
fn equality_compares_methodology_content() {
    let a = Project::new(builtin());
    let b = Project::new(builtin());
    assert_eq!(a, b, "equal content, different Arc");
    let c = Project::new(common::second());
    assert_ne!(a, c);
}

// ---------------------------------------------------------------------------
// AC14: errors never contain names.
// ---------------------------------------------------------------------------

#[test]
fn errors_have_no_labels() {
    let mut seen = BTreeSet::new();
    for (mut p, op, want) in cases(SENTINEL) {
        // Every entity in the fixture carries the sentinel name.
        assert!(p.assets().all(|(_, _, a)| a.name().contains(SENTINEL)));
        assert!(
            p.damage_scenarios()
                .all(|(_, _, d)| d.name().contains(SENTINEL))
        );
        assert!(
            p.threat_scenarios()
                .all(|(_, _, t)| t.name().contains(SENTINEL))
        );
        let err = p.apply(op).expect_err("case must fail");
        assert_eq!(err, want);
        let display = err.to_string();
        let debug = format!("{err:?}");
        assert!(!display.contains(SENTINEL), "{display}");
        assert!(!debug.contains(SENTINEL), "{debug}");
        assert!(!display.is_empty());
        seen.insert(variant(&err));
    }
    assert_eq!(seen.len(), NEW_VARIANTS.len());
}
