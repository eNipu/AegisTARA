//! The explorer dock driven from the keyboard: open a tab, move a component
//! to another system and undo it, with the keys of the command table.
#![cfg(any(target_os = "macos", target_os = "windows"))]

use std::rc::Rc;

use aegis_client::command_table::command;
use aegis_client::explorer::{ExplorerKey, Folder, Owner};
use aegis_client::i18n::{Catalog, Language};
use aegis_client::settings::Appearance;
use aegis_client::systems::system_rows;
use aegis_client::view::Selection;
use aegis_workbench::app::{Workbench, apply_appearance, bind_all};
use gpui::{Entity, TestAppContext, VisualTestContext};

fn keys(id: &str) -> String {
    command(id)
        .and_then(|c| c.keys_for(cfg!(target_os = "macos")))
        .expect("the command has keys")
}

fn press(cx: &mut VisualTestContext, id: &str) {
    cx.simulate_keystrokes(&keys(id));
    cx.run_until_parked();
}

fn open(cx: &mut TestAppContext) -> (Entity<Workbench>, &mut VisualTestContext) {
    cx.update(|cx| {
        gpui_component::init(cx);
        bind_all(cx);
    });
    let catalog = Rc::new(Catalog::load(Language::En));
    let (view, cx) = cx.add_window_view(|window, cx| {
        apply_appearance(Appearance::Light, window, cx);
        Workbench::new(catalog, Appearance::Light, None, window, cx)
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    (view, cx)
}

#[gpui::test]
fn keys_move_the_selection_and_open_a_tab(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    // Root, then Overview, then Project definition.
    press(cx, "explorer-down");
    press(cx, "explorer-down");
    let selected = cx.update(|_, cx| view.read(cx).state().selected);
    let definition = ExplorerKey::Folder(Owner::Project, Folder::ProjectDefinition);
    assert_eq!(selected, Some(definition));
    assert!(!cx.update(|_, cx| view.read(cx).has_tab(definition, cx)));
    press(cx, "open-selected");
    assert!(cx.update(|_, cx| view.read(cx).has_tab(definition, cx)));
    let shown = cx.update(|_, cx| view.read(cx).explorer_row_count(cx));
    assert!(shown > 3, "{shown}");
}

#[gpui::test]
fn pick_up_and_drop_moves_a_component_and_undo_restores(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let (tcu, gateway) = cx.update(|_, cx| {
        let ws = view.read(cx).workspace();
        let tcu = ws
            .project()
            .components()
            .find(|(_, _, c)| c.name() == "Telematics unit")
            .map(|(_, id, _)| id)
            .expect("component");
        let gw = system_rows(ws).into_iter().find(|r| r.name == "Gateway");
        (tcu, gw.expect("Gateway").id)
    });
    let components = ExplorerKey::Folder(Owner::System(gateway), Folder::Components);
    let count = |cx: &mut VisualTestContext| {
        cx.update(|_, cx| view.read(cx).row(components).and_then(|r| r.count))
    };
    assert_eq!(count(cx), Some(1));

    view.update(cx, |w, cx| w.reveal(Selection::Component(tcu), cx));
    press(cx, "pick-up");
    assert!(cx.update(|_, cx| view.read(cx).picked()).is_some());
    view.update(cx, |w, cx| w.select(ExplorerKey::System(gateway), cx));
    press(cx, "drop-here");
    assert_eq!(count(cx), Some(2));
    assert_eq!(cx.update(|_, cx| view.read(cx).picked()), None);

    press(cx, "undo");
    assert_eq!(count(cx), Some(1));
    press(cx, "redo");
    assert_eq!(count(cx), Some(2));
}

#[gpui::test]
fn a_closed_dock_stays_closed_on_the_next_start(cx: &mut TestAppContext) {
    use aegis_client::settings::Settings;
    use gpui_component::dock::DockPlacement;

    let dir = std::env::temp_dir().join(format!("aegis-wb-layout-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let path = Settings::path_in(&dir);
    cx.update(gpui_component::init);
    let start = |cx: &mut TestAppContext| {
        let (catalog, path) = (Rc::new(Catalog::load(Language::En)), path.clone());
        let (view, cx) = cx.add_window_view(|window, cx| {
            Workbench::new(catalog, Appearance::Light, Some(path), window, cx)
        });
        let area = cx.update(|_, cx| view.read(cx).area().clone());
        (area, cx.clone())
    };
    let (area, mut first) = start(cx);
    assert!(first.update(|_, cx| area.read(cx).is_dock_open(DockPlacement::Left)));
    first.update(|window, cx| {
        area.update(cx, |a, cx| a.toggle_dock(DockPlacement::Left, window, cx));
    });
    first.run_until_parked();
    let stored = Settings::load(&path).workbench_layout("sample");
    assert_eq!(stored.map(|l| l.left_open), Some(false));

    let (area, mut second) = start(cx);
    assert!(!second.update(|_, cx| area.read(cx).is_dock_open(DockPlacement::Left)));
    assert!(second.update(|_, cx| area.read(cx).is_dock_open(DockPlacement::Right)));
    let _ = std::fs::remove_dir_all(&dir);
}
