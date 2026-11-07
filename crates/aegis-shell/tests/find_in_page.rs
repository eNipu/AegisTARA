//! UI-16 in the real window: ⌘/Ctrl+F opens a find strip above a table page,
//! typing filters the rows by visual ID, name or description, ".*" makes the
//! text a regular expression, a bad expression says so and matches nothing,
//! Escape clears and closes, and another page starts unfiltered. The frame is
//! saved for the UI review (`target/shell-screenshots/find-in-page.png`).

mod common;

use aegis_client::commands::{Command, Edit};
use aegis_client::state::{Mode, ProjectView, ViewModel};
use aegis_client::view::Page;
use aegis_model::AssetId;
use common::{App, TestServer, press_chord, press_key, save, type_text};
use slint::platform::Key;

fn p(vm: &ViewModel) -> &ProjectView {
    vm.project.as_ref().expect("project")
}

fn names(vm: &ViewModel) -> Vec<String> {
    p(vm).data.assets.iter().map(|a| a.name.clone()).collect()
}

#[tokio::test(flavor = "current_thread")]
async fn find_filters_rows_by_text_and_expression_and_clears() {
    let server = TestServer::start().await;
    let app = App::new(server.admin_session("root"), 1360, 860);
    app.online().await;
    app.window.invoke_create_project("Gateway".into());
    app.until("open", |vm| vm.project.is_some()).await;
    app.window.invoke_begin_edit();
    app.until("editing", |vm| {
        vm.project.as_ref().is_some_and(|p| p.mode == Mode::Editing)
    })
    .await;
    for name in ["Brake ECU", "Gateway firmware", "ブレーキ制御の鍵"] {
        app.send(Command::Edit(Edit::AddAsset {
            id: AssetId::new_v7(),
            name: name.into(),
            properties: (0..6).collect(),
        }));
    }
    app.window.invoke_show_page(Page::Assets.index() as i32);
    app.until("assets", |vm| {
        p(vm).page == Page::Assets && p(vm).data.assets.len() == 3
    })
    .await;
    app.frame();

    // Keyboard only: the shortcut opens the strip with the field focused.
    press_chord(&app, 'f', false);
    app.frame();
    assert!(app.window.get_find_open(), "the find strip opens");
    type_text(&app, "ecu");
    app.until("plain", |vm| names(vm) == ["Brake ECU"]).await;
    app.frame();
    assert_eq!(
        (app.window.get_find_shown(), app.window.get_find_total()),
        (1, 3)
    );
    save(&app.frame(), "find-in-page");

    // Japanese text matches as typed.
    app.window.invoke_set_find("ブレーキ".into(), false);
    app.until("japanese", |vm| names(vm) == ["ブレーキ制御の鍵"])
        .await;

    // A regular expression over the visual IDs, then an invalid one.
    app.window.invoke_set_find("^A0[01]$".into(), true);
    app.until("regex", |vm| p(vm).data.assets.len() == 2).await;
    app.window.invoke_set_find("(".into(), true);
    app.until("invalid", |vm| p(vm).data.assets.is_empty())
        .await;
    app.frame();
    assert_eq!(app.window.get_find_error(), 2);
    app.window.invoke_set_find("x".repeat(300).into(), false);
    app.until("too long", |vm| {
        p(vm)
            .find
            .as_ref()
            .is_some_and(|f| f.error == Some(aegis_client::find::FindError::TooLong))
    })
    .await;
    app.frame();
    assert_eq!(app.window.get_find_error(), 1);

    // Escape clears the filter and closes the strip.
    app.window.set_find_focus(true);
    app.frame();
    press_key(&app, Key::Escape, false);
    app.until("cleared", |vm| p(vm).data.assets.len() == 3)
        .await;
    app.frame();
    assert!(!app.window.get_find_open());

    // Another page starts without a filter.
    app.window.invoke_open_find();
    app.window.invoke_set_find("ecu".into(), false);
    app.until("filtered", |vm| p(vm).data.assets.len() == 1)
        .await;
    app.window.invoke_show_page(Page::Damage.index() as i32);
    app.until("damage", |vm| {
        p(vm).page == Page::Damage && p(vm).find.is_none()
    })
    .await;
    app.frame();
    assert!(!app.window.get_find_open());
}
