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
    save(&bob.frame(), "09-read-only-second-user");

    // Alice rates a step; Bob sees the new risk without doing anything.
    let (x, y) = node_position(&alice, "Exploit");
    alice.headless.click(x, y);
    alice
        .until("selected", |vm| {
            vm.project.as_ref().is_some_and(|p| p.inspector.is_some())
        })
        .await;
    alice.window.invoke_set_rating(1);

    // Bob's window does not change by itself. It offers the newer version.
    bob.until("bob is offered the newer version", |vm| {
        vm.project.as_ref().is_some_and(|p| p.newer_version)
    })
    .await;
    bob.controller.apply_latest();
    assert!(bob.window.get_newer_version());
    assert!(
        bob.window.get_band_text().contains("newer version"),
        "{}",
        bob.window.get_band_text()
    );
    assert_eq!(
        bob.window
            .get_threats()
            .row_data(0)
            .expect("row")
            .inherent
            .as_str(),
        before,
        "the version he opened stays as it was until he reloads"
    );
    save(&bob.frame(), "09b-newer-version");

    // Reload shows Alice's saved work.
    bob.window.invoke_reload();
    bob.until("bob sees the new risk", |_| {
        bob.controller.apply_latest();
        let now = bob.window.get_threats().row_data(0).expect("row").inherent;
        now.as_str() != before
    })
    .await;
    assert!(!bob.window.get_newer_version());

    drop(bob);
    drop(alice);
    server.stop().await;
}

/// A screen reader cannot read a drawing. The window therefore offers one
/// accessible element per node on screen, each with a sentence, and keeps
/// the canvas's own label on the selected node (job 051, gap G14).
#[tokio::test(flavor = "current_thread")]
async fn a_screen_reader_can_read_the_tree_node_by_node() {
    use slint::Model;
    let server = TestServer::start().await;
    let app = app_with_sample(&server, "alice").await;
    app.controller.apply_latest();

    // After "fit" every node is on screen, and each one is described.
    let nodes = app.window.get_access_nodes();
    let total = app
        .session
        .view()
        .project
        .as_ref()
        .expect("project")
        .scene()
        .expect("scene")
        .len();
    assert_eq!(nodes.row_count(), total, "one entry per visible node");
    let labels: Vec<String> = nodes.iter().map(|n| n.label.to_string()).collect();
    assert!(
        labels.iter().any(|l| l.contains("OR gate with")),
        "{labels:?}"
    );
    assert!(labels.iter().any(|l| l.contains("Step.")), "{labels:?}");
    assert!(nodes.iter().all(|n| !n.selected));
    assert!(nodes.iter().all(|n| n.w > 0.0 && n.h > 0.0));
    // Positions are 1..=count, for "item 3 of 12".
    let indexes: Vec<i32> = nodes.iter().map(|n| n.position).collect();
    assert_eq!(
        indexes,
        (1..=i32::try_from(total).expect("fits")).collect::<Vec<_>>()
    );
    assert!(
        nodes
            .iter()
            .all(|n| usize::try_from(n.count).ok() == Some(total))
    );

    // A click in the empty top-left corner of the canvas gives it the
    // keyboard (the next test proves Tab gets there on its own). Its first
    // arrow key selects the root, and the canvas label and the entry for
    // the root both say so.
    let corner = (
        app.window.get_canvas_x() + 4.0,
        app.window.get_canvas_y() + 4.0,
    );
    app.headless.drag(corner, corner);
    let right = char::from(slint::platform::Key::RightArrow).to_string();
    app.headless.key(right.clone());
    app.until("root selected", |vm| {
        vm.project
            .as_ref()
            .is_some_and(|p| p.selected_node.is_some())
    })
    .await;
    app.controller.apply_latest();
    let summary = app.window.get_canvas_summary().to_string();
    assert!(
        summary.contains("OR gate with") && summary.contains("Selected."),
        "{summary}"
    );
    assert!(summary.contains(&format!("{total} nodes")), "{summary}");
    let selected: Vec<_> = app
        .window
        .get_access_nodes()
        .iter()
        .filter(|n| n.selected)
        .collect();
    assert_eq!(selected.len(), 1);
    assert!(
        selected[0].label.ends_with("Selected."),
        "{}",
        selected[0].label
    );

    // The next arrow key moves to a child, and the label follows.
    app.headless.key(right);
    app.until("child selected", |vm| {
        vm.project
            .as_ref()
            .and_then(|p| p.inspector.as_ref())
            .is_some_and(|i| i.kind != "OR gate")
    })
    .await;
    app.controller.apply_latest();
    let summary = app.window.get_canvas_summary().to_string();
    assert!(
        summary.contains("Child 2 of 3 under") && summary.contains("Selected."),
        "{summary}"
    );

    // Zoomed far in, only what is on screen is offered.
    for _ in 0..6 {
        app.headless.scroll(600.0, 400.0, 120.0);
    }
    app.controller.apply_latest();
    let close = app.window.get_access_nodes().row_count();
    assert!(close < total, "{close} of {total} after zooming in");

    drop(app);
    server.stop().await;
}

