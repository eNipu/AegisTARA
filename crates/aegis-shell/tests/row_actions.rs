//! UI-09.2, UI-09.4 in the real window: the row + starts a draft with the row
//! as the parent context, Duplicate clones, Remove is immediate with the undo
//! toast, and the keyboard-mapped actions move, duplicate, remove and add.

mod common;

use aegis_client::commands::{Command, Edit};
use aegis_client::state::{Mode, ViewModel};
use aegis_client::view::{DraftKind, Page};
use common::{App, TestServer};
use slint::platform::Key;

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

#[tokio::test(flavor = "current_thread")]
async fn the_plus_duplicate_and_remove_row_actions_work() {
    use aegis_model::ComponentId;
    let server = TestServer::start().await;
    let app = editing_app(&server).await;
    let component = ComponentId::new_v7();
    app.window.invoke_show_page(Page::Item.index() as i32);
    app.until("item page", |vm| p(vm).page == Page::Item).await;
    app.send(Command::Edit(Edit::AddComponent {
        id: component,
        name: "Brake ECU".into(),
        parent: None,
    }));
    app.until("component", |vm| p(vm).data.components.len() == 1)
        .await;
    app.frame();

    // + on the component row starts an asset draft with the component context.
    app.window.invoke_add_child_component(0);
    app.until("asset draft", |vm| p(vm).draft == Some(DraftKind::Asset))
        .await;
    app.frame();
    app.frame();
    assert_eq!(app.window.get_draft_kind(), "asset");
    common::type_text(&app, "Bound asset");
    common::press_key(&app, Key::Return, false);
    app.until("asset bound", |vm| {
        p(vm).data.assets.len() == 1 && p(vm).draft.is_none()
    })
    .await;
    let vm = app
        .until("bound", |vm| p(vm).data.assets[0].component == "Brake ECU")
        .await;
    assert_eq!(p(&vm).data.assets[0].name, "Bound asset");

    // Duplicate the component.
    app.window.invoke_show_page(Page::Item.index() as i32);
    app.until("item page", |vm| p(vm).page == Page::Item).await;
    app.window.invoke_duplicate_component(0);
    app.until("duplicated", |vm| p(vm).data.components.len() == 2)
        .await;

    // Remove the first component: immediate, with the undo toast.
    app.window.invoke_remove_row_component(0);
    app.until("removed", |vm| {
        p(vm).data.components.len() == 1 && vm.notices.iter().any(|n| n.action.is_some())
    })
    .await;
}

#[tokio::test(flavor = "current_thread")]
async fn the_keyboard_actions_move_duplicate_remove_and_add() {
    use aegis_model::AssetId;
    let server = TestServer::start().await;
    let app = editing_app(&server).await;
    app.window.invoke_show_page(Page::Assets.index() as i32);
    app.until("assets page", |vm| p(vm).page == Page::Assets)
        .await;
    for (i, name) in ["ECU", "Gateway"].iter().enumerate() {
        app.send(Command::Edit(Edit::AddAsset {
            id: AssetId::new_v7(),
            name: (*name).to_owned(),
            properties: (0..6).collect(),
        }));
        app.until(name, |vm| {
            p(vm).data.assets.len() == i + 1 && p(vm).data.assets.iter().any(|a| a.name == *name)
        })
        .await;
    }
    app.frame();

    // Arrow keys move the selection: the controller's pick is the same path the
    // DataTable keyboard handler calls.
    app.window.invoke_pick_asset(1);
    app.until("row 1", |vm| {
        p(vm)
            .data
            .asset
            .as_ref()
            .is_some_and(|a| a.name == "Gateway")
    })
    .await;

    // Control+D duplicates the selected asset.
    app.window.invoke_duplicate_asset(1);
    app.until("duplicated asset", |vm| p(vm).data.assets.len() == 3)
        .await;

    // Delete removes: immediate, with the undo toast.
    app.window.invoke_remove_row_asset(1);
    app.until("removed asset", |vm| {
        p(vm).data.assets.len() == 2 && vm.notices.iter().any(|n| n.action.is_some())
    })
    .await;

    // Control+Enter adds a child: an asset row starts a threat scenario draft.
    app.window.invoke_add_child_asset(0);
    app.until("threat draft", |vm| p(vm).draft == Some(DraftKind::Threat))
        .await;
}

