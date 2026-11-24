//! Integration tests for `aegis_client::attacker_profile`, headless
//! `AttackerProfileView`, edit constructors, idempotency, and toast output (137b).

use std::sync::Arc;

use aegis_client::attacker_profile::{
    attacker_profile_view, reset_edit, scalability_edit, select_all_factor_edit, toggle_chip_edit,
};
use aegis_client::commands::Edit;
use aegis_client::exclusion::ConditionSnapshot;
use aegis_client::i18n::{Catalog, Language};
use aegis_client::pages;
use aegis_client::view::Selection;
use aegis_client::workspace::{Origin, Workspace};
use aegis_model::{
    AssetId, AttackTree, DamageId, GateKind, Methodology, Node, NodeId, Op, Provenance,
    ScalabilityFilter, ThreatId, TreeId,
};

fn prov() -> Provenance {
    Provenance::human("tester", 1_792_956_600_000)
}

fn apply_edit(ws: &mut Workspace, edit: &Edit) {
    let ops = edit.to_ops(ws.project(), "tester", 1).expect("to_ops");
    ws.apply(ops, Origin::Edit).expect("apply edit");
}

#[test]
fn view_columns_chips_select_all_and_idempotency_across_every_factor() {
    let m = Arc::new(Methodology::iso21434_r155().expect("iso21434_r155"));
    let mut ws = Workspace::empty(Arc::clone(&m));
    let en = Catalog::load(Language::En);

    let initial = attacker_profile_view(&ws, &en);
    assert!(initial.is_default);
    assert_eq!(initial.scalability, ScalabilityFilter::Any);
    assert_eq!(initial.scalability_options.len(), 3);
    assert!(initial.scalability_options[0].selected);
    assert!(!initial.scalability_options[1].selected);
    assert!(!initial.scalability_options[2].selected);
    assert_eq!(initial.excluded_total, 0);
    assert_eq!(initial.explainer, en.tr("attacker-profile-explainer-none"));
    assert_eq!(initial.factors.len(), m.feasibility_factor_count());

    // L20: verify every factor column and every level chip, toggle + select_all + idempotency.
    for (col_idx, factor) in m.feasibility_factors().enumerate() {
        let col = &initial.factors[col_idx];
        assert_eq!(col.factor, factor);
        assert_eq!(
            col.key,
            m.feasibility_factor_key(factor).expect("factor key")
        );
        assert_eq!(
            col.label,
            m.feasibility_factor_label(factor).expect("factor label")
        );
        assert!(col.all_selected);
        assert_eq!(col.levels.len(), m.factor_level_count(factor));

        // When all_selected is already true, select_all_factor_edit is unchanged (no-op).
        let noop = select_all_factor_edit(ws.project(), factor);
        assert!(
            noop.to_ops(ws.project(), "tester", 1)
                .expect("to_ops")
                .is_empty(),
            "select_all_factor_edit must be unchanged when all levels already selected"
        );
        assert_eq!(
            Edit::Batch(vec![noop]).changing(ws.project()),
            Edit::Batch(vec![])
        );

        let first_level = m.factor_levels(factor).next().expect("first level");
        let chip = &col.levels[0];
        assert_eq!(chip.level, first_level);
        assert_eq!(
            chip.key,
            m.factor_level_key(factor, first_level).expect("level key")
        );
        assert_eq!(
            chip.points,
            m.factor_level_points(factor, first_level).expect("points")
        );
        assert!(chip.included);

        // Toggle off -> all_selected becomes false, is_default becomes false.
        let edit = toggle_chip_edit(ws.project(), factor, first_level);
        apply_edit(&mut ws, &edit);
        let after_off = attacker_profile_view(&ws, &en);
        assert!(!after_off.is_default);
        assert!(!after_off.factors[col_idx].all_selected);
        assert!(!after_off.factors[col_idx].levels[0].included);

        // Toggle back on -> restores all_selected and default.
        let edit = toggle_chip_edit(ws.project(), factor, first_level);
        apply_edit(&mut ws, &edit);
        let after_on = attacker_profile_view(&ws, &en);
        assert!(after_on.factors[col_idx].all_selected);
        assert!(after_on.is_default);

        // Toggle two levels off, then select_all_factor_edit restores every level of that column.
        let second_level = m.factor_levels(factor).nth(1).expect("second level");
        let edit = toggle_chip_edit(ws.project(), factor, first_level);
        apply_edit(&mut ws, &edit);
        let edit = toggle_chip_edit(ws.project(), factor, second_level);
        apply_edit(&mut ws, &edit);
        let select_all = select_all_factor_edit(ws.project(), factor);
        assert_ne!(
            Edit::Batch(vec![select_all.clone()]).changing(ws.project()),
            Edit::Batch(vec![])
        );
        apply_edit(&mut ws, &select_all);
        let restored = attacker_profile_view(&ws, &en);
        assert!(restored.factors[col_idx].all_selected);
        assert!(restored.is_default);
    }

    // Japanese and pseudo localization resolve labels cleanly.
    let ja = Catalog::load(Language::Ja);
    let ja_view = attacker_profile_view(&ws, &ja);
    assert_eq!(ja_view.factors.len(), initial.factors.len());
    assert_eq!(
        ja_view.scalability_options[0].label,
        ja.tr("attacker-profile-scalability-any")
    );
    let f0 = m.feasibility_factors().next().expect("f0");
    assert_eq!(
        ja_view.factors[0].label,
        m.feasibility_factor_label_for_lang(f0, "ja")
            .expect("ja factor label")
    );
    let pseudo = Catalog::load(Language::Pseudo);
    let pseudo_view = attacker_profile_view(&ws, &pseudo);
    assert_eq!(
        pseudo_view.explainer,
        pseudo.tr("attacker-profile-explainer-none")
    );
}

