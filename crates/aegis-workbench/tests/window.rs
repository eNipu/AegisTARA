//! The workbench window opens in GPUI's test context with its docks.
#![cfg(any(target_os = "macos", target_os = "windows"))]

use aegis_client::i18n::{Catalog, Language};
use aegis_client::settings::Appearance;
use aegis_workbench::app::{Workbench, apply_appearance};
use gpui::TestAppContext;

#[gpui::test]
fn the_window_opens_with_explorer_rows(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let catalog = Catalog::load(Language::En);
    let (view, cx) = cx.add_window_view(|window, cx| {
        apply_appearance(Appearance::Light, window, cx);
        Workbench::new(&catalog, Appearance::Light, window, cx)
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let (shown, expected) = cx.update(|_, cx| {
        let wb = view.read(cx);
        (wb.explorer_row_count(cx), wb.index().rows(wb.state()).len())
    });
    assert!(shown > 3, "{shown}");
    assert_eq!(shown, expected);
    let count = cx.update(|_, cx| view.read(cx).workspace().project().component_count());
    assert_eq!(count, 3);
}
