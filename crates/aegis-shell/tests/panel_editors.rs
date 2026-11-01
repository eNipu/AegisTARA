//! Editors in panels, not over them (UI-11, job 117): the library's Insert
//! button follows the canvas selection; the outline editor, project rename
//! and baseline creation happen on the page, not in a dialog.

mod common;

use aegis_client::state::{Mode, SyncView};
use aegis_client::view::Page;
use common::{App, TestServer, node_position, save};

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
    assert_eq!(app.window.get_library_insert_label(), "Insert as a new tree");

    drop(app);
    server.stop().await;
}
