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
    ScalabilityFilter, StepTarget, ThreatId, TreeId,
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
                target: StepTarget::Step(tr2, s2),
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

fn report_ctx() -> aegis_client::report::ReportContext {
    aegis_client::report::ReportContext {
        project: "Profile Report".into(),
        user: "Mira Chen".into(),
        now_ms: 1_792_956_600_000,
        seq: 9,
        unsaved: false,
    }
}

fn active_profile_workspace() -> (
    Workspace,
    Arc<Methodology>,
    aegis_model::FeasibilityFactor,
    aegis_model::FactorLevel,
    ThreatId,
    ThreatId,
) {
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
    let (t_unscaled, t_scaled) = (ThreatId::new_v7(), ThreatId::new_v7());
    let tree_id = TreeId::new_v7();
    let (root_id, s_bespoke, s_std) = (NodeId::new_v7(), NodeId::new_v7(), NodeId::new_v7());

    let mut tree = AttackTree::new(root_id, Node::gate(GateKind::Or, "Root")).expect("root");
    tree.add_child(tree.root(), s_bespoke, Node::step("Bespoke probe"))
        .expect("s_bespoke");
    tree.add_child(tree.root(), s_std, Node::step("Standard bus tap"))
        .expect("s_std");

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
                id: tree_id,
                tree,
                step_feasibility: vec![(s_bespoke, high), (s_std, high)],
                provenance: prov(),
            },
            Op::SetStepFactor {
                target: StepTarget::Step(tree_id, s_bespoke),
                factor,
                level: Some(bespoke),
                provenance: prov(),
            },
            Op::AddThreatScenario {
                id: t_unscaled,
                name: "Physical only".into(),
                asset,
                category: spoofing,
                catalog_refs: vec![],
                damages: vec![dmg],
                provenance: prov(),
            },
            Op::SetThreatFeasibility {
                threat: t_unscaled,
                level: Some(high),
                provenance: prov(),
            },
            Op::SetThreatScaling {
                threat: t_unscaled,
                scales: Some(false),
                reason: "needs physical access".into(),
                provenance: prov(),
            },
            Op::AddThreatScenario {
                id: t_scaled,
                name: "Remote attack".into(),
                asset,
                category: spoofing,
                catalog_refs: vec![],
                damages: vec![dmg],
                provenance: prov(),
            },
            Op::SetThreatAttackTree {
                threat: t_scaled,
                tree: Some(tree_id),
                provenance: prov(),
            },
            Op::SetAttackFilter {
                filter: aegis_model::AttackFilter::default()
                    .without(factor, bespoke)
                    .with_scalability(ScalabilityFilter::ScaledOnly),
                provenance: prov(),
            },
        ],
        Origin::Edit,
    )
    .expect("setup");

    (ws, m, factor, bespoke, t_unscaled, t_scaled)
}

