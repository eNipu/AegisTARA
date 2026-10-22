//! `Project` through the public API, over the built-in ISO/SAE 21434 +
//! UN R155 methodology: a worked example, every rejection, referential
//! integrity, inverse round trips, deterministic order, and errors that
//! never contain names.

mod common;

use std::collections::BTreeSet;
use std::sync::Arc;

use aegis_model::{
    AssetId, AttackTree, CatalogRef, DamageId, Derivation, GateKind, Methodology, ModelError, Node,
    NodeId, Op, Project, PropertySet, Provenance, ProvenanceSource, ThreatId, TreeId,
};
use common::{
    SENTINEL, asset_id, builtin, cat, cref, damage_id, feas, lvl, node_id, prop, props, scheme,
    second, threat_cat, threat_id, tree_id, variant, wide,
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
            provenance: Provenance::default(),
        },
    );
    ok(
        &mut p,
        Op::AddAsset {
            id: brake,
            name: name("Brake ECU firmware"),
            properties: props(&m, &["integrity", "availability"]),
            provenance: Provenance::default(),
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
            provenance: Provenance::default(),
        },
    );
    // Partly unrated: only privacy is rated.
    ok(
        &mut p,
        Op::AddDamageScenario {
            id: data_leak,
            name: name("Driver location leaked"),
            impacts: vec![(privacy, lvl(&m, privacy, "major"))],
            provenance: Provenance::default(),
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
            provenance: Provenance::default(),
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
            provenance: Provenance::default(),
        },
    );
    // Link out of order too.
    ok(
        &mut p,
        Op::LinkDamage {
            threat: flood,
            damage: data_leak,
            provenance: Provenance::default(),
        },
    );
    ok(
        &mut p,
        Op::LinkDamage {
            threat: flood,
            damage: no_brakes,
            provenance: Provenance::default(),
        },
    );
    ok(
        &mut p,
        Op::AddCatalogRef {
            threat: flood,
            catalog_ref: cref(&m, annex5, "4"),
            provenance: Provenance::default(),
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
    /// Spoofing on `a_all` with 16 refs, linked to `tree1`.
    t_full: ThreatId,
    /// Attack tree with OR gate `tree1_root` and step `tree1_step`, linked by `t_full`.
    tree1: TreeId,
    tree1_root: NodeId,
    tree1_step: NodeId,
    /// Unlinked attack tree with a single step `tree_free_step`.
    tree_free: TreeId,
    tree_free_step: NodeId,
    r41: CatalogRef,
    r5: CatalogRef,
    /// An unused ID of each kind.
    fresh_a: AssetId,
    fresh_d: DamageId,
    fresh_t: ThreatId,
    fresh_tree: TreeId,
    fresh_node: NodeId,
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
        tree1: tree_id(7),
        tree1_root: node_id(8),
        tree1_step: node_id(9),
        tree_free: tree_id(10),
        tree_free_step: node_id(11),
        r41: cref(&m, annex5, "4.1"),
        r5: cref(&m, annex5, "5"),
        fresh_a: asset_id(100),
        fresh_d: damage_id(101),
        fresh_t: threat_id(102),
        fresh_tree: tree_id(103),
        fresh_node: node_id(104),
        m: Arc::clone(&m),
    };
    ok(
        &mut p,
        Op::AddAsset {
            id: fx.a_all,
            name: n(),
            properties: m.all_properties(),
            provenance: Provenance::default(),
        },
    );
    ok(
        &mut p,
        Op::AddAsset {
            id: fx.a_int,
            name: n(),
            properties: props(&m, &["integrity"]),
            provenance: Provenance::default(),
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
            provenance: Provenance::default(),
        },
    );
    ok(
        &mut p,
        Op::AddDamageScenario {
            id: fx.d2,
            name: n(),
            impacts: vec![],
            provenance: Provenance::default(),
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
            provenance: Provenance::default(),
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
            provenance: Provenance::default(),
        },
    );
    let mut tree1 = AttackTree::new(fx.tree1_root, Node::gate(GateKind::Or, n())).expect("root");
    tree1
        .add_child(tree1.root(), fx.tree1_step, Node::step(n()))
        .expect("step");
    ok(
        &mut p,
        Op::AddAttackTree {
            id: fx.tree1,
            tree: tree1,
            step_feasibility: vec![(fx.tree1_step, feas(&m, "medium"))],
            provenance: Provenance::default(),
        },
    );
    ok(
        &mut p,
        Op::SetThreatAttackTree {
            threat: fx.t_full,
            tree: Some(fx.tree1),
            provenance: Provenance::default(),
        },
    );
    let tree_free = AttackTree::new(fx.tree_free_step, Node::step(n())).expect("step");
    ok(
        &mut p,
        Op::AddAttackTree {
            id: fx.tree_free,
            tree: tree_free,
            step_feasibility: vec![(fx.tree_free_step, feas(&m, "low"))],
            provenance: Provenance::default(),
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
    let bad_flvl = w.feasibility_levels().nth(9).expect("wide feasibility").0;
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
        provenance: Provenance::default(),
    };
    let mut full = Project::with_slot_limit(Arc::clone(m), 1);
    ok(
        &mut full,
        Op::AddAsset {
            id: fx.a_all,
            name: n(),
            properties: m.all_properties(),
            provenance: Provenance::default(),
        },
    );

    vec![
        (
            p.clone(),
            Op::AddAsset {
                id: fx.a_all,
                name: n(),
                properties: m.all_properties(),
                provenance: Provenance::default(),
            },
            ModelError::DuplicateAsset(fx.a_all),
        ),
        (
            p.clone(),
            Op::AddDamageScenario {
                id: fx.d1,
                name: n(),
                impacts: vec![],
                provenance: Provenance::default(),
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
                provenance: Provenance::default(),
            },
            ModelError::UnknownAsset(fx.fresh_a),
        ),
        (
            p.clone(),
            Op::SetImpact {
                id: fx.fresh_d,
                category: safety,
                level: None,
                provenance: Provenance::default(),
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
                provenance: Provenance::default(),
            },
            ModelError::EmptyName,
        ),
        (
            p.clone(),
            Op::RenameDamageScenario {
                id: fx.d1,
                name: format!("{name}{}", "x".repeat(257)).into(),
                provenance: Provenance::default(),
            },
            ModelError::NameTooLong { max: 256 },
        ),
        (
            p.clone(),
            Op::RenameAsset {
                id: fx.a_all,
                name: n(),
                provenance: Provenance::human(" \t ", 1),
            },
            ModelError::EmptyActor,
        ),
        (
            p.clone(),
            Op::RenameDamageScenario {
                id: fx.d1,
                name: n(),
                provenance: Provenance::human(format!("{name}{}", "x".repeat(257)), 1),
            },
            ModelError::ActorTooLong { max: 256 },
        ),
        (
            p.clone(),
            Op::SetImpact {
                id: fx.d1,
                category: bad_cat,
                level: None,
                provenance: Provenance::default(),
            },
            ModelError::UnknownImpactCategory(bad_cat),
        ),
        (
            p.clone(),
            Op::AddDamageScenario {
                id: fx.fresh_d,
                name: n(),
                impacts: vec![(safety, bad_level)],
                provenance: Provenance::default(),
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
                provenance: Provenance::default(),
            },
            ModelError::UnknownProperty(bad_prop),
        ),
        (
            p.clone(),
            Op::SetThreatCategory {
                id: fx.t1,
                category: bad_threat_cat,
                provenance: Provenance::default(),
            },
            ModelError::UnknownThreatCategory(bad_threat_cat),
        ),
        (
            p.clone(),
            Op::AddCatalogRef {
                threat: fx.t1,
                catalog_ref: foreign_scheme_ref,
                provenance: Provenance::default(),
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
                provenance: Provenance::default(),
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
                provenance: Provenance::default(),
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
                provenance: Provenance::default(),
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
                provenance: Provenance::default(),
            },
            ModelError::DuplicateImpactCategory(safety),
        ),
        (
            p.clone(),
            Op::LinkDamage {
                threat: fx.t1,
                damage: fx.d1,
                provenance: Provenance::default(),
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
                provenance: Provenance::default(),
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
                provenance: Provenance::default(),
            },
            ModelError::CatalogRefNotFound { threat: fx.t1 },
        ),
        (
            p.clone(),
            Op::AddCatalogRef {
                threat: fx.t1,
                catalog_ref: fx.r41,
                provenance: Provenance::default(),
            },
            ModelError::DuplicateCatalogRef { threat: fx.t1 },
        ),
        (
            p.clone(),
            Op::AddCatalogRef {
                threat: fx.t_full,
                catalog_ref: fx.r5,
                provenance: Provenance::default(),
            },
            ModelError::TooManyCatalogRefs {
                threat: fx.t_full,
                max: 16,
            },
        ),
        (
            p.clone(),
            Op::AddAttackTree {
                id: fx.tree1,
                tree: AttackTree::new(fx.fresh_node, Node::step(n())).expect("step"),
                step_feasibility: vec![],
                provenance: Provenance::default(),
            },
            ModelError::DuplicateAttackTree(fx.tree1),
        ),
        (
            p.clone(),
            Op::RemoveAttackTree { id: fx.fresh_tree },
            ModelError::UnknownAttackTree(fx.fresh_tree),
        ),
        (
            p.clone(),
            Op::SetStepFeasibility {
                tree: fx.tree1,
                node: fx.fresh_node,
                level: None,
                provenance: Provenance::default(),
            },
            ModelError::UnknownTreeNode {
                tree: fx.tree1,
                node: fx.fresh_node,
            },
        ),
        (
            p.clone(),
            Op::SetStepFeasibility {
                tree: fx.tree1,
                node: fx.tree1_root,
                level: None,
                provenance: Provenance::default(),
            },
            ModelError::NotAStep {
                tree: fx.tree1,
                node: fx.tree1_root,
            },
        ),
        (
            p.clone(),
            Op::SetStepFeasibility {
                tree: fx.tree1,
                node: fx.tree1_step,
                level: Some(bad_flvl),
                provenance: Provenance::default(),
            },
            ModelError::UnknownFeasibilityLevel(bad_flvl),
        ),
        (
            p.clone(),
            Op::AddAttackTree {
                id: fx.fresh_tree,
                tree: AttackTree::new(fx.fresh_node, Node::step(n())).expect("step"),
                step_feasibility: vec![
                    (fx.fresh_node, feas(m, "low")),
                    (fx.fresh_node, feas(m, "high")),
                ],
                provenance: Provenance::default(),
            },
            ModelError::DuplicateStepFeasibility {
                tree: fx.fresh_tree,
                node: fx.fresh_node,
            },
        ),
        (
            p.clone(),
            Op::RemoveAttackTree { id: fx.tree1 },
            ModelError::AttackTreeInUse {
                tree: fx.tree1,
                threats: 1,
            },
        ),
        (
            p.clone(),
            Op::RemoveThreatScenario { id: fx.t_full },
            ModelError::ThreatHasAttackTree {
                threat: fx.t_full,
                tree: fx.tree1,
            },
        ),
        (
            full,
            Op::AddAsset {
                id: fx.fresh_a,
                name: n(),
                properties: m.all_properties(),
                provenance: Provenance::default(),
            },
            ModelError::ProjectCapacityExceeded,
        ),
    ]
}

/// The `ModelError` variants triggered by `Project::apply`.
const NEW_VARIANTS: [&str; 36] = [
    "DuplicateAsset",
    "DuplicateDamageScenario",
    "DuplicateThreatScenario",
    "DuplicateAttackTree",
    "UnknownAsset",
    "UnknownDamageScenario",
    "UnknownThreatScenario",
    "UnknownAttackTree",
    "UnknownTreeNode",
    "NotAStep",
    "EmptyName",
    "NameTooLong",
    "EmptyActor",
    "ActorTooLong",
    "UnknownImpactCategory",
    "UnknownLevel",
    "UnknownFeasibilityLevel",
    "UnknownProperty",
    "UnknownThreatCategory",
    "UnknownCatalogScheme",
    "InvalidCatalogId",
    "NoSecurityProperties",
    "PropertyNotOnAsset",
    "PropertyInUse",
    "AssetInUse",
    "DamageScenarioInUse",
    "AttackTreeInUse",
    "ThreatHasAttackTree",
    "DuplicateImpactCategory",
    "DuplicateStepFeasibility",
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
        provenance: Provenance::default(),
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
        provenance: Provenance::default(),
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
        provenance: Provenance::default(),
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
                provenance: Provenance::default(),
            },
            &want,
        );
        rejects(
            &mut p,
            Op::AddDamageScenario {
                id: fx.fresh_d,
                name: name.clone(),
                impacts: vec![],
                provenance: Provenance::default(),
            },
            &want,
        );
        rejects(
            &mut p,
            Op::RenameAsset {
                id: fx.a_int,
                name: name.clone(),
                provenance: Provenance::default(),
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
            provenance: Provenance::default(),
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
            provenance: Provenance::default(),
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
            provenance: Provenance::default(),
        },
    );
    // Recategorising t1 to a category the asset cannot satisfy.
    rejects(
        &mut p,
        Op::SetThreatCategory {
            id: fx.t1,
            category: threat_cat(&m, "information-disclosure"),
            provenance: Provenance::default(),
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
            provenance: Provenance::default(),
        },
    );

    // After recategorising t1 to information disclosure, integrity is no
    // longer targeted, so it can be dropped.
    ok(
        &mut p,
        Op::SetAssetProperties {
            id: fx.a_all,
            properties: props(&m, &["confidentiality", "authenticity"]),
            provenance: Provenance::default(),
        },
    );
    // And confidentiality now is.
    rejects(
        &mut p,
        Op::SetAssetProperties {
            id: fx.a_all,
            properties: props(&m, &["authenticity"]),
            provenance: Provenance::default(),
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
            provenance: Provenance::default(),
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
    rejects(
        &mut p,
        Op::RemoveThreatScenario { id: fx.t_full },
        &ModelError::ThreatHasAttackTree {
            threat: fx.t_full,
            tree: fx.tree1,
        },
    );
    ok(
        &mut p,
        Op::SetThreatAttackTree {
            threat: fx.t_full,
            tree: None,
            provenance: Provenance::default(),
        },
    );
    ok(&mut p, Op::RemoveThreatScenario { id: fx.t_full });
    ok(
        &mut p,
        Op::SetAssetProperties {
            id: fx.a_all,
            properties: props(&m, &["availability"]),
            provenance: Provenance::default(),
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
            provenance: Provenance::default(),
        },
    );
    let drop_integrity = Op::SetAssetProperties {
        id: fx.a_all,
        properties: props(&m, &["authenticity"]),
        provenance: Provenance::default(),
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
    let p_edit = Provenance::human("alice", 1000);
    // Ops in canonical form (sorted lists, impacts in category order), so
    // the inverse of the inverse is the op itself.
    let ops = vec![
        Op::AddAsset {
            id: fx.fresh_a,
            name: "new".into(),
            properties: props(&m, &["integrity", "availability"]),
            provenance: p_edit.clone(),
        },
        Op::RenameAsset {
            id: fx.a_int,
            name: "renamed".into(),
            provenance: p_edit.clone(),
        },
        Op::SetAssetProperties {
            id: fx.a_int,
            properties: props(&m, &["integrity", "authorization"]),
            provenance: p_edit.clone(),
        },
        Op::RemoveAsset { id: fx.a_int },
        Op::AddDamageScenario {
            id: fx.fresh_d,
            name: "new".into(),
            impacts: vec![
                (safety, lvl(&m, safety, "negligible")),
                (financial, lvl(&m, financial, "severe")),
            ],
            provenance: p_edit.clone(),
        },
        Op::RenameDamageScenario {
            id: fx.d1,
            name: "renamed".into(),
            provenance: p_edit.clone(),
        },
        Op::SetImpact {
            id: fx.d1,
            category: safety,
            level: Some(lvl(&m, safety, "severe")),
            provenance: p_edit.clone(),
        },
        Op::SetImpact {
            id: fx.d1,
            category: safety,
            level: None,
            provenance: p_edit.clone(),
        },
        Op::SetImpact {
            id: fx.d2,
            category: financial,
            level: Some(lvl(&m, financial, "moderate")),
            provenance: p_edit.clone(),
        },
        Op::RemoveDamageScenario { id: fx.d2 },
        Op::AddThreatScenario {
            id: fx.fresh_t,
            name: "new".into(),
            asset: fx.a_all,
            category: threat_cat(&m, "repudiation"),
            catalog_refs: vec![cref(&m, annex5, "3"), cref(&m, annex5, "3.7")],
            damages: sorted.clone(),
            provenance: p_edit.clone(),
        },
        Op::RenameThreatScenario {
            id: fx.t1,
            name: "renamed".into(),
            provenance: p_edit.clone(),
        },
        Op::SetThreatCategory {
            id: fx.t1,
            category: threat_cat(&m, "elevation-of-privilege"),
            provenance: p_edit.clone(),
        },
        Op::AddCatalogRef {
            threat: fx.t1,
            catalog_ref: fx.r5,
            provenance: p_edit.clone(),
        },
        Op::RemoveCatalogRef {
            threat: fx.t1,
            catalog_ref: fx.r41,
            provenance: p_edit.clone(),
        },
        Op::LinkDamage {
            threat: fx.t1,
            damage: fx.d2,
            provenance: p_edit.clone(),
        },
        Op::UnlinkDamage {
            threat: fx.t1,
            damage: fx.d1,
            provenance: p_edit.clone(),
        },
        Op::RemoveThreatScenario { id: fx.t1 },
        Op::AddAttackTree {
            id: fx.fresh_tree,
            tree: AttackTree::new(fx.fresh_node, Node::step("new")).expect("step"),
            step_feasibility: vec![(fx.fresh_node, feas(&m, "high"))],
            provenance: p_edit.clone(),
        },
        Op::SetStepFeasibility {
            tree: fx.tree1,
            node: fx.tree1_step,
            level: Some(feas(&m, "high")),
            provenance: p_edit.clone(),
        },
        Op::SetStepFeasibility {
            tree: fx.tree1,
            node: fx.tree1_step,
            level: None,
            provenance: p_edit.clone(),
        },
        Op::SetThreatAttackTree {
            threat: fx.t1,
            tree: Some(fx.tree_free),
            provenance: p_edit.clone(),
        },
        Op::SetThreatAttackTree {
            threat: fx.t_full,
            tree: None,
            provenance: p_edit.clone(),
        },
        Op::RemoveAttackTree { id: fx.tree_free },
        // Setting a value to what it already is succeeds with an equal
        // inverse when provenance is unchanged.
        Op::RenameAsset {
            id: fx.a_all,
            name: "x".into(),
            provenance: Provenance::default(),
        },
        Op::SetThreatCategory {
            id: fx.t1,
            category: threat_cat(&m, "tampering"),
            provenance: Provenance::default(),
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
    assert_eq!(kinds.len(), 20, "every Op variant: {kinds:?}");
}

#[test]
fn inverse_of_no_op_is_equal_op() {
    let (mut p, fx) = fixture("same");
    let before = p.clone();
    let op = Op::RenameDamageScenario {
        id: fx.d2,
        name: "same".into(),
        provenance: Provenance::default(),
    };
    assert_eq!(ok(&mut p, op.clone()), op);
    let op = Op::SetAssetProperties {
        id: fx.a_all,
        properties: fx.m.all_properties(),
        provenance: Provenance::default(),
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
        provenance: Provenance::default(),
    };
    let add_damage = |id| Op::AddDamageScenario {
        id,
        name: "damage".into(),
        impacts: vec![],
        provenance: Provenance::default(),
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
            provenance: Provenance::default(),
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
            provenance: Provenance::default(),
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
            provenance: Provenance::default(),
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
            provenance: Provenance::default(),
        },
    );
    for d in [d3, d1] {
        ok(
            &mut two,
            Op::LinkDamage {
                threat: t1,
                damage: d,
                provenance: Provenance::default(),
            },
        );
    }
    for t in ["7", "4.2"] {
        ok(
            &mut two,
            Op::AddCatalogRef {
                threat: t1,
                catalog_ref: r(t),
                provenance: Provenance::default(),
            },
        );
    }
    ok(
        &mut two,
        Op::RenameThreatScenario {
            id: t2,
            name: "threat".into(),
            provenance: Provenance::default(),
        },
    );
    ok(
        &mut two,
        Op::SetThreatCategory {
            id: t2,
            category: threat_cat(&m, "tampering"),
            provenance: Provenance::default(),
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
// AC14 / AC8: errors never contain names.
// ---------------------------------------------------------------------------

#[test]
fn errors_never_contain_names() {
    let mut seen = BTreeSet::new();
    for (mut p, op, want) in cases(SENTINEL) {
        // Every entity and tree node in the fixture carries the sentinel name.
        assert!(p.assets().all(|(_, _, a)| a.name().contains(SENTINEL)));
        assert!(
            p.damage_scenarios()
                .all(|(_, _, d)| d.name().contains(SENTINEL))
        );
        assert!(
            p.threat_scenarios()
                .all(|(_, _, t)| t.name().contains(SENTINEL))
        );
        assert!(
            p.attack_trees()
                .all(|(_, _, t)| t.iter().all(|(_, _, n)| n.label().contains(SENTINEL)))
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

    // Evaluation errors on SENTINEL-named entities and tree nodes.
    let (mut p, fx) = fixture(SENTINEL);
    let err = p.evaluate_threat(fx.t1).expect_err("no attack tree");
    assert_eq!(err, ModelError::NoAttackTree(fx.t1));
    assert!(!err.to_string().contains(SENTINEL));
    assert!(!format!("{err:?}").contains(SENTINEL));

    ok(
        &mut p,
        Op::SetStepFeasibility {
            tree: fx.tree1,
            node: fx.tree1_step,
            level: None,
            provenance: Provenance::default(),
        },
    );
    let err = p.evaluate_threat(fx.t_full).expect_err("unrated step");
    assert_eq!(err, ModelError::MissingLeafValue(fx.tree1_step));
    assert!(!err.to_string().contains(SENTINEL));
    assert!(!format!("{err:?}").contains(SENTINEL));

    let empty_tree_id = fx.fresh_tree;
    let empty_gate_id = fx.fresh_node;
    let empty_tree =
        AttackTree::new(empty_gate_id, Node::gate(GateKind::And, SENTINEL)).expect("gate");
    ok(
        &mut p,
        Op::AddAttackTree {
            id: empty_tree_id,
            tree: empty_tree,
            step_feasibility: vec![],
            provenance: Provenance::default(),
        },
    );
    let err = p.evaluate_tree(empty_tree_id).expect_err("empty gate");
    assert_eq!(err, ModelError::EmptyGate(empty_gate_id));
    assert!(!err.to_string().contains(SENTINEL));
    assert!(!format!("{err:?}").contains(SENTINEL));
}

#[test]
fn errors_have_no_labels() {
    errors_never_contain_names();
}

// ---------------------------------------------------------------------------
// Job 013 AC5 & AC6: threat attack trees, integrity, validation, and explain-why.
// ---------------------------------------------------------------------------

#[test]
fn threat_attack_tree_integrity_and_validation() {
    let (mut p, fx) = fixture("x");
    let m = Arc::clone(&fx.m);

    // Sharing one attack tree across two threats tracks the exact refcount.
    ok(
        &mut p,
        Op::SetThreatAttackTree {
            threat: fx.t1,
            tree: Some(fx.tree1),
            provenance: Provenance::default(),
        },
    );
    rejects(
        &mut p,
        Op::RemoveAttackTree { id: fx.tree1 },
        &ModelError::AttackTreeInUse {
            tree: fx.tree1,
            threats: 2,
        },
    );
    rejects(
        &mut p,
        Op::RemoveThreatScenario { id: fx.t1 },
        &ModelError::ThreatHasAttackTree {
            threat: fx.t1,
            tree: fx.tree1,
        },
    );

    // Switching t1 to `tree_free` drops `tree1`'s refcount to 1 and raises
    // `tree_free`'s refcount to 1.
    let undo_switch = ok(
        &mut p,
        Op::SetThreatAttackTree {
            threat: fx.t1,
            tree: Some(fx.tree_free),
            provenance: Provenance::default(),
        },
    );
    assert_eq!(
        undo_switch,
        Op::SetThreatAttackTree {
            threat: fx.t1,
            tree: Some(fx.tree1),
            provenance: Provenance::default(),
        }
    );
    rejects(
        &mut p,
        Op::RemoveAttackTree { id: fx.tree1 },
        &ModelError::AttackTreeInUse {
            tree: fx.tree1,
            threats: 1,
        },
    );
    rejects(
        &mut p,
        Op::RemoveAttackTree { id: fx.tree_free },
        &ModelError::AttackTreeInUse {
            tree: fx.tree_free,
            threats: 1,
        },
    );

    // Unlinking both allows removal of `tree1`.
    ok(
        &mut p,
        Op::SetThreatAttackTree {
            threat: fx.t1,
            tree: None,
            provenance: Provenance::default(),
        },
    );
    ok(
        &mut p,
        Op::SetThreatAttackTree {
            threat: fx.t_full,
            tree: None,
            provenance: Provenance::default(),
        },
    );
    let restore_tree1 = ok(&mut p, Op::RemoveAttackTree { id: fx.tree1 });
    assert_eq!(p.attack_tree_count(), 1);
    ok(&mut p, restore_tree1);
    assert_eq!(p.attack_tree_count(), 2);

    // Validation rules on AddAttackTree and SetStepFeasibility.
    rejects(
        &mut p,
        Op::SetStepFeasibility {
            tree: fx.tree1,
            node: fx.tree1_root,
            level: Some(feas(&m, "low")),
            provenance: Provenance::default(),
        },
        &ModelError::NotAStep {
            tree: fx.tree1,
            node: fx.tree1_root,
        },
    );
    rejects(
        &mut p,
        Op::SetStepFeasibility {
            tree: fx.tree1,
            node: fx.fresh_node,
            level: Some(feas(&m, "low")),
            provenance: Provenance::default(),
        },
        &ModelError::UnknownTreeNode {
            tree: fx.tree1,
            node: fx.fresh_node,
        },
    );
    let step_tree = AttackTree::new(fx.fresh_node, Node::step("s")).expect("step");
    rejects(
        &mut p,
        Op::AddAttackTree {
            id: fx.fresh_tree,
            tree: step_tree.clone(),
            step_feasibility: vec![
                (fx.fresh_node, feas(&m, "low")),
                (fx.fresh_node, feas(&m, "high")),
            ],
            provenance: Provenance::default(),
        },
        &ModelError::DuplicateStepFeasibility {
            tree: fx.fresh_tree,
            node: fx.fresh_node,
        },
    );
    let bad_flvl = wide()
        .feasibility_levels()
        .nth(9)
        .expect("wide feasibility")
        .0;
    rejects(
        &mut p,
        Op::AddAttackTree {
            id: fx.fresh_tree,
            tree: step_tree,
            step_feasibility: vec![(fx.fresh_node, bad_flvl)],
            provenance: Provenance::default(),
        },
        &ModelError::UnknownFeasibilityLevel(bad_flvl),
    );
    let blank_tree = AttackTree::new(fx.fresh_node, Node::step("   ")).expect("step");
    rejects(
        &mut p,
        Op::AddAttackTree {
            id: fx.fresh_tree,
            tree: blank_tree,
            step_feasibility: vec![],
            provenance: Provenance::default(),
        },
        &ModelError::EmptyName,
    );
    let long_tree = AttackTree::new(fx.fresh_node, Node::step("x".repeat(257))).expect("step");
    rejects(
        &mut p,
        Op::AddAttackTree {
            id: fx.fresh_tree,
            tree: long_tree,
            step_feasibility: vec![],
            provenance: Provenance::default(),
        },
        &ModelError::NameTooLong { max: 256 },
    );
}

#[test]
fn evaluate_threat_feasibility_and_why_path() {
    for (m, keys) in [
        (builtin(), ["very-low", "low", "medium", "high"]),
        (second(), ["improbable", "remote", "occasional", "probable"]),
    ] {
        let [f0, f1, f2, f3] = keys.map(|k| feas(&m, k));
        let mut p = Project::new(Arc::clone(&m));
        let asset = asset_id(1);
        let threat = threat_id(2);
        let tree_id = tree_id(3);
        let cat = m.threat_categories().next().expect("threat cat");
        ok(
            &mut p,
            Op::AddAsset {
                id: asset,
                name: "ECU".into(),
                properties: m.all_properties(),
                provenance: Provenance::default(),
            },
        );
        ok(
            &mut p,
            Op::AddThreatScenario {
                id: threat,
                name: "Threat".into(),
                asset,
                category: cat,
                catalog_refs: vec![],
                damages: vec![],
                provenance: Provenance::default(),
            },
        );

        // No linked tree yet -> NoAttackTree.
        assert_eq!(
            p.evaluate_threat(threat),
            Err(ModelError::NoAttackTree(threat))
        );
        assert_eq!(
            p.evaluate_threat(threat_id(99)),
            Err(ModelError::UnknownThreatScenario(threat_id(99)))
        );
        assert_eq!(
            p.evaluate_tree(tree_id),
            Err(ModelError::UnknownAttackTree(tree_id))
        );

        // Build a DAG with OR root, AND left branch, SeqAnd right branch, and a
        // shared step `shared` linked into both branches:
        //        root (OR)
        //       /         \
        //   left (AND)   right (SAND)
        //    /     \      /       \
        //  s_a    shared (linked)  s_b
        let (root_id, left_id, right_id, sa_id, shared_id, sb_id) = (
            node_id(10),
            node_id(11),
            node_id(12),
            node_id(13),
            node_id(14),
            node_id(15),
        );
        let mut tree = AttackTree::new(root_id, Node::gate(GateKind::Or, "Root")).expect("root");
        let root_idx = tree.root();
        let left_idx = tree
            .add_child(root_idx, left_id, Node::gate(GateKind::And, "Left"))
            .expect("left");
        let right_idx = tree
            .add_child(root_idx, right_id, Node::gate(GateKind::SeqAnd, "Right"))
            .expect("right");
        let sa_idx = tree
            .add_child(left_idx, sa_id, Node::step("Step A"))
            .expect("sa");
        let shared_idx = tree
            .add_child(left_idx, shared_id, Node::step("Shared"))
            .expect("shared");
        tree.link(right_idx, shared_idx).expect("link shared");
        let sb_idx = tree
            .add_child(right_idx, sb_id, Node::step("Step B"))
            .expect("sb");

        // Initially rate s_a = f3 (high), shared = f2 (med), s_b = f1 (low):
        // left (AND) = min(f3, f2) = f2 (decided by shared)
        // right (SAND) = min(f2, f1) = f1 (decided by s_b)
        // root (OR) = max(f2, f1) = f2 (decided by left -> shared)
        ok(
            &mut p,
            Op::AddAttackTree {
                id: tree_id,
                tree,
                step_feasibility: vec![(sa_id, f3), (shared_id, f2), (sb_id, f1)],
                provenance: Provenance::default(),
            },
        );
        ok(
            &mut p,
            Op::SetThreatAttackTree {
                threat,
                tree: Some(tree_id),
                provenance: Provenance::default(),
            },
        );

        let eval = p.evaluate_threat(threat).expect("eval threat");
        let tree_eval = p.evaluate_tree(tree_id).expect("eval tree");
        assert_eq!(eval.root_value(), &f2);
        assert_eq!(tree_eval.root_value(), &f2);
        assert_eq!(eval.get(root_idx), Some(&f2));
        assert_eq!(eval.get(left_idx), Some(&f2));
        assert_eq!(eval.get(right_idx), Some(&f1));
        assert_eq!(eval.get(sa_idx), Some(&f3));
        assert_eq!(eval.get(shared_idx), Some(&f2));
        assert_eq!(eval.get(sb_idx), Some(&f1));
        assert_eq!(
            eval.derivation(root_idx),
            Some(Derivation::Selected {
                rule: GateKind::Or,
                child: left_idx,
            })
        );
        assert_eq!(
            eval.derivation(left_idx),
            Some(Derivation::Selected {
                rule: GateKind::And,
                child: shared_idx,
            })
        );
        assert_eq!(eval.derivation(shared_idx), Some(Derivation::Leaf));
        let path: Vec<_> = eval.why_path(root_idx).collect();
        assert_eq!(path, [root_idx, left_idx, shared_idx]);

        let stored_tree = p
            .attack_tree(p.attack_tree_idx(&tree_id).expect("tree idx"))
            .expect("tree");
        let root_exp = eval.explain(stored_tree, root_idx).expect("explain root");
        assert_eq!(root_exp.decisive(), Some(left_idx));
        assert_eq!(root_exp.inputs().count(), 2);

        // Now lower s_a to f0 (very-low) and raise s_b to f3 (high):
        // left (AND) = min(f0, f2) = f0 (decided by s_a)
        // right (SAND) = min(f2, f3) = f2 (decided by shared)
        // root (OR) = max(f0, f2) = f2 (now decided by right -> shared!)
        let undo_sa = ok(
            &mut p,
            Op::SetStepFeasibility {
                tree: tree_id,
                node: sa_id,
                level: Some(f0),
                provenance: Provenance::default(),
            },
        );
        let undo_sb = ok(
            &mut p,
            Op::SetStepFeasibility {
                tree: tree_id,
                node: sb_id,
                level: Some(f3),
                provenance: Provenance::default(),
            },
        );
        let eval2 = p.evaluate_threat(threat).expect("eval2");
        assert_eq!(eval2.root_value(), &f2);
        let path2: Vec<_> = eval2.why_path(root_idx).collect();
        assert_eq!(path2, [root_idx, right_idx, shared_idx]);

        // Undo both rating changes and verify the original why_path is restored.
        ok(&mut p, undo_sb);
        ok(&mut p, undo_sa);
        let eval3 = p.evaluate_threat(threat).expect("eval3");
        let path3: Vec<_> = eval3.why_path(root_idx).collect();
        assert_eq!(path3, [root_idx, left_idx, shared_idx]);

        // Clearing a step rating causes MissingLeafValue(sb_id).
        ok(
            &mut p,
            Op::SetStepFeasibility {
                tree: tree_id,
                node: sb_id,
                level: None,
                provenance: Provenance::default(),
            },
        );
        assert_eq!(
            p.evaluate_threat(threat),
            Err(ModelError::MissingLeafValue(sb_id))
        );
    }
}

// ---------------------------------------------------------------------------
// Job 010: at most 256 damage links per threat.
// ---------------------------------------------------------------------------

#[test]
fn too_many_damage_links() {
    let m = builtin();
    let max = aegis_model::MAX_DAMAGE_LINKS;
    assert_eq!(max, 256);
    let mut p = Project::new(Arc::clone(&m));
    let asset = asset_id(1);
    ok(
        &mut p,
        Op::AddAsset {
            id: asset,
            name: name("ECU"),
            properties: m.all_properties(),
            provenance: Provenance::default(),
        },
    );
    let damages: Vec<_> = (0..=max as u64).map(|i| damage_id(100 + i)).collect();
    for &id in &damages {
        ok(
            &mut p,
            Op::AddDamageScenario {
                id,
                name: name("d"),
                impacts: vec![],
                provenance: Provenance::default(),
            },
        );
    }
    let (t, other) = (threat_id(2), threat_id(3));
    let add = |id, damages: &[DamageId]| Op::AddThreatScenario {
        id,
        name: name("t"),
        asset,
        category: threat_cat(&m, "tampering"),
        catalog_refs: vec![],
        damages: damages.to_vec(),
        provenance: Provenance::default(),
    };
    // 257 links on add.
    rejects(
        &mut p,
        add(t, &damages),
        &ModelError::TooManyDamageLinks { threat: t, max },
    );
    // Exactly 256 is fine; the 257th through `LinkDamage` is not.
    ok(&mut p, add(t, &damages[..max]));
    let last = damages[max];
    rejects(
        &mut p,
        Op::LinkDamage {
            threat: t,
            damage: last,
            provenance: Provenance::default(),
        },
        &ModelError::TooManyDamageLinks { threat: t, max },
    );
    // A duplicate link is still reported as a duplicate, not as the cap.
    rejects(
        &mut p,
        Op::LinkDamage {
            threat: t,
            damage: damages[0],
            provenance: Provenance::default(),
        },
        &ModelError::DuplicateDamageLink {
            threat: t,
            damage: damages[0],
        },
    );
    // After an unlink there is room again; other threats are unaffected.
    ok(
        &mut p,
        Op::UnlinkDamage {
            threat: t,
            damage: damages[0],
            provenance: Provenance::default(),
        },
    );
    ok(
        &mut p,
        Op::LinkDamage {
            threat: t,
            damage: last,
            provenance: Provenance::default(),
        },
    );
    ok(&mut p, add(other, &[last]));
    let text = ModelError::TooManyDamageLinks { threat: t, max }.to_string();
    assert_eq!(
        text,
        format!("threat scenario {t} would link more than 256 damage scenarios")
    );
}

// ---------------------------------------------------------------------------
// Job 014 AC1–AC6: element provenance, lifecycle, undo/redo, validation, PartialEq.
// ---------------------------------------------------------------------------

#[test]
fn provenance_lifecycle_and_undo_redo_across_every_op() {
    let m = builtin();
    let mut p = Project::new(Arc::clone(&m));
    let annex5 = scheme(&m, "r155-annex5");
    let safety = cat(&m, "safety");

    let a_id = asset_id(1);
    let d_id = damage_id(2);
    let t_id = threat_id(3);
    let tree_id = tree_id(4);
    let step_id = node_id(5);

    let p_a0 = Provenance::human("alice", 100);
    let p_a1 = Provenance::import("reqif", 110);
    let p_a2 = Provenance::ai("tara-assist", 120);

    let p_d0 = Provenance::import("csv", 200);
    let p_d1 = Provenance::human("bob", 210);
    let p_d2 = Provenance::ai("tara-assist", 220);

    let p_t0 = Provenance::catalog("r155-annex5", 300);
    let p_t1 = Provenance::human("alice", 310);
    let p_t2 = Provenance::import("reqif", 320);
    let p_t3 = Provenance::catalog("r155-annex5", 330);
    let p_t4 = Provenance::human("bob", 340);
    let p_t5 = Provenance::ai("tara-assist", 350);
    let p_t6 = Provenance::human("alice", 360);
    let p_t7 = Provenance::human("bob", 370);
    let p_t8 = Provenance::ai("tara-assist", 380);

    let p_tree0 = Provenance::human("alice", 400);
    let p_tree1 = Provenance::ai("tara-assist", 410);

    let mut history: Vec<(Op, Op, Project)> = Vec::new();
    let mut record = |p: &mut Project, op: Op| {
        let before = p.clone();
        let inv = ok(p, op.clone());
        history.push((op, inv, before));
    };

    // Create all 4 element kinds with distinct provenance records.
    record(
        &mut p,
        Op::AddAsset {
            id: a_id,
            name: "ECU".into(),
            properties: m.all_properties(),
            provenance: p_a0.clone(),
        },
    );
    record(
        &mut p,
        Op::AddDamageScenario {
            id: d_id,
            name: "Crash".into(),
            impacts: vec![],
            provenance: p_d0.clone(),
        },
    );
    record(
        &mut p,
        Op::AddThreatScenario {
            id: t_id,
            name: "Threat".into(),
            asset: a_id,
            category: threat_cat(&m, "tampering"),
            catalog_refs: vec![],
            damages: vec![],
            provenance: p_t0.clone(),
        },
    );
    record(
        &mut p,
        Op::AddAttackTree {
            id: tree_id,
            tree: AttackTree::new(step_id, Node::step("Step")).expect("step"),
            step_feasibility: vec![(step_id, feas(&m, "low"))],
            provenance: p_tree0.clone(),
        },
    );

    let a_idx = p.asset_idx(&a_id).expect("asset");
    let d_idx = p.damage_scenario_idx(&d_id).expect("damage");
    let t_idx = p.threat_scenario_idx(&t_id).expect("threat");
    let tree_idx = p.attack_tree_idx(&tree_id).expect("tree");

    assert_eq!(p.asset(a_idx).map(|a| a.provenance()), Some(&p_a0));
    assert_eq!(
        p.damage_scenario(d_idx).map(|d| d.provenance()),
        Some(&p_d0)
    );
    assert_eq!(
        p.threat_scenario(t_idx).map(|t| t.provenance()),
        Some(&p_t0)
    );
    assert_eq!(p.attack_tree_provenance(tree_idx), Some(&p_tree0));

    // Mutate Asset.
    record(
        &mut p,
        Op::RenameAsset {
            id: a_id,
            name: "Brake ECU".into(),
            provenance: p_a1.clone(),
        },
    );
    assert_eq!(p.asset(a_idx).map(|a| a.provenance()), Some(&p_a1));
    record(
        &mut p,
        Op::SetAssetProperties {
            id: a_id,
            properties: props(&m, &["integrity", "availability"]),
            provenance: p_a2.clone(),
        },
    );
    assert_eq!(p.asset(a_idx).map(|a| a.provenance()), Some(&p_a2));

    // Mutate DamageScenario.
    record(
        &mut p,
        Op::RenameDamageScenario {
            id: d_id,
            name: "Brake failure".into(),
            provenance: p_d1.clone(),
        },
    );
    assert_eq!(
        p.damage_scenario(d_idx).map(|d| d.provenance()),
        Some(&p_d1)
    );
    record(
        &mut p,
        Op::SetImpact {
            id: d_id,
            category: safety,
            level: Some(lvl(&m, safety, "severe")),
            provenance: p_d2.clone(),
        },
    );
    assert_eq!(
        p.damage_scenario(d_idx).map(|d| d.provenance()),
        Some(&p_d2)
    );

    // Mutate ThreatScenario through every threat-mutating op.
    record(
        &mut p,
        Op::RenameThreatScenario {
            id: t_id,
            name: "Reflash brake ECU".into(),
            provenance: p_t1.clone(),
        },
    );
    assert_eq!(
        p.threat_scenario(t_idx).map(|t| t.provenance()),
        Some(&p_t1)
    );
    record(
        &mut p,
        Op::SetThreatCategory {
            id: t_id,
            category: threat_cat(&m, "denial-of-service"),
            provenance: p_t2.clone(),
        },
    );
    assert_eq!(
        p.threat_scenario(t_idx).map(|t| t.provenance()),
        Some(&p_t2)
    );
    record(
        &mut p,
        Op::AddCatalogRef {
            threat: t_id,
            catalog_ref: cref(&m, annex5, "5"),
            provenance: p_t3.clone(),
        },
    );
    assert_eq!(
        p.threat_scenario(t_idx).map(|t| t.provenance()),
        Some(&p_t3)
    );
    record(
        &mut p,
        Op::RemoveCatalogRef {
            threat: t_id,
            catalog_ref: cref(&m, annex5, "5"),
            provenance: p_t4.clone(),
        },
    );
    assert_eq!(
        p.threat_scenario(t_idx).map(|t| t.provenance()),
        Some(&p_t4)
    );
    record(
        &mut p,
        Op::LinkDamage {
            threat: t_id,
            damage: d_id,
            provenance: p_t5.clone(),
        },
    );
    assert_eq!(
        p.threat_scenario(t_idx).map(|t| t.provenance()),
        Some(&p_t5)
    );
    record(
        &mut p,
        Op::UnlinkDamage {
            threat: t_id,
            damage: d_id,
            provenance: p_t6.clone(),
        },
    );
    assert_eq!(
        p.threat_scenario(t_idx).map(|t| t.provenance()),
        Some(&p_t6)
    );
    record(
        &mut p,
        Op::SetThreatAttackTree {
            threat: t_id,
            tree: Some(tree_id),
            provenance: p_t7.clone(),
        },
    );
    assert_eq!(
        p.threat_scenario(t_idx).map(|t| t.provenance()),
        Some(&p_t7)
    );

    // Mutate AttackTree step feasibility.
    record(
        &mut p,
        Op::SetStepFeasibility {
            tree: tree_id,
            node: step_id,
            level: Some(feas(&m, "high")),
            provenance: p_tree1.clone(),
        },
    );
    assert_eq!(p.attack_tree_provenance(tree_idx), Some(&p_tree1));

    // Unlink tree and remove all 4 entities; their inverses carry the stored provenance.
    record(
        &mut p,
        Op::SetThreatAttackTree {
            threat: t_id,
            tree: None,
            provenance: p_t8.clone(),
        },
    );
    record(&mut p, Op::RemoveAttackTree { id: tree_id });
    assert_eq!(p.attack_tree_provenance(tree_idx), None);
    record(&mut p, Op::RemoveThreatScenario { id: t_id });
    record(&mut p, Op::RemoveDamageScenario { id: d_id });
    record(&mut p, Op::RemoveAsset { id: a_id });
    assert_eq!(p, Project::new(Arc::clone(&m)));

    // Undo the entire history step by step, verifying exact project state
    // (including provenance on all 4 element kinds) at every step, and then
    // redo each step.
    for (op, inv, before) in history.into_iter().rev() {
        let after = p.clone();
        let redo = ok(&mut p, inv);
        assert_eq!(p, before);
        assert_eq!(redo, op);
        ok(&mut p, redo);
        assert_eq!(p, after);
        ok(&mut p, ok(&mut before.clone(), op));
        p = before;
    }
    assert_eq!(p, Project::new(m));
}

#[test]
fn project_equality_compares_provenance_on_every_element_kind() {
    let (base, fx) = fixture("x");
    let other = Provenance::new("auditor", 999, ProvenanceSource::Human);

    let mut p_asset = base.clone();
    ok(
        &mut p_asset,
        Op::RenameAsset {
            id: fx.a_all,
            name: "x".into(),
            provenance: other.clone(),
        },
    );
    assert_ne!(p_asset, base);

    let mut p_damage = base.clone();
    ok(
        &mut p_damage,
        Op::RenameDamageScenario {
            id: fx.d1,
            name: "x".into(),
            provenance: other.clone(),
        },
    );
    assert_ne!(p_damage, base);

    let mut p_threat = base.clone();
    ok(
        &mut p_threat,
        Op::RenameThreatScenario {
            id: fx.t1,
            name: "x".into(),
            provenance: other.clone(),
        },
    );
    assert_ne!(p_threat, base);

    let mut p_tree = base.clone();
    ok(
        &mut p_tree,
        Op::SetStepFeasibility {
            tree: fx.tree1,
            node: fx.tree1_step,
            level: Some(feas(&fx.m, "medium")),
            provenance: other,
        },
    );
    assert_ne!(p_tree, base);
}