/// Where the row at `index` of the table on screen is (a point in its name
/// cell), from the position the table reports.
fn row_point(app: &App, index: usize) -> (f32, f32) {
    (
        app.window.get_table_x() + 80.0,
        app.window.get_table_y() + 28.0 + index as f32 * 32.0 + 16.0,
    )
}

/// A click as a hand makes it: the list is a flickable, which holds a press
/// back for 100 ms to see whether it becomes a drag, so the release comes
/// after that.
fn click_row(app: &App, x: f32, y: f32) {
    use slint::ComponentHandle;
    use slint::platform::{PointerEventButton, WindowEvent};
    let position = slint::LogicalPosition::new(x, y);
    let w = app.window.window();
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
}

async fn assets_page(app: &App) {
    app.window.invoke_show_page(Page::Assets.index() as i32);
    app.until("assets page", |vm| p(vm).page == Page::Assets)
        .await;
    app.frame();
    app.frame();
}

async fn add_assets(app: &App, names: &[&str]) {
    use aegis_model::AssetId;
    for (i, name) in names.iter().enumerate() {
        app.send(Command::Edit(Edit::AddAsset {
            id: AssetId::new_v7(),
            name: (*name).to_owned(),
            properties: (0..6).collect(),
        }));
        app.until(name, |vm| p(vm).data.assets.len() == i + 1).await;
    }
}

/// UI-09.4 with real key events: a click gives the table the keyboard, then
/// arrows, Control+D, Delete and Control+Enter act on the selected row.
#[tokio::test(flavor = "current_thread")]
async fn real_keys_drive_the_table() {
    let server = TestServer::start().await;
    let app = editing_app(&server).await;
    assets_page(&app).await;
    add_assets(&app, &["ECU", "Gateway"]).await;
    app.frame();

    let (x, y) = row_point(&app, 0);
    click_row(&app, x, y);
    app.until("row 0 picked", |vm| {
        p(vm).data.asset.as_ref().is_some_and(|a| a.name == "ECU")
    })
    .await;

    common::press_key(&app, Key::DownArrow, false);
    app.until("down", |vm| {
        p(vm)
            .data
            .asset
            .as_ref()
            .is_some_and(|a| a.name == "Gateway")
    })
    .await;
    common::press_key(&app, Key::UpArrow, false);
    app.until("up", |vm| {
        p(vm).data.asset.as_ref().is_some_and(|a| a.name == "ECU")
    })
    .await;
    common::press_key(&app, Key::End, false);
    app.until("end", |vm| {
        p(vm)
            .data
            .asset
            .as_ref()
            .is_some_and(|a| a.name == "Gateway")
    })
    .await;

    common::press_chord(&app, 'd', false);
    let copy = format!("Gateway {}", app.window.get_copy_suffix());
    app.until("duplicated", |vm| {
        p(vm).data.assets.len() == 3 && p(vm).data.assets.iter().any(|a| a.name == copy)
    })
    .await;

    common::press_key(&app, Key::Delete, false);
    app.until("deleted", |vm| {
        p(vm).data.assets.len() == 2 && vm.notices.iter().any(|n| n.action.is_some())
    })
    .await;

    // The removal cleared the selection; keys do nothing until a row is picked.
    common::press_key(&app, Key::Return, true);
    let (x, y) = row_point(&app, 0);
    click_row(&app, x, y);
    app.until("row 0 again", |vm| {
        p(vm).data.asset.as_ref().is_some_and(|a| a.name == "ECU")
    })
    .await;
    common::press_key(&app, Key::Return, true);
    app.until("threat draft", |vm| p(vm).draft == Some(DraftKind::Threat))
        .await;
}

