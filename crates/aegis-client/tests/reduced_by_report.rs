//! Job 125 (CFG-07): the report names what reduces each risk, how many risks
//! each control lowers, and lists the security settings of every component
//! and interface, in both languages.

use std::sync::Arc;

use aegis_client::commands::Edit;
use aegis_client::i18n::{Catalog, Language};
use aegis_client::pages;
use aegis_client::report::{
    Block, ReportContext, ReportLanguage, ReportOptions, ReportProfile, ReportSections, Style,
    report_doc, report_doc_with_options, report_pdf, report_workbook_xlsx,
};
use aegis_client::workspace::{Origin, Workspace};
use aegis_model::{
    AssetId, ComponentId, ControlId, DamageId, InterfaceId, ItemRef, Methodology, NodeId, ThreatId,
    TreeId,
};

fn apply(ws: &mut Workspace, edit: &Edit) {
    let ops = edit
        .to_ops(ws.project(), "mira", 1_792_956_600_000)
        .expect("builds");
    ws.apply(ops, Origin::Edit).expect("applies");
}

struct Fx {
    ws: Workspace,
    ecu: ComponentId,
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
    Fx { ws, ecu }
}

/// The fixture plus a second threat whose only step is uncapped, and
/// optionally the two settings.
fn project(with_settings: bool) -> Fx {
    let mut f = fixture(0);
    let asset =
        f.ws.project()
            .assets()
            .next()
            .map(|(_, id, _)| id)
            .expect("asset");
    let (threat, tree) = (ThreatId::new_v7(), TreeId::new_v7());
    apply(
        &mut f.ws,
        &Edit::AddThreat {
            id: threat,
            name: "Bus flooded".into(),
            asset,
            category: 0,
        },
    );
    apply(
        &mut f.ws,
        &Edit::NewTree {
            id: tree,
            root: "Goal".into(),
            threat: Some(threat),
        },
    );
    apply(
        &mut f.ws,
        &Edit::ReplaceTree {
            tree,
            outline: "OR Goal\n  - Inject on CAN bus\n".into(),
        },
    );
    let t = f.ws.project().attack_tree_idx(&tree).expect("tree");
    let node: NodeId =
        f.ws.project()
            .attack_tree(t)
            .expect("tree")
            .iter()
            .find(|(_, _, n)| n.label() == "Inject on CAN bus")
            .map(|(_, id, _)| id)
            .expect("step");
    apply(
        &mut f.ws,
        &Edit::SetStepFeasibility {
            tree,
            node,
            level: Some(2),
        },
    );
    if with_settings {
        apply(
            &mut f.ws,
            &Edit::AddCustomSetting {
                id: ControlId::new_v7(),
                name: "Secure boot".into(),
                target: ItemRef::Component(f.ecu),
                rules: vec![(0, 1)],
            },
        );
        let usb = InterfaceId::new_v7();
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
            &Edit::AddCustomSetting {
                id: ControlId::new_v7(),
                name: "Port locked".into(),
                target: ItemRef::Interface(usb),
                rules: vec![(0, 0)],
            },
        );
    }
    f
}

fn ctx() -> ReportContext {
    ReportContext {
        project: "Brake ECU".into(),
        user: "Mira Chen".into(),
        now_ms: 1_792_956_600_000,
        seq: 42,
        unsaved: false,
    }
}

fn table_with<'a>(blocks: &'a [Block], header: &str) -> (&'a [String], &'a [Vec<String>]) {
    blocks
        .iter()
        .find_map(|b| match b {
            Block::Table { headers, rows } if headers.iter().any(|h| h == header) => {
                Some((headers.as_slice(), rows.as_slice()))
            }
            _ => None,
        })
        .expect("table")
}

fn cell_of<'a>(headers: &[String], rows: &'a [Vec<String>], key: &str, col: &str) -> &'a str {
    let c = headers.iter().position(|h| h == col).expect("column");
    rows.iter()
        .find(|r| r[0].contains(key))
        .map(|r| r[c].as_str())
        .expect("row")
}

fn ja_options() -> ReportOptions {
    ReportOptions {
        profile: ReportProfile {
            language: ReportLanguage::Japanese,
            ..ReportProfile::default()
        },
        baseline_project: None,
    }
}

#[test]
fn applied_settings_lists_components_then_their_interfaces() {
    let f = project(true);
    let m = f.ws.project().methodology().clone();
    let rows = pages::applied_settings(&f.ws, "en");
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].target_label, "ADAS ECU");
    assert_eq!(rows[1].target_label, "ADAS ECU / USB");
    assert_eq!(rows[0].name, "Secure boot");
    assert_eq!(rows[1].name, "Port locked");
    assert!(!rows[0].target_visual_id.is_empty() && !rows[1].control_visual_id.is_empty());
    let low = &pages::vocabulary(&m).feasibility[1];
    assert!(rows[0].rules.contains('\u{2192}'), "{}", rows[0].rules);
    assert!(rows[0].rules.contains(low.as_str()), "{}", rows[0].rules);
}

