//! The explorer dock driven from the keyboard: open a tab, move a component
//! to another system and undo it, with the keys of the command table.
#![cfg(any(target_os = "macos", target_os = "windows"))]

use std::rc::Rc;

use aegis_client::i18n::{Catalog, Language};
use aegis_client::settings::Appearance;
use aegis_workbench::app::Workbench;
use gpui::TestAppContext;

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

#[gpui::test]
fn opening_rows_adds_tabs_by_key(cx: &mut TestAppContext) {
    use aegis_client::explorer::{ExplorerKey, Folder, Owner};
    use aegis_client::systems::system_rows;
    use aegis_client::view::Selection;

    cx.update(gpui_component::init);
    let catalog = Rc::new(Catalog::load(Language::En));
    let (view, cx) = cx
        .add_window_view(|window, cx| Workbench::new(catalog, Appearance::Light, None, window, cx));
    cx.update(|window, cx| window.draw(cx).clear(cx));

    // Root shares the Overview tab, which is open from the start.
    assert!(cx.update(|_, cx| view.read(cx).has_tab(ExplorerKey::Root, cx)));

    let definition = ExplorerKey::Folder(Owner::Project, Folder::ProjectDefinition);
    let has =
        |cx: &mut gpui::VisualTestContext, key| cx.update(|_, cx| view.read(cx).has_tab(key, cx));
    let open = |cx: &mut gpui::VisualTestContext, key| {
        cx.update(|window, cx| view.update(cx, |w, cx| w.open(key, window, cx)));
    };
    assert!(!has(cx, definition));
    open(cx, definition);
    assert!(has(cx, definition));
    open(cx, definition);
    assert!(has(cx, definition));

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
    cx.update(|_, cx| view.update(cx, |w, cx| w.reveal(Selection::Component(tcu), cx)));
    let key = cx.update(|_, cx| view.read(cx).state().selected);
    let Some(key @ ExplorerKey::Element(..)) = key else {
        panic!("the component row is not selected: {key:?}");
    };
    assert!(!has(cx, key));
    open(cx, key);
    assert!(has(cx, key));

    let rows =
        |cx: &mut gpui::VisualTestContext| cx.update(|_, cx| view.read(cx).explorer_row_count(cx));
    let toggle = |cx: &mut gpui::VisualTestContext| {
        let system = ExplorerKey::System(gateway);
        cx.update(|_, cx| view.update(cx, |w, cx| w.toggle(system, cx)));
    };
    let before = rows(cx);
    toggle(cx);
    assert_ne!(rows(cx), before);
    toggle(cx);
    assert_eq!(rows(cx), before);
}
