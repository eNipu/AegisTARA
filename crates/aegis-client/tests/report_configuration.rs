//! Report cover line, Configuration and assumptions section, and XLSX
//! applicability columns (slice 135d).

use std::sync::Arc;

use aegis_client::commands::Edit;
use aegis_client::excel::{SheetData, WorkbookData, read_workbook};
use aegis_client::exclusion::reasons_text;
use aegis_client::report::{
    Block, ReportContext, ReportDoc, ReportOptions, report_doc, report_doc_with_options,
    report_html, report_pdf, report_workbook_xlsx, risk_register_xlsx,
};
use aegis_client::workspace::{Origin, Workspace};
use aegis_model::{
    AssetId, ComponentId, ConditionId, DamageId, ElementRef, InterfaceId, Methodology, NodeId,
    ThreatId, TreeId,
};

fn workspace() -> Workspace {
    Workspace::empty(Arc::new(Methodology::iso21434_r155().expect("builtin")))
}

fn run(ws: &mut Workspace, edit: &Edit) {
    let ops = edit.to_ops(ws.project(), "mira", 1).expect("builds");
    ws.apply(ops, Origin::Edit).expect("applies");
}

fn context() -> ReportContext {
    ReportContext {
        project: "Vehicle".into(),
        user: "Mira Chen".into(),
        now_ms: 1_792_956_600_000,
        seq: 7,
        unsaved: false,
    }
}

fn sheet<'a>(book: &'a WorkbookData, name: &str) -> &'a SheetData {
    book.sheets
        .iter()
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("no sheet {name}"))
}

fn col_idx(s: &SheetData, header: &str) -> usize {
    s.headers
        .iter()
        .position(|h| h == header)
        .unwrap_or_else(|| panic!("no column {header} in sheet {}", s.name))
}

fn find_row<'a>(s: &'a SheetData, col: usize, value: &str) -> &'a [String] {
    s.rows
        .iter()
        .map(|(_, r)| r.as_slice())
        .find(|r| r.get(col).map(String::as_str) == Some(value))
        .unwrap_or_else(|| panic!("no row with {value} in sheet {}", s.name))
}

fn cover_rows(doc: &ReportDoc) -> &Vec<Vec<String>> {
    doc.blocks
        .iter()
        .find_map(|b| match b {
            Block::Table { rows, .. } => Some(rows),
            _ => None,
        })
        .expect("cover table")
}

struct Fixture {
    ws: Workspace,
    cond_eth: ConditionId,
    comp_gw: ComponentId,
    iface_eth0: InterfaceId,
    asset_fw: AssetId,
    asset_brake: AssetId,
    threat_gw: ThreatId,
    threat_brake: ThreatId,
    tree_brake: TreeId,
    step_off: NodeId,
}