/// UI-09 read-only: a user who may not edit sees no + or ⋯ on a hovered row
/// and the row keys do nothing.
#[tokio::test(flavor = "current_thread")]
async fn a_read_only_user_gets_no_row_actions() {
    let server = TestServer::start().await;
    let app = editing_app(&server).await;
    assets_page(&app).await;
    add_assets(&app, &["ECU", "Gateway"]).await;
    app.frame();

    // The distinct colours in the right end of row 1 while the pointer is on it:
    // the row background alone is one colour, two buttons are many more.
    let colours = |app: &App| {
        let (x, y) = row_point(app, 1);
        click_row(app, x, y);
        let frame = app.frame();
        let right = app.window.get_table_x() + app.window.get_table_w() - 60.0;
        let mut seen = std::collections::HashSet::new();
        for dx in 0..64 {
            for dy in -10..10 {
                seen.insert(frame.pixel(right as u32 + dx, (y as i32 + dy) as u32));
            }
        }
        seen.len()
    };
    let editing = colours(&app);
    assert!(editing > 2, "an editor sees the action buttons ({editing})");

    app.window.invoke_end_edit();
    app.until("viewing", |vm| p(vm).mode != Mode::Editing).await;
    app.frame();
    let viewing = colours(&app);
    assert!(
        viewing <= 2,
        "a read-only user sees no action buttons ({viewing})"
    );

    // Delete and Control+D do nothing, and neither do the callbacks the menu
    // buttons call (a read-only user never sees them, but a stale click must
    // not edit).
    common::press_key(&app, Key::Delete, false);
    common::press_chord(&app, 'd', false);
    app.window.invoke_remove_row_asset(0);
    app.window.invoke_duplicate_asset(0);
    for _ in 0..5 {
        app.frame();
    }
    let vm = app.until("still", |_| true).await;
    assert_eq!(p(&vm).data.assets.len(), 2);
}

/// UI-09.6: the cost of scrolling and moving the selection in a 5,000-row
/// table. The numbers are printed (run with `--nocapture`) and written in
/// `factory/jobs/115-row-actions/build.md`; the assertion is only a loose
/// ceiling so a regression to something unusable fails. This is the software
/// renderer in a headless window, not a GPU frame rate.
#[tokio::test(flavor = "current_thread")]
async fn five_thousand_rows_scroll_and_select() {
    use std::time::Instant;
    const ROWS: usize = 5000;
    let server = TestServer::start().await;
    let app = editing_app(&server).await;
    assets_page(&app).await;
    let setup = Instant::now();
    {
        use aegis_model::AssetId;
        // The command queue holds 256; send 200 at a time and wait for them.
        for start in (0..ROWS).step_by(200) {
            for n in start..start + 200 {
                app.send(Command::Edit(Edit::AddAsset {
                    id: AssetId::new_v7(),
                    name: format!("Asset {n:04}"),
                    properties: (0..6).collect(),
                }));
            }
            app.until("batch", |vm| p(vm).data.assets.len() == start + 200)
                .await;
        }
    }
    let setup = setup.elapsed();
    app.frame();

    // Frames while the wheel scrolls through the whole list.
    let (x, y) = row_point(&app, 3);
    let before = app.frame();
    let mut frames = Vec::new();
    for _ in 0..200 {
        app.headless.scroll(x, y, 300.0);
        let t = Instant::now();
        app.frame();
        frames.push(t.elapsed());
    }
    let after = app.frame();
    common::save(&after, "datatable-5000-scrolled");
    assert_ne!(
        before.rgb, after.rgb,
        "the wheel moved nothing: the measurement would be of a still list"
    );
    frames.sort();
    let scroll_median = frames[frames.len() / 2];
    let scroll_worst = frames[frames.len() - 1];

    // Selection moves: End, then PageUp from the bottom, each through the
    // controller and back, with a frame.
    click_row(&app, x, y);
    app.until("picked", |vm| p(vm).data.asset.is_some()).await;
    let mut picks = Vec::new();
    for _ in 0..50 {
        let t = Instant::now();
        common::press_key(&app, Key::DownArrow, false);
        app.controller.apply_latest();
        app.frame();
        picks.push(t.elapsed());
    }
    let t = Instant::now();
    common::press_key(&app, Key::End, false);
    app.until("last row", |vm| {
        p(vm)
            .data
            .asset
            .as_ref()
            .is_some_and(|a| a.name == "Asset 4999")
    })
    .await;
    app.frame();
    let end = t.elapsed();
    picks.sort();
    println!(
        "5000 rows (software renderer, headless): setup {setup:?}; scroll frame median {scroll_median:?} \
         worst {scroll_worst:?}; select-next + frame median {:?} worst {:?}; End to row 5000 {end:?}",
        picks[picks.len() / 2],
        picks[picks.len() - 1]
    );
    assert!(
        scroll_median < std::time::Duration::from_millis(250),
        "scrolling 5,000 rows is unusably slow: {scroll_median:?}"
    );
}

