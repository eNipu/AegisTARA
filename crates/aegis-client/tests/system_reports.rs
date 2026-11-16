//! Reports for some systems (121a): the profile names systems, the report
//! keeps what shows in them, the cover names them and the audit detail too.

use std::sync::Arc;

use aegis_client::commands::Edit;
use aegis_client::report::{
    Block, ReportContext, ReportDoc, ReportFilters, ReportLanguage, ReportOptions, ReportProfile,
    report_doc_with_options, report_html_with_options,
};
use aegis_client::workspace::{Origin, Workspace};
use aegis_model::{
    AssetId, ClaimId, ComponentId, ControlId, DamageId, DataFlowId, ElementId, ElementKind,
    ElementNameDef, GoalId, InterfaceId, ItemRef, Methodology, MethodologyDef, ThreatId, TreeId,
    TrustBoundaryId,
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

fn add_system(ws: &mut Workspace, name: &str) -> ElementId {
    let id = ElementId::new_v7();
    run(
        ws,
        &Edit::AddSystem {
            id,
            name: name.into(),
        },
    );
    id
}

fn add_component(ws: &mut Workspace, name: &str, system: Option<ElementId>) -> ComponentId {
    let id = ComponentId::new_v7();
    run(
        ws,
        &Edit::AddComponent {
            id,
            name: name.into(),
            parent: None,
        },
    );
    if system.is_some() {
        run(
            ws,
            &Edit::MoveToSystem {
                components: vec![id],
                system,
            },
        );
    }
    id
}

/// An interface, an asset and a threat on `component`; returns the threat.
fn add_parts(ws: &mut Workspace, component: ComponentId, prefix: &str) -> ThreatId {
    run(
        ws,
        &Edit::AddInterface {
            id: InterfaceId::new_v7(),
            name: format!("{prefix} port"),
            component,
            kind: 0,
        },
    );
    let asset = AssetId::new_v7();
    run(
        ws,
        &Edit::AddAssetBound {
            id: asset,
            name: format!("{prefix} data"),
            properties: (0..6).collect(),
            component,
        },
    );
    let threat = ThreatId::new_v7();
    run(
        ws,
        &Edit::AddThreat {
            id: threat,
            name: format!("{prefix} attack"),
            asset,
            category: 0,
        },
    );
    threat
}

fn add_damage(ws: &mut Workspace, name: &str, threats: &[ThreatId]) {
    let id = DamageId::new_v7();
    run(
        ws,
        &Edit::AddDamage {
            id,
            name: name.into(),
        },
    );
    for t in threats {
        run(
            ws,
            &Edit::LinkDamage {
                threat: *t,
                damage: id,
                linked: true,
            },
        );
    }
}

fn add_control(ws: &mut Workspace, name: &str, targets: &[ComponentId]) {
    let id = ControlId::new_v7();
    run(
        ws,
        &Edit::AddControl {
            id,
            name: name.into(),
        },
    );
    for c in targets {
        run(
            ws,
            &Edit::ApplyControl {
                control: id,
                target: ItemRef::Component(*c),
            },
        );
    }
}

/// Brake and ADAS, each with a component, an interface, an asset and a
/// threat, plus a component in no system with the same.
fn fixture() -> Workspace {
    fixture_in(workspace())
}

/// The same fixture in `ws`, an empty workspace of any methodology.
fn fixture_in(mut ws: Workspace) -> Workspace {
    let brake = add_system(&mut ws, "Brake");
    let adas = add_system(&mut ws, "ADAS");
    let brake_ecu = add_component(&mut ws, "Brake ECU", Some(brake));
    let adas_cam = add_component(&mut ws, "ADAS camera", Some(adas));
    let free = add_component(&mut ws, "Loose gateway", None);
    let brake_t = add_parts(&mut ws, brake_ecu, "Brake ECU");
    let adas_t = add_parts(&mut ws, adas_cam, "ADAS camera");
    add_parts(&mut ws, free, "Loose gateway");
    run(
        &mut ws,
        &Edit::AddDataFlow {
            id: DataFlowId::new_v7(),
            name: "Brake to ADAS frames".into(),
            from: brake_ecu,
            to: adas_cam,
            interface: None,
        },
    );
    add_damage(&mut ws, "Brake and ADAS shared crash", &[brake_t, adas_t]);
    add_damage(&mut ws, "ADAS only lane loss", &[adas_t]);
    add_control(
        &mut ws,
        "Brake and ADAS shared filter",
        &[brake_ecu, adas_cam],
    );
    add_control(&mut ws, "ADAS only lens check", &[adas_cam]);
    add_adas_only(&mut ws, adas_cam, adas_t);
    ws
}

/// A trust boundary around the ADAS component, and an attack tree, a goal
/// and a claim linked to the ADAS threat only.
fn add_adas_only(ws: &mut Workspace, adas_cam: ComponentId, adas_t: ThreatId) {
    let boundary = TrustBoundaryId::new_v7();
    run(
        ws,
        &Edit::AddTrustBoundary {
            id: boundary,
            name: "ADAS only zone".into(),
        },
    );
    run(
        ws,
        &Edit::SetTrustBoundaryMember {
            boundary,
            component: adas_cam,
            member: true,
        },
    );
    run(
        ws,
        &Edit::NewTree {
            id: TreeId::new_v7(),
            root: "ADAS only blinding".into(),
            threat: Some(adas_t),
        },
    );
    let goal = GoalId::new_v7();
    run(
        ws,
        &Edit::AddGoal {
            id: goal,
            name: "ADAS only clear view".into(),
        },
    );
    run(
        ws,
        &Edit::SetThreatGoal {
            threat: adas_t,
            goal: Some(goal),
        },
    );
    let claim = ClaimId::new_v7();
    run(
        ws,
        &Edit::AddClaim {
            id: claim,
            name: "ADAS only lens is sealed".into(),
        },
    );
    run(
        ws,
        &Edit::SetThreatClaim {
            threat: adas_t,
            claim: Some(claim),
        },
    );
}

const BRAKE_ONLY: [&str; 5] = [
    "Brake ECU",
    "Brake ECU port",
    "Brake ECU data",
    "Brake ECU attack",
    "Brake to ADAS frames",
];
const NO_SYSTEM: [&str; 4] = [
    "Loose gateway",
    "Loose gateway port",
    "Loose gateway data",
    "Loose gateway attack",
];
const ADAS_ONLY: [&str; 10] = [
    "ADAS camera",
    "ADAS camera port",
    "ADAS camera data",
    "ADAS camera attack",
    "ADAS only lane loss",
    "ADAS only lens check",
    "ADAS only zone",
    "ADAS only blinding",
    "ADAS only clear view",
    "ADAS only lens is sealed",
];
const SHARED: [&str; 2] = [
    "Brake and ADAS shared crash",
    "Brake and ADAS shared filter",
];

fn options(systems: &[&str], language: ReportLanguage) -> ReportOptions {
    ReportOptions {
        profile: ReportProfile {
            language,
            filters: ReportFilters {
                systems: systems.iter().map(|s| (*s).to_owned()).collect(),
                ..ReportFilters::default()
            },
            ..ReportProfile::default()
        },
        baseline_project: None,
    }
}

fn doc(ws: &Workspace, systems: &[&str]) -> ReportDoc {
    report_doc_with_options(ws, &context(), &options(systems, ReportLanguage::English))
}

/// Every heading, paragraph and table cell of the report.
fn texts(doc: &ReportDoc) -> Vec<String> {
    let mut out = Vec::new();
    for b in &doc.blocks {
        match b {
            Block::Heading(_, t) | Block::Paragraph(t, _) => out.push(t.clone()),
            Block::Table { rows, .. } => out.extend(rows.iter().flatten().cloned()),
            _ => {}
        }
    }
    out
}

/// Whether the report has an element's own entry: a cell or heading
/// `"<visual id> · <name>"`. A data flow's end cell names a component
/// without a visual ID, so it does not count as that component's entry.
fn has_entry(texts: &[String], name: &str) -> bool {
    let suffix = format!(" · {name}");
    texts.iter().any(|t| t.ends_with(&suffix))
}

/// The cover table's row for `label`, if any.
fn cover_row(doc: &ReportDoc, label: &str) -> Option<Vec<String>> {
    doc.blocks.iter().find_map(|b| match b {
        Block::Table { rows, .. } => rows
            .iter()
            .find(|r| r.first().is_some_and(|c| c == label))
            .cloned(),
        _ => None,
    })
}

/// The first table after the level-2 heading that starts with `prefix`.
fn table_under(doc: &ReportDoc, prefix: &str) -> Vec<Vec<String>> {
    table_after(doc, 2, prefix)
}

/// The first table after the heading of `level` that starts with `prefix`.
fn table_after(doc: &ReportDoc, level: u8, prefix: &str) -> Vec<Vec<String>> {
    let start = doc
        .blocks
        .iter()
        .position(|b| matches!(b, Block::Heading(l, t) if *l == level && t.starts_with(prefix)))
        .expect("heading");
    doc.blocks[start..]
        .iter()
        .find_map(|b| match b {
            Block::Table { rows, .. } => Some(rows.clone()),
            _ => None,
        })
        .expect("table")
}

#[test]
fn a_profile_with_systems_round_trips() {
    let profile = options(&["Brake", "ADAS"], ReportLanguage::English).profile;
    let text = serde_json::to_string(&profile).expect("serializes");
    let back: ReportProfile = serde_json::from_str(&text).expect("loads");
    assert_eq!(back, profile);

    let mut value = serde_json::to_value(ReportProfile::default()).expect("value");
    let filters = value
        .get_mut("filters")
        .and_then(serde_json::Value::as_object_mut)
        .expect("filters");
    assert!(filters.remove("systems").is_some());
    let old: ReportProfile =
        serde_json::from_str(&serde_json::to_string(&value).expect("text")).expect("old loads");
    assert!(old.filters.systems.is_empty());
    assert_eq!(old, ReportProfile::default());
}

#[test]
fn the_audit_detail_names_the_systems() {
    let summary = |systems: Vec<String>| {
        ReportProfile {
            filters: ReportFilters {
                systems,
                ..ReportFilters::default()
            },
            ..ReportProfile::default()
        }
        .audit_summary()
    };
    let two = summary(vec!["Brake".into(), "ADAS".into()]);
    assert!(two.ends_with(" systems=Brake+ADAS"), "{two}");
    let ja = summary(vec!["ブレーキ".into()]);
    assert!(ja.ends_with(" systems=x"), "{ja}");

    let long: Vec<String> = (0..64).map(|_| "ブ".repeat(40)).collect();
    let many = summary(long);
    assert!(many.len() <= 256, "{}", many.len());
    assert!(many.bytes().all(|b| b.is_ascii_graphic() || b == b' '));
    assert!(many.contains("+56"), "{many}");

    let none = summary(Vec::new());
    assert!(!none.contains("systems="), "{none}");
    assert_eq!(none, ReportProfile::default().audit_summary());

    // A long ASCII name is cut to the server's cap.
    let wide = summary(vec!["B".repeat(300)]);
    assert_eq!(wide.len(), 256);
}

#[test]
fn the_cover_names_the_systems() {
    let ws = fixture();
    let brake = doc(&ws, &["Brake"]);
    assert_eq!(
        cover_row(&brake, "Systems"),
        Some(vec!["Systems".to_owned(), "Brake".to_owned()])
    );

    let expected: Vec<String> = ws
        .project()
        .elements(ElementKind::System)
        .map(|(_, e)| e.name().to_owned())
        .collect();
    let both = doc(&ws, &["Brake", "ADAS"]);
    assert_eq!(
        cover_row(&both, "Systems"),
        Some(vec!["Systems".to_owned(), expected.join(", ")])
    );

    let all = doc(&ws, &[]);
    assert_eq!(
        cover_row(&all, "Systems"),
        Some(vec!["Systems".to_owned(), "All systems".to_owned()])
    );

    let ja = report_doc_with_options(
        &ws,
        &context(),
        &options(&["Brake"], ReportLanguage::Japanese),
    );
    assert_eq!(
        cover_row(&ja, "システム"),
        Some(vec!["システム".to_owned(), "Brake".to_owned()])
    );

    let mut plain = workspace();
    add_component(&mut plain, "Lone ECU", None);
    let none = doc(&plain, &["Brake"]);
    assert_eq!(cover_row(&none, "Systems"), None);
    assert_eq!(cover_row(&doc(&plain, &[]), "Systems"), None);
}

#[test]
fn a_report_for_one_system_holds_only_that_system() {
    let ws = fixture();
    let t = texts(&doc(&ws, &["Brake"]));
    for name in BRAKE_ONLY.iter().chain(&NO_SYSTEM).chain(&SHARED) {
        assert!(has_entry(&t, name), "missing {name}");
    }
    for name in ADAS_ONLY {
        assert!(!has_entry(&t, name), "unexpected {name}");
    }
    // The whole report holds every ADAS-only element.
    let all = texts(&doc(&ws, &[]));
    for name in ADAS_ONLY {
        assert!(has_entry(&all, name), "full report misses {name}");
    }

    // The security settings table: its setting cell names the control, so
    // the control table cannot stand in for it.
    let applies = |d: &ReportDoc, name: &str| {
        let suffix = format!(" · {name}");
        table_after(d, 3, "Security settings")
            .iter()
            .any(|r| r.get(1).is_some_and(|c| c.ends_with(&suffix)))
    };
    let brake = doc(&ws, &["Brake"]);
    let full = doc(&ws, &[]);
    assert!(applies(&brake, "Brake and ADAS shared filter"));
    assert!(!applies(&brake, "ADAS only lens check"));
    assert!(applies(&full, "ADAS only lens check"));

    // The executive summary counts what shows: Brake plus no system.
    assert_eq!(summary_counts(&brake), (2, 2));
    assert_eq!(summary_counts(&full), (3, 3));
}

/// The threat and asset numbers of the executive summary's counts line.
fn summary_counts(doc: &ReportDoc) -> (usize, usize) {
    let line = doc
        .blocks
        .iter()
        .find_map(|b| match b {
            Block::Paragraph(t, _) if t.starts_with("The project has") => {
                Some(t.replace(['\u{2068}', '\u{2069}'], ""))
            }
            _ => None,
        })
        .expect("summary counts");
    let words: Vec<&str> = line.split_whitespace().collect();
    let threats = words[3].parse().expect("threats");
    let on = words.iter().position(|w| *w == "on").expect("on");
    let assets = words[on + 1].parse().expect("assets");
    (threats, assets)
}

#[test]
fn a_combined_report_lists_shared_elements_once() {
    let ws = fixture();
    let both = doc(&ws, &["Brake", "ADAS"]);
    let count = |rows: &[Vec<String>], name: &str| {
        let suffix = format!(" · {name}");
        rows.iter()
            .filter(|r| r.first().is_some_and(|c| c.ends_with(&suffix)))
            .count()
    };
    let damage = table_under(&both, "3.");
    assert_eq!(count(&damage, "Brake and ADAS shared crash"), 1);
    assert_eq!(count(&damage, "ADAS only lane loss"), 1);
    let controls = table_under(&both, "7.");
    assert_eq!(count(&controls, "Brake and ADAS shared filter"), 1);
    assert_eq!(count(&controls, "ADAS only lens check"), 1);
}

#[test]
fn a_stale_system_name_is_ignored() {
    let mut ws = workspace();
    let c = add_component(&mut ws, "Lone ECU", None);
    add_parts(&mut ws, c, "Lone ECU");
    let ctx = context();
    let stale = report_html_with_options(&ws, &ctx, &options(&["Brake"], ReportLanguage::English));
    let plain = report_html_with_options(&ws, &ctx, &options(&[], ReportLanguage::English));
    assert_eq!(stale, plain);

    // With systems in the project, a stale name is dropped from the filter.
    let ws = fixture();
    let html = |systems: &[&str]| {
        report_html_with_options(&ws, &ctx, &options(systems, ReportLanguage::English))
    };
    assert_eq!(html(&["Gone"]), html(&[]));
    assert_eq!(html(&["Brake", "Gone"]), html(&["Brake"]));
    assert_ne!(html(&["Brake"]), html(&[]));
}

// ---- the workbook (121b) ---------------------------------------------------

use aegis_client::excel::{SheetData, WorkbookData, read_workbook};
use aegis_client::report::{report_workbook_xlsx, report_workbook_xlsx_in};
use aegis_client::systems::{SystemView, view_of_names};

fn workbook(ws: &Workspace, systems: &[&str]) -> WorkbookData {
    let names: Vec<String> = systems.iter().map(|s| (*s).to_owned()).collect();
    let view = view_of_names(ws.project(), &names);
    read_workbook(&report_workbook_xlsx_in(ws, &[], &view)).expect("reads")
}

fn sheet<'a>(book: &'a WorkbookData, name: &str) -> &'a SheetData {
    book.sheets
        .iter()
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("no sheet {name}"))
}

