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
    AssetId, ComponentId, ControlId, DamageId, DataFlowId, ElementId, ElementKind, InterfaceId,
    ItemRef, Methodology, ThreatId,
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
    let mut ws = workspace();
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
    ws
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
const ADAS_ONLY: [&str; 6] = [
    "ADAS camera",
    "ADAS camera port",
    "ADAS camera data",
    "ADAS camera attack",
    "ADAS only lane loss",
    "ADAS only lens check",
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
    let start = doc
        .blocks
        .iter()
        .position(|b| matches!(b, Block::Heading(2, t) if t.starts_with(prefix)))
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
}
