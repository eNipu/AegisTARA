//! `Project` through the public API, over the built-in ISO/SAE 21434 +
//! UN R155 methodology: a worked example, every rejection, referential
//! integrity, inverse round trips, deterministic order, and errors that
//! never contain names.

mod common;

use std::collections::BTreeSet;
use std::sync::Arc;

use aegis_model::{
    AssetId, AttackTree, CatalogRef, DamageId, Derivation, EntityKind, GateKind, LintCode,
    LintFinding, MAX_STEP_MITIGATIONS, Methodology, ModelError, Node, NodeId, Op, Project,
    PropertySet, Provenance, ProvenanceSource, RiskTreatment, ThreatId, TreeId,
};
use common::{
    SENTINEL, asset_id, builtin, cat, claim_id, control_id, cref, damage_id, factor, factor_lvl,
    feas, goal_id, lvl, node_id, prop, props, risk, scheme, second, threat_cat, threat_id, tree_id,
    variant, wide,
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
            ModelError::EmptyName {
                kind: EntityKind::ThreatScenario,
            },
        ),
        (
            p.clone(),
            Op::RenameDamageScenario {
                id: fx.d1,
                name: format!("{name}{}", "x".repeat(257)).into(),
                provenance: Provenance::default(),
            },
            ModelError::NameTooLong {
                kind: EntityKind::DamageScenario,
                max: 256,
            },
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
            ModelError::InvalidCatalogId {
                scheme: annex5,
                syntax: m.catalog_syntax(annex5).expect("annex5 syntax").clone(),
            },
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
            ModelError::CatalogRefNotFound {
                threat: fx.t1,
                catalog_ref: fx.r5,
            },
        ),
        (
            p.clone(),
            Op::AddCatalogRef {
                threat: fx.t1,
                catalog_ref: fx.r41,
                provenance: Provenance::default(),
            },
            ModelError::DuplicateCatalogRef {
                threat: fx.t1,
                catalog_ref: fx.r41,
            },
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
            {
                let mut big_tree =
                    AttackTree::new(fx.fresh_node, Node::gate(GateKind::Or, n())).expect("root");
                for i in 0..32 {
                    big_tree
                        .add_child(big_tree.root(), node_id(500 + i), Node::step(n()))
                        .expect("step");
                }
                Op::AddAttackTree {
                    id: fx.fresh_tree,
                    tree: big_tree,
                    step_feasibility: vec![],
                    provenance: Provenance::default(),
                }
            },
            ModelError::TooManyAttackTreeNodes {
                tree: fx.fresh_tree,
                max: 32,
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
            {
                let mut p_direct = p.clone();
                ok(
                    &mut p_direct,
                    Op::SetThreatFeasibility {
                        threat: fx.t1,
                        level: Some(feas(m, "low")),
                        provenance: Provenance::default(),
                    },
                );
                p_direct
            },
            Op::RemoveThreatScenario { id: fx.t1 },
            ModelError::ThreatHasDirectFeasibility {
                threat: fx.t1,
                feasibility: feas(m, "low"),
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
            ModelError::ProjectCapacityExceeded {
                kind: EntityKind::Asset,
            },
        ),
    ]
}