/// The names of the twelve element sheets of `ws`'s methodology.
fn element_sheets(ws: &Workspace) -> Vec<String> {
    let n = ws.project().methodology().element_names();
    let mut out = vec!["Risk register".to_owned()];
    for name in [
        &n.component.plural,
        &n.interface.plural,
        &n.data_flow.plural,
        &n.trust_boundary.plural,
        &n.asset.plural,
        &n.damage_scenario.plural,
        &n.threat_scenario.plural,
        &n.attack_tree.plural,
        &n.security_control.plural,
        &n.cybersecurity_goal.plural,
        &n.cybersecurity_claim.plural,
    ] {
        out.push(name.to_string());
    }
    out
}

fn cell(row: &[String], i: usize) -> &str {
    row.get(i).map_or("", String::as_str)
}

/// The Systems cell of the data row whose column 1 is `name`, and how many
/// rows have that name.
fn systems_of(sheet: &SheetData, name: &str) -> (usize, String) {
    let last = sheet.headers.len() - 1;
    let rows: Vec<&Vec<String>> = sheet
        .rows
        .iter()
        .map(|(_, r)| r)
        .filter(|r| cell(r, 1) == name)
        .collect();
    let text = rows.first().map_or("", |r| cell(r, last)).to_owned();
    (rows.len(), text)
}

