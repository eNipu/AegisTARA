//! The real window, rendered by the software renderer, driven like a user,
//! against a real server.

mod common;

use aegis_client::state::{Mode, SyncView};
use aegis_client::view::Page;
use common::{App, TestServer, save};
use slint::Model;

#[tokio::test(flavor = "current_thread")]
async fn the_window_goes_from_connecting_to_the_project_list_to_a_tree() {
    let server = TestServer::start().await;
    let app = App::new(server.session("alice"), 1360, 860);

    // Connecting: a card with the product name.
    let first = app.frame();
    save(&first, "01-connecting");

    app.online().await;
    let list = app.frame();
    save(&list, "02-projects-empty");
    assert!(
        list.rgb.iter().any(|b| *b != list.rgb[0]),
        "the project list is drawn"
    );

    // Create a project from the window.
    app.window.invoke_create_project("Telematics unit".into());
    app.until("project open", |vm| vm.project.is_some()).await;
    save(&app.frame(), "03-empty-project-read-only");

    // Take the lock and add the sample.
    app.window.invoke_begin_edit();
    app.until("editing", |vm| {
        vm.project.as_ref().is_some_and(|p| p.mode == Mode::Editing)
    })
    .await;
    app.window.invoke_create_sample();
    app.until("sample visible", |vm| {
        vm.project.as_ref().is_some_and(|p| p.tree.is_some())
    })
    .await;
    save(&app.frame(), "04-sample-tree");
    app.until("saved", |vm| {
        vm.project
            .as_ref()
            .is_some_and(|p| p.sync == SyncView::Synced)
    })
    .await;

    drop(app);
    server.stop().await;
}

/// Builds the sample in a new project and returns the app, in edit mode.
async fn app_with_sample(server: &TestServer, user: &str) -> App {
    let app = App::new(server.session(user), 1360, 860);
    app.online().await;
    app.window.invoke_create_project("Telematics unit".into());
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
    show_trees(&app).await;
    app
}

/// Opens the attack trees page and lets the canvas take its place.
async fn show_trees(app: &App) {
    app.window.invoke_show_page(4);
    app.until("trees page", |vm| {
        vm.project.as_ref().is_some_and(|p| p.page == Page::Trees)
    })
    .await;
    // Two frames: one lays the canvas out, one draws the fitted tree.
    app.frame();
    app.frame();
}

/// The window position (logical pixels) of the node whose label starts with
/// `label`, from the display list the controller drew.
fn node_position(app: &App, label: &str) -> (f32, f32) {
    let view = app.session.view();
    let project = view.project.as_ref().expect("project");
    let scene = project.scene().expect("scene");
    let (position, _) = (0..scene.len())
        .map(|i| (i, scene.label(i as u32)))
        .find(|(_, l)| l.starts_with(label))
        .expect("node");
    // The window reports where the canvas is.
    let idx = scene.node_idx(position as u32).expect("idx");
    let rect = scene.node_rect(idx).expect("rect");
    let centre = rect.center();
    let st = app.controller.viewport_for_tests();
    let screen = st.to_screen(aegis_canvas::Point::new(centre.x, centre.y));
    (
        app.window.get_canvas_x() + screen.x,
        app.window.get_canvas_y() + screen.y,
    )
}

