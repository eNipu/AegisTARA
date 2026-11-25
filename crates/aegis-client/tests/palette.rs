//! Palette entries and search (WBX-04): every element is found by its visual
//! ID, ranking follows the tiers, and a catalog ID finds its threat.

use std::sync::Arc;

use aegis_client::i18n::{Catalog, Language};
use aegis_client::palette::{PaletteEntry, PaletteTarget, entries, search};
use aegis_client::sample::sample_ops;
use aegis_client::view::Selection;
use aegis_client::workspace::{Origin, Workspace};
use aegis_model::{
    ClaimId, ComponentId, DataFlowId, GoalId, InterfaceId, Methodology, ModelError, Op, Provenance,
    SharedStepId, TrustBoundaryId,
};

/// The sample project plus a component (with an interface), a second
/// component, a data flow, a trust boundary, a goal and a claim, so every
/// kind the palette lists is present; the first threat carries R155 Annex 5
/// ref 4.1.
fn fixture() -> Result<Workspace, ModelError> {
    let m = Arc::new(Methodology::iso21434_r155().expect("builtin"));
    let mut ws = Workspace::empty(Arc::clone(&m));
    ws.apply(sample_ops(&m, "demo", 1).expect("sample"), Origin::Edit)
        .expect("sample applies");
    let prov = || Provenance::human("demo", 2);
    let (gateway, backend, iface) = (
        ComponentId::new_v7(),
        ComponentId::new_v7(),
        InterfaceId::new_v7(),
    );
    let threat = ws
        .project()
        .threat_scenarios()
        .next()
        .map(|(_, id, _)| id)
        .expect("a threat");
    let scheme = m.catalog_scheme("r155-annex5").expect("scheme");
    let catalog_ref = m.parse_catalog_ref(scheme, "4.1")?;
    let ops = vec![
        Op::AddComponent {
            id: gateway,
            name: "Central gateway".into(),
            parent: None,
            provenance: prov(),
        },
        Op::AddComponent {
            id: backend,
            name: "Backend server".into(),
            parent: None,
            provenance: prov(),
        },
        Op::AddInterface {
            id: iface,
            name: "Diagnostic CAN".into(),
            component: gateway,
            kind: m.interface_kind("can").expect("can"),
            provenance: prov(),
        },
        Op::AddDataFlow {
            id: DataFlowId::new_v7(),
            name: "Remote unlock request".into(),
            from: backend,
            to: gateway,
            interface: None,
            provenance: prov(),
        },
        Op::AddTrustBoundary {
            id: TrustBoundaryId::new_v7(),
            name: "Vehicle perimeter".into(),
            components: vec![gateway],
            provenance: prov(),
        },
        Op::AddCybersecurityGoal {
            id: GoalId::new_v7(),
            name: "Only signed firmware runs".into(),
            controls: vec![],
            provenance: prov(),
        },
        Op::AddCybersecurityClaim {
            id: ClaimId::new_v7(),
            name: "Remote access risk is shared with the OEM".into(),
            provenance: prov(),
        },
        Op::AddSharedStep {
            id: SharedStepId::new_v7(),
            name: "Replay a diagnostic request".into(),
            provenance: prov(),
        },
        Op::AddCatalogRef {
            threat,
            catalog_ref,
            provenance: prov(),
        },
    ];
    ws.apply(ops, Origin::Edit).expect("fixture applies");
    Ok(ws)
}

fn all_entries(ws: &Workspace) -> Vec<PaletteEntry> {
    entries(Some(ws), &Catalog::load(Language::En), true)
}

fn titles(hits: &[&PaletteEntry]) -> Vec<String> {
    hits.iter().map(|e| e.title.clone()).collect()
}

/// Every element is the first hit for its own visual ID; the expected target
/// comes from the project, not from the entry list.
#[test]
fn every_element_is_first_for_its_visual_id() -> Result<(), ModelError> {
    let ws = fixture()?;
    let all = all_entries(&ws);
    let p = ws.project();
    let mut expected: Vec<(String, Selection)> = Vec::new();
    let vid = |v: Option<String>| v.expect("visual id");
    for (i, id, _) in p.assets() {
        expected.push((vid(p.asset_visual_id(i)), Selection::Asset(id)));
    }
    for (i, id, _) in p.damage_scenarios() {
        expected.push((vid(p.damage_scenario_visual_id(i)), Selection::Damage(id)));
    }
    for (i, id, _) in p.threat_scenarios() {
        expected.push((vid(p.threat_scenario_visual_id(i)), Selection::Threat(id)));
    }
    for (i, id, _) in p.attack_trees() {
        expected.push((vid(p.attack_tree_visual_id(i)), Selection::Tree(id)));
    }
    for (i, id, _) in p.shared_steps() {
        expected.push((vid(p.shared_step_visual_id(i)), Selection::SharedStep(id)));
    }
    for (i, id, _) in p.security_controls() {
        expected.push((vid(p.security_control_visual_id(i)), Selection::Control(id)));
    }
    for (i, id, _) in p.cybersecurity_goals() {
        expected.push((vid(p.cybersecurity_goal_visual_id(i)), Selection::Goal(id)));
    }
    for (i, id, _) in p.cybersecurity_claims() {
        expected.push((
            vid(p.cybersecurity_claim_visual_id(i)),
            Selection::Claim(id),
        ));
    }
    for (i, id, _) in p.components() {
        expected.push((vid(p.component_visual_id(i)), Selection::Component(id)));
    }
    for (i, id, _) in p.interfaces() {
        expected.push((vid(p.interface_visual_id(i)), Selection::Interface(id)));
    }
    for (i, id, _) in p.data_flows() {
        expected.push((vid(p.data_flow_visual_id(i)), Selection::DataFlow(id)));
    }
    for (i, id, _) in p.trust_boundaries() {
        expected.push((
            vid(p.trust_boundary_visual_id(i)),
            Selection::TrustBoundary(id),
        ));
    }
    assert_eq!(expected.len(), 15, "fixture covers every kind");
    for (visual_id, target) in expected {
        let hits = search(&all, &visual_id, 5);
        let first = hits.first().expect("a hit");
        assert_eq!(first.target, PaletteTarget::Element(target), "{visual_id}");
        // Lower case and padding find the same element.
        let loose = search(&all, &format!("  {}  ", visual_id.to_lowercase()), 5);
        assert_eq!(loose.first().map(|e| &e.target), Some(&first.target));
    }
    Ok(())
}

