//! Descriptions through the client's edit path (job 064): setting one, and
//! deleting an element that has one, which clears the text first so a single
//! undo brings back the element and its text.

use std::sync::Arc;

use aegis_client::commands::Edit;
use aegis_client::sample::sample_ops;
use aegis_client::workspace::{Origin, Workspace};
use aegis_model::{ElementRef, Methodology, Op};

/// The sample TARA plus one control, goal and claim (the sample has none).
fn workspace() -> Workspace {
    let m = Arc::new(Methodology::iso21434_r155().expect("builtin"));
    let mut ws = Workspace::empty(Arc::clone(&m));
    ws.apply(sample_ops(&m, "demo", 1).expect("sample"), Origin::Edit)
        .expect("applies");
    for e in [
        Edit::AddControl {
            id: aegis_model::ControlId::new_v7(),
            name: "Secure boot".into(),
        },
        Edit::AddGoal {
            id: aegis_model::GoalId::new_v7(),
            name: "Authentic firmware".into(),
        },
        Edit::AddClaim {
            id: aegis_model::ClaimId::new_v7(),
            name: "Accepted risk".into(),
        },
    ] {
        let ops = e.to_ops(ws.project(), "demo", 2).expect("ops");
        ws.apply(ops, Origin::Edit).expect("applies");
    }
    ws
}

fn edit(ws: &mut Workspace, edit: &Edit) {
    let ops = edit.to_ops(ws.project(), "alice", 50).expect("ops");
    ws.apply(ops, Origin::Edit).expect("applies");
}

fn set(target: ElementRef, text: &str) -> Edit {
    Edit::SetDescription {
        target,
        text: text.to_owned(),
    }
}

#[test]
fn setting_a_description_is_one_op_with_the_editor_as_author() {
    let ws = workspace();
    let (_, asset, _) = ws.project().assets().next().expect("asset");
    let ops = set(ElementRef::Asset(asset), "Line one\r\nLine two")
        .to_ops(ws.project(), "alice", 50)
        .expect("ops");
    assert_eq!(ops.len(), 1);
    let Op::SetDescription {
        target,
        text,
        provenance,
    } = &ops[0]
    else {
        panic!("{ops:?}");
    };
    assert_eq!(*target, ElementRef::Asset(asset));
    assert_eq!(&**text, "Line one\nLine two", "line breaks are normalized");
    assert_eq!(
        (provenance.actor(), provenance.timestamp_ms()),
        ("alice", 50)
    );
}

#[test]
fn deleting_each_kind_of_element_clears_its_description_and_undo_restores_it() {
    type Remove = fn(&Workspace) -> (ElementRef, Edit);
    let cases: [(&str, Remove); 6] = [
        ("claim", |ws| {
            let (_, id, _) = ws.project().cybersecurity_claims().next().expect("claim");
            (ElementRef::Claim(id), Edit::RemoveClaim { claim: id })
        }),
        ("goal", |ws| {
            let (_, id, _) = ws.project().cybersecurity_goals().next().expect("goal");
            (ElementRef::Goal(id), Edit::RemoveGoal { goal: id })
        }),
        ("control", |ws| {
            let (_, id, _) = ws.project().security_controls().next().expect("control");
            (ElementRef::Control(id), Edit::RemoveControl { control: id })
        }),
        ("threat", |ws| {
            let (_, id, _) = ws.project().threat_scenarios().next().expect("threat");
            (ElementRef::Threat(id), Edit::RemoveThreat { threat: id })
        }),
        ("tree", |ws| {
            let (_, id, _) = ws.project().attack_trees().next().expect("tree");
            (ElementRef::Tree(id), Edit::RemoveTree { tree: id })
        }),
        ("damage", |ws| {
            let (_, id, _) = ws.project().damage_scenarios().next().expect("damage");
            (ElementRef::Damage(id), Edit::RemoveDamage { damage: id })
        }),
    ];
    for (name, pick) in cases {
        let mut ws = workspace();
        let (target, remove) = pick(&ws);
        edit(&mut ws, &set(target, "Keep this text."));
        let before = ws.project().clone();

        edit(&mut ws, &remove);
        assert_eq!(
            ws.project().description(target),
            None,
            "{name}: gone with the element"
        );
        assert!(ws.project() != &before, "{name}: removed");

        ws.undo().expect("one undo step");
        assert_eq!(ws.project(), &before, "{name}: element and text are back");
        assert_eq!(
            ws.project().description(target),
            Some("Keep this text."),
            "{name}"
        );
    }
}

