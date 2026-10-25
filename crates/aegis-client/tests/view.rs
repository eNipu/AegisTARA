//! The view model over the sample project.

use std::sync::Arc;

use aegis_canvas::Tone;
use aegis_client::commands::Edit;
use aegis_client::sample::sample_ops;
use aegis_client::view::{UiState, inspector, threat_rows, tree_view};
use aegis_client::workspace::{Origin, Workspace};
use aegis_model::Methodology;

fn workspace() -> Workspace {
    let m = Arc::new(Methodology::iso21434_r155().expect("builtin"));
    let mut ws = Workspace::empty(Arc::clone(&m));
    ws.apply(sample_ops(&m, "demo", 1).expect("sample"), Origin::Edit)
        .expect("applies");
    ws
}

fn ui_for(ws: &Workspace) -> UiState {
    let rows = threat_rows(ws);
    UiState {
        selected_threat: Some(rows[0].id),
        selected_node: None,
        show_residual: false,
    }
}

#[test]
fn the_threat_list_shows_inherent_and_residual_risk() {
    let ws = workspace();
    let rows = threat_rows(&ws);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].name, "Spoof a remote command to the TCU");
    assert_eq!(rows[0].asset, "Telematics control unit");
    assert!(rows[0].has_tree && !rows[1].has_tree);
    let (inherent, residual) = (
        rows[0].inherent.as_ref().expect("rated"),
        rows[0].residual.as_ref().expect("rated"),
    );
    assert_ne!(
        inherent.label, residual.label,
        "the controls lower the risk"
    );
    assert_ne!(inherent.tone, residual.tone, "and a cooler color");
    assert_ne!(inherent.tone, Tone::Unrated);
}

#[test]
fn the_tree_view_lays_out_eight_nodes_and_marks_the_deciding_path() {
    let ws = workspace();
    let ui = ui_for(&ws);
    let view = tree_view(&ws, &ui).expect("tree");
    assert_eq!(view.scene.len(), 8);
    assert!(!view.residual);
    // The root is annotated with the evaluated feasibility.
    let root = view.scene.style(0).expect("style");
    assert_eq!(root.score.as_deref(), Some("High"));
    assert!(root.critical, "the root is on the deciding path");
    // The deciding path ends at the debug port step, the easiest one.
    let critical: Vec<usize> = (0..8)
        .filter(|i| view.scene.style(*i as u32).is_some_and(|s| s.critical))
        .collect();
    assert_eq!(critical.len(), 2, "root and the deciding step");
    let decided = view.scene.label(critical[1] as u32);
    assert_eq!(decided, "Exploit the debug port on the TCU board");
    // The step rated through attack potential shows its points.
    let downgrade = (0..8)
        .find(|i| view.scene.label(*i as u32).starts_with("Downgrade"))
        .expect("step");
    assert_eq!(
        view.scene
            .style(downgrade as u32)
            .and_then(|s| s.score.clone())
            .as_deref(),
        Some("12 · High")
    );

    // Residual: the deciding path moves to another step and the root is cooler.
    let residual = tree_view(
        &ws,
        &UiState {
            show_residual: true,
            ..ui
        },
    )
    .expect("tree");
    assert_eq!(
        residual
            .scene
            .style(0)
            .and_then(|s| s.score.clone())
            .as_deref(),
        Some("Low")
    );
    assert!(residual.residual);
}

#[test]
fn no_tree_means_no_view() {
    let ws = workspace();
    let rows = threat_rows(&ws);
    let ui = UiState {
        selected_threat: Some(rows[1].id),
        ..UiState::default()
    };
    assert!(tree_view(&ws, &ui).is_none());
    assert!(tree_view(&ws, &UiState::default()).is_none());
    assert!(inspector(&ws, &UiState::default()).is_none());
}