#[tokio::test(flavor = "current_thread")]
async fn clicking_a_node_explains_it_and_the_inspector_edits_the_rating() {
    let server = TestServer::start().await;
    let app = app_with_sample(&server, "alice").await;

    // Click the debug-port step: the inspector shows it.
    let (x, y) = node_position(&app, "Exploit");
    app.headless.click(x, y);
    let view = app
        .until("node selected", |vm| {
            vm.project.as_ref().is_some_and(|p| p.inspector.is_some())
        })
        .await;
    let inspector = view
        .project
        .as_ref()
        .expect("project")
        .inspector
        .clone()
        .expect("inspector");
    assert_eq!(inspector.title, "Exploit the debug port on the TCU board");
    assert_eq!(app.window.get_inspector_title().as_str(), inspector.title);
    assert!(app.window.get_has_inspector());
    save(&app.frame(), "05-node-selected");

    // Rate it "Very low" through the dropdown callback (index 1 after "Not rated").
    let before = app
        .window
        .get_threats()
        .row_data(0)
        .expect("row")
        .inherent
        .to_string();
    app.window.invoke_set_rating(1);
    app.until("rating applied", |vm| {
        vm.project
            .as_ref()
            .is_some_and(|p| p.inspector.as_ref().is_some_and(|i| i.rating == Some(0)))
    })
    .await;
    let after = app
        .window
        .get_threats()
        .row_data(0)
        .expect("row")
        .inherent
        .to_string();
    assert_ne!(before, after, "the risk in the list changed at once");
    assert_eq!(app.window.get_rating_index(), 1);
    app.until("saved", |vm| {
        vm.project
            .as_ref()
            .is_some_and(|p| p.sync == SyncView::Synced)
    })
    .await;
    save(&app.frame(), "06-rating-changed");

    // Undo restores the rating and the risk.
    app.window.invoke_undo();
    app.until("undone", |vm| {
        vm.project
            .as_ref()
            .is_some_and(|p| p.inspector.as_ref().is_some_and(|i| i.rating == Some(3)))
    })
    .await;
    assert_eq!(
        app.window
            .get_threats()
            .row_data(0)
            .expect("row")
            .inherent
            .to_string(),
        before
    );

    drop(app);
    server.stop().await;
}

#[tokio::test(flavor = "current_thread")]
async fn the_canvas_pans_zooms_and_navigates_by_keyboard() {
    let server = TestServer::start().await;
    let app = app_with_sample(&server, "alice").await;
    let zoom_before = app.controller.viewport_for_tests().zoom;

    // The wheel zooms around the pointer; the text shows the percentage.
    app.headless.scroll(600.0, 400.0, 120.0);
    let zoom_in = app.controller.viewport_for_tests().zoom;
    assert!(zoom_in > zoom_before * 1.1, "{zoom_before} -> {zoom_in}");
    assert_eq!(
        app.window.get_zoom_text().as_str(),
        format!("{:.0}%", zoom_in * 100.0)
    );

    // Dragging empty canvas pans.
    let origin = app.controller.viewport_for_tests().origin;
    app.headless.drag((700.0, 700.0), (620.0, 650.0));
    let panned = app.controller.viewport_for_tests().origin;
    assert!(
        panned.x > origin.x && panned.y > origin.y,
        "{origin:?} -> {panned:?}"
    );
    // A drag is not a click: nothing got selected.
    assert!(
        app.session
            .view()
            .project
            .as_ref()
            .expect("project")
            .selected_node
            .is_none()
    );

    // "Fit" brings everything back into view.
    app.window.invoke_fit();
    let fitted = app.controller.viewport_for_tests();
    let bounds = app
        .session
        .view()
        .project
        .as_ref()
        .expect("project")
        .scene()
        .expect("scene")
        .bounds();
    let seen = fitted.world_rect();
    assert!(seen.origin.x <= bounds.origin.x && seen.right() >= bounds.right());

    // Arrow keys walk the tree: first press selects the root, then the
    // right arrow goes to a child, left goes back.
    let right = slint::platform::Key::RightArrow;
    let left = slint::platform::Key::LeftArrow;
    app.headless.key(char::from(right).to_string());
    let view = app
        .until("root selected", |vm| {
            vm.project
                .as_ref()
                .is_some_and(|p| p.selected_node.is_some())
        })
        .await;
    assert_eq!(
        view.project
            .as_ref()
            .expect("project")
            .inspector
            .as_ref()
            .expect("inspector")
            .kind,
        "OR gate"
    );
    app.headless.key(char::from(right).to_string());
    let view = app
        .until("child selected", |vm| {
            vm.project
                .as_ref()
                .and_then(|p| p.inspector.as_ref())
                .is_some_and(|i| i.kind != "OR gate")
        })
        .await;
    let child = view
        .project
        .as_ref()
        .expect("project")
        .inspector
        .as_ref()
        .expect("inspector")
        .title
        .clone();
    assert!(!child.is_empty());
    app.headless.key(char::from(left).to_string());
    app.until("back at the root", |vm| {
        vm.project
            .as_ref()
            .and_then(|p| p.inspector.as_ref())
            .is_some_and(|i| i.kind == "OR gate")
    })
    .await;
    // Escape deselects.
    app.headless
        .key(char::from(slint::platform::Key::Escape).to_string());
    app.until("deselected", |vm| {
        vm.project
            .as_ref()
            .is_some_and(|p| p.selected_node.is_none())
    })
    .await;

    drop(app);
    server.stop().await;
}

