//! Boundary flows on the item definition rows (120b).

use std::sync::Arc;

use aegis_client::combined::{boundary_flows, systems_overview};
use aegis_client::commands::Edit;
use aegis_client::pages::{
    asset_rows, claim_rows, component_rows, control_rows, damage_rows, data_flow_detail,
    data_flow_rows, goal_rows, interface_rows, overview, overview_in, problems, risk_rows,
    tree_rows, trust_boundary_rows, vocabulary,
};
use aegis_client::state::PageData;
use aegis_client::systems::{SystemView, filter_page, filter_threats, system_rows};
use aegis_client::view::{Page, UiState, inspector, threat_rows, tree_scene};
use aegis_client::workspace::{Origin, Workspace};
use aegis_model::{
    AssetId, ClaimId, ComponentId, ControlId, DamageId, DataFlowId, ElementId, GoalId, InterfaceId,
    ItemRef, Methodology, NodeId, ThreatId, TreeId, TrustBoundaryId,
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

// ---------------------------------------------------------------------------
// The overview follows the view (120e).
// ---------------------------------------------------------------------------

struct Ov {
    ws: Workspace,
    brake: ElementId,
    adas: ElementId,
    shared_damage: DamageId,
    shared_control: ControlId,
}

/// Two systems and a loose group, one element of every kind in each, one
/// damage scenario linked from both systems, one control applied to both.
fn ov_fixture() -> Ov {
    let mut ws = workspace();
    let brake = add_system(&mut ws, "Brake");
    let adas = add_system(&mut ws, "ADAS");
    let shared_damage = DamageId::new_v7();
    run(
        &mut ws,
        &Edit::AddDamage {
            id: shared_damage,
            name: "Harm".into(),
        },
    );
    let top = vocabulary(ws.project().methodology()).impact_categories[0]
        .levels
        .len()
        - 1;
    run(
        &mut ws,
        &Edit::SetImpact {
            damage: shared_damage,
            category: 0,
            level: Some(top),
        },
    );
    run(
        &mut ws,
        &Edit::AddDamage {
            id: DamageId::new_v7(),
            name: "Orphan harm".into(),
        },
    );
    let mut firsts = Vec::new();
    for (name, system) in [
        ("Brake", Some(brake)),
        ("ADAS", Some(adas)),
        ("Loose", None),
    ] {
        let a = add_component(&mut ws, &format!("{name} a"), None, system);
        let b = add_component(&mut ws, &format!("{name} b"), None, system);
        firsts.push(a);
        add_flow(&mut ws, &format!("{name} flow"), a, b);
        run(
            &mut ws,
            &Edit::AddInterface {
                id: InterfaceId::new_v7(),
                name: format!("{name} port"),
                component: a,
                kind: 0,
            },
        );
        let boundary = TrustBoundaryId::new_v7();
        run(
            &mut ws,
            &Edit::AddTrustBoundary {
                id: boundary,
                name: format!("{name} zone"),
            },
        );
        run(
            &mut ws,
            &Edit::SetTrustBoundaryMember {
                boundary,
                component: a,
                member: true,
            },
        );
        let asset = AssetId::new_v7();
        run(
            &mut ws,
            &Edit::AddAssetBound {
                id: asset,
                name: format!("{name} data"),
                properties: (0..6).collect(),
                component: a,
            },
        );
        let threat = ThreatId::new_v7();
        run(
            &mut ws,
            &Edit::AddThreat {
                id: threat,
                name: format!("{name} attack"),
                asset,
                category: 0,
            },
        );
        let (control, goal, claim) = (ControlId::new_v7(), GoalId::new_v7(), ClaimId::new_v7());
        run(
            &mut ws,
            &Edit::AddControl {
                id: control,
                name: format!("{name} control"),
            },
        );
        run(
            &mut ws,
            &Edit::ApplyControl {
                control,
                target: ItemRef::Component(a),
            },
        );
        run(
            &mut ws,
            &Edit::AddGoal {
                id: goal,
                name: format!("{name} goal"),
            },
        );
        run(
            &mut ws,
            &Edit::SetThreatGoal {
                threat,
                goal: Some(goal),
            },
        );
        run(
            &mut ws,
            &Edit::AddClaim {
                id: claim,
                name: format!("{name} claim"),
            },
        );
        run(
            &mut ws,
            &Edit::SetThreatClaim {
                threat,
                claim: Some(claim),
            },
        );
        run(
            &mut ws,
            &Edit::NewTree {
                id: TreeId::new_v7(),
                root: "Goal".into(),
                threat: Some(threat),
            },
        );
        if system.is_some() {
            run(
                &mut ws,
                &Edit::LinkDamage {
                    threat,
                    damage: shared_damage,
                    linked: true,
                },
            );
        }
        if system == Some(brake) {
            // A threat without a tree, so its feasibility is rated.
            let rated = ThreatId::new_v7();
            run(
                &mut ws,
                &Edit::AddThreat {
                    id: rated,
                    name: "Rated attack".into(),
                    asset,
                    category: 0,
                },
            );
            run(
                &mut ws,
                &Edit::LinkDamage {
                    threat: rated,
                    damage: shared_damage,
                    linked: true,
                },
            );
            run(
                &mut ws,
                &Edit::SetThreatFeasibility {
                    threat: rated,
                    level: Some(0),
                },
            );
        }
    }
    let shared_control = ControlId::new_v7();
    run(
        &mut ws,
        &Edit::AddControl {
            id: shared_control,
            name: "Shared control".into(),
        },
    );
    for target in &firsts[..2] {
        run(
            &mut ws,
            &Edit::ApplyControl {
                control: shared_control,
                target: ItemRef::Component(*target),
            },
        );
    }
    Ov {
        ws,
        brake,
        adas,
        shared_damage,
        shared_control,
    }
}

/// The rows each kind's page shows for `view`, in the overview's order:
/// asset, damage, threat, tree, control, goal, claim, component,
/// interface, data flow, trust boundary.
fn page_counts(ws: &Workspace, view: &SystemView) -> [usize; 11] {
    let p = ws.project();
    let mut d = PageData {
        assets: asset_rows(ws),
        damages: damage_rows(ws),
        trees: tree_rows(ws),
        controls: control_rows(ws),
        goals: goal_rows(ws),
        claims: claim_rows(ws),
        components: component_rows(ws),
        interfaces: interface_rows(ws),
        data_flows: data_flow_rows(ws),
        trust_boundaries: trust_boundary_rows(ws),
        ..Default::default()
    };
    for page in [
        Page::Item,
        Page::Assets,
        Page::Damage,
        Page::Trees,
        Page::Controls,
        Page::Goals,
    ] {
        filter_page(p, view, page, &mut d);
    }
    let mut threats = threat_rows(ws);
    filter_threats(p, view, &mut threats);
    [
        d.assets.len(),
        d.damages.len(),
        threats.len(),
        d.trees.len(),
        d.controls.len(),
        d.goals.len(),
        d.claims.len(),
        d.components.len(),
        d.interfaces.len(),
        d.data_flows.len(),
        d.trust_boundaries.len(),
    ]
}

fn counts_of(o: &aegis_client::pages::Overview) -> Vec<usize> {
    o.counts.iter().map(|c| c.count).collect()
}

fn level_total(dist: &[aegis_client::pages::Count], rated: bool) -> usize {
    dist.iter()
        .filter(|c| (c.label != "Not rated") == rated)
        .map(|c| c.count)
        .sum()
}

#[test]
fn every_part_of_the_overview_follows_the_view() {
    let fx = ov_fixture();
    let ws = &fx.ws;
    let view = SystemView::only(fx.brake);
    let o = overview_in(ws, &view);
    let all = overview_in(ws, &SystemView::all());
    // The eleven counts, each equal to what that kind's page shows.
    let want = page_counts(ws, &view);
    assert_eq!(counts_of(&o), want.to_vec());
    let names = ws.project().methodology().element_names();
    let labels: Vec<&str> = o.counts.iter().map(|c| c.label.as_str()).collect();
    let names: Vec<&str> = vec![
        &names.asset.plural,
        &names.damage_scenario.plural,
        &names.threat_scenario.plural,
        &names.attack_tree.plural,
        &names.security_control.plural,
        &names.cybersecurity_goal.plural,
        &names.cybersecurity_claim.plural,
        &names.component.plural,
        &names.interface.plural,
        &names.data_flow.plural,
        &names.trust_boundary.plural,
    ];
    assert_eq!(labels, names);
    assert_ne!(counts_of(&o), counts_of(&all));
    // The risk distributions cover the threats of the view only.
    for (dist, whole) in [(&o.inherent, &all.inherent), (&o.residual, &all.residual)] {
        assert_eq!(level_total(dist, false) + level_total(dist, true), want[2]);
        assert_eq!(level_total(dist, true), 1, "{dist:?}");
        assert_eq!(level_total(whole, false), 3);
    }
    let adas = overview_in(ws, &SystemView::only(fx.adas));
    assert_eq!(level_total(&adas.inherent, true), 0);
    assert_eq!(level_total(&adas.inherent, false), 2);
    // Problems: the rows whose target shows.
    let mut d = PageData {
        problems: problems(ws),
        ..Default::default()
    };
    filter_page(ws.project(), &view, Page::Problems, &mut d);
    let errors = d.problems.iter().filter(|r| r.error).count();
    assert_eq!(o.errors, errors);
    assert_eq!(o.warnings, d.problems.len() - errors);
    assert!(o.errors + o.warnings < all.errors + all.warnings);
    // The five highest risks, from the unfiltered rows.
    let mut d = PageData {
        risks: risk_rows(ws),
        ..Default::default()
    };
    filter_page(ws.project(), &view, Page::Risks, &mut d);
    d.risks.truncate(5);
    assert_eq!(o.top, d.risks);
    assert_eq!(o.top.len(), 3);
    // The systems table.
    assert_eq!(o.systems, systems_overview(ws, &view));
    let table = o.systems.expect("systems");
    assert_eq!(table.total.threats, want[2]);
    assert_eq!(table.total.components, want[7]);
    assert_eq!(table.total.assets, want[0]);
}

#[test]
fn shared_elements_show_for_each_system_and_count_once() {
    let fx = ov_fixture();
    let ws = &fx.ws;
    let rows = |view: &SystemView| {
        let mut d = PageData {
            damages: damage_rows(ws),
            controls: control_rows(ws),
            ..Default::default()
        };
        filter_page(ws.project(), view, Page::Damage, &mut d);
        filter_page(ws.project(), view, Page::Controls, &mut d);
        d
    };
    for system in [fx.brake, fx.adas] {
        let d = rows(&SystemView::only(system));
        assert!(d.damages.iter().any(|r| r.id == fx.shared_damage));
        assert!(d.controls.iter().any(|r| r.id == fx.shared_control));
    }
    let both = overview_in(ws, &SystemView::of([fx.brake, fx.adas]));
    let all = overview_in(ws, &SystemView::all());
    // Damage scenarios: shared + orphan; controls: two own + shared + the
    // loose one (in no system, so it shows everywhere); threats: 2 + 1 + loose.
    assert_eq!(both.counts[1].count, 2);
    assert_eq!(both.counts[4].count, 4);
    assert_eq!(all.counts[1].count, 2);
    assert_eq!(all.counts[4].count, 4);
    assert_eq!(both.counts[2].count, 4);
}

#[test]
fn all_view_is_the_whole_project() {
    let fx = ov_fixture();
    assert_eq!(overview_in(&fx.ws, &SystemView::all()), overview(&fx.ws));
    let mut ws = workspace();
    let c = add_component(&mut ws, "Lone", None, None);
    add_threat(&mut ws, c);
    assert_eq!(overview_in(&ws, &SystemView::all()), overview(&ws));
    assert_eq!(
        overview_in(&ws, &SystemView::only(ElementId::new_v7())),
        overview(&ws)
    );
}

#[test]
fn a_removed_system_behaves_as_all() {
    let mut fx = ov_fixture();
    let view = SystemView::only(fx.brake);
    let before = overview_in(&fx.ws, &view);
    run(&mut fx.ws, &Edit::RemoveSystem { system: fx.brake });
    let after = overview_in(&fx.ws, &view);
    assert_eq!(after, overview(&fx.ws));
    assert_ne!(counts_of(&before), counts_of(&after));
}

// ---- the Versions comparison within the view (120g) ----

mod versions {
    use aegis_client::pages::{Comparison, comparison, comparison_in};

    use super::*;

    /// What one `add_set` made, for later edits.
    struct Made {
        first: ComponentId,
        threat: ThreatId,
    }

    /// One element of every kind, named after `name`; two components so the
    /// data flow and the trust boundary stay inside `system`.
    fn add_set(ws: &mut Workspace, name: &str, system: Option<ElementId>) -> Made {
        let first = add_component(ws, &format!("{name} part"), None, system);
        let second = add_component(ws, &format!("{name} other"), None, system);
        run(
            ws,
            &Edit::AddInterface {
                id: InterfaceId::new_v7(),
                name: format!("{name} port"),
                component: first,
                kind: 0,
            },
        );
        add_flow(ws, &format!("{name} flow"), first, second);
        let boundary = TrustBoundaryId::new_v7();
        run(
            ws,
            &Edit::AddTrustBoundary {
                id: boundary,
                name: format!("{name} zone"),
            },
        );
        for component in [first, second] {
            run(
                ws,
                &Edit::SetTrustBoundaryMember {
                    boundary,
                    component,
                    member: true,
                },
            );
        }
        let asset = AssetId::new_v7();
        run(
            ws,
            &Edit::AddAssetBound {
                id: asset,
                name: format!("{name} data"),
                properties: (0..6).collect(),
                component: first,
            },
        );
        let threat = ThreatId::new_v7();
        run(
            ws,
            &Edit::AddThreat {
                id: threat,
                name: format!("{name} attack"),
                asset,
                category: 0,
            },
        );
        let damage = DamageId::new_v7();
        run(
            ws,
            &Edit::AddDamage {
                id: damage,
                name: format!("{name} harm"),
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
        run(
            ws,
            &Edit::NewTree {
                id: TreeId::new_v7(),
                root: format!("{name} tree"),
                threat: Some(threat),
            },
        );
        let (control, goal, claim) = (ControlId::new_v7(), GoalId::new_v7(), ClaimId::new_v7());
        run(
            ws,
            &Edit::AddControl {
                id: control,
                name: format!("{name} control"),
            },
        );
        run(
            ws,
            &Edit::ApplyControl {
                control,
                target: ItemRef::Component(first),
            },
        );
        run(
            ws,
            &Edit::AddGoal {
                id: goal,
                name: format!("{name} goal"),
            },
        );
        run(
            ws,
            &Edit::SetThreatGoal {
                threat,
                goal: Some(goal),
            },
        );
        run(
            ws,
            &Edit::AddClaim {
                id: claim,
                name: format!("{name} claim"),
            },
        );
        run(
            ws,
            &Edit::SetThreatClaim {
                threat,
                claim: Some(claim),
            },
        );
        Made { first, threat }
    }

    struct Vx {
        ws: Workspace,
        base: aegis_model::Project,
        brake: ElementId,
        adas: ElementId,
        b: Made,
        a: Made,
    }

    /// Two systems; `base` is taken before the elements are added.
    fn fixture() -> Vx {
        let mut ws = workspace();
        let brake = add_system(&mut ws, "Brake");
        let adas = add_system(&mut ws, "ADAS");
        let base = ws.project().clone();
        let b = add_set(&mut ws, "Brake", Some(brake));
        let a = add_set(&mut ws, "ADAS", Some(adas));
        add_set(&mut ws, "Loose", None);
        Vx {
            ws,
            base,
            brake,
            adas,
            b,
            a,
        }
    }

    fn cmp_in(fx: &Vx, view: &SystemView) -> Comparison {
        comparison_in(&fx.base, "v1", fx.ws.project(), view).expect("compares")
    }

    fn has(c: &Comparison, kind: &str, what: &str, name: &str) -> bool {
        c.lines
            .iter()
            .any(|l| l.kind == kind && l.text.starts_with(what) && l.text.contains(name))
    }

    #[test]
    fn every_kind_follows_the_view() {
        let fx = fixture();
        let full = comparison(&fx.base, "v1", fx.ws.project()).expect("compares");
        let view = cmp_in(&fx, &SystemView::only(fx.brake));
        for what in [
            "Component",
            "Interface",
            "Data flow",
            "Trust boundary",
            "Asset",
            "Damage scenario",
            "Threat scenario",
            "Attack tree",
            "Security control",
            "Goal",
            "Claim",
        ] {
            assert!(has(&view, "Added", what, "Brake"), "{what} Brake");
            assert!(has(&view, "Added", what, "Loose"), "{what} Loose");
            assert!(!has(&view, "Added", what, "ADAS"), "{what} ADAS");
            assert!(has(&full, "Added", what, "ADAS"), "{what} ADAS full");
        }
        assert_eq!(view.hidden, full.lines.len() - view.lines.len());
        assert!(view.hidden > 0);
    }

    #[test]
    fn removed_lines_follow_the_base_systems() {
        let mut fx = fixture();
        let base = fx.ws.project().clone();
        for t in [fx.b.threat, fx.a.threat] {
            run(&mut fx.ws, &Edit::RemoveThreat { threat: t });
        }
        let c = comparison_in(&base, "v1", fx.ws.project(), &SystemView::only(fx.brake))
            .expect("compares");
        assert!(has(&c, "Removed", "Threat scenario", "Brake attack"));
        assert!(!has(&c, "Removed", "Threat scenario", "ADAS attack"));
    }

    #[test]
    fn a_moved_component_shows_in_both_systems() {
        let mut fx = fixture();
        let gateway = add_system(&mut fx.ws, "Gateway");
        let base = fx.ws.project().clone();
        run(
            &mut fx.ws,
            &Edit::MoveToSystem {
                components: vec![fx.a.first],
                system: Some(fx.brake),
            },
        );
        let moved = |view: &SystemView| {
            let c = comparison_in(&base, "v1", fx.ws.project(), view).expect("compares");
            c.lines.iter().any(|l| l.text.contains("moved from"))
        };
        assert!(moved(&SystemView::only(fx.brake)));
        assert!(moved(&SystemView::only(fx.adas)));
        assert!(!moved(&SystemView::only(gateway)));
    }

    #[test]
    fn a_renamed_system_shows_in_its_own_view() {
        let mut fx = fixture();
        let base = fx.ws.project().clone();
        run(
            &mut fx.ws,
            &Edit::RenameSystem {
                system: fx.adas,
                name: "Driver assist".into(),
            },
        );
        let sys_line = |view: &SystemView| {
            comparison_in(&base, "v1", fx.ws.project(), view)
                .expect("compares")
                .lines
                .iter()
                .any(|l| l.kind == "Changed" && l.text.starts_with("System"))
        };
        assert!(sys_line(&SystemView::only(fx.adas)));
        assert!(!sys_line(&SystemView::only(fx.brake)));
    }

    /// A damage scenario and a threat on a component, without a tree (a
    /// tree would give the threat its feasibility).
    fn plain_threat(ws: &mut Workspace, name: &str, system: ElementId) -> (DamageId, ThreatId) {
        let comp = add_component(ws, name, None, Some(system));
        let asset = AssetId::new_v7();
        run(
            ws,
            &Edit::AddAssetBound {
                id: asset,
                name: format!("{name} data"),
                properties: (0..6).collect(),
                component: comp,
            },
        );
        let (threat, damage) = (ThreatId::new_v7(), DamageId::new_v7());
        run(
            ws,
            &Edit::AddThreat {
                id: threat,
                name: format!("{name} attack"),
                asset,
                category: 0,
            },
        );
        run(
            ws,
            &Edit::AddDamage {
                id: damage,
                name: format!("{name} harm"),
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
        (damage, threat)
    }

    #[test]
    fn risks_follow_their_threat() {
        let mut ws = workspace();
        let brake = add_system(&mut ws, "Brake");
        let adas = add_system(&mut ws, "ADAS");
        let rated = [
            plain_threat(&mut ws, "Brake", brake),
            plain_threat(&mut ws, "ADAS", adas),
        ];
        let base = ws.project().clone();
        for (d, t) in rated {
            run(
                &mut ws,
                &Edit::SetImpact {
                    damage: d,
                    category: 0,
                    level: Some(2),
                },
            );
            run(
                &mut ws,
                &Edit::SetThreatFeasibility {
                    threat: t,
                    level: Some(1),
                },
            );
        }
        let all = comparison(&base, "v1", ws.project()).expect("compares");
        let risks = |c: &Comparison| c.lines.iter().filter(|l| l.kind == "Risk").count();
        assert_eq!(risks(&all), 2);
        let c =
            comparison_in(&base, "v1", ws.project(), &SystemView::only(brake)).expect("compares");
        assert_eq!(risks(&c), 1);
        assert!(has(&c, "Risk", "", "Brake attack"));
    }

    #[test]
    fn the_summary_counts_the_shown_lines() {
        let fx = fixture();
        let c = cmp_in(&fx, &SystemView::only(fx.brake));
        let n = |k: &str| c.lines.iter().filter(|l| l.kind == k).count();
        assert_eq!(c.scope.as_deref(), Some("Brake"));
        let want = format!(
            "Since this baseline: {} added, {} removed, {} changed, {} risk(s) moved. {} more outside this view.",
            n("Added"),
            n("Removed"),
            n("Changed"),
            n("Risk"),
            c.hidden
        );
        assert_eq!(c.summary, want);
        assert!(
            c.summary
                .ends_with(&format!(" {} more outside this view.", c.hidden))
        );
    }

    #[test]
    fn nothing_shown_still_counts_what_is_hidden() {
        let mut ws = workspace();
        let brake = add_system(&mut ws, "Brake");
        let adas = add_system(&mut ws, "ADAS");
        let base = ws.project().clone();
        add_set(&mut ws, "ADAS", Some(adas));
        let full = comparison(&base, "v1", ws.project()).expect("compares");
        let c =
            comparison_in(&base, "v1", ws.project(), &SystemView::only(brake)).expect("compares");
        assert!(c.lines.is_empty(), "{:?}", c.lines);
        assert_eq!(c.hidden, full.lines.len());
        assert_eq!(
            c.summary,
            format!(
                "Nothing changed in this view since this baseline. {} more outside this view.",
                c.hidden
            )
        );
    }

    #[test]
    fn all_systems_is_the_plain_comparison() {
        let fx = fixture();
        let plain = comparison(&fx.base, "v1", fx.ws.project()).expect("compares");
        let all = cmp_in(&fx, &SystemView::all());
        assert_eq!(all, plain);
        assert_eq!(all.scope, None);
        assert_eq!(all.hidden, 0);
    }
}
