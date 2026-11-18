//! The workbench sample: two systems, three components, nothing to undo.

use aegis_client::explorer::{ExplorerIndex, ExplorerKey, ExplorerState, Folder, Owner};
use aegis_client::i18n::{Catalog, Language};
use aegis_client::systems::system_rows;
use aegis_workbench::sample::{SAMPLE_NAME, workbench_sample};

#[test]
fn the_sample_has_two_systems_three_components_and_no_undo() {
    let ws = workbench_sample();
    assert_eq!(system_rows(&ws).len(), 2);
    assert_eq!(ws.project().component_count(), 3);
    assert!(!ws.can_undo());
}

#[test]
fn the_explorer_shows_two_systems_and_the_unassigned_node() {
    let ws = workbench_sample();
    let names = ws.project().methodology().element_names().clone();
    let catalog = Catalog::load(Language::En);
    let index = ExplorerIndex::build(&ws, &names, &catalog, SAMPLE_NAME);
    let rows = index.rows(&ExplorerState::default());
    let systems = rows
        .iter()
        .find(|r| r.key == ExplorerKey::Folder(Owner::Project, Folder::Systems))
        .map(|r| r.count);
    assert_eq!(systems, Some(Some(2)));
    assert!(rows.iter().any(|r| r.key == ExplorerKey::Unassigned));
}
