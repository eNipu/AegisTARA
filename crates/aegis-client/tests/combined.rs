//! Boundary flows on the item definition rows (120b).

use std::sync::Arc;

use aegis_client::combined::{boundary_flows, systems_overview};
use aegis_client::commands::Edit;
use aegis_client::pages::{data_flow_detail, data_flow_rows, overview, risk_rows, vocabulary};
use aegis_client::state::PageData;
use aegis_client::systems::{SystemView, filter_page, system_rows};
use aegis_client::view::{Page, UiState, inspector, tree_scene};
use aegis_client::workspace::{Origin, Workspace};
use aegis_model::{
    AssetId, ComponentId, ControlId, DamageId, DataFlowId, ElementId, InterfaceId, ItemRef,
    Methodology, NodeId, ThreatId, TreeId,
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

fn add_component(
    ws: &mut Workspace,
    name: &str,
    parent: Option<ComponentId>,
    system: Option<ElementId>,
) -> ComponentId {
    let id = ComponentId::new_v7();
    run(
        ws,
        &Edit::AddComponent {
            id,
            name: name.into(),
            parent,
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

fn add_flow(ws: &mut Workspace, name: &str, from: ComponentId, to: ComponentId) -> DataFlowId {
    let id = DataFlowId::new_v7();
    run(
        ws,
        &Edit::AddDataFlow {
            id,
            name: name.into(),
            from,
            to,
            interface: None,
        },
    );
    id
}

fn add_threat(ws: &mut Workspace, component: ComponentId) {
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
    run(
        ws,
        &Edit::AddThreat {
            id: ThreatId::new_v7(),
            name: "Attack".into(),
            asset,
            category: 0,
        },
    );
}

struct Fx {
    ws: Workspace,
    request: DataFlowId,
}

fn fixture() -> Fx {
    let mut ws = workspace();
    let brake = add_system(&mut ws, "Brake");
    let adas = add_system(&mut ws, "ADAS");
    let gateway = add_system(&mut ws, "Gateway");
    let esc = add_component(&mut ws, "ESC ECU", None, Some(brake));
    let valve = add_component(&mut ws, "Valve", Some(esc), None);
    let wheel = add_component(&mut ws, "Wheel sensor", None, Some(brake));
    let adas_ecu = add_component(&mut ws, "ADAS ECU", None, Some(adas));
    let gw = add_component(&mut ws, "GW", None, Some(gateway));
    let tester = add_component(&mut ws, "Tester", None, None);
    let request = add_flow(&mut ws, "Brake request", adas_ecu, gw);
    add_flow(&mut ws, "Brake command", gw, valve);
    add_flow(&mut ws, "Local", wheel, esc);
    add_flow(&mut ws, "Diag", tester, gw);
    add_threat(&mut ws, gw);
    Fx { ws, request }
}

#[test]
fn boundary_flows_are_the_cross_system_ones() {
    let fx = fixture();
    let flows = boundary_flows(fx.ws.project());
    assert_eq!(flows.len(), 2);
    assert_eq!(flows[0].name, "Brake request");
    assert_eq!(flows[0].systems, "ADAS → Gateway");
    assert_eq!(flows[1].name, "Brake command");
    assert_eq!(flows[1].systems, "Gateway → Brake");
    for f in &flows {
        assert_eq!(f.threats, 1);
        assert!(!f.no_threat);
    }
}

#[test]
fn rows_and_detail_carry_the_boundary() {
    let fx = fixture();
    let rows = data_flow_rows(&fx.ws);
    let by = |n: &str| rows.iter().find(|r| r.name == n).expect("row");
    assert_eq!(
        by("Brake request").boundary.as_deref(),
        Some("ADAS → Gateway")
    );
    assert_eq!(
        by("Brake command").boundary.as_deref(),
        Some("Gateway → Brake")
    );
    assert_eq!(by("Local").boundary, None);
    assert_eq!(by("Diag").boundary, None);
    let row = by("Brake request");
    let d = data_flow_detail(&fx.ws, fx.request).expect("detail");
    assert_eq!(d.boundary, row.boundary);
    assert_eq!(d.threats, row.threats);
    assert_eq!(d.no_threat, row.no_threat);
    assert_eq!(d.threats, 1);
}

#[test]
fn boundary_flow_without_threats_is_flagged() {
    let mut ws = workspace();
    let a = add_system(&mut ws, "A");
    let b = add_system(&mut ws, "B");
    let x = add_component(&mut ws, "X", None, Some(a));
    let y = add_component(&mut ws, "Y", None, Some(b));
    let id = add_flow(&mut ws, "Link", x, y);
    let flows = boundary_flows(ws.project());
    assert_eq!(flows.len(), 1);
    assert_eq!(flows[0].threats, 0);
    assert!(flows[0].no_threat);
    let rows = data_flow_rows(&ws);
    assert!(rows[0].no_threat);
    assert!(data_flow_detail(&ws, id).expect("detail").no_threat);
}

#[test]
fn system_rows_count_boundary_flows() {
    let fx = fixture();
    let rows = system_rows(&fx.ws);
    let by = |n: &str| rows.iter().find(|r| r.name == n).expect("row");
    assert_eq!(by("Gateway").boundary_flows, 2);
    assert_eq!(by("Gateway").boundary_text, "2 systems 2");
    assert_eq!(by("Brake").boundary_flows, 1);
    assert_eq!(by("Brake").boundary_text, "Gateway 1");
    assert_eq!(by("ADAS").boundary_flows, 1);
    assert_eq!(by("ADAS").boundary_text, "Gateway 1");
}

#[test]
fn no_systems_means_no_boundary() {
    let mut ws = workspace();
    let a = add_component(&mut ws, "A", None, None);
    let b = add_component(&mut ws, "B", None, None);
    add_flow(&mut ws, "Link", a, b);
    assert!(boundary_flows(ws.project()).is_empty());
    for r in data_flow_rows(&ws) {
        assert_eq!(r.boundary, None);
    }
}

// Steps that leave their tree's systems (120c).

struct Via {
    ws: Workspace,
    brake: ElementId,
    tree: TreeId,
    threat: ThreatId,
    gw: ComponentId,
    obd: InterfaceId,
    brake_ecu: ComponentId,
    step: NodeId,
}

/// A tree with one step "Open port", linked to a threat on a Brake component
/// when `linked`; GW is in Gateway and has an interface OBD.
fn via_fixture(linked: bool, systems: bool) -> Via {
    let mut ws = workspace();
    let (brake, gateway) = if systems {
        (add_system(&mut ws, "Brake"), add_system(&mut ws, "Gateway"))
    } else {
        (ElementId::new_v7(), ElementId::new_v7())
    };
    let sys = |id: ElementId| systems.then_some(id);
    let brake_ecu = add_component(&mut ws, "ESC ECU", None, sys(brake));
    let gw = add_component(&mut ws, "GW", None, sys(gateway));
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
    let asset = AssetId::new_v7();
    run(
        &mut ws,
        &Edit::AddAssetBound {
            id: asset,
            name: "Data".into(),
            properties: (0..6).collect(),
            component: brake_ecu,
        },
    );
    let threat = ThreatId::new_v7();
    run(
        &mut ws,
        &Edit::AddThreat {
            id: threat,
            name: "Attack".into(),
            asset,
            category: 0,
        },
    );
    let tree = TreeId::new_v7();
    run(
        &mut ws,
        &Edit::NewTree {
            id: tree,
            root: "Goal".into(),
            threat: linked.then_some(threat),
        },
    );
    run(
        &mut ws,
        &Edit::ReplaceTree {
            tree,
            outline: "OR Goal\n  - Open port\n".into(),
        },
    );
    let step = {
        let p = ws.project();
        let t = p
            .attack_tree_idx(&tree)
            .and_then(|i| p.attack_tree(i))
            .expect("tree");
        t.iter()
            .find(|(_, _, n)| n.label() == "Open port")
            .map(|(_, i, _)| i)
            .expect("step")
    };
    Via {
        ws,
        brake,
        tree,
        threat,
        gw,
        obd,
        brake_ecu,
        step,
    }
}

fn set_via(fx: &mut Via, via: ItemRef) {
    run(
        &mut fx.ws,
        &Edit::SetStepVia {
            tree: fx.tree,
            node: fx.step,
            via: Some(via),
        },
    );
}

fn chip(fx: &Via) -> Option<String> {
    let scene = tree_scene(&fx.ws, fx.tree, false).expect("scene");
    let v = (0..u32::try_from(scene.len()).expect("len"))
        .find(|&v| scene.label(v) == "Open port")
        .expect("vertex");
    scene.style(v).and_then(|s| s.via.clone())
}

fn through_system(fx: &Via) -> Option<String> {
    let ui = UiState {
        selected_tree: Some(fx.tree),
        selected_node: Some(fx.step),
        ..Default::default()
    };
    inspector(&fx.ws, &ui).expect("inspector").through_system
}

#[test]
fn step_via_another_system_names_it() {
    let mut fx = via_fixture(true, true);
    let via = ItemRef::Component(fx.gw);
    set_via(&mut fx, via);
    assert_eq!(chip(&fx).as_deref(), Some("GW · Gateway"));
    assert_eq!(through_system(&fx).as_deref(), Some("Gateway"));
    let via = ItemRef::Interface(fx.obd);
    set_via(&mut fx, via);
    assert_eq!(chip(&fx).as_deref(), Some("GW / OBD · Gateway"));
    assert_eq!(through_system(&fx).as_deref(), Some("Gateway"));
}

#[test]
fn step_via_a_component_of_the_tree_system_has_no_suffix() {
    let mut fx = via_fixture(true, true);
    let via = ItemRef::Component(fx.brake_ecu);
    set_via(&mut fx, via);
    assert_eq!(chip(&fx).as_deref(), Some("ESC ECU"));
    assert_eq!(through_system(&fx), None);
}

#[test]
fn step_in_a_tree_without_a_system_has_no_suffix() {
    let mut fx = via_fixture(false, true);
    let via = ItemRef::Component(fx.gw);
    set_via(&mut fx, via);
    assert_eq!(chip(&fx).as_deref(), Some("GW"));
    assert_eq!(through_system(&fx), None);
}

#[test]
fn step_in_a_project_without_systems_has_no_suffix() {
    let mut fx = via_fixture(true, false);
    let via = ItemRef::Component(fx.gw);
    set_via(&mut fx, via);
    assert_eq!(chip(&fx).as_deref(), Some("GW"));
    assert_eq!(through_system(&fx), None);
}

#[test]
fn other_systems_setting_caps_the_step_in_every_view() {
    let mut fx = via_fixture(true, true);
    let via = ItemRef::Component(fx.gw);
    set_via(&mut fx, via);
    run(
        &mut fx.ws,
        &Edit::SetStepFeasibility {
            tree: fx.tree,
            node: fx.step,
            level: Some(3),
        },
    );
    run(
        &mut fx.ws,
        &Edit::SetStepAttackType {
            tree: fx.tree,
            node: fx.step,
            attack_type: Some(0),
        },
    );
    run(
        &mut fx.ws,
        &Edit::AddCustomSetting {
            id: ControlId::new_v7(),
            name: "Port disabled".into(),
            target: ItemRef::Component(fx.gw),
            rules: vec![(0, 0)],
        },
    );
    let scene = tree_scene(&fx.ws, fx.tree, true).expect("scene");
    let v = (0..u32::try_from(scene.len()).expect("len"))
        .find(|&v| scene.label(v) == "Open port")
        .expect("vertex");
    assert_eq!(
        scene.style(v).and_then(|s| s.capped.as_deref()),
        Some("Port disabled on GW")
    );
    let rows = risk_rows(&fx.ws);
    let all = rows
        .iter()
        .find(|r| r.id == fx.threat)
        .expect("row")
        .clone();
    let mut d = PageData {
        risks: rows,
        ..Default::default()
    };
    filter_page(
        fx.ws.project(),
        &SystemView::only(fx.brake),
        Page::Risks,
        &mut d,
    );
    let kept = d.risks.iter().find(|r| r.id == fx.threat).expect("kept");
    assert!(kept == &all);
}

// The systems table of the overview (120d).

/// A threat on a new asset bound to `component`, linked to `damage` and
/// rated with feasibility `level` when given.
fn rated_threat(
    ws: &mut Workspace,
    component: ComponentId,
    damage: DamageId,
    level: Option<usize>,
) -> ThreatId {
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
    let threat = ThreatId::new_v7();
    run(
        ws,
        &Edit::AddThreat {
            id: threat,
            name: "Attack".into(),
            asset,
            category: 0,
        },
    );
    run(
        ws,
        &Edit::LinkDamage {
            threat,
            damage,
            linked: true,
        },
    );
    run(ws, &Edit::SetThreatFeasibility { threat, level });
    threat
}

struct Sys {
    ws: Workspace,
    brake: ElementId,
    adas: ElementId,
    gateway: ElementId,
}

/// Three systems, a child component, a component in no system, and
/// threats: Brake rated, ADAS unrated, Gateway two rated, no system rated.
fn sys_fixture() -> Sys {
    let mut ws = workspace();
    let m = Methodology::iso21434_r155().expect("builtin");
    let vocab = vocabulary(&m);
    let damage = DamageId::new_v7();
    run(
        &mut ws,
        &Edit::AddDamage {
            id: damage,
            name: "Crash".into(),
        },
    );
    let severe = vocab.impact_categories[0].levels.len() - 1;
    run(
        &mut ws,
        &Edit::SetImpact {
            damage,
            category: 0,
            level: Some(severe),
        },
    );
    let brake = add_system(&mut ws, "Brake");
    let adas = add_system(&mut ws, "ADAS");
    let gateway = add_system(&mut ws, "Gateway");
    let esc = add_component(&mut ws, "ESC ECU", None, Some(brake));
    add_component(&mut ws, "Valve", Some(esc), None);
    let adas_ecu = add_component(&mut ws, "ADAS ECU", None, Some(adas));
    let gw = add_component(&mut ws, "GW", None, Some(gateway));
    let tester = add_component(&mut ws, "Tester", None, None);
    add_flow(&mut ws, "Brake request", adas_ecu, gw);
    add_flow(&mut ws, "Brake command", gw, esc);
    add_flow(&mut ws, "Diag", tester, gw);
    let top = vocab.feasibility.len() - 1;
    rated_threat(&mut ws, esc, damage, Some(top));
    rated_threat(&mut ws, adas_ecu, damage, None);
    rated_threat(&mut ws, gw, damage, Some(0));
    rated_threat(&mut ws, gw, damage, Some(top));
    rated_threat(&mut ws, tester, damage, Some(0));
    Sys {
        ws,
        brake,
        adas,
        gateway,
    }
}

/// The label of the worst residual risk among the threats of `system`
/// (`None`: the threats in no system), by the model, uncached.
fn worst_label(ws: &Workspace, system: Option<ElementId>) -> Option<String> {
    let p = ws.project();
    let m = p.methodology();
    let uses = p.system_use();
    let mut best: Option<usize> = None;
    for (idx, id, _) in p.threat_scenarios() {
        if uses.ids(uses.threat(idx)).next() != system {
            continue;
        }
        let level = p.evaluate_residual_risk(id).ok().and_then(|r| r.max_risk());
        let pos = level.and_then(|l| m.risk_levels().position(|(x, _, _)| x == l));
        best = best.max(pos);
    }
    let (_, _, label) = m.risk_levels().nth(best?)?;
    Some(label.to_owned())
}

#[test]
fn rows_add_up_to_the_total() {
    let fx = sys_fixture();
    let p = fx.ws.project();
    let o = systems_overview(&fx.ws, &SystemView::all()).expect("systems");
    assert_eq!(o.total.name, "All");
    let names: Vec<&str> = o.rows.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, ["Brake", "ADAS", "Gateway", "No system"]);
    assert_eq!(o.rows.iter().map(|r| r.components).sum::<usize>(), 5);
    assert_eq!(o.total.components, p.component_count());
    assert_eq!(o.total.assets, p.asset_count());
    assert_eq!(o.total.threats, p.threat_scenario_count());
    assert_eq!(
        o.rows.iter().map(|r| r.assets).sum::<usize>(),
        o.total.assets
    );
    assert_eq!(
        o.rows.iter().map(|r| r.threats).sum::<usize>(),
        o.total.threats
    );
    let by = |n: &str| o.rows.iter().find(|r| r.name == n).expect("row");
    assert_eq!(by("Brake").components, 2);
    assert_eq!(by("Gateway").threats, 2);
    assert_eq!(by("No system").components, 1);
    assert_eq!(by("No system").threats, 1);
}

#[test]
fn boundary_column_counts_each_flow_at_both_ends() {
    let fx = sys_fixture();
    let o = systems_overview(&fx.ws, &SystemView::all()).expect("systems");
    assert_eq!(o.total.boundary_flows, 2);
    assert_eq!(o.flows.len(), 2);
    assert_eq!(o.rows.iter().map(|r| r.boundary_flows).sum::<usize>(), 4);
    let by = |n: &str| o.rows.iter().find(|r| r.name == n).expect("row");
    assert_eq!(by("Gateway").boundary_text, "2 systems 2");
    assert_eq!(by("Brake").boundary_text, "Gateway 1");
    assert_eq!(by("No system").boundary_text, "");
    assert_eq!(o.total.boundary_text, "");
}

#[test]
fn a_row_shows_its_worst_rated_threat() {
    let fx = sys_fixture();
    let o = systems_overview(&fx.ws, &SystemView::all()).expect("systems");
    for r in &o.rows {
        let got = r.highest_residual.as_ref().map(|v| v.label.clone());
        assert_eq!(got, worst_label(&fx.ws, r.system), "{}", r.name);
    }
    let by = |n: &str| o.rows.iter().find(|r| r.name == n).expect("row");
    assert!(by("Brake").highest_residual.is_some());
    assert!(by("Gateway").highest_residual.is_some());
    assert_eq!(by("ADAS").highest_residual, None);
    assert!(o.total.highest_residual.is_some());
}

#[test]
fn one_system_view_keeps_no_system_and_its_flows() {
    let fx = sys_fixture();
    let o = systems_overview(&fx.ws, &SystemView::only(fx.brake)).expect("systems");
    assert_eq!(o.total.name, "This view");
    let names: Vec<&str> = o.rows.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, ["Brake", "No system"]);
    assert_eq!(o.flows.len(), 1);
    assert_eq!(o.flows[0].name, "Brake command");
    assert_eq!(o.total.boundary_flows, 1);
    assert_eq!(o.total.components, 3);
    assert_eq!(o.total.threats, 2);
    let _ = (fx.adas, fx.gateway);
}

#[test]
fn no_systems_means_no_systems_table() {
    let mut ws = workspace();
    add_component(&mut ws, "A", None, None);
    assert_eq!(systems_overview(&ws, &SystemView::all()), None);
    assert_eq!(overview(&ws).systems, None);
    let fx = sys_fixture();
    assert!(overview(&fx.ws).systems.is_some());
}