#[test]
fn deleting_an_asset_clears_its_description_first() {
    let m = Arc::new(Methodology::iso21434_r155().expect("builtin"));
    let mut ws = Workspace::empty(Arc::clone(&m));
    let id = aegis_model::AssetId::new_v7();
    edit(
        &mut ws,
        &Edit::AddAsset {
            id,
            name: "Gateway".into(),
            properties: vec![0],
        },
    );
    edit(&mut ws, &set(ElementRef::Asset(id), "Text"));
    let before = ws.project().clone();
    edit(&mut ws, &Edit::RemoveAsset { asset: id });
    assert_eq!(ws.project().asset_count(), 0);
    ws.undo().expect("undo");
    assert_eq!(ws.project(), &before);
}

#[test]
fn every_page_that_shows_elements_offers_a_description_box() {
    use aegis_client::view::{Page, UiState, page_descriptions, threat_rows};

    let mut ws = workspace();
    let (_, asset, _) = ws.project().assets().next().expect("asset");
    let (_, damage, _) = ws.project().damage_scenarios().next().expect("damage");
    let (_, control, _) = ws.project().security_controls().next().expect("control");
    let (_, goal, _) = ws.project().cybersecurity_goals().next().expect("goal");
    let (_, claim, _) = ws.project().cybersecurity_claims().next().expect("claim");
    let threat = threat_rows(&ws)[0].id;
    edit(&mut ws, &set(ElementRef::Project, "Scope: the ECU."));
    edit(&mut ws, &set(ElementRef::Asset(asset), "About the asset"));

    let ui = |page| UiState {
        page,
        selected_threat: Some(threat),
        selected_asset: Some(asset),
        selected_damage: Some(damage),
        selected_control: Some(control),
        selected_goal: Some(goal),
        selected_claim: Some(claim),
        ..UiState::default()
    };
    let kinds = |page| -> Vec<&'static str> {
        page_descriptions(&ws, &ui(page))
            .iter()
            .map(|d| d.kind)
            .collect()
    };
    assert_eq!(kinds(Page::Overview), ["project"]);
    assert_eq!(kinds(Page::Assets), ["asset"]);
    assert_eq!(kinds(Page::Damage), ["damage"]);
    assert_eq!(kinds(Page::Threats), ["threat"]);
    assert_eq!(kinds(Page::Risks), ["threat"]);
    assert_eq!(kinds(Page::Trees), ["tree"]);
    assert_eq!(kinds(Page::Controls), ["control"]);
    assert_eq!(kinds(Page::Goals), ["goal", "claim"]);
    for page in [
        Page::Problems,
        Page::Versions,
        Page::History,
        Page::Reports,
        Page::Admin,
    ] {
        assert!(kinds(page).is_empty(), "{page:?} describes no element");
    }

    let notes = &page_descriptions(&ws, &ui(Page::Overview))[0];
    assert_eq!(notes.target, ElementRef::Project);
    assert_eq!(notes.text, "Scope: the ECU.");
    assert!(notes.by.contains("alice"), "{}", notes.by);
    let asset_box = &page_descriptions(&ws, &ui(Page::Assets))[0];
    assert_eq!(asset_box.target, ElementRef::Asset(asset));
    assert_eq!(asset_box.text, "About the asset");
    let empty = &page_descriptions(&ws, &ui(Page::Damage))[0];
    assert_eq!((empty.text.as_str(), empty.by.as_str()), ("", ""));

    // A selection that no longer exists offers nothing.
    let mut gone = ui(Page::Assets);
    gone.selected_asset = Some(aegis_model::AssetId::new_v7());
    assert!(page_descriptions(&ws, &gone).is_empty());
}