fn names_in(sheet: &SheetData) -> Vec<String> {
    sheet
        .rows
        .iter()
        .map(|(_, r)| cell(r, 1).to_owned())
        .collect()
}

#[test]
fn every_element_sheet_has_a_systems_column() {
    let ws = fixture();
    let book = workbook(&ws, &[]);
    let sheets = element_sheets(&ws);
    assert_eq!(sheets.len(), 12);
    for name in &sheets {
        let s = sheet(&book, name);
        assert_eq!(
            s.headers.last().map(String::as_str),
            Some("Systems"),
            "{name}"
        );
    }

    let project = ws.project();
    let comps = sheet(&book, &sheets[1]);
    let last = comps.headers.len() - 1;
    let mut seen = 0;
    for (idx, _, c) in project.components() {
        let expected = project
            .component_system(idx)
            .and_then(|s| project.element(s))
            .map_or(String::new(), |e| e.name().to_owned());
        let row = comps
            .rows
            .iter()
            .find(|(_, r)| cell(r, 1) == c.name())
            .expect("component row");
        assert_eq!(cell(&row.1, last), expected, "{}", c.name());
        seen += 1;
    }
    assert_eq!(seen, 3);
}

#[test]
fn a_shared_row_names_both_systems_once() {
    let ws = fixture();
    let book = workbook(&ws, &[]);
    let n = ws.project().methodology().element_names();
    let both: std::collections::BTreeSet<&str> = ["Brake", "ADAS"].into();
    let damage = sheet(&book, &n.damage_scenario.plural);
    let controls = sheet(&book, &n.security_control.plural);
    for (s, name) in [(damage, SHARED[0]), (controls, SHARED[1])] {
        let (count, text) = systems_of(s, name);
        assert_eq!(count, 1, "{name}");
        let got: std::collections::BTreeSet<&str> = text.split(", ").collect();
        assert_eq!(got, both, "{name}");
    }
    assert_eq!(
        systems_of(damage, "ADAS only lane loss"),
        (1, "ADAS".into())
    );
}

