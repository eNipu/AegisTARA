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
