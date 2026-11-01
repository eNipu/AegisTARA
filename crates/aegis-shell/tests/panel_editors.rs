//! Editors in panels, not over them (UI-11, job 117): the library's Insert
//! button follows the canvas selection; the outline editor, project rename
//! and baseline creation happen on the page, not in a dialog.

mod common;

use aegis_client::state::{Mode, SyncView};
use aegis_client::view::Page;
use common::{App, TestServer, node_position, press_chord, press_key, save, type_text};
use slint::ComponentHandle;
use slint::platform::{Key, WindowEvent};

fn click(app: &App, x: f32, y: f32) {
    let w = app.window.window();
    let position = slint::LogicalPosition::new(x, y);
    w.dispatch_event(WindowEvent::PointerMoved { position });
    w.dispatch_event(WindowEvent::PointerPressed {
        position,
        button: slint::platform::PointerEventButton::Left,
    });
    w.dispatch_event(WindowEvent::PointerReleased {
        position,
        button: slint::platform::PointerEventButton::Left,
    });
}

fn project_name(vm: &aegis_client::state::ViewModel) -> String {
    vm.project
        .as_ref()
        .map(|p| p.entry.name.clone())
        .unwrap_or_default()
}

async fn app_with_sample(server: &TestServer) -> App {
    let app = App::new(server.session("alice"), 1360, 860);
    app.online().await;
    app.window.invoke_create_project("Editors".into());
    app.until("project open", |vm| vm.project.is_some()).await;
    app.window.invoke_begin_edit();
    app.until("editing", |vm| {
        vm.project.as_ref().is_some_and(|p| p.mode == Mode::Editing)
    })
    .await;
    app.window.invoke_create_sample();
    app.until("saved", |vm| {
        vm.project
            .as_ref()
            .is_some_and(|p| p.tree.is_some() && p.sync == SyncView::Synced)
    })
    .await;
    show(&app, Page::Trees).await;
    app
}

async fn show(app: &App, page: Page) {
    app.window.invoke_show_page(page.index() as i32);
    app.until("page", |vm| {
        vm.project.as_ref().is_some_and(|p| p.page == page)
    })
    .await;
    // Two frames: one lays the page out, one draws it.
    app.frame();
    app.frame();
}

#[tokio::test(flavor = "current_thread")]
async fn insert_follows_the_canvas_selection_and_says_what_it_will_do() {
    let server = TestServer::start().await;
    let app = app_with_sample(&server).await;
    app.window.invoke_library_opened();
    app.window.set_library_open(true);
    app.frame();
    app.frame();
    app.window.invoke_library_pick(0);

    // Under a gate: nothing is selected yet, so Insert has no place to name.
    app.window.set_library_mode(1);
    app.frame();
    assert_eq!(app.window.get_library_insert_label(), "Insert");
    save(&app.frame(), "editors-after-insert-none");

    // Clicking a gate on the canvas, with the library still open, names it.
    let (x, y) = node_position(&app, "Attack the cellular link");
    app.headless.click(x, y);
    app.until("gate selected", |vm| {
        vm.project.as_ref().is_some_and(|p| p.inspector.is_some())
    })
    .await;
    app.frame();
    assert!(app.window.get_library_can_gate());
    assert_eq!(
        app.window.get_library_insert_label(),
        format!("Insert under '{}'", app.window.get_inspector_title())
    );
    save(&app.frame(), "editors-after-insert-gate");

    // As a new tree it needs no place.
    app.window.set_library_mode(0);
    assert_eq!(
        app.window.get_library_insert_label(),
        "Insert as a new tree"
    );

    drop(app);
    server.stop().await;
}

#[tokio::test(flavor = "current_thread")]
async fn the_outline_editor_is_a_mode_of_the_page_with_apply_and_cancel() {
    let server = TestServer::start().await;
    let app = app_with_sample(&server).await;
    let nodes = |app: &App| {
        let view = app.session.view();
        view.project
            .as_ref()
            .and_then(|p| p.scene())
            .map_or(0, |s| s.len())
    };
    let before = nodes(&app);
    let outline = app.window.get_outline_text().to_string();
    assert!(outline.contains("Compromise the backend credentials"));

    // Opening it replaces the canvas area; no dialog, the page is still there.
    app.window.set_outline_draft(outline.clone().into());
    app.window.set_outline_editing(true);
    app.frame();
    save(&app.frame(), "editors-after-outline");

    // Cancel leaves the tree as it was.
    app.window
        .set_outline_draft(format!("{outline}\n  - A new step").into());
    app.window.set_outline_editing(false);
    app.frame();
    assert_eq!(nodes(&app), before);

    // Apply writes the text back as the tree and leaves the mode.
    let changed = outline.replace(
        "Compromise the backend credentials",
        "Compromise the backend credentials\n    - Steal a build token",
    );
    app.window.set_outline_draft(changed.into());
    app.window.set_outline_editing(true);
    app.window
        .invoke_apply_outline(app.window.get_outline_draft());
    app.window.set_outline_editing(false);
    app.until("tree replaced", |vm| {
        vm.project
            .as_ref()
            .and_then(|p| p.scene())
            .is_some_and(|s| s.len() == before + 1)
    })
    .await;

    // Picking another tree, or another page, leaves the mode.
    app.window.set_outline_editing(true);
    show(&app, Page::Risks).await;
    assert!(!app.window.get_outline_editing());

    drop(app);
    server.stop().await;
}