#[test]
fn report_cover_method_html_pdf_and_xlsx_render_active_profile_in_en_and_ja() {
    use aegis_client::excel::{SheetData, read_workbook};
    use aegis_client::report::{
        Block, ReportLanguage, ReportOptions, ReportProfile, Style, report_doc_with_options,
        report_html_with_options, report_pdf_with_options, report_workbook_xlsx,
        report_workbook_xlsx_in, risk_register_xlsx,
    };

    let (ws, m, factor, bespoke, _t_unscaled, _t_scaled) = active_profile_workspace();
    assert_eq!(ws.project().conditions().count(), 0);
    assert_eq!(ws.project().assumptions().count(), 0);
    let ctx = report_ctx();

    for (lang, r_lang) in [
        (Language::En, ReportLanguage::English),
        (Language::Ja, ReportLanguage::Japanese),
    ] {
        let cat = Catalog::load(lang);
        let opts = ReportOptions {
            profile: ReportProfile {
                language: r_lang,
                ..ReportProfile::default()
            },
            ..ReportOptions::default()
        };

        let doc = report_doc_with_options(&ws, &ctx, &opts);
        let f_label = m
            .feasibility_factor_label_for_lang(factor, lang.code())
            .expect("factor label");
        let l_label = m
            .factor_level_label_for_lang(factor, bespoke, lang.code())
            .expect("level label");
        let scaled_label = cat.tr("attacker-profile-scalability-scaled");

        // Cover table includes Attacker profile row with localized factor/level and scalability.
        let cover_rows = doc
            .blocks
            .iter()
            .find_map(|b| match b {
                Block::Table { rows, .. } => Some(rows),
                _ => None,
            })
            .expect("cover table");
        let ap_row = cover_rows
            .iter()
            .find(|r| r[0] == cat.tr("report-attacker-profile"))
            .expect("Attacker profile cover row");
        assert!(ap_row[1].contains(f_label), "{:?}", ap_row);
        assert!(ap_row[1].contains(l_label), "{:?}", ap_row);
        assert!(ap_row[1].contains(&scaled_label), "{:?}", ap_row);

        // Method heading(2) -> methodology line -> Attacker profile heading(3) -> table -> fallback Excluded.
        let pos_method = doc
            .blocks
            .iter()
            .position(|b| matches!(b, Block::Heading(2, t) if t == &cat.tr("section-method")))
            .expect("Method heading");
        assert!(matches!(
            &doc.blocks[pos_method + 1],
            Block::Paragraph(t, Style::Normal) if t.contains(m.key()) && t.contains(m.label_for_lang(lang.code()))
        ));
        assert!(matches!(
            &doc.blocks[pos_method + 2],
            Block::Heading(3, t) if t == &cat.tr("report-attacker-profile")
        ));
        let Block::Table { headers, rows } = &doc.blocks[pos_method + 3] else {
            panic!("expected Attacker profile table");
        };
        assert_eq!(
            headers,
            &[
                cat.tr("report-attacker-profile-factor"),
                cat.tr("report-attacker-profile-allowed"),
                cat.tr("report-attacker-profile-excluded"),
            ]
        );
        assert_eq!(rows.len(), m.feasibility_factor_count() + 1);
        let equip_row = rows.iter().find(|r| r[0] == f_label).expect("factor row");
        assert!(
            !equip_row[1].split(", ").any(|item| item == l_label),
            "excluded level {l_label} should not be in allowed list: {}",
            equip_row[1]
        );
        assert_eq!(equip_row[2], l_label);
        let other_row = rows
            .iter()
            .find(|r| r[0] != f_label && r[0] != cat.tr("report-attacker-profile-scalability"))
            .expect("other factor");
        assert_eq!(other_row[1], cat.tr("report-attacker-profile-all-allowed"));
        assert_eq!(
            other_row[2],
            cat.tr("report-attacker-profile-none-excluded")
        );
        assert_eq!(
            rows.last().expect("scalability row"),
            &vec![
                cat.tr("report-attacker-profile-scalability"),
                scaled_label.clone(),
                String::new(),
            ]
        );

        assert!(matches!(
            &doc.blocks[pos_method + 4],
            Block::Heading(3, t) if t == &cat.tr("report-excluded")
        ));
        let Block::Table {
            headers: ex_headers,
            rows: ex_rows,
        } = &doc.blocks[pos_method + 5]
        else {
            panic!("expected excluded table");
        };
        assert_eq!(
            ex_headers,
            &[
                cat.tr("report-excluded-item"),
                cat.tr("report-excluded-reason")
            ]
        );
        assert!(
            ex_rows
                .iter()
                .any(|r| r[1].contains("Outside attacker profile")),
            "missing factor exclusion in {ex_rows:?}"
        );
        assert!(
            ex_rows.iter().any(|r| r[1].contains("Scalability")),
            "missing scalability exclusion in {ex_rows:?}"
        );

        // HTML & PDF render cleanly.
        let html = report_html_with_options(&ws, &ctx, &opts);
        assert!(html.contains(&cat.tr("section-method")));
        assert!(html.contains(&cat.tr("report-attacker-profile")));
        assert!(html.contains(&scaled_label));
        let pdf = report_pdf_with_options(&ws, &ctx, &opts).expect("pdf");
        assert!(pdf.starts_with(b"%PDF-"));
    }

    // XLSX full workbook + standalone risk register (no conditions/assumptions, active profile).
    let book = read_workbook(&report_workbook_xlsx(&ws)).expect("workbook");
    let sheet = |name: &str| -> &SheetData {
        book.sheets
            .iter()
            .find(|s| s.name == name)
            .unwrap_or_else(|| panic!("missing sheet {name}"))
    };
    let ap_sheet = sheet("Attacker profile");
    assert_eq!(ap_sheet.headers, vec!["Factor", "Allowed", "Excluded"]);
    assert_eq!(ap_sheet.rows.len(), m.feasibility_factor_count() + 1);
    assert_eq!(
        ap_sheet.rows.last().expect("last").1,
        vec![
            "Scalability".to_owned(),
            "Scalable only".to_owned(),
            String::new(),
        ]
    );

    // Overview counts stay aligned (Attacker profile sheet is not in Overview).
    let overview = sheet("Overview");
    assert!(
        !overview
            .rows
            .iter()
            .any(|(_, r)| r[0] == "Attacker profile"),
        "Overview must not list Attacker profile sheet"
    );
    let scoped = read_workbook(&report_workbook_xlsx_in(
        &ws,
        &[],
        &aegis_client::systems::SystemView::all(),
    ))
    .expect("scoped workbook");
    assert_eq!(scoped.sheets.len(), book.sheets.len());

    // Excluded and not applicable cells appear in Risk register and Threat scenarios.
    let rr_only = read_workbook(&risk_register_xlsx(&ws, &ctx)).expect("rr only");
    let rr_only_sheet = rr_only
        .sheets
        .iter()
        .find(|s| s.name == "Risk register")
        .expect("rr");
    for s in [
        sheet("Risk register"),
        rr_only_sheet,
        sheet("Threat scenarios"),
    ] {
        let name_col = s
            .headers
            .iter()
            .position(|h| h == "Threat scenario")
            .expect("name col");
        let before_col = s
            .headers
            .iter()
            .position(|h| h == "Risk before controls")
            .expect("before col");
        let after_col = s
            .headers
            .iter()
            .position(|h| h == "Risk after controls")
            .expect("after col");
        let ex_col = s
            .headers
            .iter()
            .position(|h| h == "Excluded")
            .expect("excluded col");
        let unscaled = &s
            .rows
            .iter()
            .find(|(_, r)| r[name_col] == "Physical only")
            .expect("unscaled row")
            .1;
        assert_eq!(unscaled[before_col], "not applicable");
        assert_eq!(unscaled[after_col], "not applicable");
        assert!(unscaled[ex_col].contains("Scalability"));
        let scaled = &s
            .rows
            .iter()
            .find(|(_, r)| r[name_col] == "Remote attack")
            .expect("scaled row")
            .1;
        assert_ne!(scaled[before_col], "not applicable");
        assert_eq!(scaled[ex_col], "");
    }
}

