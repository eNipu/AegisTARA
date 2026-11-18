//! The palette driven by the keyboard alone: reach a threat scenario by its
//! visual ID, and run a command by its label (WBX-04).
#![cfg(any(target_os = "macos", target_os = "windows"))]

use std::rc::Rc;

use aegis_client::command_table::command;
use aegis_client::explorer::ExplorerKey;
use aegis_client::i18n::{Catalog, Language};
use aegis_client::pages::selection_visual_id;
use aegis_client::settings::Appearance;
use aegis_client::view::Selection;
use aegis_workbench::app::{Workbench, apply_appearance, bind_all};
use gpui::{Entity, TestAppContext, VisualTestContext};
use gpui_component::dock::DockPlacement;

fn redraw(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
}

fn press(cx: &mut VisualTestContext, keys: &str) {
    cx.simulate_keystrokes(keys);
    redraw(cx);
}

fn open_palette(cx: &mut VisualTestContext) {
    let keys = command("palette")
        .and_then(|c| c.keys_for(cfg!(target_os = "macos")))
        .expect("the palette has keys");
    press(cx, &keys);
}

/// Types `text` word by word, a space key between words.
fn type_text(cx: &mut VisualTestContext, text: &str) {
    for (i, word) in text.split(' ').enumerate() {
        if i > 0 {
            cx.simulate_keystrokes("space");
        }
        cx.simulate_input(word);
        redraw(cx);
    }
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
    redraw(cx);
    (view, cx)
}

#[gpui::test]
fn the_palette_reaches_a_threat_and_runs_a_command(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let (threat, vid) = cx.update(|_, cx| {
        let project = view.read(cx).workspace().project();
        let id = project
            .threat_scenarios()
            .nth(1)
            .map(|(_, id, _)| id)
            .expect("a second threat scenario");
        let sel = Selection::Threat(id);
        (sel, selection_visual_id(project, sel))
    });

    open_palette(cx);
    assert!(cx.update(|_, cx| view.read(cx).palette_open()));
    type_text(cx, &vid);
    press(cx, "enter");
    assert!(!cx.update(|_, cx| view.read(cx).palette_open()));
    let selected = cx.update(|_, cx| view.read(cx).state().selected);
    let Some(key @ ExplorerKey::Element(_, _, sel)) = selected else {
        panic!("not an element row: {selected:?}");
    };
    assert_eq!(sel, threat);
    assert!(cx.update(|_, cx| view.read(cx).has_tab(key, cx)));

    let area = cx.update(|_, cx| view.read(cx).area().clone());
    assert!(cx.update(|_, cx| area.read(cx).is_dock_open(DockPlacement::Left)));
    open_palette(cx);
    type_text(cx, &Catalog::load(Language::En).tr("cmd-toggle-explorer"));
    press(cx, "enter");
    assert!(!cx.update(|_, cx| area.read(cx).is_dock_open(DockPlacement::Left)));
}