#[test]
fn an_element_in_no_system_has_an_empty_cell() {
    let ws = fixture();
    let book = workbook(&ws, &[]);
    let comps = sheet(
        &book,
        &ws.project().methodology().element_names().component.plural,
    );
    assert_eq!(systems_of(comps, NO_SYSTEM[0]), (1, String::new()));
    assert_eq!(systems_of(comps, BRAKE_ONLY[0]), (1, "Brake".into()));
}

#[test]
fn a_project_with_no_systems_exports_as_before() {
    let mut ws = workspace();
    let c = add_component(&mut ws, "Lone ECU", None);
    add_parts(&mut ws, c, "Lone ECU");
    let bytes = report_workbook_xlsx_in(&ws, &[], &SystemView::all());
    assert_eq!(bytes, report_workbook_xlsx(&ws));
    let book = read_workbook(&bytes).expect("reads");
    for s in &book.sheets {
        assert!(!s.headers.iter().any(|h| h == "Systems"), "{}", s.name);
    }
    // A stale filter changes nothing either.
    let named = workbook(&ws, &["Brake"]);
    assert_eq!(named, book);
}

#[test]
fn a_filtered_workbook_holds_only_the_filter() {
    let mut ws = fixture();
    // A step under the ADAS-only tree, for the Attack steps sheet.
    let (_, tree, _) = ws.project().attack_trees().next().expect("tree");
    run(
        &mut ws,
        &Edit::ReplaceTree {
            tree,
            outline: "OR ADAS only blinding\n  - Shine a laser at the lens\n".into(),
        },
    );
    // A data flow inside ADAS, for the data flows sheet.
    let project = ws.project();
    let (cam, adas) = project
        .components()
        .find(|(_, _, c)| c.name() == ADAS_ONLY[0])
        .map(|(idx, id, _)| (id, project.component_system(idx)))
        .expect("ADAS camera");
    let radar = add_component(&mut ws, "ADAS only radar", adas);
    run(
        &mut ws,
        &Edit::AddDataFlow {
            id: DataFlowId::new_v7(),
            name: "ADAS only radar feed".into(),
            from: radar,
            to: cam,
            interface: None,
        },
    );
    let book = workbook(&ws, &["Brake"]);
    let n = ws.project().methodology().element_names();
    let overview = sheet(&book, "Overview");
    for (plural, at) in [
        (n.component.plural.to_string(), 0),
        (n.asset.plural.to_string(), 2),
        (n.threat_scenario.plural.to_string(), 3),
    ] {
        let s = sheet(&book, &plural);
        let names = names_in(s);
        assert!(names.contains(&BRAKE_ONLY[at].to_owned()), "{plural}");
        assert!(names.contains(&NO_SYSTEM[at].to_owned()), "{plural}");
        assert!(!names.contains(&ADAS_ONLY[at].to_owned()), "{plural}");
        let count = overview
            .rows
            .iter()
            .find(|(_, r)| cell(r, 0) == plural)
            .map(|(_, r)| cell(r, 1).to_owned())
            .expect("overview row");
        assert_eq!(count, s.rows.len().to_string(), "{plural}");
    }
    // The whole workbook still holds the ADAS rows.
    let all = workbook(&ws, &[]);
    assert!(names_in(sheet(&all, &n.component.plural)).contains(&ADAS_ONLY[0].to_owned()));

    // Every sheet but the Overview: an ADAS-only name shows somewhere in the
    // whole workbook and nowhere in the Brake one.
    let holds = |book: &WorkbookData, name: &str| -> Vec<String> {
        let mut at = Vec::new();
        for s in book.sheets.iter().filter(|s| s.name != "Overview") {
            for (_, row) in &s.rows {
                for (col, text) in row.iter().enumerate() {
                    // The Brake data flow goes to the ADAS camera.
                    let to_adas = s.name == *n.data_flow.plural && col == 3;
                    if text == name && !to_adas {
                        at.push(s.name.clone());
                    }
                }
            }
        }
        at
    };
    for name in ADAS_ONLY
        .iter()
        .chain(&["ADAS only radar", "ADAS only radar feed"])
    {
        assert!(
            !holds(&all, name).is_empty(),
            "{name} not in the whole book"
        );
        assert_eq!(holds(&book, name), Vec::<String>::new(), "{name}");
    }
    let tree_rows = |book: &WorkbookData| {
        sheet(book, "Attack steps")
            .rows
            .iter()
            .filter(|(_, r)| cell(r, 1) == "ADAS only blinding")
            .count()
    };
    assert!(tree_rows(&all) > 0);
    assert_eq!(tree_rows(&book), 0);
    // Problems name the element they are about.
    let adas_problems = |book: &WorkbookData| {
        sheet(book, "Problems")
            .rows
            .iter()
            .filter(|(_, r)| cell(r, 1).contains(ADAS_ONLY[0]))
            .count()
    };
    assert!(adas_problems(&all) > 0);
    assert_eq!(adas_problems(&book), 0);
}

