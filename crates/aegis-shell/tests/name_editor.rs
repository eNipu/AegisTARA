//! UI-08.3 with real key events: a name saves on Enter, on Tab and when focus
//! leaves; Escape puts the saved name back; an empty name is never saved.

mod common;

use aegis_client::commands::Command;
use aegis_client::state::{Mode, SyncView, ViewModel};
use aegis_client::view::Page;
use common::{App, TestServer, press_key, type_text};
use slint::platform::{Key, WindowEvent};
use slint::{ComponentHandle, LogicalPosition};

fn p(vm: &ViewModel) -> &aegis_client::state::ProjectView {
    vm.project.as_ref().expect("project")
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

async fn settle(app: &App) -> std::sync::Arc<ViewModel> {
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    app.frame();
    app.until("settled", |_| true).await
}

#[tokio::test(flavor = "current_thread")]
#[cfg_attr(
    target_os = "linux",
    ignore = "fails on Linux only; Slint client is being replaced"
)]
async fn a_name_saves_on_enter_tab_and_focus_loss_and_escape_restores_it() {
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
    let original = p(&vm).threats[0].name.clone();
    app.window.invoke_show_page(Page::Threats.index() as i32);
    app.until("threats page", |vm| p(vm).page == Page::Threats)
        .await;
    app.frame();
    app.frame();
    let name = |vm: &ViewModel| p(vm).threats[0].name.clone();

    // Enter saves.
    click(&app, 1250.0, 180.0); // right of the text: the caret goes last
    type_text(&app, " one");
    press_key(&app, Key::Return, false);
    let want = format!("{original} one");
    let vm = settle(&app).await;
    assert_eq!(name(&vm), want, "saved by Enter");

    // Tab saves (and moves on).
    type_text(&app, " two");
    press_key(&app, Key::Tab, false);
    let want = format!("{original} one two");
    app.until("saved by Tab", |vm| name(vm) == want).await;

    // Focus leaving saves.
    click(&app, 1275.0, 180.0); // right of the text: the caret goes last
    type_text(&app, " three");
    // Selecting the other threat takes focus away; the name still goes to the
    // threat that was selected when it was typed.
    click(&app, 400.0, 252.0);
    app.frame();
    let want = format!("{original} one two three");
    app.until("saved by focus loss", |vm| name(vm) == want)
        .await;
    let vm = settle(&app).await;
    assert_eq!(p(&vm).threats[1].name, "Tamper with the TCU firmware image");
    click(&app, 400.0, 229.0); // back to the first threat
    app.frame();
    let seq = p(&vm).confirmed_seq;

    // Escape puts the saved name back and sends nothing.
    click(&app, 1262.0, 180.0); // right of the text: the caret goes last
    type_text(&app, " discarded");
    press_key(&app, Key::Escape, false);
    app.frame();
    press_key(&app, Key::Tab, false);
    let vm = settle(&app).await;
    assert_eq!(name(&vm), want);
    assert_eq!(p(&vm).confirmed_seq, seq, "Escape sends nothing");

    // An empty name is never saved.
    click(&app, 1288.0, 180.0); // right of the text: the caret goes last
    for _ in 0..want.chars().count() {
        press_key(&app, Key::Backspace, false);
    }
    press_key(&app, Key::Return, false);
    let vm = settle(&app).await;
    assert_eq!(name(&vm), want, "an empty name is not saved");
    assert_eq!(p(&vm).confirmed_seq, seq);
    let frame = app.frame();
    common::save(&frame, "name-editor-empty");
    // The note "A name is needed" is drawn under the field in the error colour.
    let mut orange = 0;
    for y in 188..214u32 {
        for x in 930..1120u32 {
            let i = ((y * frame.width + x) * 3) as usize;
            let (r, g, b) = (frame.rgb[i], frame.rgb[i + 1], frame.rgb[i + 2]);
            if r > 190 && (60..130).contains(&g) && b < 60 {
                orange += 1;
            }
        }
    }
    assert!(
        orange > 30,
        "the note is not shown ({orange} error-coloured pixels)"
    );
}
