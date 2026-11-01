//! The history panel (UI-03, job 083) in the bottom panel slot (UI-02): it
//! lists this session's edits, previews going back to one, and goes back to
//! it as one undo step. Its height is remembered like the other panels.

mod common;

use std::sync::Arc;

use aegis_client::settings::Settings;
use aegis_client::state::{Mode, SyncView, ViewModel};
use aegis_client::view::Page;
use aegis_shell::AppWindow;
use aegis_shell::headless::Headless;
use aegis_shell::launcher::Launcher;
use common::{App, TestServer, save};
use slint::Model;

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() <= 2.0
}

fn trees(vm: &ViewModel) -> usize {
    vm.project.as_ref().map_or(0, |p| p.data.trees.len())
}

fn edits(vm: &ViewModel) -> usize {
    vm.project.as_ref().map_or(0, |p| p.edits.len())
}

async fn app_with_edits(server: &TestServer) -> App {
    let app = App::new(server.session("alice"), 1360, 860);
    app.online().await;
    app.window.invoke_create_project("History".into());
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
    app
}

async fn show(app: &App, page: Page) {
    app.window.invoke_show_page(page.index() as i32);
    app.until("page", |vm| {
        vm.project.as_ref().is_some_and(|p| p.page == page)
    })
    .await;
    app.frame();
    app.frame();
}

#[tokio::test(flavor = "current_thread")]
async fn the_history_panel_lists_previews_and_goes_back_as_one_step() {
    let server = TestServer::start().await;
    let app = app_with_edits(&server).await;
    show(&app, Page::Trees).await;
    let before = edits(&app.session.view());
    let trees_before = trees(&app.session.view());
    for name in ["Tree A", "Tree B", "Tree C"] {
        app.window.invoke_new_tree(name.into());
    }
    app.until("three trees", |vm| trees(vm) == trees_before + 3)
        .await;

    // The toolbar button opens the panel in the bottom slot.
    assert!(!app.window.get_history_shown());
    app.window.set_history_shown(true);
    app.frame();
    app.frame();
    let rows = app.window.get_history_edit_rows();
    // Every edit, then the state the project was opened in.
    assert_eq!(rows.row_count(), before + 3 + 1);
    let cell = |r: usize, c: usize| {
        rows.row_data(r)
            .expect("row")
            .row_data(c)
            .expect("cell")
            .text
    };
    assert_eq!(cell(0, 0).as_str(), "add attack tree");
    assert_eq!(cell(0, 1).as_str(), "alice");
    assert!(!cell(0, 2).is_empty(), "the time");
    assert_eq!(cell(before + 3, 0).as_str(), "Project opened");
    save(&app.frame(), "history-after-panel");

    // Picking the current state offers nothing to undo.
    app.window.set_history_edit_selected(0);
    app.window.invoke_history_pick(0);
    assert!(app.window.get_history_preview().contains("current state"));

    // Picking an older row previews exactly what going back would undo.
    app.window.set_history_edit_selected(2);
    app.window.invoke_history_pick(2);
    let preview = app.window.get_history_preview().to_string();
    assert!(preview.contains("2 changes"), "{preview}");
    assert_eq!(preview.matches("add attack tree").count(), 2, "{preview}");
    app.frame();
    save(&app.frame(), "history-after-preview");
    assert_eq!(
        trees(&app.session.view()),
        trees_before + 3,
        "only a preview"
    );

    // Going back undoes both as one step: one redo brings them back.
    app.window.invoke_history_go_back(2);
    app.until("went back", |vm| trees(vm) == trees_before + 1)
        .await;
    assert_eq!(edits(&app.session.view()), before + 1);
    app.frame();
    assert_eq!(app.window.get_history_edit_selected(), -1);
    assert_eq!(app.window.get_history_preview(), "");
    app.window.invoke_redo();
    app.until("redone", |vm| trees(vm) == trees_before + 3)
        .await;

    // The redo made the batch one entry again; the "Project opened" row goes
    // back to the very start of the session's edits.
    let count = edits(&app.session.view());
    assert_eq!(count, before + 2, "the batch is one entry");
    app.window.invoke_history_go_back(count as i32);
    app.until("all undone", |vm| trees(vm) == 0).await;
    assert_eq!(edits(&app.session.view()), 0);

    drop(app);
    server.stop().await;
}

#[tokio::test(flavor = "current_thread")]
async fn dragging_the_bottom_edge_resizes_the_slot_and_a_small_window_keeps_its_page() {
    let server = TestServer::start().await;
    let mut app = app_with_edits(&server).await;
    show(&app, Page::Trees).await;
    app.window.set_history_shown(true);
    app.frame();
    app.frame();
    assert!(near(app.window.get_bottom_size(), 0.0), "the default");

    // At 1360 x 860 the panel is 200 px above the 27 px status bar, so its
    // top edge is at y = 632. Dragging it 50 px up makes the panel 50 px taller.
    app.headless.drag((700.0, 632.5), (700.0, 582.5));
    assert!(
        near(app.window.get_bottom_size(), 250.0),
        "{}",
        app.window.get_bottom_size()
    );
    save(&app.frame(), "history-after-resized");

    // It never takes more than a third of a small window.
    app.window.set_bottom_size(600.0);
    app.resize(980, 600);
    show(&app, Page::Trees).await;
    let frame = app.frame();
    assert_eq!((frame.width, frame.height), (980, 600));
    save(&frame, "history-after-small");
    drop(app);
    server.stop().await;
}

#[tokio::test(flavor = "current_thread")]
async fn the_bottom_height_is_remembered_across_a_restart() {
    let dir = std::env::temp_dir().join(format!("aegis-history-{}", std::process::id()));
    let path = Settings::path_in(&dir);
    let _ = std::fs::remove_file(&path);
    let start = |path: &std::path::Path| {
        let headless = Headless::install(1100, 760);
        let window = AppWindow::new().expect("window");
        let launcher = Launcher::new(
            &window,
            tokio::runtime::Handle::current(),
            Some(path.to_path_buf()),
            Arc::new(|| {}),
        );
        (headless, window, launcher)
    };

    let (_headless, window, launcher) = start(&path);
    assert!(near(window.get_bottom_size(), 0.0), "the default");
    window.set_bottom_size(260.0);
    window.invoke_panels_resized();
    drop(launcher);
    drop(window);

    assert_eq!(Settings::load(&path).bottom_panel, 260);
    let (_headless, window, _launcher) = start(&path);
    assert!(near(window.get_bottom_size(), 260.0));
    let _ = std::fs::remove_dir_all(&dir);
}