#[tokio::test(flavor = "current_thread")]
async fn the_residual_toggle_switches_the_tree_and_the_title() {
    let server = TestServer::start().await;
    let app = app_with_sample(&server, "alice").await;
    let inherent_title = app.window.get_tree_title().to_string();
    assert!(
        inherent_title.contains("before controls"),
        "{inherent_title}"
    );

    app.window.invoke_set_residual(true);
    app.until("residual view", |vm| {
        vm.project.as_ref().is_some_and(|p| p.show_residual)
    })
    .await;
    app.controller.apply_latest();
    assert!(app.window.get_show_residual());
    assert!(
        app.window.get_tree_title().contains("after controls"),
        "{}",
        app.window.get_tree_title()
    );
    save(&app.frame(), "07-residual");

    drop(app);
    server.stop().await;
}

#[tokio::test(flavor = "current_thread")]
async fn rating_all_five_factors_in_the_inspector_gives_an_attack_potential() {
    let server = TestServer::start().await;
    let app = app_with_sample(&server, "alice").await;
    let (x, y) = node_position(&app, "Exploit");
    app.headless.click(x, y);
    app.until("selected", |vm| {
        vm.project.as_ref().is_some_and(|p| p.inspector.is_some())
    })
    .await;
    assert_eq!(app.window.get_factors().row_count(), 5);
    assert!(app.window.get_potential_text().is_empty());

    for i in 0..5 {
        app.window.invoke_set_factor(i, 2);
        let wanted = i + 1;
        app.until("factor applied", |vm| {
            vm.project
                .as_ref()
                .and_then(|p| p.inspector.as_ref())
                .is_some_and(|ins| {
                    ins.factors.iter().filter(|f| f.selected.is_some()).count() as i32 >= wanted
                })
        })
        .await;
    }
    app.controller.apply_latest();
    assert!(
        !app.window.get_potential_text().is_empty(),
        "five factors give a potential"
    );
    assert_eq!(
        app.window.get_factors().row_data(0).expect("row").selected,
        2
    );
    save(&app.frame(), "08-factors");

    drop(app);
    server.stop().await;
}

#[tokio::test(flavor = "current_thread")]
async fn a_second_user_sees_the_project_read_only_and_follows_the_editor() {
    let server = TestServer::start().await;
    let alice = app_with_sample(&server, "alice").await;
    let project = alice
        .session
        .view()
        .project
        .as_ref()
        .expect("project")
        .entry
        .id
        .clone();

    let bob = App::new(server.session("bob"), 1360, 860);
    bob.online().await;
    bob.window.invoke_open_project(project.into());
    bob.until("open", |vm| {
        vm.project.as_ref().is_some_and(|p| p.tree.is_some())
    })
    .await;
    bob.controller.apply_latest();
    assert!(!bob.window.get_editing());
    assert_eq!(bob.window.get_band_kind(), 1, "someone else holds the lock");
    assert!(
        bob.window.get_band_text().contains("alice"),
        "{}",
        bob.window.get_band_text()
    );
    let before = bob
        .window
        .get_threats()
        .row_data(0)
        .expect("row")
        .inherent
        .to_string();
    save(&bob.frame(), "09-observer");

    // Alice rates a step; Bob sees the new risk without doing anything.
    let (x, y) = node_position(&alice, "Exploit");
    alice.headless.click(x, y);
    alice
        .until("selected", |vm| {
            vm.project.as_ref().is_some_and(|p| p.inspector.is_some())
        })
        .await;
    alice.window.invoke_set_rating(1);
    bob.until("bob follows", |_| {
        bob.controller.apply_latest();
        let now = bob.window.get_threats().row_data(0).expect("row").inherent;
        now.as_str() != before
    })
    .await;

    drop(bob);
    drop(alice);
    server.stop().await;
}
