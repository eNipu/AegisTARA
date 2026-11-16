//! Top attack paths and choke points over the view (120f).

use std::sync::Arc;

use aegis_client::commands::Edit;
use aegis_client::pages::{overview, overview_in};
use aegis_client::systems::SystemView;
use aegis_client::top_paths::{TOP, TopPaths, top_paths};
use aegis_client::workspace::{Origin, Workspace};
use aegis_model::{
    AssetId, ComponentId, ElementId, InterfaceId, ItemRef, Methodology, ThreatId, TreeId,
};

fn workspace() -> Workspace {
    Workspace::empty(Arc::new(Methodology::iso21434_r155().expect("builtin")))
}

fn run(ws: &mut Workspace, edit: &Edit) {
    let ops = edit.to_ops(ws.project(), "mira", 1).expect("builds");
    ws.apply(ops, Origin::Edit).expect("applies");
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

fn add_threat(ws: &mut Workspace, component: ComponentId) -> ThreatId {
    let asset = AssetId::new_v7();
    run(
        ws,
        &Edit::AddAssetBound {
            id: asset,
            name: "Data".into(),
            properties: (0..6).collect(),
            component,
        },
    );
    let id = ThreatId::new_v7();
    run(
        ws,
        &Edit::AddThreat {
            id,
            name: "Attack".into(),
            asset,
            category: 0,
        },
    );
    id
}

/// A tree from an outline, linked to `threat`; `rates` gives the feasibility
/// position of steps by label.
fn tree(
    ws: &mut Workspace,
    threat: Option<ThreatId>,
    outline: &str,
    rates: &[(&str, usize)],
) -> TreeId {
    let id = TreeId::new_v7();
    run(
        ws,
        &Edit::NewTree {
            id,
            root: "Goal".into(),
            threat,
        },
    );
    run(
        ws,
        &Edit::ReplaceTree {
            tree: id,
            outline: outline.into(),
        },
    );
    for (label, level) in rates {
        let node = node(ws, id, label);
        run(
            ws,
            &Edit::SetStepFeasibility {
                tree: id,
                node,
                level: Some(*level),
            },
        );
    }
    id
}

fn node(ws: &Workspace, tree: TreeId, label: &str) -> aegis_model::NodeId {
    let p = ws.project();
    let t = p
        .attack_tree_idx(&tree)
        .and_then(|i| p.attack_tree(i))
        .expect("tree");
    t.iter()
        .find(|(_, _, n)| n.label() == label)
        .map(|(_, i, _)| i)
        .expect("node")
}

fn plain(ws: &mut Workspace, outline: &str, rates: &[(&str, usize)]) -> TreeId {
    let c = add_component(ws, "ECU", None);
    let t = add_threat(ws, c);
    tree(ws, Some(t), outline, rates)
}

#[test]
fn one_row_per_step_easiest_first() {
    let mut ws = workspace();
    let t = plain(
        &mut ws,
        "OR Goal\n  - Low step\n  - High step\n  - Medium step\n",
        &[("Low step", 1), ("High step", 3), ("Medium step", 2)],
    );
    let top = top_paths(&ws, &SystemView::all());
    let steps: Vec<_> = top.paths.iter().map(|p| p.steps.clone()).collect();
    assert_eq!(
        steps,
        [vec!["High step"], vec!["Medium step"], vec!["Low step"]]
    );
    let m = ws.project().methodology();
    let labels: Vec<_> = m
        .feasibility_levels()
        .map(|(_, _, l)| l.to_owned())
        .collect();
    let got: Vec<_> = top.paths.iter().map(|p| p.feasibility.clone()).collect();
    assert_eq!(
        got,
        [labels[3].clone(), labels[2].clone(), labels[1].clone()]
    );
    assert_eq!(top.paths[0].bottleneck, node(&ws, t, "High step"));
    assert_eq!(top.paths[0].tree, t);
    assert_eq!(top.paths[0].goal, "Goal");
}

#[test]
fn shared_step_is_the_first_choke_point() {
    let mut ws = workspace();
    plain(
        &mut ws,
        "AND Goal\n  - x\n  OR Inner\n    - y\n    - z\n",
        &[("x", 3), ("y", 3), ("z", 2)],
    );
    let top = top_paths(&ws, &SystemView::all());
    assert_eq!(top.paths.len(), 2);
    assert!(top.paths.iter().all(|p| p.steps.contains(&"x".to_owned())));
    let first = &top.choke_points[0];
    assert_eq!(first.step, "x");
    assert_eq!((first.paths, first.of), (2, 2));
}

#[test]
fn the_view_limits_the_trees() {
    let mut ws = workspace();
    let brake = add_system(&mut ws, "Brake");
    let adas = add_system(&mut ws, "ADAS");
    let c1 = add_component(&mut ws, "ESC", Some(brake));
    let c2 = add_component(&mut ws, "Cam", Some(adas));
    let t1 = add_threat(&mut ws, c1);
    let t2 = add_threat(&mut ws, c2);
    tree(
        &mut ws,
        Some(t1),
        "OR Goal\n  - Brake step\n",
        &[("Brake step", 1)],
    );
    tree(
        &mut ws,
        Some(t2),
        "OR Goal\n  - Adas step\n",
        &[("Adas step", 3)],
    );
    let only = top_paths(&ws, &SystemView::only(brake));
    assert_eq!(only.paths.len(), 1);
    assert_eq!(only.paths[0].steps, ["Brake step"]);
    assert_eq!(only.choke_points.len(), 1);
    assert_eq!(only.choke_points[0].step, "Brake step");
    let all = top_paths(&ws, &SystemView::all());
    let steps: Vec<_> = all.paths.iter().map(|p| p.steps[0].clone()).collect();
    assert_eq!(steps, ["Adas step", "Brake step"]);
}

#[test]
fn ties_keep_tree_order_and_calls_agree() {
    let mut ws = workspace();
    plain(&mut ws, "OR Goal\n  - First\n", &[("First", 2)]);
    plain(&mut ws, "OR Goal\n  - Second\n", &[("Second", 2)]);
    let a = top_paths(&ws, &SystemView::all());
    let b = top_paths(&ws, &SystemView::all());
    assert_eq!(a, b);
    let steps: Vec<_> = a.paths.iter().map(|p| p.steps[0].clone()).collect();
    assert_eq!(steps, ["First", "Second"]);
}

#[test]
fn seven_trees_give_the_five_easiest() {
    let mut ws = workspace();
    let levels = [0, 1, 2, 3, 0, 1, 2];
    for (i, l) in levels.iter().enumerate() {
        let name = format!("S{i}");
        plain(
            &mut ws,
            &format!("OR Goal\n  - {name}\n"),
            &[(name.as_str(), *l)],
        );
    }
    let top = top_paths(&ws, &SystemView::all());
    assert_eq!(top.paths.len(), TOP);
    let steps: Vec<_> = top.paths.iter().map(|p| p.steps[0].clone()).collect();
    assert_eq!(steps, ["S3", "S2", "S6", "S1", "S5"]);
}

#[test]
fn unrated_unlinked_and_empty() {
    let mut ws = workspace();
    assert_eq!(top_paths(&ws, &SystemView::all()), TopPaths::default());
    plain(&mut ws, "OR Goal\n  - Unrated\n", &[]);
    tree(&mut ws, None, "OR Goal\n  - Loose\n", &[("Loose", 3)]);
    assert_eq!(top_paths(&ws, &SystemView::all()), TopPaths::default());
}

#[test]
fn a_step_via_another_system_names_it() {
    let mut ws = workspace();
    let brake = add_system(&mut ws, "Brake");
    let gateway = add_system(&mut ws, "Gateway");
    let esc = add_component(&mut ws, "ESC", Some(brake));
    let gw = add_component(&mut ws, "GW", Some(gateway));
    let obd = InterfaceId::new_v7();
    run(
        &mut ws,
        &Edit::AddInterface {
            id: obd,
            name: "OBD".into(),
            component: gw,
            kind: 0,
        },
    );
    let th = add_threat(&mut ws, esc);
    let t = tree(
        &mut ws,
        Some(th),
        "OR Goal\n  - Open port\n",
        &[("Open port", 2)],
    );
    let n = node(&ws, t, "Open port");
    run(
        &mut ws,
        &Edit::SetStepVia {
            tree: t,
            node: n,
            via: Some(ItemRef::Component(gw)),
        },
    );
    let top = top_paths(&ws, &SystemView::all());
    assert_eq!(top.paths[0].steps, ["Open port via GW · Gateway"]);
    run(
        &mut ws,
        &Edit::SetStepVia {
            tree: t,
            node: n,
            via: Some(ItemRef::Interface(obd)),
        },
    );
    let top = top_paths(&ws, &SystemView::all());
    assert_eq!(top.paths[0].steps, ["Open port via GW / OBD · Gateway"]);
}

#[test]
fn overview_carries_the_paths() {
    let mut ws = workspace();
    let brake = add_system(&mut ws, "Brake");
    let c = add_component(&mut ws, "ESC", Some(brake));
    let th = add_threat(&mut ws, c);
    tree(
        &mut ws,
        Some(th),
        "OR Goal\n  - A\n  - B\n",
        &[("A", 3), ("B", 1)],
    );
    for view in [SystemView::all(), SystemView::only(brake)] {
        let o = overview_in(&ws, &view);
        let t = top_paths(&ws, &view);
        assert_eq!(o.top_paths, t.paths);
        assert_eq!(o.choke_points, t.choke_points);
        assert_eq!(o.top_paths.len(), 2);
    }
    assert_eq!(overview(&ws).top_paths.len(), 2);
}
