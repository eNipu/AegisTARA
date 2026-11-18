//! A component moved by the explorer's drop rule is confirmed locally, and
//! undo and redo give back the exact project bytes.

use aegis_client::explorer::{ExplorerKey, Folder, Owner, drop_edit};
use aegis_client::systems::system_rows;
use aegis_client::view::Selection;
use aegis_client::workspace::Workspace;
use aegis_model::ComponentId;
use aegis_workbench::local;
use aegis_workbench::sample::workbench_sample;

fn bytes(ws: &Workspace) -> Vec<u8> {
    let mut out = Vec::new();
    ws.project().save(&mut out).expect("saves");
    out
}

fn component(ws: &Workspace, name: &str) -> ComponentId {
    ws.project()
        .components()
        .find(|(_, _, c)| c.name() == name)
        .map(|(_, id, _)| id)
        .expect("component")
}

fn system_of(ws: &Workspace, id: ComponentId) -> Option<aegis_model::ElementId> {
    let p = ws.project();
    p.component_idx(&id)
        .and_then(|i| p.component(i))
        .and_then(|c| c.system())
}

#[test]
fn a_moved_component_undoes_and_redoes_to_the_same_bytes() {
    let mut ws = workbench_sample();
    let gateway = system_rows(&ws)
        .into_iter()
        .find(|r| r.name == "Gateway")
        .expect("Gateway")
        .id;
    let tcu = component(&ws, "Telematics unit");
    assert_eq!(system_of(&ws, tcu), None);
    let before = bytes(&ws);

    let dragged = ExplorerKey::Element(
        Owner::Unassigned,
        Folder::Components,
        Selection::Component(tcu),
    );
    let edit = drop_edit(ws.project(), &dragged, &ExplorerKey::System(gateway)).expect("an edit");
    local::apply_edit(&mut ws, &edit, &local::actor(), 1_000).expect("applies");
    assert_eq!(system_of(&ws, tcu), Some(gateway));
    assert_eq!(ws.pending_batches(), 0);
    let after = bytes(&ws);
    assert_ne!(before, after);

    local::undo(&mut ws).expect("undoes");
    assert_eq!(ws.pending_batches(), 0);
    assert_eq!(system_of(&ws, tcu), None);
    assert_eq!(bytes(&ws), before);

    local::redo(&mut ws).expect("redoes");
    assert_eq!(ws.pending_batches(), 0);
    assert_eq!(bytes(&ws), after);
}

#[test]
fn undo_with_nothing_to_undo_is_an_error_text() {
    let mut ws = workbench_sample();
    let err = local::undo(&mut ws).expect_err("nothing to undo");
    assert!(!err.is_empty());
    assert!(local::redo(&mut ws).is_err());
    assert!(!local::actor().trim().is_empty());
}