/// A keyboard-only user, and a screen reader, reach the canvas with Tab and
/// then move between nodes with the arrow keys (gap G14).
#[tokio::test(flavor = "current_thread")]
async fn the_canvas_can_be_reached_with_the_tab_key_alone() {
    let server = TestServer::start().await;
    let app = app_with_sample(&server, "alice").await;
    let tab = char::from(slint::platform::Key::Tab).to_string();
    let right = char::from(slint::platform::Key::RightArrow).to_string();
    let mut reached = None;
    for presses in 1..=80 {
        app.headless.key(tab.clone());
        // Harmless on any other control; on the canvas it selects the root.
        app.headless.key(right.clone());
        app.controller.apply_latest();
        if app
            .session
            .view()
            .project
            .as_ref()
            .is_some_and(|p| p.selected_node.is_some())
        {
            reached = Some(presses);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    assert!(
        reached.is_some(),
        "Tab never reached the canvas in 80 presses"
    );
    drop(app);
    server.stop().await;
}

/// The screen reader's default action on a node (Enter, or "activate")
/// selects it, like a click does.
#[tokio::test(flavor = "current_thread")]
async fn activating_a_node_entry_selects_that_node() {
    use slint::Model;
    let server = TestServer::start().await;
    let app = app_with_sample(&server, "alice").await;
    app.controller.apply_latest();
    // Pick the last entry, a step far from the root.
    let nodes = app.window.get_access_nodes();
    let last = nodes.row_data(nodes.row_count() - 1).expect("an entry");
    assert!(!last.selected);
    app.window.invoke_canvas_node_activated(last.node);
    app.until("selected", |vm| {
        vm.project.as_ref().is_some_and(|p| {
            p.selected_node.map(|n| i32::try_from(n.index()).ok()) == Some(Some(last.node))
        })
    })
    .await;
    app.controller.apply_latest();
    let after: Vec<_> = app
        .window
        .get_access_nodes()
        .iter()
        .filter(|n| n.selected)
        .collect();
    assert_eq!(after.len(), 1);
    assert_eq!(after[0].node, last.node);
    assert!(after[0].label.ends_with("Selected."));
    drop(app);
    server.stop().await;
}

#[tokio::test(flavor = "current_thread")]
async fn the_library_panel_searches_and_copies_an_item_into_the_project() {
    let server = TestServer::start().await;
    let app = app_with_sample(&server, "alice").await;
    let trees = |app: &App| {
        app.session
            .view()
            .project
            .as_ref()
            .map_or(0, |p| p.data.trees.len())
    };
    let before = trees(&app);

    // Opening the panel lists the whole demo pack; typing narrows it.
    app.window.invoke_library_opened();
    app.frame();
    let all = app.window.get_library_rows().row_count();
    assert!(all >= 10, "{all}");
    app.window.invoke_library_query("flood".into());
    let found = app.window.get_library_rows().row_count();
    assert!(found >= 1 && found < all, "{found} of {all}");
    assert_eq!(app.window.get_library_selected(), -1);

    // Picking shows the preview with the license.
    app.window.invoke_library_pick(0);
    assert!(app.window.get_library_preview().contains("CC0-1.0"));
    assert!(app.window.get_library_selected() >= 0);

    // The selection on the canvas is the root gate or nothing: a step cannot
    // be replaced until one is selected.
    assert!(!app.window.get_library_can_leaf());

    // As a new tree (mode 0), without ratings.
    app.window.invoke_library_insert(0, false, false);
    app.until("tree copied", |vm| {
        vm.project
            .as_ref()
            .is_some_and(|p| p.data.trees.len() == before + 1 && p.sync == SyncView::Synced)
    })
    .await;
    assert_eq!(trees(&app), before + 1);

    // One undo takes the copy away.
    app.window.invoke_undo();
    app.until("undone", |vm| {
        vm.project
            .as_ref()
            .is_some_and(|p| p.data.trees.len() == before)
    })
    .await;

    // With ratings: the steps come marked, and the inspector says so until a
    // person confirms the rating.
    app.window
        .invoke_library_query("deny service can bus".into());
    app.window.invoke_library_pick(0);
    app.window.invoke_library_insert(0, true, false);
    app.until("tree with ratings", |vm| {
        vm.project.as_ref().is_some_and(|p| {
            p.data.trees.len() == before + 1 && p.sync == SyncView::Synced && p.tree.is_some()
        })
    })
    .await;
    app.frame();
    app.frame();
    let (x, y) = node_position(&app, "Flood the bus");
    app.headless.click(x, y);
    app.until("step selected", |vm| {
        vm.project.as_ref().is_some_and(|p| p.inspector.is_some())
    })
    .await;
    app.controller.apply_latest();
    assert!(app.window.get_inspector_needs_review());
    app.window.invoke_mark_reviewed();
    app.until("reviewed", |vm| {
        vm.project
            .as_ref()
            .and_then(|p| p.inspector.as_ref())
            .is_some_and(|i| !i.needs_review)
    })
    .await;
    app.window.invoke_undo();
    app.window.invoke_undo();
    app.until("back", |vm| {
        vm.project
            .as_ref()
            .is_some_and(|p| p.data.trees.len() == before)
    })
    .await;

    // A place that the selection does not offer inserts nothing.
    app.window.invoke_library_insert(2, false, false);
    app.frame();
    assert_eq!(trees(&app), before);

    drop(app);
    server.stop().await;
}

#[tokio::test(flavor = "current_thread")]
async fn the_inspector_expands_tree_nodes_at_leaf_and_intermediate_points() {
    let server = TestServer::start().await;
    let app = app_with_sample(&server, "alice").await;

    let initial_len = app
        .session
        .view()
        .project
        .as_ref()
        .expect("project")
        .scene()
        .expect("scene")
        .len();

    // Select a leaf step ("Exploit the debug port on the TCU board").
    let (x, y) = node_position(&app, "Exploit");
    app.headless.click(x, y);
    app.until("step selected", |vm| {
        vm.project.as_ref().is_some_and(|p| p.inspector.is_some())
    })
    .await;
    app.controller.apply_latest();
    assert!(app.window.get_inspector_is_step());
    assert!(!app.window.get_inspector_is_root());

    // 1. Expand at the end of a leaf step: add a child step, which branches
    // the step into an AND gate and moves its rating onto the new child.
    app.window
        .invoke_add_tree_child(0, "Locate exposed JTAG pads".into());
    app.until("leaf branched with child", |vm| {
        vm.project
            .as_ref()
            .and_then(|p| p.scene())
            .is_some_and(|s| s.len() == initial_len + 1)
    })
    .await;
    app.controller.apply_latest();
    assert_eq!(
        app.window.get_inspector_title().as_str(),
        "Locate exposed JTAG pads"
    );

    // 2. Add a sibling step under that newly created gate.
    app.window
        .invoke_add_sibling_node(0, "Attach logic analyzer".into());
    app.until("sibling added", |vm| {
        vm.project
            .as_ref()
            .and_then(|p| p.scene())
            .is_some_and(|s| s.len() == initial_len + 2)
    })
    .await;
    app.controller.apply_latest();
    assert_eq!(
        app.window.get_inspector_title().as_str(),
        "Attach logic analyzer"
    );
    assert!(app.window.get_inspector_can_move_up());

    // 3. Insert an intermediate OR gate above the selected node.
    app.window
        .invoke_insert_parent_gate(0, "Hardware interface access".into());
    app.until("intermediate parent gate inserted", |vm| {
        vm.project
            .as_ref()
            .and_then(|p| p.scene())
            .is_some_and(|s| s.len() == initial_len + 3)
    })
    .await;
    app.controller.apply_latest();
    assert_eq!(
        app.window.get_inspector_title().as_str(),
        "Hardware interface access"
    );
    assert!(!app.window.get_inspector_is_step());
    assert_eq!(app.window.get_inspector_gate_index(), 0);

    // 4. Change the gate logic to SEQ (index 2).
    app.window.invoke_set_tree_node_gate(2);
    app.until("gate changed to SEQ", |vm| {
        vm.project
            .as_ref()
            .and_then(|p| p.inspector.as_ref())
            .is_some_and(|i| i.gate_index == 2)
    })
    .await;
    app.controller.apply_latest();
    assert_eq!(app.window.get_inspector_gate_index(), 2);

    drop(app);
    server.stop().await;
}