#[test]
fn applied_settings_uses_the_report_language() {
    let f = project(true);
    let m = f.ws.project().methodology().clone();
    let en = pages::applied_settings(&f.ws, "en");
    let ja = pages::applied_settings(&f.ws, "ja");
    let low = &pages::vocabulary_for_lang(&m, "ja").feasibility[1];
    assert!(ja[0].rules.contains(low.as_str()), "{}", ja[0].rules);
    assert_ne!(ja[0].rules, en[0].rules);
}

#[test]
fn the_risk_table_has_a_reduced_by_column() {
    let f = project(true);
    let doc = report_doc(&f.ws, &ctx());
    let (h, rows) = table_with(&doc.blocks, "Reduced by");
    assert_eq!(
        cell_of(h, rows, "Firmware replaced", "Reduced by"),
        "Secure boot (ADAS ECU)"
    );
    assert_eq!(
        cell_of(h, rows, "Bus flooded", "Reduced by"),
        "Not reduced; deciding step: Inject on CAN bus"
    );
}

#[test]
fn the_controls_table_counts_lowered_risks() {
    let f = project(true);
    let doc = report_doc(&f.ws, &ctx());
    let (h, rows) = table_with(&doc.blocks, "Risks lowered");
    assert_eq!(cell_of(h, rows, "Secure boot", "Risks lowered"), "1");
    assert_eq!(cell_of(h, rows, "Port locked", "Risks lowered"), "0");
}

#[test]
fn the_item_definition_lists_security_settings() {
    let f = project(true);
    let doc = report_doc(&f.ws, &ctx());
    let at = doc
        .blocks
        .iter()
        .position(|b| matches!(b, Block::Heading(3, t) if t == "Security settings"))
        .expect("heading");
    assert!(matches!(
        &doc.blocks[at + 1],
        Block::Paragraph(_, Style::Muted)
    ));
    let Block::Table { rows, .. } = &doc.blocks[at + 2] else {
        panic!("table expected");
    };
    assert_eq!(rows.len(), 2);
    assert!(rows[0][0].ends_with("ADAS ECU"), "{}", rows[0][0]);
    assert!(rows[1][0].ends_with("ADAS ECU / USB"), "{}", rows[1][0]);
}

#[test]
fn no_applied_setting_means_no_settings_table() {
    let has = |doc: &aegis_client::report::ReportDoc| {
        doc.blocks
            .iter()
            .any(|b| matches!(b, Block::Heading(3, t) if t == "Security settings"))
    };
    assert!(!has(&report_doc(&project(false).ws, &ctx())));
    let sections = ReportSections {
        item_definition: false,
        ..ReportSections::default()
    };
    let opts = ReportOptions {
        profile: ReportProfile {
            sections,
            ..ReportProfile::default()
        },
        baseline_project: None,
    };
    assert!(!has(&report_doc_with_options(
        &project(true).ws,
        &ctx(),
        &opts
    )));
}

#[test]
fn the_japanese_report_translates_the_new_text() {
    let f = project(true);
    let doc = report_doc_with_options(&f.ws, &ctx(), &ja_options());
    assert!(
        doc.blocks
            .iter()
            .any(|b| matches!(b, Block::Heading(3, t) if t == "セキュリティ設定"))
    );
    let _ = table_with(&doc.blocks, "低減策");
}

#[test]
fn every_new_key_is_in_every_catalog() {
    let keys = [
        "reduced-by",
        "reduced-by-none",
        "risks-lowered",
        "security-settings",
        "security-settings-note",
        "applied-to",
        "setting",
        "rules",
    ];
    for lang in [Language::En, Language::Ja, Language::Pseudo] {
        let c = Catalog::load(lang);
        for k in keys {
            let text = c.tr(k);
            assert!(!text.is_empty() && text != format!("[{k}]"), "{lang:?} {k}");
        }
    }
}

#[test]
fn the_pdf_still_renders() {
    let f = project(true);
    let pdf = report_pdf(&f.ws, &ctx()).expect("pdf");
    assert!(pdf.starts_with(b"%PDF"));
}

fn sheet(wb: &aegis_client::excel::WorkbookData, name: &str) -> aegis_client::excel::SheetData {
    wb.sheets
        .iter()
        .find(|s| s.name == name)
        .cloned()
        .unwrap_or_else(|| panic!("sheet {name}"))
}

