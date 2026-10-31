//! UI-10.2, UI-10.5, UI-10.9 in the real window: a removal happens at once
//! with a toast that offers Undo; Undo says what it undid and offers Redo.
//! The frames are saved for the UI review (`target/shell-screenshots/toast-*.png`).

mod common;

use aegis_client::commands::{Command, Edit};
use aegis_client::state::{Mode, SyncView};
use aegis_client::view::Page;
use common::{App, TestServer, save};
use slint::{ComponentHandle, Model};

#[tokio::test(flavor = "current_thread")]
async fn a_removal_toasts_with_undo_and_undo_toasts_with_redo() {
    let server = TestServer::start().await;
    let app = App::new(server.admin_session("root"), 1360, 860);
    app.online().await;
    app.window.invoke_create_project("Brake ECU".into());
    app.until("open", |vm| vm.project.is_some()).await;
    app.window.invoke_begin_edit();
    app.until("editing", |vm| {
        vm.project.as_ref().is_some_and(|p| p.mode == Mode::Editing)
    })
    .await;
    app.send(Command::CreateSample);
    let vm = app
        .until("sample", |vm| {
            vm.project
                .as_ref()
                .is_some_and(|p| p.threats.len() == 2 && p.sync == SyncView::Synced)
        })
        .await;
    let threat = vm.project.as_ref().expect("project").threats[0].clone();
    app.window.invoke_show_page(Page::Threats.index() as i32);
    app.frame();

    app.send(Command::Edit(Edit::RemoveThreat { threat: threat.id }));
    app.until("removal toast", |vm| {
        vm.notices.iter().any(|n| n.action.is_some())
    })
    .await;
    let frame = app.frame();
    save(&frame, "toast-removed");
    let notices: Vec<_> = app.window.get_notices().iter().collect();
    assert_eq!(notices.len(), 1, "one toast");
    assert_eq!(notices[0].action, 1, "the button is Undo");
    assert!(
        notices[0]
            .text
            .starts_with(&format!("Threat scenario '{}' removed", threat.name))
    );
    assert!(
        app.window
            .get_undo_label()
            .starts_with(&format!("remove threat scenario '{}'", threat.name)),
        "the tooltip names the action"
    );

    // The toast's own button is the Undo callback.
    app.window.invoke_undo();
    app.until("undone toast", |vm| {
        vm.notices
            .iter()
            .any(|n| n.text.starts_with("Undone: remove threat scenario"))
    })
    .await;
    let frame = app.frame();
    save(&frame, "toast-undone");
    let notices: Vec<_> = app.window.get_notices().iter().collect();
    assert_eq!(notices.len(), 1, "the new toast replaced the old one");
    assert_eq!(notices[0].action, 2, "the button is Redo");
    assert!(app.window.get_can_redo());
}

/// UI-12: a dialog that stays names the thing, its main button says the
/// action, and Escape cancels.
#[tokio::test(flavor = "current_thread")]
async fn the_confirmation_that_stays_names_the_thing_and_the_action() {
    let server = TestServer::start().await;
    let app = App::new(server.admin_session("root"), 1360, 860);
    app.online().await;
    app.window.invoke_create_project("ECU Gen3".into());
    app.until("open", |vm| vm.project.is_some()).await;
    app.window.invoke_confirm(
        "delete-project".into(),
        "Delete this project, its history and its baselines? This cannot be undone.".into(),
    );
    let frame = app.frame();
    save(&frame, "toast-confirm-delete-project");
    assert!(app.window.get_confirm_open());
    assert_eq!(app.window.get_confirm_title(), "Delete project 'ECU Gen3'?");
    assert_eq!(app.window.get_confirm_action(), "Delete project");

    app.window
        .window()
        .dispatch_event(slint::platform::WindowEvent::KeyPressed {
            text: slint::platform::Key::Escape.into(),
        });
    app.frame();
    assert!(!app.window.get_confirm_open(), "Escape cancels");
}