/// UI-09.4: Return on a row moves the keyboard into the details panel's name
/// field, so the next keys edit the name.
#[tokio::test(flavor = "current_thread")]
async fn return_moves_focus_into_the_inspector() {
    let server = TestServer::start().await;
    let app = editing_app(&server).await;
    assets_page(&app).await;
    add_assets(&app, &["ECU", "Gateway"]).await;
    app.frame();

    // Row 0 picked by a click, then Return (the panel already exists).
    let (x, y) = row_point(&app, 0);
    click_row(&app, x, y);
    app.until("row 0", |vm| {
        p(vm).data.asset.as_ref().is_some_and(|a| a.name == "ECU")
    })
    .await;
    common::press_key(&app, Key::Return, false);
    app.frame();
    common::type_text(&app, "!");
    common::press_key(&app, Key::Return, false);
    app.until("renamed from the keyboard", |vm| {
        p(vm).data.assets.iter().any(|a| a.name.contains('!')) && p(vm).data.assets.len() == 2
    })
    .await;

    // The keyboard is in the name field now, not the table: a click on row 1
    // gives the table back, and Return takes the keyboard into row 1's panel.
    let (x, y) = row_point(&app, 1);
    click_row(&app, x, y);
    app.until("row 1", |vm| {
        p(vm)
            .data
            .asset
            .as_ref()
            .is_some_and(|a| a.name == "Gateway")
    })
    .await;
    common::press_key(&app, Key::Return, false);
    app.frame();
    common::type_text(&app, "?");
    common::press_key(&app, Key::Return, false);
    app.until("second rename", |vm| {
        p(vm)
            .data
            .assets
            .iter()
            .any(|a| a.name.contains('?') && a.name.contains("ateway"))
    })
    .await;
}

/// The distinct colours in the right end of row `index`: the row background
/// alone is one colour, the + and ⋯ buttons are many more.
fn action_colours(app: &App, index: usize) -> usize {
    let (_, y) = row_point(app, index);
    let frame = app.frame();
    let right = app.window.get_table_x() + app.window.get_table_w() - 60.0;
    let mut seen = std::collections::HashSet::new();
    for dx in 0..64 {
        for dy in -10..10 {
            seen.insert(frame.pixel(right as u32 + dx, (y as i32 + dy) as u32));
        }
    }
    seen.len()
}

/// A row that is only a draft has no row actions.
#[tokio::test(flavor = "current_thread")]
async fn a_draft_row_has_no_row_actions() {
    let server = TestServer::start().await;
    let app = editing_app(&server).await;
    assets_page(&app).await;
    add_assets(&app, &["ECU", "Gateway"]).await;
    app.frame();
    let (x, y) = row_point(&app, 1);
    click_row(&app, x, y);
    app.until("row 1", |vm| {
        p(vm)
            .data
            .asset
            .as_ref()
            .is_some_and(|a| a.name == "Gateway")
    })
    .await;
    assert!(action_colours(&app, 1) > 2, "a real row has buttons");

    common::press_chord(&app, 'n', true);
    app.until("draft", |vm| p(vm).draft == Some(DraftKind::Asset))
        .await;
    app.frame();
    app.frame();
    // The draft is the third row and is selected; hover it too.
    let (x, y) = row_point(&app, 2);
    {
        use slint::ComponentHandle;
        app.window
            .window()
            .dispatch_event(slint::platform::WindowEvent::PointerMoved {
                position: slint::LogicalPosition::new(x, y),
            });
    }
    assert!(action_colours(&app, 2) <= 2, "the draft row has no buttons");
}