fn build_fixture() -> Fixture {
    let mut ws = workspace();

    let cond_eth = ConditionId::new_v7();
    let cond_dbg = ConditionId::new_v7();
    run(
        &mut ws,
        &Edit::AddCondition {
            id: cond_eth,
            name: "Ethernet".into(),
        },
    );
    run(
        &mut ws,
        &Edit::SetConditionPresent {
            condition: cond_eth,
            present: false,
        },
    );
    run(
        &mut ws,
        &Edit::AddCondition {
            id: cond_dbg,
            name: "Debug port".into(),
        },
    );

    let comp_gw = ComponentId::new_v7();
    run(
        &mut ws,
        &Edit::AddComponent {
            id: comp_gw,
            name: "Gateway".into(),
            parent: None,
        },
    );
    run(
        &mut ws,
        &Edit::SetConditionLink {
            element: ElementRef::Component(comp_gw),
            condition: cond_eth,
            linked: true,
        },
    );

    let iface_eth0 = InterfaceId::new_v7();
    run(
        &mut ws,
        &Edit::AddInterface {
            id: iface_eth0,
            name: "ETH0".into(),
            component: comp_gw,
            kind: 0,
        },
    );

    let asset_fw = AssetId::new_v7();
    run(
        &mut ws,
        &Edit::AddAssetBound {
            id: asset_fw,
            name: "Gateway Firmware".into(),
            properties: vec![0, 1, 2, 3],
            component: comp_gw,
        },
    );

    let asset_brake = AssetId::new_v7();
    run(
        &mut ws,
        &Edit::AddAsset {
            id: asset_brake,
            name: "Brake Signal".into(),
            properties: vec![0, 1, 2, 3],
        },
    );

    let dmg = DamageId::new_v7();
    run(
        &mut ws,
        &Edit::AddDamage {
            id: dmg,
            name: "Loss of braking".into(),
        },
    );
    run(
        &mut ws,
        &Edit::SetImpact {
            damage: dmg,
            category: 0,
            level: Some(2),
        },
    );

    let threat_gw = ThreatId::new_v7();
    run(
        &mut ws,
        &Edit::AddThreatLeading {
            id: threat_gw,
            name: "Spoof gateway".into(),
            asset: asset_fw,
            category: 0,
            damage: dmg,
        },
    );
    run(
        &mut ws,
        &Edit::SetThreatFeasibility {
            threat: threat_gw,
            level: Some(2),
        },
    );

    let threat_brake = ThreatId::new_v7();
    run(
        &mut ws,
        &Edit::AddThreatLeading {
            id: threat_brake,
            name: "Tamper brake".into(),
            asset: asset_brake,
            category: 0,
            damage: dmg,
        },
    );

    let tree_brake = TreeId::new_v7();
    run(
        &mut ws,
        &Edit::NewTree {
            id: tree_brake,
            root: "Tamper brake root".into(),
            threat: Some(threat_brake),
        },
    );
    let root_id = {
        let p = ws.project();
        let tidx = p.attack_tree_idx(&tree_brake).expect("tree");
        let tree = p.attack_tree(tidx).expect("tree");
        tree.id_of(tree.root()).expect("root")
    };

    let step_off = NodeId::new_v7();
    run(
        &mut ws,
        &Edit::AddTreeChild {
            tree: tree_brake,
            parent: root_id,
            id: step_off,
            gate: None,
            label: "Inject Ethernet frame".into(),
        },
    );
    run(
        &mut ws,
        &Edit::SetStepFeasibility {
            tree: tree_brake,
            node: step_off,
            level: Some(2),
        },
    );
    run(
        &mut ws,
        &Edit::SetStepConditionLink {
            tree: tree_brake,
            node: step_off,
            condition: cond_eth,
            linked: true,
        },
    );

    let step_on = NodeId::new_v7();
    run(
        &mut ws,
        &Edit::AddTreeChild {
            tree: tree_brake,
            parent: root_id,
            id: step_on,
            gate: None,
            label: "Physical tap".into(),
        },
    );
    run(
        &mut ws,
        &Edit::SetStepFeasibility {
            tree: tree_brake,
            node: step_on,
            level: Some(2),
        },
    );

    Fixture {
        ws,
        cond_eth,
        comp_gw,
        iface_eth0,
        asset_fw,
        asset_brake,
        threat_gw,
        threat_brake,
        tree_brake,
        step_off,
    }
}

#[test]
fn no_conditions_omits_configuration_and_excluded_columns() {
    let mut ws = workspace();
    let comp = ComponentId::new_v7();
    run(
        &mut ws,
        &Edit::AddComponent {
            id: comp,
            name: "ECU".into(),
            parent: None,
        },
    );
    run(
        &mut ws,
        &Edit::AddInterface {
            id: InterfaceId::new_v7(),
            name: "CAN0".into(),
            component: comp,
            kind: 0,
        },
    );
    let asset = AssetId::new_v7();
    run(
        &mut ws,
        &Edit::AddAssetBound {
            id: asset,
            name: "Key".into(),
            properties: vec![0, 1, 2, 3],
            component: comp,
        },
    );
    let dmg = DamageId::new_v7();
    run(
        &mut ws,
        &Edit::AddDamage {
            id: dmg,
            name: "Key leak".into(),
        },
    );
    run(
        &mut ws,
        &Edit::SetImpact {
            damage: dmg,
            category: 0,
            level: Some(1),
        },
    );
    let threat = ThreatId::new_v7();
    run(
        &mut ws,
        &Edit::AddThreatLeading {
            id: threat,
            name: "Read key".into(),
            asset,
            category: 0,
            damage: dmg,
        },
    );
    run(
        &mut ws,
        &Edit::SetThreatFeasibility {
            threat,
            level: Some(1),
        },
    );

    let ctx = context();
    let doc = report_doc(&ws, &ctx);
    let cover = cover_rows(&doc);
    assert!(!cover.iter().any(|r| r[0] == "Configuration"));
    assert!(!cover.iter().any(|r| r[0] == "Absent conditions"));
    assert!(
        !doc.blocks
            .iter()
            .any(|b| matches!(b, Block::Heading(2, t) if t == "Configuration and assumptions"))
    );

    let book = read_workbook(&report_workbook_xlsx(&ws)).expect("reads workbook");
    assert!(!book.sheets.iter().any(|s| s.name == "Conditions"));
    for name in [
        "Risk register",
        "Assets",
        "Threat scenarios",
        "Components",
        "Interfaces",
    ] {
        let s = sheet(&book, name);
        assert!(
            !s.headers.iter().any(|h| h == "Excluded"),
            "unexpected Excluded column in {name}"
        );
    }

    let rr_only = read_workbook(&risk_register_xlsx(&ws, &ctx)).expect("reads risk register");
    assert!(
        !sheet(&rr_only, "Risk register")
            .headers
            .iter()
            .any(|h| h == "Excluded")
    );
}

