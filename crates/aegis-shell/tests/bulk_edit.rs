//! UI-04 in the real window: rows are picked together with ctrl and shift, a
//! bulk change is one undo step (and one redo) with the undo toast, and a
//! user who may not edit gets no bulk actions. The frames are saved for the UI
//! review (`target/shell-screenshots/bulk-edit-*.png`).

mod common;

use aegis_client::commands::{Command, Edit};
use aegis_client::state::{Mode, SyncView, ViewModel};
use aegis_client::view::Page;
use common::{App, TestServer, save};
use slint::platform::Key;
use slint::{ComponentHandle, Model};

fn p(vm: &ViewModel) -> &aegis_client::state::ProjectView {
    vm.project.as_ref().expect("project")
}

async fn editing_app(server: &TestServer) -> App {
    let app = App::new(server.admin_session("root"), 1360, 860);
    app.online().await;
    app.window.invoke_create_project("Brake ECU".into());
    app.until("open", |vm| vm.project.is_some()).await;
    app.window.invoke_begin_edit();
    app.until("editing", |vm| {
        vm.project.as_ref().is_some_and(|p| p.mode == Mode::Editing)
    })
    .await;
    app
}

async fn assets_page(app: &App, names: &[&str]) {
    use aegis_model::AssetId;
    app.window.invoke_show_page(Page::Assets.index() as i32);
    app.until("assets page", |vm| p(vm).page == Page::Assets)
        .await;
    for (i, name) in names.iter().enumerate() {
        app.send(Command::Edit(Edit::AddAsset {
            id: AssetId::new_v7(),
            name: (*name).to_owned(),
            properties: (0..6).collect(),
        }));
        app.until(name, |vm| p(vm).data.assets.len() == i + 1).await;
    }
    app.frame();
    app.frame();
}

fn row_point(app: &App, index: usize) -> (f32, f32) {
    (
        app.window.get_table_x() + 80.0,
        app.window.get_table_y() + 28.0 + index as f32 * 32.0 + 16.0,
    )
}

/// A click on row `index` with the given keys held: the list is a flickable,
/// which holds a press back for 100 ms, so the release comes after that.
fn click(app: &App, index: usize, ctrl: bool, shift: bool) {
    use slint::platform::{PointerEventButton, WindowEvent};
    let (x, y) = row_point(app, index);
    let position = slint::LogicalPosition::new(x, y);
    let w = app.window.window();
    let held: Vec<slint::SharedString> = [(ctrl, Key::Control), (shift, Key::Shift)]
        .into_iter()
        .filter(|(on, _)| *on)
        .map(|(_, k)| k.into())
        .collect();
    for key in &held {
        w.dispatch_event(WindowEvent::KeyPressed { text: key.clone() });
    }
    w.dispatch_event(WindowEvent::PointerMoved { position });
    w.dispatch_event(WindowEvent::PointerPressed {
        position,
        button: PointerEventButton::Left,
    });
    std::thread::sleep(std::time::Duration::from_millis(150));
    app.headless.tick();
    w.dispatch_event(WindowEvent::PointerReleased {
        position,
        button: PointerEventButton::Left,
    });
    for key in held {
        w.dispatch_event(WindowEvent::KeyReleased { text: key });
    }
}

/// The rows picked together in the asset table.
fn picked(app: &App) -> Vec<bool> {
    let marks = app.window.get_asset_marks();
    assert_eq!(
        marks.count as usize,
        marks.rows.iter().filter(|m| *m).count(),
        "the count matches the flags"
    );
    marks.rows.iter().collect()
}

async fn selected_is(app: &App, name: &str) {
    app.until(name, |vm| {
        p(vm).data.asset.as_ref().is_some_and(|a| a.name == name)
    })
    .await;
}

#[tokio::test(flavor = "current_thread")]
async fn click_ctrl_and_shift_pick_rows() {
    let server = TestServer::start().await;
    let app = editing_app(&server).await;
    assets_page(&app, &["A", "B", "C", "D"]).await;
    save(&app.frame(), "bulk-edit-before");

    // A plain click selects one row and picks nothing together.
    click(&app, 0, false, false);
    selected_is(&app, "A").await;
    assert_eq!(picked(&app), [false; 4]);

    // Ctrl adds a row: the selected row and the clicked one.
    click(&app, 2, true, false);
    selected_is(&app, "C").await;
    assert_eq!(picked(&app), [true, false, true, false]);
    // Ctrl on a picked row drops it.
    click(&app, 0, true, false);
    assert_eq!(picked(&app), [false, false, true, false]);
    // A plain click goes back to one row.
    click(&app, 3, false, false);
    selected_is(&app, "D").await;
    assert_eq!(picked(&app), [false; 4]);

    // Shift picks the range from the selected row.
    click(&app, 1, false, false);
    selected_is(&app, "B").await;
    click(&app, 3, false, true);
    assert_eq!(picked(&app), [false, true, true, true]);
    assert_eq!(app.window.get_asset_marks().count, 3);
    save(&app.frame(), "bulk-edit-after");

    // Escape puts the table back to one row.
    common::press_key(&app, Key::Escape, false);
    assert_eq!(picked(&app), [false; 4]);
}