/// A visual-ID prefix (CL00) beats a title prefix (Close tab), which beats
/// a title substring ("vehicle"); ties keep input order.
#[test]
fn visual_id_prefix_beats_name_substring() -> Result<(), ModelError> {
    let ws = fixture()?;
    let all = all_entries(&ws);
    assert_eq!(
        titles(&search(&all, "cl", 10)),
        [
            "Remote access risk is shared with the OEM",
            "Close tab",
            "Unauthorized remote command reaches the vehicle",
            "Vehicle perimeter",
        ]
    );
    Ok(())
}

/// A title that starts with the query beats one with a later word that does.
#[test]
fn name_prefix_beats_word_prefix() -> Result<(), ModelError> {
    let ws = fixture()?;
    let all = all_entries(&ws);
    assert_eq!(
        titles(&search(&all, "Remote", 10)),
        [
            "Remote access risk is shared with the OEM",
            "Remote unlock request",
            "Unauthorized remote command reaches the vehicle",
            "Spoof a remote command to the TCU",
            "Execute a forged remote command",
        ]
    );
    Ok(())
}

/// A catalog ID, by its ref or its whole text, finds the threat that carries it.
#[test]
fn catalog_id_finds_the_threat() -> Result<(), ModelError> {
    let ws = fixture()?;
    let all = all_entries(&ws);
    let threat = ws
        .project()
        .threat_scenarios()
        .next()
        .map(|(_, id, _)| Selection::Threat(id))
        .expect("a threat");
    for query in ["4.1", "UN R155 Annex 5 4.1", "un r155"] {
        let hits = search(&all, query, 5);
        assert_eq!(
            hits.first().map(|e| &e.target),
            Some(&PaletteTarget::Element(threat)),
            "{query}"
        );
        assert_eq!(hits[0].catalog, ["UN R155 Annex 5 4.1"]);
    }
    Ok(())
}

/// Empty query lists entries in order (commands first); no match is empty;
/// the limit holds.
#[test]
fn empty_query_no_match_and_limit() -> Result<(), ModelError> {
    let ws = fixture()?;
    let all = all_entries(&ws);
    let first = search(&all, "   ", 3);
    let targets: Vec<&PaletteTarget> = first.iter().map(|e| &e.target).collect();
    assert_eq!(
        targets,
        [
            &PaletteTarget::Command("open-folder"),
            &PaletteTarget::Command("save"),
            &PaletteTarget::Command("save-as"),
        ]
    );
    assert_eq!(first[0].keys, "⌘O");
    assert!(search(&all, "zzqx", 10).is_empty());
    assert_eq!(search(&all, "e", 4).len(), 4);
    assert!(search(&all, "e", 100).len() > 4);
    assert!(search(&all, "e", 0).is_empty());
    Ok(())
}

/// Without a project the palette lists only the commands, keyed per platform.
#[test]
fn no_project_lists_commands_only() {
    let catalog = Catalog::load(Language::En);
    let off_mac = entries(None, &catalog, false);
    assert!(
        off_mac
            .iter()
            .all(|e| matches!(e.target, PaletteTarget::Command(_)))
    );
    let redo = off_mac
        .iter()
        .find(|e| e.target == PaletteTarget::Command("redo"))
        .expect("redo");
    assert_eq!(
        (redo.title.as_str(), redo.keys.as_str()),
        ("Redo", "Ctrl+Y")
    );
    assert!(
        !off_mac
            .iter()
            .any(|e| e.target == PaletteTarget::Command("palette"))
    );
}

/// The developer source toggle appears in the palette, while grid-scoped
/// editing commands stay out of the palette (`in_palette: false`).
#[test]
fn developer_source_toggle_in_palette_and_grid_commands_hidden() {
    let catalog = Catalog::load(Language::En);
    let list = entries(None, &catalog, true);
    assert!(
        list.iter()
            .any(|e| e.target == PaletteTarget::Command("toggle-developer-source")),
        "toggle-developer-source should appear in the palette"
    );
    for id in [
        "grid-add-row",
        "grid-delete-rows",
        "grid-fill-down",
        "grid-copy",
        "grid-paste",
    ] {
        assert!(
            !list.iter().any(|e| e.target == PaletteTarget::Command(id)),
            "{id} should not appear in the palette"
        );
    }
}