#[test]
fn report_and_xlsx_surface_conditions_and_exclusions() {
    let f = build_fixture();
    let ctx = context();

    let mut opts = ReportOptions {
        baseline_project: Some(Arc::new(f.ws.project().clone())),
        ..ReportOptions::default()
    };
    opts.profile.filters.baseline = Some("v1".to_owned());

    let doc = report_doc_with_options(&f.ws, &ctx, &opts);

    // Cover rows: Configuration immediately followed by Absent conditions.
    let cover = cover_rows(&doc);
    let cfg_pos = cover
        .iter()
        .position(|r| {
            r == &vec![
                "Configuration".to_owned(),
                "1 of 2 conditions present".to_owned(),
            ]
        })
        .expect("Configuration cover row");
    assert_eq!(
        cover.get(cfg_pos + 1),
        Some(&vec!["Absent conditions".to_owned(), "Ethernet".to_owned()])
    );

    // Heading ordering: Open problems < Configuration and assumptions < Comparison with baseline.
    let pos_problems = doc
        .blocks
        .iter()
        .position(|b| matches!(b, Block::Heading(2, t) if t == "9. Open problems"))
        .expect("Open problems heading");
    let pos_config = doc
        .blocks
        .iter()
        .position(|b| matches!(b, Block::Heading(2, t) if t == "Configuration and assumptions"))
        .expect("Configuration and assumptions heading");
    let pos_baseline = doc
        .blocks
        .iter()
        .position(|b| matches!(b, Block::Heading(2, t) if t == "Comparison with baseline"))
        .expect("Comparison with baseline heading");
    assert!(pos_problems < pos_config);
    assert!(pos_config < pos_baseline);

    // Conditions table inside Configuration and assumptions.
    assert!(matches!(
        &doc.blocks[pos_config + 1],
        Block::Heading(3, t) if t == "Conditions"
    ));
    let Block::Table {
        headers: cond_headers,
        rows: cond_rows,
    } = &doc.blocks[pos_config + 2]
    else {
        panic!("expected Conditions table");
    };
    assert_eq!(cond_headers, &["ID", "Condition", "Present"]);
    assert_eq!(
        cond_rows,
        &[
            vec!["CF00".to_owned(), "Ethernet".to_owned(), "no".to_owned()],
            vec!["CF01".to_owned(), "Debug port".to_owned(), "yes".to_owned()],
        ]
    );

    // Excluded elements and steps table.
    assert!(matches!(
        &doc.blocks[pos_config + 3],
        Block::Heading(3, t) if t == "Excluded elements and steps"
    ));
    let Block::Table {
        headers: ex_headers,
        rows: ex_rows,
    } = &doc.blocks[pos_config + 4]
    else {
        panic!("expected Excluded elements and steps table");
    };
    assert_eq!(ex_headers, &["Element or step", "Reason"]);

    let p = f.ws.project();
    let app = p.applicability();
    let expected_comp_reason = reasons_text(p, app.reasons(ElementRef::Component(f.comp_gw)));
    let expected_iface_reason = reasons_text(p, app.reasons(ElementRef::Interface(f.iface_eth0)));
    let expected_asset_reason = reasons_text(p, app.reasons(ElementRef::Asset(f.asset_fw)));
    let expected_threat_reason = reasons_text(p, app.reasons(ElementRef::Threat(f.threat_gw)));
    let expected_step_reason = reasons_text(p, app.node_reasons(f.tree_brake, f.step_off));

    let comp_vis = p
        .component_visual_id(p.component_idx(&f.comp_gw).expect("idx"))
        .expect("vis");
    let iface_vis = p
        .interface_visual_id(p.interface_idx(&f.iface_eth0).expect("idx"))
        .expect("vis");
    let asset_vis = p
        .asset_visual_id(p.asset_idx(&f.asset_fw).expect("idx"))
        .expect("vis");
    let threat_vis = p
        .threat_scenario_visual_id(p.threat_scenario_idx(&f.threat_gw).expect("idx"))
        .expect("vis");
    let tree_vis = p
        .attack_tree_visual_id(p.attack_tree_idx(&f.tree_brake).expect("idx"))
        .expect("vis");

    for expected in [
        vec![
            format!("{comp_vis} · Gateway"),
            expected_comp_reason.clone(),
        ],
        vec![format!("{iface_vis} · ETH0"), expected_iface_reason.clone()],
        vec![
            format!("{asset_vis} · Gateway Firmware"),
            expected_asset_reason.clone(),
        ],
        vec![
            format!("{threat_vis} · Spoof gateway"),
            expected_threat_reason.clone(),
        ],
        vec![
            format!("{tree_vis} · Inject Ethernet frame"),
            expected_step_reason,
        ],
    ] {
        assert!(
            ex_rows.contains(&expected),
            "missing excluded row {expected:?} in {ex_rows:?}"
        );
    }

    // HTML and PDF output.
    let html = report_html(&f.ws, &ctx);
    assert!(html.contains("Configuration and assumptions"));
    assert!(html.contains("1 of 2 conditions present"));

    let pdf = report_pdf(&f.ws, &ctx).expect("pdf renders");
    assert!(pdf.starts_with(b"%PDF-"));

    // XLSX full workbook and standalone risk register.
    let book = read_workbook(&report_workbook_xlsx(&f.ws)).expect("reads workbook");
    let cond_sheet = sheet(&book, "Conditions");
    assert_eq!(
        cond_sheet.headers,
        vec!["ID", "Name", "Present", "Needed by"]
    );
    let cond_cells: Vec<&Vec<String>> = cond_sheet.rows.iter().map(|(_, r)| r).collect();
    assert_eq!(
        cond_cells,
        vec![
            &vec![
                "CF00".to_owned(),
                "Ethernet".to_owned(),
                "no".to_owned(),
                "2".to_owned(),
            ],
            &vec![
                "CF01".to_owned(),
                "Debug port".to_owned(),
                "yes".to_owned(),
                "0".to_owned(),
            ],
        ]
    );

    // Risk register (both in workbook and standalone).
    let rr_only = read_workbook(&risk_register_xlsx(&f.ws, &ctx)).expect("reads risk register");
    for rr in [
        sheet(&book, "Risk register"),
        sheet(&rr_only, "Risk register"),
    ] {
        let name_col = col_idx(rr, "Threat scenario");
        let before_col = col_idx(rr, "Risk before controls");
        let after_col = col_idx(rr, "Risk after controls");
        let ex_col = col_idx(rr, "Excluded");

        let gw_row = find_row(rr, name_col, "Spoof gateway");
        assert_eq!(gw_row[before_col], "not applicable");
        assert_eq!(gw_row[after_col], "not applicable");
        assert_eq!(gw_row[ex_col], expected_threat_reason);

        let brake_row = find_row(rr, name_col, "Tamper brake");
        assert_ne!(brake_row[before_col], "not applicable");
        assert_ne!(brake_row[before_col], "not rated");
        assert_ne!(brake_row[after_col], "not applicable");
        assert_ne!(brake_row[after_col], "not rated");
        assert_eq!(brake_row[ex_col], "");
    }

    // Threat scenarios sheet.
    let threats = sheet(&book, "Threat scenarios");
    let t_name = col_idx(threats, "Threat scenario");
    let t_before = col_idx(threats, "Risk before controls");
    let t_after = col_idx(threats, "Risk after controls");
    let t_ex = col_idx(threats, "Excluded");
    let t_gw = find_row(threats, t_name, "Spoof gateway");
    assert_eq!(t_gw[t_before], "not applicable");
    assert_eq!(t_gw[t_after], "not applicable");
    assert_eq!(t_gw[t_ex], expected_threat_reason);
    let t_brake = find_row(threats, t_name, "Tamper brake");
    assert_ne!(t_brake[t_before], "not applicable");
    assert_ne!(t_brake[t_before], "not rated");
    assert_eq!(t_brake[t_ex], "");

    // Assets sheet.
    let assets = sheet(&book, "Assets");
    let a_name = col_idx(assets, "Asset");
    let a_ex = col_idx(assets, "Excluded");
    let a_fw = find_row(assets, a_name, "Gateway Firmware");
    assert_eq!(a_fw[a_ex], expected_asset_reason);
    let a_brake = find_row(assets, a_name, "Brake Signal");
    assert_eq!(a_brake[a_ex], "");

    // Components sheet.
    let comps = sheet(&book, "Components");
    let c_name = col_idx(comps, "Component");
    let c_ex = col_idx(comps, "Excluded");
    let c_gw = find_row(comps, c_name, "Gateway");
    assert_eq!(c_gw[c_ex], expected_comp_reason);

    // Interfaces sheet.
    let ifaces = sheet(&book, "Interfaces");
    let i_name = col_idx(ifaces, "Interface");
    let i_ex = col_idx(ifaces, "Excluded");
    let i_eth0 = find_row(ifaces, i_name, "ETH0");
    assert_eq!(i_eth0[i_ex], expected_iface_reason);

    // Silence unused fields check.
    let _ = (f.asset_brake, f.threat_brake);
}