#[tokio::test(flavor = "current_thread")]
async fn a_bulk_remove_is_one_undo_step_and_one_redo() {
    let server = TestServer::start().await;
    let app = editing_app(&server).await;
    assets_page(&app, &["A", "B", "C", "D"]).await;

    click(&app, 0, false, false);
    selected_is(&app, "A").await;
    click(&app, 2, false, true);
    assert_eq!(picked(&app), [true, true, true, false]);

    // Delete removes the picked rows, not just the selected one.
    common::press_key(&app, Key::Delete, false);
    let vm = app
        .until("three removed", |vm| {
            p(vm).data.assets.len() == 1 && vm.notices.iter().any(|n| n.action.is_some())
        })
        .await;
    assert_eq!(p(&vm).data.assets[0].name, "D");
    let notices: Vec<_> = app.window.get_notices().iter().collect();
    assert_eq!(notices.len(), 1, "one toast");
    assert_eq!(notices[0].text, "3 assets removed");
    assert_eq!(app.window.get_undo_label(), "remove 3 assets");
    assert_eq!(app.window.get_asset_marks().count, 0, "the picks are gone");

    // One undo brings all three back.
    app.window.invoke_undo();
    app.until("undone", |vm| p(vm).data.assets.len() == 4).await;
    assert_eq!(app.window.get_redo_label(), "remove 3 assets");
    // One redo removes them again.
    app.window.invoke_redo();
    app.until("redone", |vm| p(vm).data.assets.len() == 1).await;
}

#[tokio::test(flavor = "current_thread")]
async fn a_bulk_treatment_is_one_undo_step() {
    let server = TestServer::start().await;
    let app = editing_app(&server).await;
    app.send(Command::CreateSample);
    app.until("sample", |vm| {
        vm.project
            .as_ref()
            .is_some_and(|p| p.threats.len() == 2 && p.sync == SyncView::Synced)
    })
    .await;
    app.window.invoke_show_page(Page::Risks.index() as i32);
    app.until("risks", |vm| p(vm).page == Page::Risks).await;
    app.frame();
    app.frame();

    let treatments = |vm: &ViewModel| -> Vec<String> {
        p(vm)
            .data
            .risks
            .iter()
            .map(|r| r.treatment.clone())
            .collect()
    };
    let vm = app.until("risks", |vm| p(vm).data.risks.len() == 2).await;
    let before = treatments(&vm);

    app.window.invoke_mark_click("risk".into(), 0, false);
    app.window.invoke_mark_click("risk".into(), 1, false);
    assert_eq!(app.window.get_risk_marks().count, 2);
    // The second option is the first real treatment ("none" is first).
    app.window.invoke_treat_marked(2);
    let vm = app
        .until("treated", |vm| {
            treatments(vm).iter().all(|t| *t == treatments(vm)[0]) && treatments(vm) != before
        })
        .await;
    let after = treatments(&vm);
    app.window.invoke_undo();
    app.until("undone", |vm| treatments(vm) == before).await;
    app.window.invoke_redo();
    app.until("redone", |vm| treatments(vm) == after).await;
}

#[tokio::test(flavor = "current_thread")]
async fn a_read_only_user_gets_no_bulk_actions() {
    let server = TestServer::start().await;
    let app = editing_app(&server).await;
    assets_page(&app, &["A", "B", "C"]).await;
    app.window.invoke_end_edit();
    app.until("viewing", |vm| p(vm).mode != Mode::Editing).await;
    app.frame();
    app.frame();

    click(&app, 0, false, false);
    selected_is(&app, "A").await;
    click(&app, 2, false, true);
    click(&app, 1, true, false);
    assert_eq!(picked(&app), [false; 3], "clicks pick nothing");
    // A stale callback picks nothing and removes nothing either.
    app.window.invoke_mark_click("asset".into(), 1, true);
    app.window.invoke_mark_click("asset".into(), 2, false);
    app.window.invoke_remove_marked("asset".into());
    common::press_key(&app, Key::Delete, false);
    for _ in 0..5 {
        app.frame();
    }
    let vm = app.until("still", |_| true).await;
    assert_eq!(picked(&app), [false; 3]);
    assert_eq!(p(&vm).data.assets.len(), 3);
}
