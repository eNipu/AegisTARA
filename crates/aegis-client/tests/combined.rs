//! Boundary flows on the item definition rows (120b).

use std::sync::Arc;

use aegis_client::combined::boundary_flows;
use aegis_client::commands::Edit;
use aegis_client::pages::{data_flow_detail, data_flow_rows};
use aegis_client::systems::system_rows;
use aegis_client::workspace::{Origin, Workspace};
use aegis_model::{AssetId, ComponentId, DataFlowId, ElementId, Methodology, ThreatId};

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