#[tokio::test(flavor = "current_thread")]
async fn clicking_the_project_name_renames_it_in_place() {
    let server = TestServer::start().await;
    let app = app_with_sample(&server).await;
    // The name in the header, with the pointer at its right end.
    let at = (299.0, 83.0);

    // Click: the name becomes a field with the keyboard in it. Enter saves.
    click(&app, at.0, at.1);
    app.frame();
    assert!(app.window.get_renaming_project());
    save(&app.frame(), "editors-after-rename");
    press_chord(&app, 'a', false);
    type_text(&app, "Editors v2");
    press_key(&app, Key::Return, false);
    app.until("renamed", |vm| project_name(vm) == "Editors v2")
        .await;
    app.frame();
    assert!(!app.window.get_renaming_project());

    // Escape puts the saved name back and sends nothing.
    click(&app, at.0, at.1);
    app.frame();
    press_chord(&app, 'a', false);
    type_text(&app, "nope");
    press_key(&app, Key::Escape, false);
    app.frame();
    assert!(!app.window.get_renaming_project());
    assert_eq!(project_name(&app.session.view()), "Editors v2");

    // An empty name is never saved; the field says a name is needed.
    click(&app, at.0, at.1);
    app.frame();
    press_chord(&app, 'a', false);
    press_key(&app, Key::Backspace, false);
    press_key(&app, Key::Return, false);
    app.frame();
    assert!(
        app.window.get_renaming_project(),
        "stays open with the note"
    );
    assert_eq!(project_name(&app.session.view()), "Editors v2");
    save(&app.frame(), "editors-after-rename-empty");

    // Leaving the field saves what was typed.
    press_chord(&app, 'a', false);
    type_text(&app, "Editors v3");
    click(&app, 700.0, 500.0);
    app.until("saved by focus loss", |vm| project_name(vm) == "Editors v3")
        .await;
    app.frame();
    assert!(!app.window.get_renaming_project());

    drop(app);
    server.stop().await;
}

#[tokio::test(flavor = "current_thread")]
async fn a_baseline_is_named_in_a_row_at_the_top_of_the_versions_table() {
    let server = TestServer::start().await;
    let app = app_with_sample(&server).await;
    let baselines =
        |vm: &aegis_client::state::ViewModel| vm.project.as_ref().map_or(0, |p| p.baselines.len());

    // The menu command goes to the Versions page and opens the row.
    app.window.invoke_begin_baseline();
    app.until("versions", |vm| {
        vm.project
            .as_ref()
            .is_some_and(|p| p.page == Page::Versions)
    })
    .await;
    app.frame();
    app.frame();
    assert!(app.window.get_creating_baseline());
    save(&app.frame(), "editors-after-baseline");

    // Escape closes it and creates nothing.
    type_text(&app, "B0");
    press_key(&app, Key::Escape, false);
    app.frame();
    assert!(!app.window.get_creating_baseline());
    assert_eq!(baselines(&app.session.view()), 0);

    // Enter creates the baseline under the typed name and closes the row.
    app.window.invoke_begin_baseline();
    app.frame();
    type_text(&app, "B1 Design freeze");
    press_key(&app, Key::Return, false);
    app.until("baseline created", |vm| baselines(vm) == 1).await;
    app.frame();
    assert!(!app.window.get_creating_baseline());
    let view = app.session.view();
    assert_eq!(
        view.project.as_ref().unwrap().baselines[0].name,
        "B1 Design freeze"
    );

    // An empty name creates nothing.
    app.window.invoke_begin_baseline();
    app.frame();
    press_key(&app, Key::Return, false);
    app.frame();
    assert!(app.window.get_creating_baseline());
    assert_eq!(baselines(&app.session.view()), 1);

    drop(app);
    server.stop().await;
}