#[test]
fn each_overview_count_is_its_own_sheet_when_two_kinds_share_a_name() {
    let mut def =
        MethodologyDef::read_pack(Methodology::iso21434_r155_pack().as_bytes()).expect("pack");
    def.element_names.cybersecurity_goal = ElementNameDef::new("Requirement", "Requirements");
    def.element_names.cybersecurity_claim = ElementNameDef::new("Requirement", "Requirements");
    let m = Arc::new(Methodology::new(def).expect("valid"));
    let mut ws = fixture_in(Workspace::empty(m));
    // A Brake claim: the Brake view keeps one claim and no goal.
    let brake_t = ws
        .project()
        .threat_scenarios()
        .find(|(_, _, t)| t.name() == "Brake ECU attack")
        .map(|(_, id, _)| id)
        .expect("brake threat");
    let claim = ClaimId::new_v7();
    run(
        &mut ws,
        &Edit::AddClaim {
            id: claim,
            name: "Brake pads are checked".into(),
        },
    );
    run(
        &mut ws,
        &Edit::SetThreatClaim {
            threat: brake_t,
            claim: Some(claim),
        },
    );
    let book = workbook(&ws, &["Brake"]);
    let counts: Vec<String> = sheet(&book, "Overview")
        .rows
        .iter()
        .filter(|(_, r)| cell(r, 0) == "Requirements")
        .map(|(_, r)| cell(r, 1).to_owned())
        .collect();
    let goals = sheet(&book, "Requirements").rows.len();
    let claims = sheet(&book, "Requirements (2)").rows.len();
    assert_eq!((goals, claims), (0, 1));
    assert_eq!(counts, vec![goals.to_string(), claims.to_string()]);
}