#[test]
fn report_method_handles_zero_factor_pack_and_conditions_without_duplicate_excluded() {
    use aegis_client::report::{Block, report_doc};

    // Zero-factor methodology pack (EU CRA) still emits non-empty Method table with Scalability row.
    let cra = Arc::new(Methodology::eu_cra().expect("eu_cra"));
    let mut cra_ws = Workspace::empty(cra);
    let cra_edit = scalability_edit(cra_ws.project(), ScalabilityFilter::UnscaledOnly);
    apply_edit(&mut cra_ws, &cra_edit);
    let cra_doc = report_doc(&cra_ws, &report_ctx());
    let pos = cra_doc
        .blocks
        .iter()
        .position(|b| matches!(b, Block::Heading(3, t) if t == "Attacker profile"))
        .expect("Attacker profile heading");
    let Block::Table { rows, .. } = &cra_doc.blocks[pos + 1] else {
        panic!("expected table");
    };
    assert_eq!(
        rows,
        &[vec![
            "Scalability".to_owned(),
            "Non-scalable only".to_owned(),
            String::new(),
        ]]
    );

    // When conditions exist, Method does NOT emit a second Excluded elements and steps table.
    let (mut ws, _, _, _, _, _) = active_profile_workspace();
    apply_edit(
        &mut ws,
        &Edit::AddCondition {
            id: aegis_model::ConditionId::new_v7(),
            name: "CAN bus".into(),
        },
    );
    let doc = report_doc(&ws, &report_ctx());
    let excluded_headings = doc
        .blocks
        .iter()
        .filter(|b| matches!(b, Block::Heading(3, t) if t == "Excluded elements and steps"))
        .count();
    assert_eq!(excluded_headings, 1);
}

#[test]
fn baseline_comparison_and_report_record_attacker_profile_changes() {
    use aegis_client::report::{
        Block, ReportFilters, ReportOptions, ReportProfile, report_doc_with_options,
    };

    let m = Arc::new(Methodology::iso21434_r155().expect("iso"));
    let base = Workspace::empty(Arc::clone(&m));
    let mut current = Workspace::empty(m);

    // Same filter emits no Attacker profile changed line.
    let same = pages::comparison(base.project(), "v1", current.project()).expect("comparison");
    assert!(
        !same
            .lines
            .iter()
            .any(|l| l.text == "Attacker profile changed"),
        "{:?}",
        same.lines
    );

    let cur_edit = scalability_edit(current.project(), ScalabilityFilter::ScaledOnly);
    apply_edit(&mut current, &cur_edit);
    let cmp = pages::comparison(base.project(), "v1", current.project()).expect("comparison");
    assert!(
        cmp.lines
            .iter()
            .any(|l| { l.kind == "Changed" && l.text == "Attacker profile changed" })
    );
    assert!(cmp.summary.contains("1 changed"), "{}", cmp.summary);

    let opts = ReportOptions {
        baseline_project: Some(Arc::new(base.project().clone())),
        profile: ReportProfile {
            filters: ReportFilters {
                baseline: Some("v1".to_owned()),
                ..Default::default()
            },
            ..ReportProfile::default()
        },
    };
    let doc = report_doc_with_options(&current, &report_ctx(), &opts);
    let pos_cmp = doc
        .blocks
        .iter()
        .position(|b| matches!(b, Block::Heading(2, t) if t == "Comparison with baseline"))
        .expect("comparison heading");
    let pos_method = doc
        .blocks
        .iter()
        .position(|b| matches!(b, Block::Heading(2, t) if t == "Method"))
        .expect("method heading");
    assert!(pos_method < pos_cmp);
    assert!(
        doc.blocks[pos_cmp..]
            .iter()
            .any(|b| matches!(b, Block::Table { rows, .. } if rows.iter().any(|r| r == &vec!["Changed".to_owned(), "Attacker profile changed".to_owned()])))
    );
}