#[test]
fn the_inspector_describes_steps_and_gates() {
    let ws = workspace();
    let mut ui = ui_for(&ws);
    let view = tree_view(&ws, &ui).expect("tree");
    let project = ws.project();
    let tree = project
        .attack_tree(project.attack_trees().next().expect("tree").0)
        .expect("tree");
    let id_of = |label_start: &str| {
        let i = (0..8)
            .find(|i| view.scene.label(*i as u32).starts_with(label_start))
            .expect("node");
        tree.id_of(view.scene.node_idx(i as u32).expect("node"))
            .expect("id")
    };
    // A directly rated step.
    ui.selected_node = Some(id_of("Phish"));
    let step = inspector(&ws, &ui).expect("inspector");
    assert!(step.is_step);
    assert_eq!(step.kind, "Step");
    assert_eq!(
        step.levels
            .iter()
            .map(|c| c.label.as_str())
            .collect::<Vec<_>>(),
        ["Very low", "Low", "Medium", "High"]
    );
    assert_eq!(step.rating, Some(1));
    assert_eq!(step.effective.as_deref(), Some("Low"));
    assert_eq!(step.potential, None);
    assert_eq!(step.factors.len(), 5);
    assert!(step.factors.iter().all(|f| f.selected.is_none()));

    // The step rated through attack potential.
    ui.selected_node = Some(id_of("Downgrade"));
    let step = inspector(&ws, &ui).expect("inspector");
    assert_eq!(step.potential.as_deref(), Some("12 (High)"));
    assert!(step.factors.iter().all(|f| f.selected.is_some()));
    assert_eq!(step.factors[0].options[1].label, "<= 1 week (1)");
    assert_eq!(step.factors[0].selected, Some(1));

    // A gate explains its value.
    ui.selected_node = Some(id_of("Execute"));
    let gate = inspector(&ws, &ui).expect("inspector");
    assert!(!gate.is_step);
    assert_eq!(gate.kind, "OR gate");
    assert_eq!(gate.evaluated.as_deref(), Some("High"));
    assert!(gate.factors.is_empty() && gate.effective.is_none());
    assert_eq!(gate.why[0], "OR: the easiest child decides");
    assert!(gate.why[1].contains("Exploit the debug port"));
}

#[test]
fn edits_turn_into_typed_ops_and_change_what_the_view_shows() {
    let mut ws = workspace();
    let mut ui = ui_for(&ws);
    let view = tree_view(&ws, &ui).expect("tree");
    let project = ws.project();
    let (tree_idx, tree_id, _) = project.attack_trees().next().expect("tree");
    let tree = project.attack_tree(tree_idx).expect("tree");
    let debug = (0..8)
        .find(|i| view.scene.label(*i as u32).starts_with("Exploit"))
        .expect("step");
    let node = tree
        .id_of(view.scene.node_idx(debug as u32).expect("node"))
        .expect("id");
    ui.selected_node = Some(node);

    // Rate the debug port step "very low": the tree gets harder to attack.
    let before = threat_rows(&ws)[0].inherent.clone();
    let ops = Edit::SetStepFeasibility {
        tree: tree_id,
        node,
        level: Some(0),
    }
    .to_ops(ws.project(), "demo", 2)
    .expect("ops");
    ws.apply(ops, Origin::Edit).expect("applies");
    let after = threat_rows(&ws)[0].inherent.clone();
    assert_ne!(before, after);
    assert_eq!(inspector(&ws, &ui).expect("inspector").rating, Some(0));

    // Bad positions are refused before any op exists.
    assert!(
        Edit::SetStepFeasibility {
            tree: tree_id,
            node,
            level: Some(9)
        }
        .to_ops(ws.project(), "d", 1)
        .is_err()
    );
    assert!(
        Edit::SetStepFactor {
            tree: tree_id,
            node,
            factor: 9,
            level: None
        }
        .to_ops(ws.project(), "d", 1)
        .is_err()
    );
    assert!(
        Edit::SetStepFactor {
            tree: tree_id,
            node,
            factor: 0,
            level: Some(99)
        }
        .to_ops(ws.project(), "d", 1)
        .is_err()
    );
}
