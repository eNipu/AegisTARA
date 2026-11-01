//! Panel slots (UI-02, job 082): the left and right panels resize by
//! dragging their edge and keep the size across a restart; the inspector
//! shows the whole selection with its explanation; the attack tree library
//! sits in the right slot, next to the canvas, instead of over it.

mod common;

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use aegis_client::settings::Settings;
use aegis_client::state::{Mode, SyncView};
use aegis_client::view::Page;
use aegis_shell::AppWindow;
use aegis_shell::headless::Headless;
use aegis_shell::launcher::Launcher;
use common::{App, TestServer, node_position, save};

async fn app_with_sample(server: &TestServer) -> App {
    let app = App::new(server.session("alice"), 1360, 860);
    app.online().await;
    app.window.invoke_create_project("Panels".into());
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
    // Two frames: one lays the page out, one draws it.
    app.frame();
    app.frame();
}

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() <= 2.0
}

#[tokio::test(flavor = "current_thread")]
async fn dragging_a_panel_edge_resizes_the_slot() {
    let server = TestServer::start().await;
    let app = app_with_sample(&server).await;
    show(&app, Page::Trees).await;
    save(&app.frame(), "panels-after-trees");
    let released = Rc::new(Cell::new(0));
    let count = Rc::clone(&released);
    app.window
        .on_panels_resized(move || count.set(count.get() + 1));

    // The right slot: at 1360 px the inspector is 340 px wide, so its edge
    // is the rule at x = 1019. Dragging it 100 px left widens it by 100.
    assert!(near(app.window.get_right_size(), 0.0), "follows the window");
    app.headless.drag((1019.5, 400.0), (919.5, 400.0));
    assert!(
        near(app.window.get_right_size(), 440.0),
        "{}",
        app.window.get_right_size()
    );
    assert_eq!(released.get(), 1, "letting go says so once");

    // The left slot: the TARA steps end at x = 210.
    app.headless.drag((210.5, 700.0), (260.5, 700.0));
    assert!(
        near(app.window.get_left_size(), 260.0),
        "{}",
        app.window.get_left_size()
    );
    assert_eq!(released.get(), 2);
    save(&app.frame(), "panels-after-resized");

    // Every page's inspector uses the same right slot.
    show(&app, Page::Threats).await;
    app.window.invoke_select_threat(0);
    app.frame();
    save(&app.frame(), "panels-after-threat-resized");

    // A size never pushes the page off a small window.
    let mut app = app;
    app.resize(980, 600);
    show(&app, Page::Trees).await;
    let frame = app.frame();
    assert_eq!((frame.width, frame.height), (980, 600));
    save(&frame, "panels-after-small");
    drop(app);
    server.stop().await;
}

#[tokio::test(flavor = "current_thread")]
async fn the_panel_sizes_are_remembered_across_a_restart() {
    let dir = std::env::temp_dir().join(format!("aegis-panels-{}", std::process::id()));
    let path = Settings::path_in(&dir);
    let _ = std::fs::remove_file(&path);
    // A new window each time, as after a restart.
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
    assert!(near(window.get_left_size(), 210.0), "the default");
    assert!(near(window.get_right_size(), 0.0), "follows the window");
    // What a drag leaves behind (see the test above), then letting go.
    window.set_left_size(250.0);
    window.set_right_size(420.0);
    window.invoke_panels_resized();
    drop(launcher);
    drop(window);

    let saved = Settings::load(&path);
    assert_eq!((saved.left_panel, saved.right_panel), (250, 420));
    let (_headless, window, _launcher) = start(&path);
    assert!(near(window.get_left_size(), 250.0));
    assert!(near(window.get_right_size(), 420.0));
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test(flavor = "current_thread")]
async fn the_threat_inspector_shows_every_property_with_why() {
    let server = TestServer::start().await;
    let app = app_with_sample(&server).await;
    show(&app, Page::Threats).await;
    app.window.invoke_select_threat(0);
    app.until("threat", |vm| {
        vm.project.as_ref().is_some_and(|p| p.data.threat.is_some())
    })
    .await;
    app.frame();
    let w = &app.window;
    assert!(w.get_has_threat_detail());
    // The risk is explained, not just stated.
    assert!(!w.get_td_inherent().is_empty());
    let why: Vec<String> = slint::Model::iter(&w.get_td_why())
        .map(|s| s.to_string())
        .collect();
    assert!(why.iter().any(|l| l.contains('x')), "{why:?}");
    // The decision fields are here too, not only on the Risks page:
    // set the treatment from this inspector.
    assert!(slint::Model::row_count(&w.get_treatment_options()) > 1);
    w.invoke_set_treatment(1);
    app.until("treatment", |vm| {
        vm.project
            .as_ref()
            .and_then(|p| p.data.threat.as_ref())
            .is_some_and(|t| t.treatment == Some(0))
    })
    .await;
    app.frame();
    assert_eq!(app.window.get_td_treatment(), 1);
    save(&app.frame(), "panels-after-threat");
    drop(app);
    server.stop().await;
}

#[tokio::test(flavor = "current_thread")]
async fn the_library_opens_in_the_right_slot_next_to_the_canvas() {
    let server = TestServer::start().await;
    let app = app_with_sample(&server).await;
    show(&app, Page::Trees).await;

    // The Library button opens it in place of the inspector.
    app.window.invoke_library_opened();
    app.window.set_library_open(true);
    app.frame();
    app.frame();
    assert!(app.window.get_library_open());
    assert!(slint::Model::row_count(&app.window.get_library_rows()) >= 10);
    save(&app.frame(), "panels-after-library");

    // It is not modal: a node on the canvas can still be clicked, and the
    // library stays open beside it.
    let (x, y) = node_position(&app, "Exploit");
    assert!(x < 1360.0 - 340.0, "the node is left of the slot");
    app.headless.click(x, y);
    app.until("node selected", |vm| {
        vm.project.as_ref().is_some_and(|p| p.inspector.is_some())
    })
    .await;
    app.frame();
    assert!(app.window.get_library_open());
    save(&app.frame(), "panels-after-library-node");

    // Close gives the slot back to the inspector.
    app.window.set_library_open(false);
    app.frame();
    assert!(app.window.get_has_inspector());
    drop(app);
    server.stop().await;
}
