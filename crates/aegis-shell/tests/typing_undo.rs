//! UI-10.4 with real key events: the platform undo shortcut undoes the last
//! project change, unless a saved field has focus and differs from its saved
//! value, when it undoes that field's own typing and leaves the project alone.

mod common;

use aegis_client::commands::{Command, Edit};
use aegis_client::state::{Mode, SyncView};
use aegis_client::view::Page;
use common::{App, TestServer};
use slint::platform::{Key, WindowEvent};
use slint::{ComponentHandle, LogicalPosition};

/// The platform's undo shortcut as key events. Slint's `Control` is the
/// Command key on macOS and Ctrl elsewhere, like the shortcut table.
fn press_undo(app: &App) {
    let modifier: slint::SharedString = Key::Control.into();
    let w = app.window.window();
    w.dispatch_event(WindowEvent::KeyPressed {
        text: modifier.clone(),
    });
    w.dispatch_event(WindowEvent::KeyPressed { text: "z".into() });
    w.dispatch_event(WindowEvent::KeyReleased { text: "z".into() });
    w.dispatch_event(WindowEvent::KeyReleased { text: modifier });
}

fn click(app: &App, x: f32, y: f32) {
    let w = app.window.window();
    let position = LogicalPosition::new(x, y);
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

fn type_text(app: &App, text: &str) {
    for c in text.chars() {
        let s: slint::SharedString = c.to_string().into();
        app.window
            .window()
            .dispatch_event(WindowEvent::KeyPressed { text: s.clone() });
        app.window
            .window()
            .dispatch_event(WindowEvent::KeyReleased { text: s });
    }
}

#[tokio::test(flavor = "current_thread")]
async fn the_undo_key_undoes_typing_in_a_dirty_field_and_the_project_otherwise() {
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
    for name in ["Renamed once", "Renamed twice"] {
        app.send(Command::Edit(Edit::RenameThreat {
            threat: threat.id,
            name: name.into(),
        }));
        app.until(name, |vm| {
            vm.project
                .as_ref()
                .is_some_and(|p| p.threats[0].name == name)
        })
        .await;
    }
    app.frame();
    assert!(app.window.get_can_undo());

    // No field has focus: the shortcut undoes the last project change.
    press_undo(&app);
    app.until("project undone", |vm| {
        vm.project
            .as_ref()
            .is_some_and(|p| p.threats[0].name == "Renamed once")
    })
    .await;

    // Focus the rename field (top of the inspector) and type into it. Its text
    // now differs from the saved name: the shortcut is for the field.
    app.frame();
    click(&app, 1090.0, 182.0);
    type_text(&app, "xyz");
    app.frame();
    press_undo(&app);
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    app.frame();
    let vm = app.until("settled", |_| true).await;
    assert_eq!(
        vm.project.as_ref().expect("project").threats[0].name,
        "Renamed once",
        "the project change was not undone while the field was dirty"
    );
}

/// UI-10.5: undo opens the page of the element and scrolls its row into view,
/// even in a long list on a page that was not on screen.
#[tokio::test(flavor = "current_thread")]
async fn undo_scrolls_the_restored_row_into_view_in_a_long_list() {
    use aegis_model::{AssetId, ThreatId};
    let server = TestServer::start().await;
    let app = App::new(server.admin_session("root"), 1360, 860);
    app.online().await;
    app.window.invoke_create_project("Long list".into());
    app.until("open", |vm| vm.project.is_some()).await;
    app.window.invoke_begin_edit();
    app.until("editing", |vm| {
        vm.project.as_ref().is_some_and(|p| p.mode == Mode::Editing)
    })
    .await;
    let asset = AssetId::new_v7();
    app.send(Command::Edit(Edit::AddAsset {
        id: asset,
        name: "ECU".into(),
        properties: (0..6).collect(),
    }));
    let mut last = ThreatId::new_v7();
    for n in 0..40 {
        last = ThreatId::new_v7();
        app.send(Command::Edit(Edit::AddThreat {
            id: last,
            name: format!("Threat {n:02}"),
            asset,
            category: 0,
        }));
    }
    app.until("40 threats", |vm| {
        vm.project.as_ref().is_some_and(|p| p.threats.len() == 40)
    })
    .await;
    app.send(Command::Edit(Edit::RenameThreat {
        threat: last,
        name: "Renamed last".into(),
    }));
    app.until("renamed", |vm| {
        vm.project
            .as_ref()
            .is_some_and(|p| p.threats.iter().any(|t| t.name == "Renamed last"))
    })
    .await;

    // Away from the Threats page, then undo.
    app.window.invoke_show_page(Page::Overview.index() as i32);
    app.frame();
    app.window.invoke_undo();
    app.until("on the threats page", |vm| {
        vm.project.as_ref().is_some_and(|p| {
            p.page == Page::Threats && p.threats.iter().any(|t| t.name == "Threat 39")
        })
    })
    .await;
    app.frame();
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    app.frame();
    let frame = app.frame();
    common::save(&frame, "undo-scroll-long-list");

    // The selected row is drawn in the selection colour (Theme.selection, light
    // or dark); some of it is on screen inside the list (left of the
    // inspector, below the header).
    let h = frame.height.min(830) - 330;
    let count = frame.count_color(240, 330, 640, h, [0xF9, 0xDD, 0xD4])
        + frame.count_color(240, 330, 640, h, [0x3A, 0x1E, 0x16]);
    assert!(
        count > 2000,
        "the selected row is not in view ({count} selection-coloured pixels)"
    );
}
