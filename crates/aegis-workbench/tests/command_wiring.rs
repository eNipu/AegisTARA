//! Every command of the table has an icon and a handler, and every command
//! with keys gets a binding (WBX-11).
#![cfg(any(target_os = "macos", target_os = "windows"))]

use std::rc::Rc;

use aegis_client::command_table::commands;
use aegis_client::commands::Edit;
use aegis_client::explorer::ExplorerKey;
use aegis_client::i18n::{Catalog, Language};
use aegis_client::settings::{Appearance, Settings};
use aegis_client::view::Selection;
use aegis_model::ElementRef;
use aegis_workbench::app::{Workbench, bindings, dispatch_id, icon};
use aegis_workbench::local;
use aegis_workbench::sample::workbench_sample;
use gpui::{AppContext as _, TestAppContext};

#[test]
fn every_command_has_an_icon_and_a_handler() {
    for spec in commands() {
        assert!(
            icon(spec.icon).is_some(),
            "{}: no icon {}",
            spec.id,
            spec.icon
        );
        assert!(dispatch_id(spec.id).is_some(), "{}: no handler", spec.id);
    }
    assert!(icon("no-such-icon").is_none());
    assert!(dispatch_id("no-such-command").is_none());
}

#[gpui::test]
fn every_command_with_keys_gets_one_binding(cx: &mut TestAppContext) {
    let mac = cfg!(target_os = "macos");
    let with_keys = commands()
        .iter()
        .filter(|c| c.keys_for(mac).is_some())
        .count();
    let bound = cx.update(|cx| bindings(cx).len());
    assert_eq!(bound, with_keys);
}

#[test]
fn empty_op_batch_does_not_pollute_undo_or_clear_redo() {
    let mut ws = workbench_sample();
    let edit1 = Edit::SetDescription {
        target: ElementRef::Project,
        text: "first note".into(),
    };
    let edit2 = Edit::SetDescription {
        target: ElementRef::Project,
        text: "second note".into(),
    };

    assert!(local::apply_edit(&mut ws, &edit1, "tester", 1).is_ok());
    assert!(local::apply_edit(&mut ws, &edit2, "tester", 2).is_ok());
    assert!(local::undo(&mut ws).is_ok());
    assert!(ws.can_undo());
    assert!(ws.can_redo());

    // Applying an empty batch must neither clear redo nor push an empty undo entry.
    assert!(local::apply_edit(&mut ws, &Edit::Batch(vec![]), "tester", 3).is_ok());
    assert!(ws.can_undo());
    assert!(ws.can_redo());
    assert!(local::redo(&mut ws).is_ok());
    assert_eq!(
        ws.project().description(ElementRef::Project),
        Some("second note")
    );

    // Undo both edits so can_undo is false and can_redo is true.
    assert!(local::undo(&mut ws).is_ok());
    assert!(local::undo(&mut ws).is_ok());
    assert!(!ws.can_undo());
    assert!(ws.can_redo());

    assert!(local::apply_edit(&mut ws, &Edit::Batch(vec![]), "tester", 4).is_ok());
    assert!(!ws.can_undo());
    assert!(ws.can_redo());
}

#[gpui::test]
fn workbench_hooks_toggle_developer_source_apply_edit_and_open_target(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let dir = std::env::temp_dir().join(format!("aegis-189c1a-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let settings_path = dir.join("settings.json");
    let _ = std::fs::remove_file(&settings_path);

    let catalog = Rc::new(Catalog::load(Language::En));
    let (wb, vcx) = cx.add_window_view(|window, cx| {
        Workbench::new(
            Rc::clone(&catalog),
            Appearance::Light,
            Some(settings_path.clone()),
            window,
            cx,
        )
    });

    // Default developer_source_view is false; command toggles and persists it.
    assert!(!vcx.read_entity(&wb, |w, _| w.developer_source_view()));
    wb.update_in(vcx, |w, window, cx| {
        w.run("toggle-developer-source", window, cx);
    });
    assert!(vcx.read_entity(&wb, |w, _| w.developer_source_view()));
    assert!(Settings::load(&settings_path).developer_source_view);

    // apply_edit applies changes and returns true.
    let applied = wb.update(vcx, |w, cx| {
        w.apply_edit(
            &Edit::SetDescription {
                target: ElementRef::Project,
                text: "via hook".into(),
            },
            cx,
        )
    });
    assert!(applied);
    assert_eq!(
        vcx.read_entity(&wb, |w, _| {
            w.workspace()
                .project()
                .description(ElementRef::Project)
                .map(str::to_owned)
        }),
        Some("via hook".to_owned())
    );

    // open_target reveals, selects, and opens an element tab.
    let asset = vcx
        .read_entity(&wb, |w, _| {
            w.workspace().project().assets().next().map(|(_, id, _)| id)
        })
        .expect("sample asset");
    let sel = Selection::Asset(asset);
    assert_eq!(
        vcx.read_entity(&wb, |w, _| w.state().selected),
        Some(ExplorerKey::Root)
    );
    wb.update_in(vcx, |w, window, cx| {
        w.open_target(sel, window, cx);
    });
    let selected = vcx.read_entity(&wb, |w, _| w.state().selected);
    let key = selected.expect("selected element row");
    assert_ne!(key, ExplorerKey::Root);
    assert!(
        matches!(key, ExplorerKey::Element(_, _, s) if s == sel),
        "expected element row for {sel:?}, got {key:?}"
    );
    assert!(vcx.read_entity(&wb, |w, cx| w.has_tab(key, cx)));

    // apply_edit refuses mutations on a read-only workbench, sets note(), and leaves project intact.
    let seq_before = vcx.read_entity(&wb, |w, _| w.workspace().confirmed_seq());
    wb.update(vcx, |w, cx| {
        w.set_read_only(Some("locked for review".into()), cx);
    });
    let refused = wb.update(vcx, |w, cx| {
        w.apply_edit(
            &Edit::SetDescription {
                target: ElementRef::Project,
                text: "blocked edit".into(),
            },
            cx,
        )
    });
    assert!(!refused);
    assert_eq!(
        vcx.read_entity(&wb, |w, _| w.note().map(str::to_owned)),
        Some("locked for review".to_owned())
    );
    assert_eq!(
        vcx.read_entity(&wb, |w, _| {
            w.workspace()
                .project()
                .description(ElementRef::Project)
                .map(str::to_owned)
        }),
        Some("via hook".to_owned())
    );
    assert_eq!(
        vcx.read_entity(&wb, |w, _| w.workspace().confirmed_seq()),
        seq_before
    );

    // Reloading Workbench from the same settings file restores developer_source_view = true.
    let (wb2, vcx2) = cx.add_window_view(|window, cx| {
        Workbench::new(
            Rc::clone(&catalog),
            Appearance::Light,
            Some(settings_path.clone()),
            window,
            cx,
        )
    });
    assert!(vcx2.read_entity(&wb2, |w, _| w.developer_source_view()));

    let _ = std::fs::remove_dir_all(&dir);
}