fn workbook(ws: &Workspace) -> aegis_client::excel::WorkbookData {
    aegis_client::excel::read_workbook(&report_workbook_xlsx(ws)).expect("reads")
}

fn column(s: &aegis_client::excel::SheetData, key: &str, col: &str) -> String {
    let c = s.headers.iter().position(|h| h == col).expect("column");
    s.rows
        .iter()
        .find(|(_, r)| r.iter().any(|v| v.contains(key)))
        .map(|(_, r)| r[c].clone())
        .expect("row")
}

#[test]
fn the_workbook_has_reduced_by_columns() {
    let f = project(true);
    let wb = workbook(&f.ws);
    for name in ["Risk register", "Threat scenarios"] {
        let s = sheet(&wb, name);
        assert_eq!(
            column(&s, "Firmware replaced", "Reduced by"),
            "Secure boot (ADAS ECU)",
            "{name}"
        );
        assert_eq!(
            column(&s, "Bus flooded", "Reduced by"),
            "Not reduced; deciding step: Inject on CAN bus",
            "{name}"
        );
        assert!(s.rows.iter().all(|(_, r)| r.len() == s.headers.len()));
    }
}

#[test]
fn the_workbook_counts_lowered_risks() {
    let f = project(true);
    let s = sheet(&workbook(&f.ws), "Security controls");
    assert_eq!(column(&s, "Secure boot", "Risks lowered"), "1");
    assert_eq!(column(&s, "Port locked", "Risks lowered"), "0");
}

#[test]
fn the_workbook_lists_security_settings() {
    let f = project(true);
    let s = sheet(&workbook(&f.ws), "Security settings");
    assert_eq!(
        s.headers,
        ["ID", "Applied to", "Setting ID", "Setting", "Rules"]
    );
    assert_eq!(s.rows.len(), 2);
    assert_eq!(s.rows[0].1[1], "ADAS ECU");
    assert_eq!(s.rows[1].1[1], "ADAS ECU / USB");

    let f = project(false);
    let s = sheet(&workbook(&f.ws), "Security settings");
    assert!(s.rows.is_empty());

    let m = Arc::new(Methodology::iso21434_r155().expect("builtin"));
    let wb = workbook(&Workspace::empty(m));
    assert!(wb.sheets.iter().all(|s| s.name != "Security settings"));
}

#[test]
fn names_that_look_like_formulas_stay_text() {
    let mut f = project(true);
    apply(
        &mut f.ws,
        &Edit::RenameComponent {
            component: f.ecu,
            name: "+ECU".into(),
        },
    );
    let setting = pages::applied_settings(&f.ws, "en")[0].control;
    apply(
        &mut f.ws,
        &Edit::RenameControl {
            control: setting,
            name: "=HYPERLINK(\"http://x\",\"y\")".into(),
        },
    );
    let (tree, node) = {
        let p = f.ws.project();
        p.attack_trees()
            .find_map(|(idx, id, t)| {
                let tr = p.attack_tree(idx)?;
                let _ = t;
                tr.iter()
                    .find(|(_, _, n)| n.label() == "Inject on CAN bus")
                    .map(|(_, n, _)| (id, n))
            })
            .expect("step")
    };
    let control = ControlId::new_v7();
    apply(
        &mut f.ws,
        &Edit::AddControl {
            id: control,
            name: "@SUM(A1)".into(),
        },
    );
    apply(
        &mut f.ws,
        &Edit::SetStepMitigation {
            tree,
            node,
            control,
            level: Some(1),
        },
    );
    let wb = workbook(&f.ws);
    let s = sheet(&wb, "Risk register");
    assert_eq!(column(&s, "Bus flooded", "Reduced by"), "'@SUM(A1)");

    let bytes = report_workbook_xlsx(&f.ws);
    let raw = String::from_utf8_lossy(&bytes);
    for needle in ["<t>'=HYPERLINK(", "<t>'+ECU", "<t>'@SUM(A1)"] {
        assert!(raw.contains(needle), "{needle}");
    }
    for bad in ["<t>=", "<t>+", "<t>-", "<t>@"] {
        assert!(!raw.contains(bad), "{bad}");
    }
}

#[test]
fn the_exported_workbook_still_imports() {
    use aegis_client::excel::{ExcelMapping, preview_import};
    let f = project(true);
    let wb = workbook(&f.ws);
    let m = Arc::new(Methodology::iso21434_r155().expect("builtin"));
    let empty_ws = Workspace::empty(m);
    let preview = preview_import(
        empty_ws.project(),
        &wb,
        &ExcelMapping::default(),
        "mira",
        1_792_956_600_000,
    )
    .expect("preview");
    assert!(preview.threats_to_add > 0);
}