#[test]
fn scalability_reset_eu_cra_excluded_counts_and_toast() {
    // EU CRA has zero feasibility factors, only scalability options.
    let cra = Arc::new(Methodology::eu_cra().expect("eu_cra"));
    let mut cra_ws = Workspace::empty(cra);
    let en = Catalog::load(Language::En);
    let cra_view = attacker_profile_view(&cra_ws, &en);
    assert!(cra_view.factors.is_empty());
    assert_eq!(cra_view.scalability_options.len(), 3);
    let edit = scalability_edit(cra_ws.project(), ScalabilityFilter::ScaledOnly);
    apply_edit(&mut cra_ws, &edit);
    let cra_scaled = attacker_profile_view(&cra_ws, &en);
    assert!(!cra_scaled.is_default);
    assert_eq!(cra_scaled.scalability, ScalabilityFilter::ScaledOnly);
    assert!(cra_scaled.scalability_options[1].selected);
    apply_edit(&mut cra_ws, &reset_edit());
    assert!(attacker_profile_view(&cra_ws, &en).is_default);

    // Build an ISO 21434 workspace with rated steps and scaled/unscaled threats for counts + toast.
    let m = Arc::new(Methodology::iso21434_r155().expect("iso"));
    let safety = m.impact_category("safety").expect("safety");
    let severe = m.level(safety, "severe").expect("severe");
    let spoofing = m.threat_category("spoofing").expect("spoofing");
    let high = m.feasibility_level("high").expect("high");
    let factor = m.feasibility_factor("equipment").expect("equipment");
    let bespoke = m.factor_level(factor, "bespoke").expect("bespoke");

    let mut ws = Workspace::empty(Arc::clone(&m));
    let asset = AssetId::new_v7();
    let dmg = DamageId::new_v7();
    let (t1, t2) = (ThreatId::new_v7(), ThreatId::new_v7());
    let (tr1, tr2) = (TreeId::new_v7(), TreeId::new_v7());
    let (r1, s1) = (NodeId::new_v7(), NodeId::new_v7());
    let (r2, s2, s3) = (NodeId::new_v7(), NodeId::new_v7(), NodeId::new_v7());

    let mut tree1 = AttackTree::new(r1, Node::gate(GateKind::Or, "R1")).expect("r1");
    tree1
        .add_child(tree1.root(), s1, Node::step("S1"))
        .expect("s1");
    let mut tree2 = AttackTree::new(r2, Node::gate(GateKind::Or, "R2")).expect("r2");
    tree2
        .add_child(tree2.root(), s2, Node::step("S2"))
        .expect("s2");
    tree2
        .add_child(tree2.root(), s3, Node::step("S3"))
        .expect("s3");

    ws.apply(
        vec![
            Op::AddAsset {
                id: asset,
                name: "ECU".into(),
                properties: m.all_properties(),
                provenance: prov(),
            },
            Op::AddDamageScenario {
                id: dmg,
                name: "Crash".into(),
                impacts: vec![(safety, severe)],
                provenance: prov(),
            },
            Op::AddAttackTree {
                id: tr1,
                tree: tree1,
                step_feasibility: vec![(s1, high)],
                provenance: prov(),
            },
            Op::AddAttackTree {
                id: tr2,
                tree: tree2,
                step_feasibility: vec![(s2, high), (s3, high)],
                provenance: prov(),
            },
            Op::SetStepFactor {
                tree: tr2,
                node: s2,
                factor,
                level: Some(bespoke),
                provenance: prov(),
            },
            Op::AddThreatScenario {
                id: t1,
                name: "Local physical".into(),
                asset,
                category: spoofing,
                catalog_refs: vec![],
                damages: vec![dmg],
                provenance: prov(),
            },
            Op::SetThreatAttackTree {
                threat: t1,
                tree: Some(tr1),
                provenance: prov(),
            },
            Op::SetThreatScaling {
                threat: t1,
                scales: Some(false),
                reason: "physical".into(),
                provenance: prov(),
            },
            Op::AddThreatScenario {
                id: t2,
                name: "Remote".into(),
                asset,
                category: spoofing,
                catalog_refs: vec![],
                damages: vec![dmg],
                provenance: prov(),
            },
            Op::SetThreatAttackTree {
                threat: t2,
                tree: Some(tr2),
                provenance: prov(),
            },
        ],
        Origin::Edit,
    )
    .expect("setup");

    let risk_label = m
        .risk_level_label(
            ws.cache()
                .evaluate_residual_risk(ws.project(), t1)
                .expect("t1 risk")
                .max_risk()
                .expect("max"),
        )
        .expect("label")
        .to_owned();
    let t1_vis = pages::selection_visual_id(ws.project(), Selection::Threat(t1));

    // Update profile: exclude bespoke equipment + ScaledOnly -> t1 (and owned tr1) + s2 excluded.
    let update_edit = Edit::SetAttackFilter {
        filter: ws
            .project()
            .attack_filter()
            .without(factor, bespoke)
            .with_scalability(ScalabilityFilter::ScaledOnly),
    };
    let ops_update = update_edit
        .to_ops(ws.project(), "tester", 10)
        .expect("update ops");
    let snap_update = ConditionSnapshot::capture(&ws, &ops_update).expect("capture update");
    ws.apply(ops_update, Origin::Edit).expect("apply update");

    let updated = attacker_profile_view(&ws, &en);
    // Excluded elements: t1 + tr1 (2). Excluded leaf step: s2 (1). Total = 3.
    assert_eq!(updated.excluded.elements, 2);
    assert_eq!(updated.excluded.steps, 1);
    assert_eq!(updated.excluded_total, 3);
    assert_eq!(
        updated.explainer,
        en.tr_with("attacker-profile-explainer", &[("n", "3")])
    );
    assert_eq!(
        snap_update.toast(&ws),
        format!(
            "Attacker profile updated: 1 step excluded, 0 included, 1 risk moved ({t1_vis} 'Local physical' {risk_label} \u{2192} not applicable)"
        )
    );

    // Reset profile -> toast says "Attacker profile reset to default", counts return to 0.
    let ops_reset = reset_edit()
        .to_ops(ws.project(), "tester", 20)
        .expect("reset ops");
    let snap_reset = ConditionSnapshot::capture(&ws, &ops_reset).expect("capture reset");
    ws.apply(ops_reset, Origin::Edit).expect("apply reset");

    let after_reset = attacker_profile_view(&ws, &en);
    assert!(after_reset.is_default);
    assert_eq!(after_reset.excluded_total, 0);
    assert_eq!(
        after_reset.explainer,
        en.tr("attacker-profile-explainer-none")
    );
    assert_eq!(
        snap_reset.toast(&ws),
        format!(
            "Attacker profile reset to default: 0 steps excluded, 1 included, 1 risk moved ({t1_vis} 'Local physical' not applicable \u{2192} {risk_label})"
        )
    );
}