#[test]
fn all_conditions_present_omits_absent_row_and_has_empty_excluded_table() {
    let mut f = build_fixture();
    run(
        &mut f.ws,
        &Edit::SetConditionPresent {
            condition: f.cond_eth,
            present: true,
        },
    );

    let doc = report_doc(&f.ws, &context());
    let cover = cover_rows(&doc);
    assert!(cover.iter().any(|r| r
        == &vec![
            "Configuration".to_owned(),
            "2 of 2 conditions present".to_owned()
        ]));
    assert!(!cover.iter().any(|r| r[0] == "Absent conditions"));

    let pos_config = doc
        .blocks
        .iter()
        .position(|b| matches!(b, Block::Heading(2, t) if t == "Configuration and assumptions"))
        .expect("Configuration and assumptions heading");
    let Block::Table { rows: ex_rows, .. } = &doc.blocks[pos_config + 4] else {
        panic!("expected Excluded elements and steps table");
    };
    assert!(
        ex_rows.is_empty(),
        "expected 0 excluded rows, got {ex_rows:?}"
    );
}

#[test]
fn xlsx_formula_guard_on_condition_name_and_excluded_reason() {
    let mut ws = workspace();
    let cond = ConditionId::new_v7();
    run(
        &mut ws,
        &Edit::AddCondition {
            id: cond,
            name: "=HYPERLINK(\"x\")".into(),
        },
    );
    run(
        &mut ws,
        &Edit::SetConditionPresent {
            condition: cond,
            present: false,
        },
    );
    let comp = ComponentId::new_v7();
    run(
        &mut ws,
        &Edit::AddComponent {
            id: comp,
            name: "ECU".into(),
            parent: None,
        },
    );
    run(
        &mut ws,
        &Edit::SetConditionLink {
            element: ElementRef::Component(comp),
            condition: cond,
            linked: true,
        },
    );

    let book = read_workbook(&report_workbook_xlsx(&ws)).expect("reads workbook");
    let conds = sheet(&book, "Conditions");
    let name_col = col_idx(conds, "Name");
    assert_eq!(conds.rows[0].1[name_col], "'=HYPERLINK(\"x\")");

    let comps = sheet(&book, "Components");
    let ex_col = col_idx(comps, "Excluded");
    let reason = &comps.rows[0].1[ex_col];
    assert!(!reason.starts_with('='));
    assert!(
        reason.contains("Needs CF00 =HYPERLINK(\"x\") (off)"),
        "unexpected reason cell: {reason}"
    );
}