fn context() -> aegis_client::report::ReportContext {
    aegis_client::report::ReportContext {
        project: "Brake ECU".into(),
        user: "alice".into(),
        now_ms: 1_700_000_000_000,
        seq: 20,
        unsaved: false,
    }
}

/// Describes the project and one element of every kind.
fn described_workspace() -> (Workspace, Vec<&'static str>) {
    let mut ws = workspace();
    let (_, asset, _) = ws.project().assets().next().expect("asset");
    let (_, damage, _) = ws.project().damage_scenarios().next().expect("damage");
    let (_, threat, _) = ws.project().threat_scenarios().next().expect("threat");
    let (_, tree, _) = ws.project().attack_trees().next().expect("tree");
    let (_, control, _) = ws.project().security_controls().next().expect("control");
    let (_, goal, _) = ws.project().cybersecurity_goals().next().expect("goal");
    let (_, claim, _) = ws.project().cybersecurity_claims().next().expect("claim");
    let texts = [
        (ElementRef::Project, "NOTES scope <b>ECU</b> & gateway"),
        (ElementRef::Asset(asset), "ASSET text"),
        (ElementRef::Damage(damage), "DAMAGE text"),
        (
            ElementRef::Threat(threat),
            "THREAT line one\nTHREAT line two",
        ),
        (ElementRef::Tree(tree), "TREE text"),
        (ElementRef::Control(control), "CONTROL text"),
        (ElementRef::Goal(goal), "GOAL text"),
        (ElementRef::Claim(claim), "CLAIM text"),
    ];
    for (target, text) in texts {
        edit(&mut ws, &set(target, text));
    }
    (
        ws,
        vec![
            "ASSET",
            "DAMAGE",
            "THREAT line one",
            "THREAT line two",
            "TREE",
            "CONTROL",
            "GOAL",
            "CLAIM",
        ],
    )
}

#[test]
fn the_html_report_prints_notes_and_descriptions_escaped() {
    let (ws, words) = described_workspace();
    let html = aegis_client::report::report_html(&ws, &context());
    assert!(html.contains("<h2>Project notes</h2>"));
    assert!(
        html.contains("NOTES scope &lt;b&gt;ECU&lt;/b&gt; &amp; gateway"),
        "text is escaped"
    );
    assert!(!html.contains("<b>ECU</b>"));
    for word in words {
        assert!(html.contains(word), "{word} is in the report");
    }
}

#[test]
fn a_report_of_a_project_without_descriptions_has_no_description_parts() {
    let ws = workspace();
    let html = aegis_client::report::report_html(&ws, &context());
    assert!(!html.contains("Project notes"));
    assert!(!html.contains("class=\"desc\""));
    let doc = aegis_client::report::report_doc(&ws, &context());
    assert!(
        !doc.blocks.iter().any(
            |b| matches!(b, aegis_client::report::Block::Heading(_, h) if h == "Project notes")
        )
    );
}

#[test]
fn the_pdf_report_has_the_same_notes_and_descriptions() {
    use aegis_client::report::Block;
    let (ws, words) = described_workspace();
    let doc = aegis_client::report::report_doc(&ws, &context());
    assert!(
        doc.blocks
            .iter()
            .any(|b| matches!(b, Block::Heading(2, h) if h == "Project notes"))
    );
    let text: String = doc
        .blocks
        .iter()
        .filter_map(|b| match b {
            Block::Paragraph(t, _) => Some(t.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        text.contains("NOTES scope <b>ECU</b> & gateway"),
        "plain text, not markup"
    );
    for word in words {
        assert!(text.contains(word), "{word} is in the PDF report");
    }
}