/// The `ModelError` variants triggered by `Project::apply`.
const NEW_VARIANTS: [&str; 38] = [
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
    "ThreatHasDirectFeasibility",
    "DuplicateImpactCategory",
    "DuplicateStepFeasibility",
    "TooManyAttackTreeNodes",
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
        &ModelError::DuplicateCatalogRef {
            threat: fx.fresh_t,
            catalog_ref: fx.r5,
        },
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
    let want_for = |kind, name: &Arc<str>| {
        if name.is_empty() {
            ModelError::EmptyName { kind }
        } else {
            ModelError::NameTooLong { kind, max: 256 }
        }
    };
    for name in [Arc::<str>::from(""), long.clone()] {
        rejects(
            &mut p,
            Op::AddAsset {
                id: fx.fresh_a,
                name: name.clone(),
                properties: fx.m.all_properties(),
                provenance: Provenance::default(),
            },
            &want_for(EntityKind::Asset, &name),
        );
        rejects(
            &mut p,
            Op::AddDamageScenario {
                id: fx.fresh_d,
                name: name.clone(),
                impacts: vec![],
                provenance: Provenance::default(),
            },
            &want_for(EntityKind::DamageScenario, &name),
        );
        rejects(
            &mut p,
            Op::RenameAsset {
                id: fx.a_int,
                name: name.clone(),
                provenance: Provenance::default(),
            },
            &want_for(EntityKind::Asset, &name),
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
        Op::SetThreatFeasibility {
            threat: fx.t1,
            level: Some(feas(&m, "medium")),
            provenance: p_edit.clone(),
        },
        Op::SetThreatFeasibility {
            threat: fx.t1,
            level: None,
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
    assert_eq!(kinds.len(), 21, "every Op variant: {kinds:?}");
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
        &ModelError::EmptyName {
            kind: EntityKind::TreeNode,
        },
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
        &ModelError::NameTooLong {
            kind: EntityKind::TreeNode,
            max: 256,
        },
    );

    // 32 nodes (`MAX_ATTACK_TREE_NODES`) is accepted; 33 is rejected.
    let mut tree32 =
        AttackTree::new(fx.fresh_node, Node::gate(GateKind::Or, "root")).expect("root");
    for i in 0..31 {
        tree32
            .add_child(tree32.root(), node_id(600 + i), Node::step("s"))
            .expect("step");
    }
    assert_eq!(tree32.len(), aegis_model::MAX_TREE_OP_NODES);
    let mut tree33 = tree32.clone();
    tree33
        .add_child(tree33.root(), node_id(699), Node::step("s"))
        .expect("33rd step");
    rejects(
        &mut p,
        Op::AddAttackTree {
            id: fx.fresh_tree,
            tree: tree33,
            step_feasibility: vec![],
            provenance: Provenance::default(),
        },
        &ModelError::TooManyAttackTreeNodes {
            tree: fx.fresh_tree,
            max: 32,
        },
    );
    let undo32 = ok(
        &mut p,
        Op::AddAttackTree {
            id: fx.fresh_tree,
            tree: tree32,
            step_feasibility: vec![],
            provenance: Provenance::default(),
        },
    );
    ok(&mut p, undo32);
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
    let p_t9 = Provenance::human("alice", 385);
    let p_t10 = Provenance::ai("tara-assist", 390);

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
    record(
        &mut p,
        Op::SetThreatFeasibility {
            threat: t_id,
            level: Some(feas(&m, "medium")),
            provenance: p_t9.clone(),
        },
    );
    assert_eq!(
        p.threat_scenario(t_idx).map(|t| t.provenance()),
        Some(&p_t9)
    );
    assert_eq!(
        p.threat_scenario(t_idx)
            .and_then(|t| t.direct_feasibility()),
        Some(feas(&m, "medium"))
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

    // Unlink tree, clear direct feasibility, and remove all 4 entities; their inverses carry the stored provenance.
    record(
        &mut p,
        Op::SetThreatAttackTree {
            threat: t_id,
            tree: None,
            provenance: p_t8.clone(),
        },
    );
    record(
        &mut p,
        Op::SetThreatFeasibility {
            threat: t_id,
            level: None,
            provenance: p_t10.clone(),
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

// ---------------------------------------------------------------------------
// Job 015 AC2–AC6: specific ModelError messages without entity names.
// ---------------------------------------------------------------------------

#[test]
fn specific_model_error_messages() {
    assert_eq!(EntityKind::Asset.as_str(), "asset");
    assert_eq!(EntityKind::DamageScenario.as_str(), "damage scenario");
    assert_eq!(EntityKind::ThreatScenario.as_str(), "threat scenario");
    assert_eq!(EntityKind::AttackTree.as_str(), "attack tree");
    assert_eq!(EntityKind::TreeNode.as_str(), "tree node");
    assert_eq!(EntityKind::SecurityControl.as_str(), "security control");
    for kind in [
        EntityKind::Asset,
        EntityKind::DamageScenario,
        EntityKind::ThreatScenario,
        EntityKind::AttackTree,
        EntityKind::TreeNode,
        EntityKind::SecurityControl,
    ] {
        assert_eq!(kind.to_string(), kind.as_str());
        assert_eq!(
            ModelError::EmptyName { kind }.to_string(),
            format!("{kind} name is empty")
        );
        assert_eq!(
            ModelError::NameTooLong { kind, max: 256 }.to_string(),
            format!("{kind} name is longer than 256 chars")
        );
        assert_eq!(
            ModelError::ProjectCapacityExceeded { kind }.to_string(),
            format!("project {kind} capacity exceeded")
        );
    }

    let (p, fx) = fixture(SENTINEL);
    let annex5 = scheme(&fx.m, "r155-annex5");
    let syntax = fx.m.catalog_syntax(annex5).expect("syntax").clone();
    let bad_id = ModelError::InvalidCatalogId {
        scheme: annex5,
        syntax,
    };
    assert_eq!(
        bad_id.to_string(),
        "catalog ID does not match syntax <1..=32>[.<1..=99>] of scheme index 0"
    );

    let not_found = ModelError::CatalogRefNotFound {
        threat: fx.t1,
        catalog_ref: fx.r5,
    };
    assert_eq!(
        not_found.to_string(),
        format!("threat scenario {} does not have catalog ref 5", fx.t1)
    );

    let dup_ref = ModelError::DuplicateCatalogRef {
        threat: fx.t1,
        catalog_ref: fx.r41,
    };
    assert_eq!(
        dup_ref.to_string(),
        format!("threat scenario {} already has catalog ref 4.1", fx.t1)
    );

    // Every name-bearing entity kind produces its own EmptyName and NameTooLong.
    let long: Arc<str> = format!("{SENTINEL}{}", "x".repeat(257)).into();
    for (op, expected) in [
        (
            Op::AddAsset {
                id: fx.fresh_a,
                name: " ".into(),
                properties: fx.m.all_properties(),
                provenance: Provenance::default(),
            },
            ModelError::EmptyName {
                kind: EntityKind::Asset,
            },
        ),
        (
            Op::RenameAsset {
                id: fx.a_int,
                name: long.clone(),
                provenance: Provenance::default(),
            },
            ModelError::NameTooLong {
                kind: EntityKind::Asset,
                max: 256,
            },
        ),
        (
            Op::AddDamageScenario {
                id: fx.fresh_d,
                name: " ".into(),
                impacts: vec![],
                provenance: Provenance::default(),
            },
            ModelError::EmptyName {
                kind: EntityKind::DamageScenario,
            },
        ),
        (
            Op::RenameDamageScenario {
                id: fx.d1,
                name: long.clone(),
                provenance: Provenance::default(),
            },
            ModelError::NameTooLong {
                kind: EntityKind::DamageScenario,
                max: 256,
            },
        ),
        (
            Op::AddThreatScenario {
                id: fx.fresh_t,
                name: " ".into(),
                asset: fx.a_all,
                category: threat_cat(&fx.m, "tampering"),
                catalog_refs: vec![],
                damages: vec![],
                provenance: Provenance::default(),
            },
            ModelError::EmptyName {
                kind: EntityKind::ThreatScenario,
            },
        ),
        (
            Op::RenameThreatScenario {
                id: fx.t1,
                name: long.clone(),
                provenance: Provenance::default(),
            },
            ModelError::NameTooLong {
                kind: EntityKind::ThreatScenario,
                max: 256,
            },
        ),
        (
            Op::AddAttackTree {
                id: fx.fresh_tree,
                tree: AttackTree::new(fx.fresh_node, Node::step(" ")).expect("step"),
                step_feasibility: vec![],
                provenance: Provenance::default(),
            },
            ModelError::EmptyName {
                kind: EntityKind::TreeNode,
            },
        ),
        (
            Op::AddAttackTree {
                id: fx.fresh_tree,
                tree: AttackTree::new(fx.fresh_node, Node::step(long.to_string())).expect("step"),
                step_feasibility: vec![],
                provenance: Provenance::default(),
            },
            ModelError::NameTooLong {
                kind: EntityKind::TreeNode,
                max: 256,
            },
        ),
    ] {
        let mut q = p.clone();
        let err = q.apply(op).expect_err("must reject");
        assert_eq!(err, expected);
        assert!(!err.to_string().contains(SENTINEL));
        assert!(!format!("{err:?}").contains(SENTINEL));
    }
}

#[test]
fn evaluate_risk_across_damages_and_explain_why() {
    let m = builtin();
    let mut p = Project::new(Arc::clone(&m));
    let safety = cat(&m, "safety");
    let financial = cat(&m, "financial");
    let operational = cat(&m, "operational");

    let brake = asset_id(1);
    let d_low = damage_id(10);
    let d_high = damage_id(20);
    let d_unrated = damage_id(30);
    let t_spoof = threat_id(100);
    let t_no_tree = threat_id(101);
    let tr = tree_id(200);
    let root = node_id(201);
    let and_gate = node_id(202);
    let step_obd = node_id(203);
    let step_can = node_id(204);
    let step_rf = node_id(205);

    ok(
        &mut p,
        Op::AddAsset {
            id: brake,
            name: name("Brake controller"),
            properties: m.all_properties(),
            provenance: Provenance::default(),
        },
    );
    // d_low: operational = moderate, financial = major
    ok(
        &mut p,
        Op::AddDamageScenario {
            id: d_low,
            name: name("Reduced assist"),
            impacts: vec![
                (operational, lvl(&m, operational, "moderate")),
                (financial, lvl(&m, financial, "major")),
            ],
            provenance: Provenance::default(),
        },
    );
    // d_high: safety = severe, financial = severe
    ok(
        &mut p,
        Op::AddDamageScenario {
            id: d_high,
            name: name("Unintended braking"),
            impacts: vec![
                (safety, lvl(&m, safety, "severe")),
                (financial, lvl(&m, financial, "severe")),
            ],
            provenance: Provenance::default(),
        },
    );
    ok(
        &mut p,
        Op::AddDamageScenario {
            id: d_unrated,
            name: name("Unrated damage"),
            impacts: vec![],
            provenance: Provenance::default(),
        },
    );

    // Link damages out of ID order: d_high, d_unrated, d_low.
    ok(
        &mut p,
        Op::AddThreatScenario {
            id: t_spoof,
            name: name("Spoof brake torque"),
            asset: brake,
            category: threat_cat(&m, "spoofing"),
            catalog_refs: vec![],
            damages: vec![d_high, d_unrated, d_low],
            provenance: Provenance::default(),
        },
    );
    ok(
        &mut p,
        Op::AddThreatScenario {
            id: t_no_tree,
            name: name("Tamper calibration"),
            asset: brake,
            category: threat_cat(&m, "tampering"),
            catalog_refs: vec![],
            damages: vec![d_unrated],
            provenance: Provenance::default(),
        },
    );

    // Build OR(AND(step_obd, step_can), step_rf).
    let mut tree = AttackTree::new(root, Node::gate(GateKind::Or, "Inject frame")).expect("root");
    let g_and = tree
        .add_child(
            tree.root(),
            and_gate,
            Node::gate(GateKind::And, "Physical OBD"),
        )
        .expect("and");
    tree.add_child(g_and, step_obd, Node::step("Plug into OBD"))
        .expect("obd");
    tree.add_child(g_and, step_can, Node::step("Send CAN frame"))
        .expect("can");
    let n_rf = tree
        .add_child(
            tree.root(),
            step_rf,
            Node::step("Remote telematics exploit"),
        )
        .expect("rf");

    ok(
        &mut p,
        Op::AddAttackTree {
            id: tr,
            tree: tree.clone(),
            step_feasibility: vec![
                (step_obd, feas(&m, "medium")),
                (step_can, feas(&m, "high")),
                (step_rf, feas(&m, "low")),
            ],
            provenance: Provenance::default(),
        },
    );
    ok(
        &mut p,
        Op::SetThreatAttackTree {
            threat: t_spoof,
            tree: Some(tr),
            provenance: Provenance::default(),
        },
    );

    // AND(medium, high) = medium; OR(medium, low) = medium.
    let res = p.evaluate_risk(t_spoof).expect("evaluate_risk");
    assert_eq!(res.feasibility(), feas(&m, "medium"));
    // Entries are in canonical (DamageId, ImpactCategory) order:
    // 1. (d_low, financial=major, medium) -> r3
    // 2. (d_low, operational=moderate, medium) -> r2
    // 3. (d_high, safety=severe, medium) -> r4
    // 4. (d_high, financial=severe, medium) -> r4
    let entries: Vec<_> = res
        .entries()
        .iter()
        .map(|e| {
            (
                e.damage(),
                e.category(),
                e.impact(),
                e.feasibility(),
                e.risk(),
            )
        })
        .collect();
    assert_eq!(
        entries,
        vec![
            (
                d_low,
                financial,
                lvl(&m, financial, "major"),
                feas(&m, "medium"),
                risk(&m, "r3"),
            ),
            (
                d_low,
                operational,
                lvl(&m, operational, "moderate"),
                feas(&m, "medium"),
                risk(&m, "r2"),
            ),
            (
                d_high,
                safety,
                lvl(&m, safety, "severe"),
                feas(&m, "medium"),
                risk(&m, "r4"),
            ),
            (
                d_high,
                financial,
                lvl(&m, financial, "severe"),
                feas(&m, "medium"),
                risk(&m, "r4"),
            ),
        ]
    );
    assert_eq!(res.max_risk(), Some(risk(&m, "r4")));
    // Tie between (d_high, safety) and (d_high, financial) picks first in canonical order: safety.
    let deciding = res.deciding_entry().expect("deciding");
    assert_eq!(deciding.damage(), d_high);
    assert_eq!(deciding.category(), safety);
    assert_eq!(deciding.risk(), risk(&m, "r4"));

    // Explain-why trace through tree_evaluation: root -> g_and -> step_obd (the bottleneck of AND).
    let eval = res.tree_evaluation().expect("tree evaluation");
    let why: Vec<_> = eval
        .why_path(tree.root())
        .map(|idx| tree.id_of(idx).expect("node id"))
        .collect();
    assert_eq!(why, vec![root, and_gate, step_obd]);
    assert_ne!(n_rf, g_and);

    // Threat linked only to unrated damage has empty entries and None max_risk.
    ok(
        &mut p,
        Op::SetThreatAttackTree {
            threat: t_no_tree,
            tree: Some(tr),
            provenance: Provenance::default(),
        },
    );
    let empty_risk = p.evaluate_risk(t_no_tree).expect("unrated damage risk");
    assert_eq!(empty_risk.feasibility(), feas(&m, "medium"));
    assert!(empty_risk.entries().is_empty());
    assert_eq!(empty_risk.max_risk(), None);
    assert_eq!(empty_risk.deciding_entry(), None);

    // Error cases: unknown threat, no attack tree, unrated step.
    ok(
        &mut p,
        Op::SetThreatAttackTree {
            threat: t_no_tree,
            tree: None,
            provenance: Provenance::default(),
        },
    );
    assert_eq!(
        p.evaluate_risk(threat_id(999)),
        Err(ModelError::UnknownThreatScenario(threat_id(999)))
    );
    assert_eq!(
        p.evaluate_risk(t_no_tree),
        Err(ModelError::NoAttackTree(t_no_tree))
    );
    ok(
        &mut p,
        Op::SetStepFeasibility {
            tree: tr,
            node: step_rf,
            level: None,
            provenance: Provenance::default(),
        },
    );
    assert_eq!(
        p.evaluate_risk(t_spoof),
        Err(ModelError::MissingLeafValue(step_rf))
    );

    // Direct threat feasibility: fallback when no attack tree is linked, and
    // attack-tree override when a tree is linked.
    ok(
        &mut p,
        Op::LinkDamage {
            threat: t_no_tree,
            damage: d_high,
            provenance: Provenance::default(),
        },
    );
    ok(
        &mut p,
        Op::SetThreatFeasibility {
            threat: t_no_tree,
            level: Some(feas(&m, "low")),
            provenance: Provenance::default(),
        },
    );
    let direct_res = p.evaluate_risk(t_no_tree).expect("direct feasibility risk");
    assert_eq!(direct_res.feasibility(), feas(&m, "low"));
    assert_eq!(direct_res.tree_evaluation(), None);
    // d_high (safety=severe, financial=severe) at feasibility=low -> r3.
    assert_eq!(direct_res.max_risk(), Some(risk(&m, "r3")));

    // Fix step_rf on tr so tr evaluates to medium again, then link tr to t_no_tree.
    // The linked attack tree (medium -> r4) overrides direct_feasibility (low -> r3),
    // while preserving direct_feasibility on the threat scenario if the tree is later unlinked.
    ok(
        &mut p,
        Op::SetStepFeasibility {
            tree: tr,
            node: step_rf,
            level: Some(feas(&m, "low")),
            provenance: Provenance::default(),
        },
    );
    ok(
        &mut p,
        Op::SetThreatAttackTree {
            threat: t_no_tree,
            tree: Some(tr),
            provenance: Provenance::default(),
        },
    );
    let overridden = p.evaluate_risk(t_no_tree).expect("tree overrides direct");
    assert_eq!(overridden.feasibility(), feas(&m, "medium"));
    assert!(overridden.tree_evaluation().is_some());
    assert_eq!(overridden.max_risk(), Some(risk(&m, "r4")));
    assert_eq!(
        p.threat_scenario(p.threat_scenario_idx(&t_no_tree).expect("idx"))
            .and_then(|t| t.direct_feasibility()),
        Some(feas(&m, "low"))
    );

    // Unlinking the tree falls back to direct_feasibility (low -> r3) again.
    ok(
        &mut p,
        Op::SetThreatAttackTree {
            threat: t_no_tree,
            tree: None,
            provenance: Provenance::default(),
        },
    );
    let fallback = p.evaluate_risk(t_no_tree).expect("fallback to direct");
    assert_eq!(fallback.feasibility(), feas(&m, "low"));
    assert_eq!(fallback.tree_evaluation(), None);
    assert_eq!(fallback.max_risk(), Some(risk(&m, "r3")));
}

#[test]
fn security_controls_risk_treatment_and_residual_risk() {
    let m = builtin();
    let mut p = Project::new(Arc::clone(&m));
    let safety = cat(&m, "safety");
    let financial = cat(&m, "financial");

    let brake = asset_id(1);
    let d_severe = damage_id(10);
    let t_spoof = threat_id(100);
    let t_direct = threat_id(101);
    let tr = tree_id(200);
    let root = node_id(201);
    let and_gate = node_id(202);
    let step_obd = node_id(203);
    let step_can = node_id(204);
    let step_rf = node_id(205);
    let c_mac = control_id(301);
    let c_lock = control_id(302);
    let c_fw = control_id(303);

    ok(
        &mut p,
        Op::AddAsset {
            id: brake,
            name: name("Brake controller"),
            properties: m.all_properties(),
            provenance: Provenance::default(),
        },
    );
    ok(
        &mut p,
        Op::AddDamageScenario {
            id: d_severe,
            name: name("Unintended braking"),
            impacts: vec![
                (safety, lvl(&m, safety, "severe")),
                (financial, lvl(&m, financial, "moderate")),
            ],
            provenance: Provenance::default(),
        },
    );
    ok(
        &mut p,
        Op::AddThreatScenario {
            id: t_spoof,
            name: name("Spoof brake torque"),
            asset: brake,
            category: threat_cat(&m, "spoofing"),
            catalog_refs: vec![],
            damages: vec![d_severe],
            provenance: Provenance::default(),
        },
    );
    ok(
        &mut p,
        Op::AddThreatScenario {
            id: t_direct,
            name: name("Tamper calibration"),
            asset: brake,
            category: threat_cat(&m, "tampering"),
            catalog_refs: vec![],
            damages: vec![d_severe],
            provenance: Provenance::default(),
        },
    );

    // Risk treatment lifecycle on t_spoof.
    for treatment in [
        RiskTreatment::Avoid,
        RiskTreatment::Share,
        RiskTreatment::Retain,
        RiskTreatment::Reduce,
    ] {
        let inv = ok(
            &mut p,
            Op::SetRiskTreatment {
                threat: t_spoof,
                treatment: Some(treatment),
                provenance: Provenance::human("lead", 10),
            },
        );
        let t_idx = p.threat_scenario_idx(&t_spoof).expect("t_spoof");
        assert_eq!(
            p.threat_scenario(t_idx).and_then(|t| t.treatment()),
            Some(treatment)
        );
        assert!(matches!(inv, Op::SetRiskTreatment { .. }));
    }
    rejects(
        &mut p,
        Op::SetRiskTreatment {
            threat: threat_id(999),
            treatment: Some(RiskTreatment::Reduce),
            provenance: Provenance::default(),
        },
        &ModelError::UnknownThreatScenario(threat_id(999)),
    );

    // Security control lifecycle.
    ok(
        &mut p,
        Op::AddSecurityControl {
            id: c_mac,
            name: name("SecOC message authentication"),
            provenance: Provenance::human("architect", 20),
        },
    );
    ok(
        &mut p,
        Op::AddSecurityControl {
            id: c_lock,
            name: name("OBD gateway filter"),
            provenance: Provenance::default(),
        },
    );
    ok(
        &mut p,
        Op::AddSecurityControl {
            id: c_fw,
            name: name("Telematics firewall"),
            provenance: Provenance::default(),
        },
    );
    assert_eq!(p.security_control_count(), 3);
    let mac_idx = p.security_control_idx(&c_mac).expect("c_mac");
    assert_eq!(p.security_control_id(mac_idx), Some(c_mac));
    assert_eq!(
        p.security_control(mac_idx).map(|c| c.name()),
        Some("SecOC message authentication")
    );
    rejects(
        &mut p,
        Op::AddSecurityControl {
            id: c_mac,
            name: name("Duplicate"),
            provenance: Provenance::default(),
        },
        &ModelError::DuplicateSecurityControl(c_mac),
    );
    rejects(
        &mut p,
        Op::RenameSecurityControl {
            id: control_id(999),
            name: name("Missing"),
            provenance: Provenance::default(),
        },
        &ModelError::UnknownSecurityControl(control_id(999)),
    );
    rejects(
        &mut p,
        Op::RemoveSecurityControl {
            id: control_id(999),
        },
        &ModelError::UnknownSecurityControl(control_id(999)),
    );

    // Build OR(AND(step_obd=high, step_can=high), step_rf=medium).
    let mut tree = AttackTree::new(root, Node::gate(GateKind::Or, "Inject frame")).expect("root");
    let g_and = tree
        .add_child(
            tree.root(),
            and_gate,
            Node::gate(GateKind::And, "Physical OBD"),
        )
        .expect("and");
    let n_obd = tree
        .add_child(g_and, step_obd, Node::step("Plug into OBD"))
        .expect("obd");
    let n_can = tree
        .add_child(g_and, step_can, Node::step("Send CAN frame"))
        .expect("can");
    let n_rf = tree
        .add_child(
            tree.root(),
            step_rf,
            Node::step("Remote telematics exploit"),
        )
        .expect("rf");

    ok(
        &mut p,
        Op::AddAttackTree {
            id: tr,
            tree: tree.clone(),
            step_feasibility: vec![
                (step_obd, feas(&m, "high")),
                (step_can, feas(&m, "high")),
                (step_rf, feas(&m, "medium")),
            ],
            provenance: Provenance::default(),
        },
    );
    ok(
        &mut p,
        Op::SetThreatAttackTree {
            threat: t_spoof,
            tree: Some(tr),
            provenance: Provenance::default(),
        },
    );

    // Inherent: AND(high, high) = high; OR(high, medium) = high -> r5 on safety=severe.
    let inherent = p.evaluate_risk(t_spoof).expect("inherent risk");
    assert_eq!(inherent.feasibility(), feas(&m, "high"));
    assert_eq!(inherent.max_risk(), Some(risk(&m, "r5")));
    // Without mitigations, residual matches inherent.
    let res0 = p.evaluate_residual_risk(t_spoof).expect("residual 0");
    assert_eq!(res0, inherent);

    // Attach c_mac to step_can, capping step_can at very-low.
    // The AND gate (Physical OBD) drops from high to very-low,
    // EXCEPT the OR gate at root is circumvented by step_rf (medium)!
    // Residual feasibility is therefore medium (not very-low), and residual risk is r4.
    ok(
        &mut p,
        Op::SetStepMitigation {
            tree: tr,
            node: step_can,
            control: c_mac,
            feasibility: Some(feas(&m, "very-low")),
            provenance: Provenance::human("architect", 30),
        },
    );
    let tr_idx = p.attack_tree_idx(&tr).expect("tr");
    assert_eq!(
        p.residual_step_feasibility(tr_idx, n_can),
        Some(feas(&m, "very-low"))
    );
    assert_eq!(
        p.residual_step_feasibility(tr_idx, n_obd),
        Some(feas(&m, "high"))
    );
    let res1 = p.evaluate_residual_risk(t_spoof).expect("residual 1");
    assert_eq!(res1.feasibility(), feas(&m, "medium"));
    assert_eq!(res1.max_risk(), Some(risk(&m, "r4")));
    // Explain-why on residual tree shows the attacker circumvents Physical OBD via step_rf!
    let res1_eval = res1.tree_evaluation().expect("res1 tree");
    let why1: Vec<_> = res1_eval
        .why_path(tree.root())
        .map(|idx| tree.id_of(idx).expect("id"))
        .collect();
    assert_eq!(why1, vec![root, step_rf]);
    // Meanwhile inherent evaluation is untouched (still high via Physical OBD).
    let inh_eval = p.evaluate_tree(tr).expect("inherent tree");
    assert_eq!(inh_eval.root_value(), &feas(&m, "high"));

    // Now also mitigate step_rf with c_fw down to very-low.
    // Both OR branches are now very-low, so residual risk drops to r2!
    ok(
        &mut p,
        Op::SetStepMitigation {
            tree: tr,
            node: step_rf,
            control: c_fw,
            feasibility: Some(feas(&m, "very-low")),
            provenance: Provenance::human("architect", 40),
        },
    );
    assert_eq!(
        p.residual_step_feasibility(tr_idx, n_rf),
        Some(feas(&m, "very-low"))
    );
    let res2 = p.evaluate_residual_risk(t_spoof).expect("residual 2");
    assert_eq!(res2.feasibility(), feas(&m, "very-low"));
    assert_eq!(res2.max_risk(), Some(risk(&m, "r2")));

    // A control whose cap is weaker than inherent feasibility cannot raise feasibility.
    ok(
        &mut p,
        Op::SetStepMitigation {
            tree: tr,
            node: step_rf,
            control: c_lock,
            feasibility: Some(feas(&m, "high")),
            provenance: Provenance::default(),
        },
    );
    assert_eq!(
        p.residual_step_feasibility(tr_idx, n_rf),
        Some(feas(&m, "very-low"))
    );

    // Direct feasibility threat without a tree evaluates residual risk from direct_feasibility.
    ok(
        &mut p,
        Op::SetThreatFeasibility {
            threat: t_direct,
            level: Some(feas(&m, "low")),
            provenance: Provenance::default(),
        },
    );
    let direct_res = p
        .evaluate_residual_risk(t_direct)
        .expect("direct residual risk");
    assert_eq!(direct_res.feasibility(), feas(&m, "low"));
    assert_eq!(direct_res.max_risk(), Some(risk(&m, "r3")));

    // Rejections on SetStepMitigation: gate node, unknown node, unknown tree, unknown control,
    // foreign feasibility level, and MAX_STEP_MITIGATIONS cap.
    rejects(
        &mut p,
        Op::SetStepMitigation {
            tree: tr,
            node: root,
            control: c_mac,
            feasibility: Some(feas(&m, "low")),
            provenance: Provenance::default(),
        },
        &ModelError::NotAStep {
            tree: tr,
            node: root,
        },
    );
    rejects(
        &mut p,
        Op::SetStepMitigation {
            tree: tr,
            node: node_id(999),
            control: c_mac,
            feasibility: Some(feas(&m, "low")),
            provenance: Provenance::default(),
        },
        &ModelError::UnknownTreeNode {
            tree: tr,
            node: node_id(999),
        },
    );
    rejects(
        &mut p,
        Op::SetStepMitigation {
            tree: tree_id(999),
            node: step_obd,
            control: c_mac,
            feasibility: Some(feas(&m, "low")),
            provenance: Provenance::default(),
        },
        &ModelError::UnknownAttackTree(tree_id(999)),
    );
    rejects(
        &mut p,
        Op::SetStepMitigation {
            tree: tr,
            node: step_obd,
            control: control_id(999),
            feasibility: Some(feas(&m, "low")),
            provenance: Provenance::default(),
        },
        &ModelError::UnknownSecurityControl(control_id(999)),
    );
    for i in 0..MAX_STEP_MITIGATIONS {
        let cid = control_id(500 + i as u64);
        ok(
            &mut p,
            Op::AddSecurityControl {
                id: cid,
                name: name(&format!("Control {i}")),
                provenance: Provenance::default(),
            },
        );
        ok(
            &mut p,
            Op::SetStepMitigation {
                tree: tr,
                node: step_obd,
                control: cid,
                feasibility: Some(feas(&m, "low")),
                provenance: Provenance::default(),
            },
        );
    }
    let c_overflow = control_id(600);
    ok(
        &mut p,
        Op::AddSecurityControl {
            id: c_overflow,
            name: name("Overflow control"),
            provenance: Provenance::default(),
        },
    );
    rejects(
        &mut p,
        Op::SetStepMitigation {
            tree: tr,
            node: step_obd,
            control: c_overflow,
            feasibility: Some(feas(&m, "low")),
            provenance: Provenance::default(),
        },
        &ModelError::TooManyStepMitigations {
            tree: tr,
            node: step_obd,
            max: MAX_STEP_MITIGATIONS,
        },
    );
    // Updating an already-attached control at the cap is allowed.
    ok(
        &mut p,
        Op::SetStepMitigation {
            tree: tr,
            node: step_obd,
            control: control_id(500),
            feasibility: Some(feas(&m, "very-low")),
            provenance: Provenance::default(),
        },
    );
}

// ---------------------------------------------------------------------------
// Job 020: completeness and consistency linting.
// ---------------------------------------------------------------------------

#[test]
fn lint_all_rules_and_resolutions() {
    let m = builtin();
    let mut p = Project::new(Arc::clone(&m));
    assert!(p.lint().is_empty());

    let a1 = asset_id(1);
    ok(
        &mut p,
        Op::AddAsset {
            id: a1,
            name: name("Brake ECU"),
            properties: props(&m, &["integrity", "availability"]),
            provenance: Provenance::default(),
        },
    );
    assert_eq!(
        p.lint(),
        vec![LintFinding::new(LintCode::AssetWithoutThreats {
            asset: a1
        })]
    );

    let d1 = damage_id(10);
    ok(
        &mut p,
        Op::AddDamageScenario {
            id: d1,
            name: name("Loss of braking"),
            impacts: vec![],
            provenance: Provenance::default(),
        },
    );
    assert_eq!(
        p.lint(),
        vec![
            LintFinding::new(LintCode::AssetWithoutThreats { asset: a1 }),
            LintFinding::new(LintCode::DamageScenarioWithoutImpacts { damage: d1 }),
            LintFinding::new(LintCode::DamageScenarioUnlinked { damage: d1 }),
        ]
    );

    // Tampering violates integrity only, leaving availability untested on a1.
    let t1 = threat_id(20);
    ok(
        &mut p,
        Op::AddThreatScenario {
            id: t1,
            name: name("Spoof brake command"),
            asset: a1,
            category: threat_cat(&m, "tampering"),
            catalog_refs: vec![],
            damages: vec![],
            provenance: Provenance::default(),
        },
    );
    let availability = prop(&m, "availability");
    assert_eq!(
        p.lint(),
        vec![
            LintFinding::new(LintCode::AssetPropertyUntested {
                asset: a1,
                property: availability,
            }),
            LintFinding::new(LintCode::DamageScenarioWithoutImpacts { damage: d1 }),
            LintFinding::new(LintCode::DamageScenarioUnlinked { damage: d1 }),
            LintFinding::new(LintCode::ThreatWithoutDamages { threat: t1 }),
            LintFinding::new(LintCode::ThreatWithoutFeasibility { threat: t1 }),
            LintFinding::new(LintCode::ThreatWithoutTreatment { threat: t1 }),
        ]
    );

    // Rate d1 and link it to t1.
    let safety = cat(&m, "safety");
    ok(
        &mut p,
        Op::SetImpact {
            id: d1,
            category: safety,
            level: Some(lvl(&m, safety, "severe")),
            provenance: Provenance::default(),
        },
    );
    ok(
        &mut p,
        Op::LinkDamage {
            threat: t1,
            damage: d1,
            provenance: Provenance::default(),
        },
    );
    assert_eq!(
        p.lint(),
        vec![
            LintFinding::new(LintCode::AssetPropertyUntested {
                asset: a1,
                property: availability,
            }),
            LintFinding::new(LintCode::ThreatWithoutFeasibility { threat: t1 }),
            LintFinding::new(LintCode::ThreatWithoutTreatment { threat: t1 }),
        ]
    );

    // Add an attack tree with an empty sub-gate and an unrated step.
    let tr1 = tree_id(30);
    let root = node_id(31);
    let g_empty = node_id(32);
    let s_unrated = node_id(33);
    let s_rated = node_id(34);
    let mut tree = AttackTree::new(root, Node::gate(GateKind::Or, "Compromise brake")).expect("ok");
    let root_idx = tree.root();
    tree.add_child(root_idx, g_empty, Node::gate(GateKind::And, "Empty gate"))
        .expect("ok");
    tree.add_child(root_idx, s_unrated, Node::step("Unrated step"))
        .expect("ok");
    tree.add_child(root_idx, s_rated, Node::step("Rated step"))
        .expect("ok");
    ok(
        &mut p,
        Op::AddAttackTree {
            id: tr1,
            tree,
            step_feasibility: vec![(s_rated, feas(&m, "high"))],
            provenance: Provenance::default(),
        },
    );
    assert_eq!(
        p.lint(),
        vec![
            LintFinding::new(LintCode::AssetPropertyUntested {
                asset: a1,
                property: availability,
            }),
            LintFinding::new(LintCode::ThreatWithoutFeasibility { threat: t1 }),
            LintFinding::new(LintCode::ThreatWithoutTreatment { threat: t1 }),
            LintFinding::new(LintCode::AttackTreeUnlinked { tree: tr1 }),
            LintFinding::new(LintCode::AttackTreeGateEmpty {
                tree: tr1,
                node: g_empty,
            }),
            LintFinding::new(LintCode::AttackTreeStepUnrated {
                tree: tr1,
                node: s_unrated,
            }),
        ]
    );

    // Setting both direct feasibility and an attack tree triggers RedundantDirectFeasibility.
    ok(
        &mut p,
        Op::SetThreatFeasibility {
            threat: t1,
            level: Some(feas(&m, "medium")),
            provenance: Provenance::default(),
        },
    );
    ok(
        &mut p,
        Op::SetThreatAttackTree {
            threat: t1,
            tree: Some(tr1),
            provenance: Provenance::default(),
        },
    );
    assert_eq!(
        p.lint(),
        vec![
            LintFinding::new(LintCode::AssetPropertyUntested {
                asset: a1,
                property: availability,
            }),
            LintFinding::new(LintCode::RedundantDirectFeasibility { threat: t1 }),
            LintFinding::new(LintCode::ThreatWithoutTreatment { threat: t1 }),
            LintFinding::new(LintCode::AttackTreeGateEmpty {
                tree: tr1,
                node: g_empty,
            }),
            LintFinding::new(LintCode::AttackTreeStepUnrated {
                tree: tr1,
                node: s_unrated,
            }),
        ]
    );

    // Clear the redundant direct feasibility, rate s_unrated, and populate g_empty.
    ok(
        &mut p,
        Op::SetThreatFeasibility {
            threat: t1,
            level: None,
            provenance: Provenance::default(),
        },
    );
    ok(
        &mut p,
        Op::SetStepFeasibility {
            tree: tr1,
            node: s_unrated,
            level: Some(feas(&m, "low")),
            provenance: Provenance::default(),
        },
    );
    let mut fixed_tree =
        AttackTree::new(root, Node::gate(GateKind::Or, "Compromise brake")).expect("ok");
    let root_idx = fixed_tree.root();
    let g_idx = fixed_tree
        .add_child(root_idx, g_empty, Node::gate(GateKind::And, "Filled gate"))
        .expect("ok");
    fixed_tree
        .add_child(g_idx, s_unrated, Node::step("Now rated step"))
        .expect("ok");
    fixed_tree
        .add_child(root_idx, s_rated, Node::step("Rated step"))
        .expect("ok");
    ok(
        &mut p,
        Op::SetThreatAttackTree {
            threat: t1,
            tree: None,
            provenance: Provenance::default(),
        },
    );
    ok(&mut p, Op::RemoveAttackTree { id: tr1 });
    ok(
        &mut p,
        Op::AddAttackTree {
            id: tr1,
            tree: fixed_tree,
            step_feasibility: vec![(s_unrated, feas(&m, "low")), (s_rated, feas(&m, "high"))],
            provenance: Provenance::default(),
        },
    );
    ok(
        &mut p,
        Op::SetThreatAttackTree {
            threat: t1,
            tree: Some(tr1),
            provenance: Provenance::default(),
        },
    );

    // Set Reduce treatment on t1 without any step mitigations.
    ok(
        &mut p,
        Op::SetRiskTreatment {
            threat: t1,
            treatment: Some(RiskTreatment::Reduce),
            provenance: Provenance::default(),
        },
    );
    let c1 = control_id(40);
    ok(
        &mut p,
        Op::AddSecurityControl {
            id: c1,
            name: name("Message authentication"),
            provenance: Provenance::default(),
        },
    );
    assert_eq!(
        p.lint(),
        vec![
            LintFinding::new(LintCode::AssetPropertyUntested {
                asset: a1,
                property: availability,
            }),
            LintFinding::new(LintCode::ReduceTreatmentWithoutControls { threat: t1 }),
            LintFinding::new(LintCode::ReduceTreatmentWithoutGoal { threat: t1 }),
            LintFinding::new(LintCode::SecurityControlUnused { control: c1 }),
        ]
    );

    // Attach c1 to s_rated and link a cybersecurity goal g1 (backed by c1) to t1:
    // resolves ReduceTreatmentWithoutControls, ReduceTreatmentWithoutGoal, and SecurityControlUnused.
    let g1 = goal_id(50);
    ok(
        &mut p,
        Op::AddCybersecurityGoal {
            id: g1,
            name: name("Verify CAN message authenticity"),
            controls: vec![c1],
            provenance: Provenance::default(),
        },
    );
    ok(
        &mut p,
        Op::SetThreatGoal {
            threat: t1,
            goal: Some(g1),
            provenance: Provenance::default(),
        },
    );
    ok(
        &mut p,
        Op::SetStepMitigation {
            tree: tr1,
            node: s_rated,
            control: c1,
            feasibility: Some(feas(&m, "very-low")),
            provenance: Provenance::default(),
        },
    );
    assert_eq!(
        p.lint(),
        vec![LintFinding::new(LintCode::AssetPropertyUntested {
            asset: a1,
            property: availability,
        })]
    );

    // Add a second threat covering availability on a1; initially with Reduce and direct feasibility.
    let t2 = threat_id(21);
    ok(
        &mut p,
        Op::AddThreatScenario {
            id: t2,
            name: name("Flood CAN bus"),
            asset: a1,
            category: threat_cat(&m, "denial-of-service"),
            catalog_refs: vec![],
            damages: vec![d1],
            provenance: Provenance::default(),
        },
    );
    ok(
        &mut p,
        Op::SetThreatFeasibility {
            threat: t2,
            level: Some(feas(&m, "low")),
            provenance: Provenance::default(),
        },
    );
    ok(
        &mut p,
        Op::SetRiskTreatment {
            threat: t2,
            treatment: Some(RiskTreatment::Reduce),
            provenance: Provenance::default(),
        },
    );
    // Reduce on a direct-feasibility threat (no tree, no goal) triggers both Reduce warnings.
    assert_eq!(
        p.lint(),
        vec![
            LintFinding::new(LintCode::ReduceTreatmentWithoutControls { threat: t2 }),
            LintFinding::new(LintCode::ReduceTreatmentWithoutGoal { threat: t2 }),
        ]
    );

    // Switching t2 to Retain and linking a claim clears the remaining findings.
    let cl1 = claim_id(60);
    ok(
        &mut p,
        Op::AddCybersecurityClaim {
            id: cl1,
            name: name("Bus flood mitigated by controller hardware filter"),
            provenance: Provenance::default(),
        },
    );
    ok(
        &mut p,
        Op::SetRiskTreatment {
            threat: t2,
            treatment: Some(RiskTreatment::Retain),
            provenance: Provenance::default(),
        },
    );
    ok(
        &mut p,
        Op::SetThreatClaim {
            threat: t2,
            claim: Some(cl1),
            provenance: Provenance::default(),
        },
    );
    assert!(p.lint().is_empty());
}

#[test]
fn lint_never_contains_names_or_actors() {
    let m = builtin();
    let mut p = Project::new(Arc::clone(&m));
    let prov = Provenance::new(SENTINEL, 1_700_000_000_000, ProvenanceSource::Ai);

    let a_unused = asset_id(1);
    let a_partial = asset_id(2);
    ok(
        &mut p,
        Op::AddAsset {
            id: a_unused,
            name: name(SENTINEL),
            properties: props(&m, &["integrity"]),
            provenance: prov.clone(),
        },
    );
    ok(
        &mut p,
        Op::AddAsset {
            id: a_partial,
            name: name(SENTINEL),
            properties: props(&m, &["integrity", "availability"]),
            provenance: prov.clone(),
        },
    );

    let d_unlinked = damage_id(10);
    ok(
        &mut p,
        Op::AddDamageScenario {
            id: d_unlinked,
            name: name(SENTINEL),
            impacts: vec![],
            provenance: prov.clone(),
        },
    );

    let tr_unlinked = tree_id(20);
    let tr_linked = tree_id(21);
    let mut tree1 =
        AttackTree::new(node_id(22), Node::gate(GateKind::Or, SENTINEL)).expect("tree1");
    let r1 = tree1.root();
    tree1
        .add_child(r1, node_id(23), Node::step(SENTINEL))
        .expect("step");
    let s_incomplete = node_id(25);
    let s_redundant = node_id(26);
    tree1
        .add_child(r1, s_incomplete, Node::step(SENTINEL))
        .expect("step incomplete");
    tree1
        .add_child(r1, s_redundant, Node::step(SENTINEL))
        .expect("step redundant");
    ok(
        &mut p,
        Op::AddAttackTree {
            id: tr_unlinked,
            tree: tree1,
            step_feasibility: vec![(s_redundant, feas(&m, "medium"))],
            provenance: prov.clone(),
        },
    );
    let f_elapsed = factor(&m, "elapsed-time");
    ok(
        &mut p,
        Op::SetStepFactor {
            tree: tr_unlinked,
            node: s_incomplete,
            factor: f_elapsed,
            level: Some(factor_lvl(&m, f_elapsed, "le-1d")),
            provenance: prov.clone(),
        },
    );
    for (fk, lk) in [
        ("elapsed-time", "le-1d"),
        ("expertise", "layman"),
        ("knowledge", "public"),
        ("window-of-opportunity", "unlimited"),
        ("equipment", "standard"),
    ] {
        let f = factor(&m, fk);
        let l = factor_lvl(&m, f, lk);
        ok(
            &mut p,
            Op::SetStepFactor {
                tree: tr_unlinked,
                node: s_redundant,
                factor: f,
                level: Some(l),
                provenance: prov.clone(),
            },
        );
    }

    let tree2 = AttackTree::new(node_id(24), Node::gate(GateKind::And, SENTINEL)).expect("tree2");
    ok(
        &mut p,
        Op::AddAttackTree {
            id: tr_linked,
            tree: tree2,
            step_feasibility: vec![],
            provenance: prov.clone(),
        },
    );

    let g_linked = goal_id(50);
    let g_unlinked = goal_id(51);
    ok(
        &mut p,
        Op::AddCybersecurityGoal {
            id: g_linked,
            name: name(SENTINEL),
            controls: vec![],
            provenance: prov.clone(),
        },
    );
    ok(
        &mut p,
        Op::AddCybersecurityGoal {
            id: g_unlinked,
            name: name(SENTINEL),
            controls: vec![],
            provenance: prov.clone(),
        },
    );

    let cl_linked = claim_id(60);
    let cl_unlinked = claim_id(61);
    ok(
        &mut p,
        Op::AddCybersecurityClaim {
            id: cl_linked,
            name: name(SENTINEL),
            provenance: prov.clone(),
        },
    );
    ok(
        &mut p,
        Op::AddCybersecurityClaim {
            id: cl_unlinked,
            name: name(SENTINEL),
            provenance: prov.clone(),
        },
    );

    let t_bare = threat_id(30);
    let t_redundant = threat_id(31);
    let t_retain = threat_id(32);
    ok(
        &mut p,
        Op::AddThreatScenario {
            id: t_bare,
            name: name(SENTINEL),
            asset: a_partial,
            category: threat_cat(&m, "tampering"),
            catalog_refs: vec![],
            damages: vec![],
            provenance: prov.clone(),
        },
    );
    ok(
        &mut p,
        Op::AddThreatScenario {
            id: t_redundant,
            name: name(SENTINEL),
            asset: a_partial,
            category: threat_cat(&m, "tampering"),
            catalog_refs: vec![],
            damages: vec![],
            provenance: prov.clone(),
        },
    );
    ok(
        &mut p,
        Op::SetThreatFeasibility {
            threat: t_redundant,
            level: Some(feas(&m, "high")),
            provenance: prov.clone(),
        },
    );
    ok(
        &mut p,
        Op::SetThreatAttackTree {
            threat: t_redundant,
            tree: Some(tr_linked),
            provenance: prov.clone(),
        },
    );
    ok(
        &mut p,
        Op::SetRiskTreatment {
            threat: t_redundant,
            treatment: Some(RiskTreatment::Reduce),
            provenance: prov.clone(),
        },
    );
    ok(
        &mut p,
        Op::SetThreatClaim {
            threat: t_redundant,
            claim: Some(cl_linked),
            provenance: prov.clone(),
        },
    );

    ok(
        &mut p,
        Op::AddThreatScenario {
            id: t_retain,
            name: name(SENTINEL),
            asset: a_partial,
            category: threat_cat(&m, "tampering"),
            catalog_refs: vec![],
            damages: vec![],
            provenance: prov.clone(),
        },
    );
    ok(
        &mut p,
        Op::SetRiskTreatment {
            threat: t_retain,
            treatment: Some(RiskTreatment::Retain),
            provenance: prov.clone(),
        },
    );
    ok(
        &mut p,
        Op::SetThreatGoal {
            threat: t_retain,
            goal: Some(g_linked),
            provenance: prov.clone(),
        },
    );

    let c_unused = control_id(40);
    ok(
        &mut p,
        Op::AddSecurityControl {
            id: c_unused,
            name: name(SENTINEL),
            provenance: prov,
        },
    );

    let findings = p.lint();
    let kinds: BTreeSet<String> = findings.iter().map(|f| variant(&f.code())).collect();
    assert_eq!(kinds.len(), 22, "expected all 22 lint variants: {kinds:?}");
    for finding in &findings {
        for rendered in [
            finding.to_string(),
            format!("{finding:?}"),
            finding.code().to_string(),
            format!("{:?}", finding.code()),
            finding.severity().to_string(),
        ] {
            assert!(
                !rendered.contains(SENTINEL),
                "sentinel leaked in lint output: {rendered}"
            );
        }
    }
}

#[test]
fn lint_order_is_independent_of_history() {
    let m = builtin();
    let mut p1 = Project::new(Arc::clone(&m));
    let mut p2 = Project::new(Arc::clone(&m));

    let a1 = asset_id(1);
    let a2 = asset_id(2);
    let d1 = damage_id(10);
    let d2 = damage_id(11);
    let c1 = control_id(20);
    let c2 = control_id(21);

    // Build p1 in ascending order.
    for a in [a1, a2] {
        ok(
            &mut p1,
            Op::AddAsset {
                id: a,
                name: name("Asset"),
                properties: props(&m, &["integrity"]),
                provenance: Provenance::default(),
            },
        );
    }
    for d in [d1, d2] {
        ok(
            &mut p1,
            Op::AddDamageScenario {
                id: d,
                name: name("Damage"),
                impacts: vec![],
                provenance: Provenance::default(),
            },
        );
    }
    for c in [c1, c2] {
        ok(
            &mut p1,
            Op::AddSecurityControl {
                id: c,
                name: name("Control"),
                provenance: Provenance::default(),
            },
        );
    }

    // Build p2 in descending order with extra churn.
    let a_temp = asset_id(99);
    ok(
        &mut p2,
        Op::AddAsset {
            id: a_temp,
            name: name("Temp"),
            properties: props(&m, &["integrity"]),
            provenance: Provenance::default(),
        },
    );
    for c in [c2, c1] {
        ok(
            &mut p2,
            Op::AddSecurityControl {
                id: c,
                name: name("Control"),
                provenance: Provenance::default(),
            },
        );
    }
    for d in [d2, d1] {
        ok(
            &mut p2,
            Op::AddDamageScenario {
                id: d,
                name: name("Damage"),
                impacts: vec![],
                provenance: Provenance::default(),
            },
        );
    }
    for a in [a2, a1] {
        ok(
            &mut p2,
            Op::AddAsset {
                id: a,
                name: name("Asset"),
                properties: props(&m, &["integrity"]),
                provenance: Provenance::default(),
            },
        );
    }
    ok(&mut p2, Op::RemoveAsset { id: a_temp });

    assert_eq!(p1, p2);
    assert_eq!(p1.lint(), p2.lint());
}

// ---------------------------------------------------------------------------
// Job 021: top-k inherent and residual attack paths on Project.
// ---------------------------------------------------------------------------

#[test]
fn top_k_inherent_and_residual_paths_on_project() {
    let m = builtin();
    let mut p = Project::new(Arc::clone(&m));
    let very_low = feas(&m, "very-low");
    let low = feas(&m, "low");
    let med = feas(&m, "medium");
    let high = feas(&m, "high");

    let a1 = asset_id(1);
    ok(
        &mut p,
        Op::AddAsset {
            id: a1,
            name: name("Telematics unit"),
            properties: m.all_properties(),
            provenance: Provenance::default(),
        },
    );
    let t1 = threat_id(10);
    let t_no_tree = threat_id(11);
    let cat = threat_cat(&m, "tampering");
    for id in [t1, t_no_tree] {
        ok(
            &mut p,
            Op::AddThreatScenario {
                id,
                name: name("Inject command"),
                asset: a1,
                category: cat,
                catalog_refs: vec![],
                damages: vec![],
                provenance: Provenance::default(),
            },
        );
    }

    // Build tree: OR(AND(s_prep, s_obd), s_ota, s_phys)
    let (n_root, n_and, n_prep, n_obd, n_ota, n_phys) = (
        node_id(1),
        node_id(2),
        node_id(3),
        node_id(4),
        node_id(5),
        node_id(6),
    );
    let mut tree = AttackTree::new(n_root, Node::gate(GateKind::Or, "Goal")).expect("root");
    let g_and = tree
        .add_child(tree.root(), n_and, Node::gate(GateKind::And, "OBD chain"))
        .expect("and");
    let i_prep = tree
        .add_child(g_and, n_prep, Node::step("Prep"))
        .expect("prep");
    let i_obd = tree
        .add_child(g_and, n_obd, Node::step("OBD"))
        .expect("obd");
    let i_ota = tree
        .add_child(tree.root(), n_ota, Node::step("OTA"))
        .expect("ota");
    let i_phys = tree
        .add_child(tree.root(), n_phys, Node::step("Physical"))
        .expect("phys");

    let tr = tree_id(20);
    ok(
        &mut p,
        Op::AddAttackTree {
            id: tr,
            tree,
            step_feasibility: vec![(n_prep, high), (n_obd, high), (n_ota, med), (n_phys, low)],
            provenance: Provenance::default(),
        },
    );
    ok(
        &mut p,
        Op::SetThreatAttackTree {
            threat: t1,
            tree: Some(tr),
            provenance: Provenance::default(),
        },
    );

    // Inherent top-3 paths: [prep, obd] (high), [ota] (medium), [phys] (low).
    let inherent = p.top_k_threat_paths(t1, 3).expect("inherent paths");
    assert_eq!(inherent.len(), 3);
    assert_eq!(inherent[0].steps(), &[i_prep, i_obd]);
    assert_eq!(*inherent[0].feasibility(), high);
    assert_eq!(inherent[0].bottleneck(), i_prep);
    assert_eq!(inherent[1].steps(), &[i_ota]);
    assert_eq!(*inherent[1].feasibility(), med);
    assert_eq!(inherent[2].steps(), &[i_phys]);
    assert_eq!(*inherent[2].feasibility(), low);
    assert_eq!(p.top_k_tree_paths(tr, 3).expect("tree paths"), inherent);

    // Mitigate `n_obd` to `very-low`: now [ota] (medium) becomes #1, [phys] (low) #2,
    // and [prep, obd] (very-low, bottleneck = i_obd) drops to #3.
    let ctrl = control_id(30);
    ok(
        &mut p,
        Op::AddSecurityControl {
            id: ctrl,
            name: name("Lock OBD"),
            provenance: Provenance::default(),
        },
    );
    ok(
        &mut p,
        Op::SetStepMitigation {
            tree: tr,
            node: n_obd,
            control: ctrl,
            feasibility: Some(very_low),
            provenance: Provenance::default(),
        },
    );

    let residual = p
        .top_k_residual_threat_paths(t1, 3)
        .expect("residual paths");
    assert_eq!(residual.len(), 3);
    assert_eq!(residual[0].steps(), &[i_ota]);
    assert_eq!(*residual[0].feasibility(), med);
    assert_eq!(residual[1].steps(), &[i_phys]);
    assert_eq!(*residual[1].feasibility(), low);
    assert_eq!(residual[2].steps(), &[i_prep, i_obd]);
    assert_eq!(*residual[2].feasibility(), very_low);
    assert_eq!(residual[2].bottleneck(), i_obd);
    assert_eq!(
        p.top_k_residual_tree_paths(tr, 3)
            .expect("residual tree paths"),
        residual
    );

    // Error paths.
    let missing_tree = tree_id(99);
    let missing_threat = threat_id(99);
    assert_eq!(
        p.top_k_tree_paths(missing_tree, 1),
        Err(ModelError::UnknownAttackTree(missing_tree))
    );
    assert_eq!(
        p.top_k_residual_tree_paths(missing_tree, 1),
        Err(ModelError::UnknownAttackTree(missing_tree))
    );
    assert_eq!(
        p.top_k_threat_paths(missing_threat, 1),
        Err(ModelError::UnknownThreatScenario(missing_threat))
    );
    assert_eq!(
        p.top_k_residual_threat_paths(missing_threat, 1),
        Err(ModelError::UnknownThreatScenario(missing_threat))
    );
    assert_eq!(
        p.top_k_threat_paths(t_no_tree, 1),
        Err(ModelError::NoAttackTree(t_no_tree))
    );
    assert_eq!(
        p.top_k_residual_threat_paths(t_no_tree, 1),
        Err(ModelError::NoAttackTree(t_no_tree))
    );
}

#[test]
fn project_diff_tracks_structural_changes_and_risk_deltas() {
    let m = builtin();
    let other_m = second();
    let p_other_m = Project::new(Arc::clone(&other_m));
    let mut base = Project::new(Arc::clone(&m));

    // MethodologyMismatch is reported when comparing different methodologies.
    assert_eq!(
        base.diff(&p_other_m),
        Err(ModelError::MethodologyMismatch {
            expected: m.key().into(),
            expected_version: m.version().into(),
            found: other_m.key().into(),
            found_version: other_m.version().into(),
        })
    );

    let sentinel = "DIFF_SECRET_SENTINEL_NAME";
    let a1 = asset_id(1);
    let a2 = asset_id(2);
    let d1 = damage_id(1);
    let t1 = threat_id(1);
    let t2 = threat_id(2);
    let tr1 = tree_id(1);
    let ctrl1 = control_id(1);

    let safety = cat(&m, "safety");
    let moderate = lvl(&m, safety, "moderate");
    let severe = lvl(&m, safety, "severe");
    let low = feas(&m, "low");
    let high = feas(&m, "high");
    let very_low = feas(&m, "very-low");
    let r2 = risk(&m, "r2");
    let r3 = risk(&m, "r3");
    let r5 = risk(&m, "r5");
    let spoof = threat_cat(&m, "spoofing");

    ok(
        &mut base,
        Op::AddAsset {
            id: a1,
            name: sentinel.into(),
            properties: m.all_properties(),
            provenance: Provenance::default(),
        },
    );
    ok(
        &mut base,
        Op::AddAsset {
            id: a2,
            name: name("Removed asset"),
            properties: m.all_properties(),
            provenance: Provenance::default(),
        },
    );
    ok(
        &mut base,
        Op::AddDamageScenario {
            id: d1,
            name: sentinel.into(),
            impacts: vec![(safety, moderate)],
            provenance: Provenance::default(),
        },
    );

    let n_root = node_id(1);
    let n_step = node_id(2);
    let mut tree = AttackTree::new(n_root, Node::gate(GateKind::Or, sentinel)).expect("root");
    tree.add_child(tree.root(), n_step, Node::step(sentinel))
        .expect("step");
    ok(
        &mut base,
        Op::AddAttackTree {
            id: tr1,
            tree,
            step_feasibility: vec![(n_step, high)],
            provenance: Provenance::default(),
        },
    );
    ok(
        &mut base,
        Op::AddSecurityControl {
            id: ctrl1,
            name: sentinel.into(),
            provenance: Provenance::default(),
        },
    );
    ok(
        &mut base,
        Op::AddThreatScenario {
            id: t1,
            name: sentinel.into(),
            asset: a1,
            category: spoof,
            catalog_refs: vec![],
            damages: vec![d1],
            provenance: Provenance::default(),
        },
    );
    ok(
        &mut base,
        Op::SetThreatAttackTree {
            threat: t1,
            tree: Some(tr1),
            provenance: Provenance::default(),
        },
    );
    ok(
        &mut base,
        Op::AddThreatScenario {
            id: t2,
            name: sentinel.into(),
            asset: a2,
            category: spoof,
            catalog_refs: vec![],
            damages: vec![d1],
            provenance: Provenance::default(),
        },
    );
    ok(
        &mut base,
        Op::SetThreatFeasibility {
            threat: t2,
            level: Some(low),
            provenance: Provenance::default(),
        },
    );

    // Reflexive diff is empty.
    let self_diff = base.diff(&base).expect("self diff");
    assert!(self_diff.is_empty());
    assert!(self_diff.risk_deltas().is_empty());

    // Build `curr` by:
    // - removing `t2` and `a2`
    // - adding `a3` and `t3` (newly evaluated)
    // - raising `d1` impact from moderate to severe (indirectly raises `t1` inherent risk r4 -> r5)
    // - attaching `ctrl1` mitigation on `tr1` step to very-low (lowers `t1` residual risk r4 -> r2)
    // - renaming `ctrl1`
    let mut curr = base.clone();
    ok(
        &mut curr,
        Op::SetThreatFeasibility {
            threat: t2,
            level: None,
            provenance: Provenance::default(),
        },
    );
    ok(
        &mut curr,
        Op::UnlinkDamage {
            threat: t2,
            damage: d1,
            provenance: Provenance::default(),
        },
    );
    ok(&mut curr, Op::RemoveThreatScenario { id: t2 });
    ok(&mut curr, Op::RemoveAsset { id: a2 });
    let a3 = asset_id(3);
    let t3 = threat_id(3);
    ok(
        &mut curr,
        Op::AddAsset {
            id: a3,
            name: sentinel.into(),
            properties: m.all_properties(),
            provenance: Provenance::default(),
        },
    );
    ok(
        &mut curr,
        Op::AddThreatScenario {
            id: t3,
            name: sentinel.into(),
            asset: a3,
            category: spoof,
            catalog_refs: vec![],
            damages: vec![d1],
            provenance: Provenance::default(),
        },
    );
    ok(
        &mut curr,
        Op::SetThreatFeasibility {
            threat: t3,
            level: Some(high),
            provenance: Provenance::default(),
        },
    );
    ok(
        &mut curr,
        Op::SetImpact {
            id: d1,
            category: safety,
            level: Some(severe),
            provenance: Provenance::default(),
        },
    );
    ok(
        &mut curr,
        Op::SetStepMitigation {
            tree: tr1,
            node: n_step,
            control: ctrl1,
            feasibility: Some(very_low),
            provenance: Provenance::default(),
        },
    );
    ok(
        &mut curr,
        Op::RenameSecurityControl {
            id: ctrl1,
            name: name("Updated control"),
            provenance: Provenance::default(),
        },
    );

    let diff = base.diff(&curr).expect("diff");
    assert!(!diff.is_empty());
    assert_eq!(diff.assets().added(), &[a3]);
    assert_eq!(diff.assets().removed(), &[a2]);
    assert!(diff.assets().modified().is_empty());
    assert_eq!(diff.damage_scenarios().modified(), &[d1]);
    assert_eq!(diff.attack_trees().modified(), &[tr1]);
    assert_eq!(diff.security_controls().modified(), &[ctrl1]);
    assert_eq!(diff.threat_scenarios().added(), &[t3]);
    assert_eq!(diff.threat_scenarios().removed(), &[t2]);
    assert!(diff.threat_scenarios().modified().is_empty());

    // Even though `t1` was not directly modified, its linked damage `d1` and tree `tr1` changed!
    let deltas = diff.risk_deltas();
    assert_eq!(deltas.len(), 3);
    assert_eq!(deltas[0].threat(), t1);
    assert_eq!(
        deltas[0].inherent().before(),
        Some(aegis_model::RiskSnapshot::new(high, Some(r3)))
    );
    assert_eq!(
        deltas[0].inherent().after(),
        Some(aegis_model::RiskSnapshot::new(high, Some(r5)))
    );
    assert_eq!(
        deltas[0].inherent().change(),
        aegis_model::RiskChange::Increased
    );
    assert_eq!(
        deltas[0].residual().before(),
        Some(aegis_model::RiskSnapshot::new(high, Some(r3)))
    );
    assert_eq!(
        deltas[0].residual().after(),
        Some(aegis_model::RiskSnapshot::new(very_low, Some(r2)))
    );
    assert_eq!(
        deltas[0].residual().change(),
        aegis_model::RiskChange::Decreased
    );

    assert_eq!(deltas[1].threat(), t2);
    assert_eq!(
        deltas[1].inherent().change(),
        aegis_model::RiskChange::NoLongerEvaluated
    );
    assert_eq!(deltas[1].inherent().after(), None);

    assert_eq!(deltas[2].threat(), t3);
    assert_eq!(
        deltas[2].inherent().change(),
        aegis_model::RiskChange::NewlyEvaluated
    );
    assert_eq!(
        deltas[2].inherent().after(),
        Some(aegis_model::RiskSnapshot::new(high, Some(r5)))
    );
    assert_eq!(diff.changed_risks().count(), 3);

    // Rule R6: Debug output of `ProjectDiff` must never leak entity names.
    let debug_text = format!("{diff:?}");
    assert!(
        !debug_text.contains(sentinel),
        "diff leaked sentinel: {debug_text}"
    );
}

#[test]
fn cybersecurity_goals_claims_lifecycle_lint_and_diff() {
    use aegis_model::{EntityKind, LintCode, LintFinding, MAX_GOAL_CONTROLS};
    use common::{claim_id, control_id, goal_id};

    let m = builtin();
    let mut p = Project::new(Arc::clone(&m));
    let a1 = asset_id(1);
    let d1 = damage_id(1);
    let t_reduce = threat_id(1);
    let t_retain = threat_id(2);
    let t_avoid = threat_id(3);
    let c1 = control_id(1);
    let c2 = control_id(2);
    let g1 = goal_id(1);
    let g2 = goal_id(2);
    let cl1 = claim_id(1);
    let cl2 = claim_id(2);
    let spoof = threat_cat(&m, "spoofing");
    let safety = cat(&m, "safety");
    let moderate = lvl(&m, safety, "moderate");
    let high = feas(&m, "high");

    ok(
        &mut p,
        Op::AddAsset {
            id: a1,
            name: name("Brake ECU"),
            properties: m.all_properties(),
            provenance: Provenance::default(),
        },
    );
    ok(
        &mut p,
        Op::AddDamageScenario {
            id: d1,
            name: name("Collision"),
            impacts: vec![(safety, moderate)],
            provenance: Provenance::default(),
        },
    );
    for (tid, label) in [
        (t_reduce, "Spoof CAN frame"),
        (t_retain, "Physical tap"),
        (t_avoid, "Debug port"),
    ] {
        ok(
            &mut p,
            Op::AddThreatScenario {
                id: tid,
                name: name(label),
                asset: a1,
                category: spoof,
                catalog_refs: vec![],
                damages: vec![d1],
                provenance: Provenance::default(),
            },
        );
        ok(
            &mut p,
            Op::SetThreatFeasibility {
                threat: tid,
                level: Some(high),
                provenance: Provenance::default(),
            },
        );
    }

    // Add controls, goals, and claims.
    ok(
        &mut p,
        Op::AddSecurityControl {
            id: c1,
            name: name("SecOC"),
            provenance: Provenance::default(),
        },
    );
    ok(
        &mut p,
        Op::AddSecurityControl {
            id: c2,
            name: name("HSM key store"),
            provenance: Provenance::default(),
        },
    );

    // Goal validation errors: empty name, duplicate control, unknown control, > MAX_GOAL_CONTROLS.
    rejects(
        &mut p,
        Op::AddCybersecurityGoal {
            id: g1,
            name: "  ".into(),
            controls: vec![],
            provenance: Provenance::default(),
        },
        &ModelError::EmptyName {
            kind: EntityKind::CybersecurityGoal,
        },
    );
    rejects(
        &mut p,
        Op::AddCybersecurityGoal {
            id: g1,
            name: name("Goal"),
            controls: vec![c1, c1],
            provenance: Provenance::default(),
        },
        &ModelError::DuplicateGoalControl {
            goal: g1,
            control: c1,
        },
    );
    rejects(
        &mut p,
        Op::AddCybersecurityGoal {
            id: g1,
            name: name("Goal"),
            controls: vec![control_id(99)],
            provenance: Provenance::default(),
        },
        &ModelError::UnknownSecurityControl(control_id(99)),
    );
    let too_many: Vec<_> = (0..=MAX_GOAL_CONTROLS as u64)
        .map(|i| control_id(100 + i))
        .collect();
    rejects(
        &mut p,
        Op::AddCybersecurityGoal {
            id: g1,
            name: name("Goal"),
            controls: too_many,
            provenance: Provenance::default(),
        },
        &ModelError::TooManyGoalControls {
            goal: g1,
            max: MAX_GOAL_CONTROLS,
        },
    );

    // Add g1 with [c2, c1] out of order -> canonicalized to [c1, c2].
    let inv_add_g1 = ok(
        &mut p,
        Op::AddCybersecurityGoal {
            id: g1,
            name: name("Ensure CAN frame authenticity"),
            controls: vec![c2, c1],
            provenance: Provenance::default(),
        },
    );
    assert_eq!(inv_add_g1, Op::RemoveCybersecurityGoal { id: g1 });
    let g1_idx = p.cybersecurity_goal_idx(&g1).expect("g1");
    let c1_idx = p.security_control_idx(&c1).expect("c1");
    let c2_idx = p.security_control_idx(&c2).expect("c2");
    assert_eq!(
        p.cybersecurity_goal(g1_idx).expect("g1").controls(),
        &[c1_idx, c2_idx]
    );

    // Duplicate goal rejected.
    rejects(
        &mut p,
        Op::AddCybersecurityGoal {
            id: g1,
            name: name("Dup"),
            controls: vec![],
            provenance: Provenance::default(),
        },
        &ModelError::DuplicateCybersecurityGoal(g1),
    );

    // Add g2 without controls, cl1 and cl2.
    ok(
        &mut p,
        Op::AddCybersecurityGoal {
            id: g2,
            name: name("Unlinked empty goal"),
            controls: vec![],
            provenance: Provenance::default(),
        },
    );
    ok(
        &mut p,
        Op::AddCybersecurityClaim {
            id: cl1,
            name: name("Harness is inside locked cabin"),
            provenance: Provenance::default(),
        },
    );
    ok(
        &mut p,
        Op::AddCybersecurityClaim {
            id: cl2,
            name: name("Unlinked claim"),
            provenance: Provenance::default(),
        },
    );
    rejects(
        &mut p,
        Op::AddCybersecurityClaim {
            id: cl1,
            name: name("Dup"),
            provenance: Provenance::default(),
        },
        &ModelError::DuplicateCybersecurityClaim(cl1),
    );

    // Set treatments on t_reduce (Reduce), t_retain (Retain), t_avoid (Avoid).
    ok(
        &mut p,
        Op::SetRiskTreatment {
            threat: t_reduce,
            treatment: Some(RiskTreatment::Reduce),
            provenance: Provenance::default(),
        },
    );
    ok(
        &mut p,
        Op::SetRiskTreatment {
            threat: t_retain,
            treatment: Some(RiskTreatment::Retain),
            provenance: Provenance::default(),
        },
    );
    ok(
        &mut p,
        Op::SetRiskTreatment {
            threat: t_avoid,
            treatment: Some(RiskTreatment::Avoid),
            provenance: Provenance::default(),
        },
    );

    // Before linking goals/claims to threats, lint reports:
    // - ReduceTreatmentWithoutGoal on t_reduce
    // - RetainOrShareTreatmentWithoutClaim on t_retain
    // - CybersecurityGoalUnlinked on g1 and g2
    // - CybersecurityGoalWithoutControls on g2
    // - CybersecurityClaimUnlinked on cl1 and cl2
    let findings = p.lint();
    assert!(
        findings.contains(&LintFinding::new(LintCode::ReduceTreatmentWithoutGoal {
            threat: t_reduce
        }))
    );
    assert!(findings.contains(&LintFinding::new(
        LintCode::RetainOrShareTreatmentWithoutClaim { threat: t_retain }
    )));
    assert!(
        findings.contains(&LintFinding::new(LintCode::CybersecurityGoalUnlinked {
            goal: g1,
        }))
    );
    assert!(findings.contains(&LintFinding::new(
        LintCode::CybersecurityGoalWithoutControls { goal: g2 }
    )));
    assert!(
        findings.contains(&LintFinding::new(LintCode::CybersecurityClaimUnlinked {
            claim: cl1,
        }))
    );

    // Link g1 to t_reduce and cl1 to t_retain; also link g1 and cl1 to t_avoid (triggers mismatch warnings).
    let base = p.clone();
    ok(
        &mut p,
        Op::SetThreatGoal {
            threat: t_reduce,
            goal: Some(g1),
            provenance: Provenance::default(),
        },
    );
    ok(
        &mut p,
        Op::SetThreatClaim {
            threat: t_retain,
            claim: Some(cl1),
            provenance: Provenance::default(),
        },
    );
    ok(
        &mut p,
        Op::SetThreatGoal {
            threat: t_avoid,
            goal: Some(g1),
            provenance: Provenance::default(),
        },
    );
    ok(
        &mut p,
        Op::SetThreatClaim {
            threat: t_avoid,
            claim: Some(cl1),
            provenance: Provenance::default(),
        },
    );

    let findings2 = p.lint();
    assert!(!findings2.iter().any(|f| {
        matches!(
            f.code(),
            LintCode::ReduceTreatmentWithoutGoal { .. }
                | LintCode::RetainOrShareTreatmentWithoutClaim { .. }
        )
    }));
    assert!(
        findings2.contains(&LintFinding::new(LintCode::GoalOnNonReduceThreat {
            threat: t_avoid,
            goal: g1,
        }))
    );
    assert!(
        findings2.contains(&LintFinding::new(LintCode::ClaimOnNonRetainOrShareThreat {
            threat: t_avoid,
            claim: cl1,
        }))
    );
    assert!(
        !findings2.contains(&LintFinding::new(LintCode::CybersecurityGoalUnlinked {
            goal: g1,
        })),
        "g1 is now linked"
    );
    assert!(
        !findings2.contains(&LintFinding::new(LintCode::CybersecurityClaimUnlinked {
            claim: cl1,
        })),
        "cl1 is now linked"
    );

    // Diff between `base` and `p` reports modified threats.
    let diff = base.diff(&p).expect("diff");
    assert_eq!(
        diff.threat_scenarios().modified(),
        &[t_reduce, t_retain, t_avoid]
    );
    assert!(diff.cybersecurity_goals().is_empty());
    assert!(diff.cybersecurity_claims().is_empty());
}

// ---------------------------------------------------------------------------
// Job 025: Incremental evaluation cache (`ProjectEvalCache` + `apply_cached`)
// ---------------------------------------------------------------------------

#[test]
fn project_eval_cache_incremental_updates_and_counters() {
    let m = builtin();
    let mut p = Project::new(Arc::clone(&m));

    let a1 = asset_id(1);
    let d1 = damage_id(1);
    let d2 = damage_id(2);
    let tr1 = tree_id(1);
    let tr2 = tree_id(2);
    let t1 = threat_id(1);
    let t2 = threat_id(2);
    let c1 = control_id(1);
    let g1 = goal_id(1);
    let cl1 = claim_id(1);

    let r1 = node_id(10);
    let mid1 = node_id(11);
    let s1 = node_id(12);
    let s2 = node_id(13);
    let s3 = node_id(14);
    let mut tree1 = AttackTree::new(r1, Node::gate(GateKind::Or, "Root")).expect("root");
    let g_mid = tree1
        .add_child(tree1.root(), mid1, Node::gate(GateKind::And, "Mid"))
        .expect("mid");
    tree1.add_child(g_mid, s1, Node::step("S1")).expect("s1");
    tree1.add_child(g_mid, s2, Node::step("S2")).expect("s2");
    tree1
        .add_child(tree1.root(), s3, Node::step("S3"))
        .expect("s3");

    let r2 = node_id(20);
    let s20 = node_id(21);
    let mut tree2 = AttackTree::new(r2, Node::gate(GateKind::Or, "Root2")).expect("root2");
    tree2
        .add_child(tree2.root(), s20, Node::step("S20"))
        .expect("s20");

    let safety = cat(&m, "safety");
    ok(
        &mut p,
        Op::AddAsset {
            id: a1,
            name: "ECU".into(),
            properties: props(&m, &["integrity"]),
            provenance: Provenance::default(),
        },
    );
    ok(
        &mut p,
        Op::AddDamageScenario {
            id: d1,
            name: "Braking loss".into(),
            impacts: vec![(safety, lvl(&m, safety, "severe"))],
            provenance: Provenance::default(),
        },
    );
    ok(
        &mut p,
        Op::AddDamageScenario {
            id: d2,
            name: "Minor fault".into(),
            impacts: vec![(safety, lvl(&m, safety, "negligible"))],
            provenance: Provenance::default(),
        },
    );
    ok(
        &mut p,
        Op::AddAttackTree {
            id: tr1,
            tree: tree1,
            step_feasibility: vec![
                (s1, feas(&m, "medium")),
                (s2, feas(&m, "medium")),
                (s3, feas(&m, "low")),
            ],
            provenance: Provenance::default(),
        },
    );
    ok(
        &mut p,
        Op::AddAttackTree {
            id: tr2,
            tree: tree2,
            step_feasibility: vec![(s20, feas(&m, "low"))],
            provenance: Provenance::default(),
        },
    );
    ok(
        &mut p,
        Op::AddThreatScenario {
            id: t1,
            name: "Threat 1".into(),
            asset: a1,
            category: threat_cat(&m, "tampering"),
            catalog_refs: vec![],
            damages: vec![d1],
            provenance: Provenance::default(),
        },
    );
    ok(
        &mut p,
        Op::SetThreatAttackTree {
            threat: t1,
            tree: Some(tr1),
            provenance: Provenance::default(),
        },
    );
    ok(
        &mut p,
        Op::AddThreatScenario {
            id: t2,
            name: "Threat 2".into(),
            asset: a1,
            category: threat_cat(&m, "tampering"),
            catalog_refs: vec![],
            damages: vec![d2],
            provenance: Provenance::default(),
        },
    );
    ok(
        &mut p,
        Op::SetThreatAttackTree {
            threat: t2,
            tree: Some(tr2),
            provenance: Provenance::default(),
        },
    );
    ok(
        &mut p,
        Op::AddSecurityControl {
            id: c1,
            name: "HSM".into(),
            provenance: Provenance::default(),
        },
    );

    let mut cache = p.eval_cache();
    assert_eq!(cache.evaluate_tree(&p, tr1).cloned(), p.evaluate_tree(tr1));
    assert_eq!(cache.evaluate_risk(&p, t1).cloned(), p.evaluate_risk(t1));
    assert_eq!(cache.evaluate_risk(&p, t2).cloned(), p.evaluate_risk(t2));

    // 1. Metadata / governance ops do zero recomputation.
    cache.reset_counters();
    p.apply_cached(
        Op::RenameThreatScenario {
            id: t1,
            name: "Threat 1 renamed".into(),
            provenance: Provenance::default(),
        },
        &mut cache,
    )
    .expect("rename");
    p.apply_cached(
        Op::AddCybersecurityGoal {
            id: g1,
            name: "Goal 1".into(),
            controls: vec![c1],
            provenance: Provenance::default(),
        },
        &mut cache,
    )
    .expect("add goal");
    p.apply_cached(
        Op::AddCybersecurityClaim {
            id: cl1,
            name: "Claim 1".into(),
            provenance: Provenance::default(),
        },
        &mut cache,
    )
    .expect("add claim");
    p.apply_cached(
        Op::SetRiskTreatment {
            threat: t1,
            treatment: Some(RiskTreatment::Reduce),
            provenance: Provenance::default(),
        },
        &mut cache,
    )
    .expect("set treatment");
    p.apply_cached(
        Op::SetThreatGoal {
            threat: t1,
            goal: Some(g1),
            provenance: Provenance::default(),
        },
        &mut cache,
    )
    .expect("set goal");
    assert_eq!(cache.recomputed_nodes(), 0);
    assert_eq!(cache.recomputed_threats(), 0);

    // 2. Raising s1 from medium to high under AND(s1, s2=medium) leaves Mid at medium (early cutoff!).
    // Inherent + residual each update s1 and fold Mid (2 nodes each = 4 nodes total), and Root is NOT refolded.
    cache.reset_counters();
    p.apply_cached(
        Op::SetStepFeasibility {
            tree: tr1,
            node: s1,
            level: Some(feas(&m, "high")),
            provenance: Provenance::default(),
        },
        &mut cache,
    )
    .expect("set s1 high");
    assert_eq!(cache.recomputed_nodes(), 4);
    // Root value didn't change, and only Mid's tie-break (still s2=medium vs s1=high -> s2) changed,
    // wait: s1 went from medium (tied with s2, so Selected(s1)) to high (so AND picks s2 -> Selected(s2))!
    // Thus Mid's derivation changed, so t1's tree_evaluation is synced in O(1), while t2 is untouched.
    assert_eq!(cache.recomputed_threats(), 2); // t1 inherent + t1 residual; t2 untouched
    assert_eq!(cache.evaluate_tree(&p, tr1).cloned(), p.evaluate_tree(tr1));
    assert_eq!(cache.evaluate_risk(&p, t1).cloned(), p.evaluate_risk(t1));

    // 3. SetStepMitigation only touches residual evaluation of tr1 and residual evaluation of t1.
    cache.reset_counters();
    p.apply_cached(
        Op::SetStepMitigation {
            tree: tr1,
            node: s2,
            control: c1,
            feasibility: Some(feas(&m, "very-low")),
            provenance: Provenance::default(),
        },
        &mut cache,
    )
    .expect("mitigate s2");
    assert_eq!(cache.recomputed_nodes(), 3); // residual only: s2 + Mid + Root
    assert_eq!(cache.recomputed_threats(), 1); // t1 residual only!
    assert_eq!(
        cache.evaluate_residual_tree(&p, tr1).cloned(),
        p.evaluate_residual_tree(tr1)
    );
    assert_eq!(
        cache.evaluate_residual_risk(&p, t1).cloned(),
        p.evaluate_residual_risk(t1)
    );

    // 4. SetImpact on d1 only recomputes t1 (2 evaluations: inherent + residual), 0 tree nodes.
    cache.reset_counters();
    p.apply_cached(
        Op::SetImpact {
            id: d1,
            category: safety,
            level: Some(lvl(&m, safety, "moderate")),
            provenance: Provenance::default(),
        },
        &mut cache,
    )
    .expect("set impact");
    assert_eq!(cache.recomputed_nodes(), 0);
    assert_eq!(cache.recomputed_threats(), 2);
    assert_eq!(cache.evaluate_risk(&p, t1).cloned(), p.evaluate_risk(t1));
    assert_eq!(
        cache.evaluate_residual_risk(&p, t1).cloned(),
        p.evaluate_residual_risk(t1)
    );

    // 5. Failed op via apply_cached leaves project and cache unchanged and counters at 0.
    cache.reset_counters();
    let err = p.apply_cached(Op::RemoveAsset { id: asset_id(999) }, &mut cache);
    assert!(err.is_err());
    assert_eq!(cache.recomputed_nodes(), 0);
    assert_eq!(cache.recomputed_threats(), 0);
}

// ---------------------------------------------------------------------------
// Job 026: Choke points, hitting set, and tipping-point sensitivity.
// ---------------------------------------------------------------------------

#[test]
fn choke_points_and_hitting_set_on_project() {
    let m = builtin();
    let mut p = Project::new(Arc::clone(&m));
    let (very_low, low, med, high) = (
        feas(&m, "very-low"),
        feas(&m, "low"),
        feas(&m, "medium"),
        feas(&m, "high"),
    );

    let asset = asset_id(1);
    ok(
        &mut p,
        Op::AddAsset {
            id: asset,
            name: name(SENTINEL),
            properties: props(&m, &["integrity"]),
            provenance: Provenance::default(),
        },
    );
    let t1 = threat_id(10);
    let t_no_tree = threat_id(11);
    for id in [t1, t_no_tree] {
        ok(
            &mut p,
            Op::AddThreatScenario {
                id,
                name: name(SENTINEL),
                asset,
                category: threat_cat(&m, "tampering"),
                catalog_refs: vec![],
                damages: vec![],
                provenance: Provenance::default(),
            },
        );
    }

    // Build shared-step DAG: OR(AND(s_shared, s_obd), AND(s_shared, s_ota), s_phys)
    let (n_root, n_g1, n_g2, n_shared, n_obd, n_ota, n_phys) = (
        node_id(1),
        node_id(2),
        node_id(3),
        node_id(4),
        node_id(5),
        node_id(6),
        node_id(7),
    );
    let mut tree = AttackTree::new(n_root, Node::gate(GateKind::Or, SENTINEL)).expect("root");
    let g1 = tree
        .add_child(tree.root(), n_g1, Node::gate(GateKind::And, SENTINEL))
        .expect("g1");
    let g2 = tree
        .add_child(tree.root(), n_g2, Node::gate(GateKind::And, SENTINEL))
        .expect("g2");
    let i_shared = tree
        .add_child(g1, n_shared, Node::step(SENTINEL))
        .expect("shared");
    let _i_obd = tree
        .add_child(g1, n_obd, Node::step(SENTINEL))
        .expect("obd");
    tree.link(g2, i_shared).expect("link shared");
    let _i_ota = tree
        .add_child(g2, n_ota, Node::step(SENTINEL))
        .expect("ota");
    let i_phys = tree
        .add_child(tree.root(), n_phys, Node::step(SENTINEL))
        .expect("phys");

    let tr = tree_id(20);
    ok(
        &mut p,
        Op::AddAttackTree {
            id: tr,
            tree,
            step_feasibility: vec![(n_shared, high), (n_obd, high), (n_ota, med), (n_phys, low)],
            provenance: Provenance::default(),
        },
    );
    ok(
        &mut p,
        Op::SetThreatAttackTree {
            threat: t1,
            tree: Some(tr),
            provenance: Provenance::default(),
        },
    );

    // Inherent top-3 paths: [shared, obd] (high), [shared, ota] (med), [phys] (low).
    let cp = p.threat_choke_points(t1, 3).expect("choke points");
    assert_eq!(p.tree_choke_points(tr, 3).expect("tree cp"), cp);
    assert_eq!(cp.len(), 4);
    assert_eq!(cp[0].step(), i_shared);
    assert_eq!(cp[0].path_count(), 2);
    assert_eq!(cp[0].bottleneck_count(), 1);
    assert_eq!(*cp[0].easiest_path_feasibility(), high);
    // Rule R6: Debug contains no user-supplied names/labels.
    assert!(!format!("{cp:?}").contains(SENTINEL));

    // Hitting set: top-2 paths covered by [i_shared], top-3 covered by [i_shared, i_phys].
    assert_eq!(p.threat_hitting_set(t1, 2), Ok(vec![i_shared]));
    assert_eq!(p.tree_hitting_set(tr, 2), Ok(vec![i_shared]));
    assert_eq!(p.threat_hitting_set(t1, 3), Ok(vec![i_shared, i_phys]));
    assert_eq!(p.tree_hitting_set(tr, 3), Ok(vec![i_shared, i_phys]));

    // Mitigate `n_shared` to `very-low`: now top-1 path is [i_phys] (low)!
    let ctrl = control_id(30);
    ok(
        &mut p,
        Op::AddSecurityControl {
            id: ctrl,
            name: name(SENTINEL),
            provenance: Provenance::default(),
        },
    );
    ok(
        &mut p,
        Op::SetStepMitigation {
            tree: tr,
            node: n_shared,
            control: ctrl,
            feasibility: Some(very_low),
            provenance: Provenance::default(),
        },
    );

    let res_cp1 = p.residual_threat_choke_points(t1, 1).expect("res cp 1");
    assert_eq!(
        p.residual_tree_choke_points(tr, 1).expect("res tree cp 1"),
        res_cp1
    );
    assert_eq!(res_cp1.len(), 1);
    assert_eq!(res_cp1[0].step(), i_phys);
    assert_eq!(*res_cp1[0].easiest_path_feasibility(), low);
    assert_eq!(p.residual_threat_hitting_set(t1, 1), Ok(vec![i_phys]));
    assert_eq!(p.residual_tree_hitting_set(tr, 1), Ok(vec![i_phys]));

    // Error paths.
    let missing_tree = tree_id(99);
    let missing_threat = threat_id(99);
    assert_eq!(
        p.tree_choke_points(missing_tree, 1),
        Err(ModelError::UnknownAttackTree(missing_tree))
    );
    assert_eq!(
        p.residual_tree_choke_points(missing_tree, 1),
        Err(ModelError::UnknownAttackTree(missing_tree))
    );
    assert_eq!(
        p.threat_choke_points(missing_threat, 1),
        Err(ModelError::UnknownThreatScenario(missing_threat))
    );
    assert_eq!(
        p.residual_threat_choke_points(missing_threat, 1),
        Err(ModelError::UnknownThreatScenario(missing_threat))
    );
    assert_eq!(
        p.threat_choke_points(t_no_tree, 1),
        Err(ModelError::NoAttackTree(t_no_tree))
    );
    assert_eq!(
        p.residual_threat_choke_points(t_no_tree, 1),
        Err(ModelError::NoAttackTree(t_no_tree))
    );
    assert_eq!(
        p.tree_hitting_set(missing_tree, 1),
        Err(ModelError::UnknownAttackTree(missing_tree))
    );
    assert_eq!(
        p.residual_tree_hitting_set(missing_tree, 1),
        Err(ModelError::UnknownAttackTree(missing_tree))
    );
    assert_eq!(
        p.threat_hitting_set(missing_threat, 1),
        Err(ModelError::UnknownThreatScenario(missing_threat))
    );
    assert_eq!(
        p.residual_threat_hitting_set(missing_threat, 1),
        Err(ModelError::UnknownThreatScenario(missing_threat))
    );
    assert_eq!(
        p.threat_hitting_set(t_no_tree, 1),
        Err(ModelError::NoAttackTree(t_no_tree))
    );
    assert_eq!(
        p.residual_threat_hitting_set(t_no_tree, 1),
        Err(ModelError::NoAttackTree(t_no_tree))
    );
}

#[test]
fn sensitivity_and_tipping_points_on_project() {
    let m = builtin();
    let mut p = Project::new(Arc::clone(&m));
    let (very_low, low, med, high) = (
        feas(&m, "very-low"),
        feas(&m, "low"),
        feas(&m, "medium"),
        feas(&m, "high"),
    );
    let safety = cat(&m, "safety");
    let financial = cat(&m, "financial");
    let (neg, mod_lvl, maj, sev) = (
        lvl(&m, safety, "negligible"),
        lvl(&m, safety, "moderate"),
        lvl(&m, safety, "major"),
        lvl(&m, safety, "severe"),
    );

    let asset = asset_id(1);
    ok(
        &mut p,
        Op::AddAsset {
            id: asset,
            name: name(SENTINEL),
            properties: props(&m, &["integrity"]),
            provenance: Provenance::default(),
        },
    );
    let d1 = damage_id(2);
    let d2 = damage_id(3);
    ok(
        &mut p,
        Op::AddDamageScenario {
            id: d1,
            name: name(SENTINEL),
            impacts: vec![(safety, maj)],
            provenance: Provenance::default(),
        },
    );
    ok(
        &mut p,
        Op::AddDamageScenario {
            id: d2,
            name: name(SENTINEL),
            impacts: vec![],
            provenance: Provenance::default(),
        },
    );

    let t_tree = threat_id(10);
    let t_direct = threat_id(11);
    let t_unrated = threat_id(12);
    for id in [t_tree, t_direct, t_unrated] {
        ok(
            &mut p,
            Op::AddThreatScenario {
                id,
                name: name(SENTINEL),
                asset,
                category: threat_cat(&m, "tampering"),
                catalog_refs: vec![],
                damages: vec![],
                provenance: Provenance::default(),
            },
        );
    }
    ok(
        &mut p,
        Op::LinkDamage {
            threat: t_tree,
            damage: d1,
            provenance: Provenance::default(),
        },
    );
    ok(
        &mut p,
        Op::LinkDamage {
            threat: t_direct,
            damage: d1,
            provenance: Provenance::default(),
        },
    );

    // Tree: AND(s1=med, s2=high) => root = med.
    // Perturbing s1 by -1 -> low (root becomes low); by +1 -> high (root becomes high).
    // Perturbing s2 by -1 -> med (root stays med); by +1 -> none (at top of scale).
    let (n_root, n_s1, n_s2) = (node_id(1), node_id(2), node_id(3));
    let mut tree = AttackTree::new(n_root, Node::gate(GateKind::And, SENTINEL)).expect("root");
    let i_s1 = tree
        .add_child(tree.root(), n_s1, Node::step(SENTINEL))
        .expect("s1");
    let _i_s2 = tree
        .add_child(tree.root(), n_s2, Node::step(SENTINEL))
        .expect("s2");
    let tr = tree_id(20);
    ok(
        &mut p,
        Op::AddAttackTree {
            id: tr,
            tree,
            step_feasibility: vec![(n_s1, med), (n_s2, high)],
            provenance: Provenance::default(),
        },
    );
    ok(
        &mut p,
        Op::SetThreatAttackTree {
            threat: t_tree,
            tree: Some(tr),
            provenance: Provenance::default(),
        },
    );

    let tips = p.tree_tipping_points(tr).expect("tree tips");
    assert_eq!(tips.len(), 2);
    assert_eq!(tips[0].step(), i_s1);
    assert_eq!(tips[0].node(), n_s1);
    assert_eq!(tips[0].from_step(), med);
    assert_eq!(tips[0].to_step(), low);
    assert_eq!(tips[0].from_root(), med);
    assert_eq!(tips[0].to_root(), low);
    assert_eq!(tips[1].step(), i_s1);
    assert_eq!(tips[1].to_step(), high);
    assert_eq!(tips[1].to_root(), high);
    assert!(!format!("{tips:?}").contains(SENTINEL));

    // Threat sensitivity on tree-linked threat (d1: safety=major, feas=medium => risk=r3).
    let sens = p.threat_sensitivity(t_tree).expect("sens");
    assert_eq!(sens.feasibility(), med);
    assert_eq!(sens.max_risk(), Some(risk(&m, "r3")));
    assert!(!sens.is_stable());
    assert_eq!(sens.feasibility_tipping_points().len(), 2);
    assert_eq!(sens.feasibility_tipping_points()[0].step(), Some(i_s1));
    assert_eq!(sens.feasibility_tipping_points()[0].node(), Some(n_s1));
    assert_eq!(
        sens.feasibility_tipping_points()[0].from_risk(),
        risk(&m, "r3")
    );
    assert_eq!(
        sens.feasibility_tipping_points()[0].to_risk(),
        risk(&m, "r2")
    );
    assert_eq!(
        sens.feasibility_tipping_points()[1].to_risk(),
        risk(&m, "r4")
    );
    // Impact tipping points on d1 (safety=major at feas=medium: -1 -> moderate gives risk=r2, +1 -> severe gives risk=r4).
    assert_eq!(sens.impact_tipping_points().len(), 2);
    assert_eq!(sens.impact_tipping_points()[0].damage(), d1);
    assert_eq!(sens.impact_tipping_points()[0].category(), safety);
    assert_eq!(sens.impact_tipping_points()[0].from_impact(), maj);
    assert_eq!(sens.impact_tipping_points()[0].to_impact(), mod_lvl);
    assert_eq!(sens.impact_tipping_points()[0].to_risk(), risk(&m, "r2"));
    assert_eq!(sens.impact_tipping_points()[1].to_impact(), sev);
    assert_eq!(sens.impact_tipping_points()[1].to_risk(), risk(&m, "r4"));
    assert!(!format!("{sens:?}").contains(SENTINEL));

    // Mitigate s2 to `very-low`: now s2 is the sole bottleneck of AND(s1=med, s2=very_low)!
    let ctrl = control_id(30);
    ok(
        &mut p,
        Op::AddSecurityControl {
            id: ctrl,
            name: name(SENTINEL),
            provenance: Provenance::default(),
        },
    );
    ok(
        &mut p,
        Op::SetStepMitigation {
            tree: tr,
            node: n_s2,
            control: ctrl,
            feasibility: Some(very_low),
            provenance: Provenance::default(),
        },
    );
    let res_tips = p.residual_tree_tipping_points(tr).expect("res tips");
    assert_eq!(res_tips.len(), 1);
    assert_eq!(res_tips[0].node(), n_s2);
    assert_eq!(res_tips[0].from_step(), very_low);
    assert_eq!(res_tips[0].to_step(), low);
    let res_sens = p.residual_threat_sensitivity(t_tree).expect("res sens");
    assert_eq!(res_sens.feasibility(), very_low);
    assert_eq!(res_sens.max_risk(), Some(risk(&m, "r1")));

    // Direct feasibility threat: step and node are None on feasibility_tipping_points.
    ok(
        &mut p,
        Op::SetThreatFeasibility {
            threat: t_direct,
            level: Some(med),
            provenance: Provenance::default(),
        },
    );
    let direct_sens = p.threat_sensitivity(t_direct).expect("direct sens");
    assert_eq!(direct_sens.feasibility_tipping_points().len(), 2);
    assert_eq!(direct_sens.feasibility_tipping_points()[0].step(), None);
    assert_eq!(direct_sens.feasibility_tipping_points()[0].node(), None);

    // Tied deciding impact entries: link d2 with financial=major (also risk=r3 at feas=medium).
    // Now lowering d1 or d2 alone by -1 keeps max_risk at r3 (no downward tipping point!),
    // while raising either to severe raises max_risk to r4.
    ok(
        &mut p,
        Op::SetImpact {
            id: d2,
            category: financial,
            level: Some(lvl(&m, financial, "major")),
            provenance: Provenance::default(),
        },
    );
    ok(
        &mut p,
        Op::LinkDamage {
            threat: t_direct,
            damage: d2,
            provenance: Provenance::default(),
        },
    );
    let tied_sens = p.threat_sensitivity(t_direct).expect("tied sens");
    assert_eq!(tied_sens.impact_tipping_points().len(), 2);
    for it in tied_sens.impact_tipping_points() {
        assert_eq!(it.from_risk(), risk(&m, "r3"));
        assert_eq!(it.to_risk(), risk(&m, "r4"));
    }

    // Threat with no rated damages: max_risk is None, tipping points are empty, is_stable is true.
    ok(
        &mut p,
        Op::SetThreatFeasibility {
            threat: t_unrated,
            level: Some(neg_feas(&m)),
            provenance: Provenance::default(),
        },
    );
    let unrated_sens = p.threat_sensitivity(t_unrated).expect("unrated sens");
    assert_eq!(unrated_sens.max_risk(), None);
    assert!(unrated_sens.is_stable());
    let _ = neg;

    // Error cases.
    let missing_tree = tree_id(99);
    let missing_threat = threat_id(99);
    assert_eq!(
        p.tree_tipping_points(missing_tree),
        Err(ModelError::UnknownAttackTree(missing_tree))
    );
    assert_eq!(
        p.residual_tree_tipping_points(missing_tree),
        Err(ModelError::UnknownAttackTree(missing_tree))
    );
    assert_eq!(
        p.threat_sensitivity(missing_threat),
        Err(ModelError::UnknownThreatScenario(missing_threat))
    );
    assert_eq!(
        p.residual_threat_sensitivity(missing_threat),
        Err(ModelError::UnknownThreatScenario(missing_threat))
    );
}

fn neg_feas(m: &Methodology) -> aegis_model::FeasibilityLevel {
    feas(m, "very-low")
}

#[test]
fn item_definition_lifecycle_lint_diff_and_store_round_trip() {
    use aegis_model::{
        ComponentId, DataFlowId, InterfaceId, MAX_COMPONENT_DEPTH, OpLogReader, OpLogWriter,
        TrustBoundaryId,
    };

    let m = builtin();
    let mut p = Project::new(Arc::clone(&m));
    let baseline = p.clone();
    let mut ops = Vec::new();

    let c_vehicle = ComponentId::new_v7();
    let c_gateway = ComponentId::new_v7();
    let c_brake = ComponentId::new_v7();
    let i_can = InterfaceId::new_v7();
    let i_eth = InterfaceId::new_v7();
    let df_cmd = DataFlowId::new_v7();
    let tb_safety = TrustBoundaryId::new_v7();
    let a_brake = asset_id(740);
    let a_unbound = asset_id(741);
    let d_crash = damage_id(750);
    let t_spoof = threat_id(760);

    let can_kind = m.interface_kind("can").expect("can interface kind");
    let eth_kind = m
        .interface_kind("ethernet")
        .expect("ethernet interface kind");
    let safety = cat(&m, "safety");

    // Add assets before any components exist -> no AssetUnbound lint yet.
    let op = Op::AddAsset {
        id: a_brake,
        name: name("Brake torque command"),
        properties: m.all_properties(),
        provenance: Provenance::default(),
    };
    ok(&mut p, op.clone());
    ops.push(op);

    let op = Op::AddAsset {
        id: a_unbound,
        name: name("Unbound diagnostic key"),
        properties: m.all_properties(),
        provenance: Provenance::default(),
    };
    ok(&mut p, op.clone());
    ops.push(op);
    assert!(
        !p.lint()
            .iter()
            .any(|f| matches!(f.code(), LintCode::AssetUnbound { .. })),
        "AssetUnbound only fires when project defines at least one component"
    );

    // Add components (vehicle -> gateway, vehicle -> brake).
    let op = Op::AddComponent {
        id: c_vehicle,
        name: name("Vehicle Item"),
        parent: None,
        provenance: Provenance::default(),
    };
    ok(&mut p, op.clone());
    ops.push(op);

    let op = Op::AddComponent {
        id: c_gateway,
        name: name("Telematics Gateway"),
        parent: Some(c_vehicle),
        provenance: Provenance::default(),
    };
    ok(&mut p, op.clone());
    ops.push(op);

    let op = Op::AddComponent {
        id: c_brake,
        name: name("Brake ECU"),
        parent: Some(c_vehicle),
        provenance: Provenance::default(),
    };
    ok(&mut p, op.clone());
    ops.push(op);

    // Cycle detection on SetComponentParent.
    rejects(
        &mut p,
        Op::SetComponentParent {
            id: c_vehicle,
            parent: Some(c_gateway),
            provenance: Provenance::default(),
        },
        &ModelError::ComponentCycle {
            component: c_vehicle,
            parent: c_gateway,
        },
    );

    // Bind a_brake to c_brake; leave a_unbound unbound -> AssetUnbound(a_unbound) fires.
    let op = Op::SetAssetComponent {
        asset: a_brake,
        component: Some(c_brake),
        provenance: Provenance::default(),
    };
    ok(&mut p, op.clone());
    ops.push(op);
    assert!(
        p.lint()
            .iter()
            .any(|f| f.code() == LintCode::AssetUnbound { asset: a_unbound })
    );
    assert!(
        !p.lint()
            .iter()
            .any(|f| f.code() == LintCode::AssetUnbound { asset: a_brake })
    );

    // Add interfaces on gateway and brake.
    let op = Op::AddInterface {
        id: i_can,
        name: name("Chassis CAN-FD"),
        component: c_brake,
        kind: can_kind,
        provenance: Provenance::default(),
    };
    ok(&mut p, op.clone());
    ops.push(op);

    let op = Op::AddInterface {
        id: i_eth,
        name: name("Automotive Ethernet"),
        component: c_vehicle,
        kind: eth_kind,
        provenance: Provenance::default(),
    };
    ok(&mut p, op.clone());
    ops.push(op);

    // DataFlow cannot reference an interface that does not belong to from or to.
    rejects(
        &mut p,
        Op::AddDataFlow {
            id: df_cmd,
            name: name("Brake frame"),
            from: c_gateway,
            to: c_brake,
            interface: Some(i_eth),
            provenance: Provenance::default(),
        },
        &ModelError::InterfaceNotOnEndpoint {
            flow: df_cmd,
            interface: i_eth,
        },
    );

    // Valid DataFlow between c_gateway and c_brake over i_can.
    let op = Op::AddDataFlow {
        id: df_cmd,
        name: name("Brake torque frame"),
        from: c_gateway,
        to: c_brake,
        interface: Some(i_can),
        provenance: Provenance::default(),
    };
    ok(&mut p, op.clone());
    ops.push(op);

    // Add trust boundary containing only c_brake -> df_cmd crosses tb_safety without any threat!
    let op = Op::AddTrustBoundary {
        id: tb_safety,
        name: name("Safety Domain"),
        components: vec![c_brake],
        provenance: Provenance::default(),
    };
    ok(&mut p, op.clone());
    ops.push(op);

    assert!(
        p.lint().iter().any(|f| {
            f.code() == LintCode::BoundaryCrossingDataFlowWithoutThreat { flow: df_cmd }
        })
    );

    // Referential integrity: cannot remove c_brake while referenced by asset, interface, flow, boundary.
    assert!(p.validate(&Op::RemoveComponent { id: c_brake }).is_err());

    // Add threat targeting a_brake (bound to c_brake) -> clears BoundaryCrossingDataFlowWithoutThreat!
    let op = Op::AddDamageScenario {
        id: d_crash,
        name: name("Collision"),
        impacts: vec![(safety, lvl(&m, safety, "severe"))],
        provenance: Provenance::default(),
    };
    ok(&mut p, op.clone());
    ops.push(op);

    let op = Op::AddThreatScenario {
        id: t_spoof,
        name: name("Spoof brake frame on CAN"),
        asset: a_brake,
        category: threat_cat(&m, "spoofing"),
        catalog_refs: vec![],
        damages: vec![d_crash],
        provenance: Provenance::default(),
    };
    ok(&mut p, op.clone());
    ops.push(op);

    assert!(
        !p.lint().iter().any(|f| {
            f.code() == LintCode::BoundaryCrossingDataFlowWithoutThreat { flow: df_cmd }
        })
    );

    // Diff against baseline reports added item-definition entities.
    let diff = baseline.diff(&p).expect("diff");
    assert_eq!(diff.components().added().len(), 3);
    assert_eq!(diff.interfaces().added().len(), 2);
    assert_eq!(diff.data_flows().added(), &[df_cmd]);
    assert_eq!(diff.trust_boundaries().added(), &[tb_safety]);

    // Snapshot round-trip.
    let mut snap_bytes = Vec::new();
    p.save(&mut snap_bytes).expect("save snapshot");
    let loaded = Project::load(snap_bytes.as_slice(), Arc::clone(&m)).expect("load snapshot");
    assert_eq!(loaded, p);

    // Op-log round-trip.
    let mut log_bytes = Vec::new();
    {
        let mut writer =
            OpLogWriter::create(&mut log_bytes, Arc::clone(&m)).expect("create writer");
        for op in &ops {
            writer.append(op).expect("append op");
        }
    }
    let mut replayed = Project::new(Arc::clone(&m));
    let reader = OpLogReader::open(log_bytes.as_slice(), Arc::clone(&m)).expect("open reader");
    for rec in reader {
        let rec = rec.expect("read op record");
        replayed.apply(rec.op).expect("replay op");
    }
    assert_eq!(replayed, p);
    let _ = MAX_COMPONENT_DEPTH;
}
