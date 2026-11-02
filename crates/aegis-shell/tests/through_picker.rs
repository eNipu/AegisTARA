//! The Through and Attack type pickers (job 122b, CFG-01, CFG-02): reached
//! by Tab, opened from the keyboard, narrowed by typing, picked with Return,
//! closed with Escape; still usable with 200 components in a small window.

mod common;

use aegis_client::commands::{Command, Edit};
use aegis_client::state::{Mode, SyncView};
use aegis_client::view::Page;
use aegis_model::{ComponentId, InterfaceId};
use common::{App, TestServer, node_position, press_key, save, type_text};
use slint::Model;
use slint::platform::Key;

const STEP: &str = "Exploit the debug port";

async fn edit(app: &App, edit: Edit) {
    let before = app
        .session
        .view()
        .project
        .as_ref()
        .map_or(0, |p| p.confirmed_seq);
    app.send(Command::Edit(edit));
    app.until("edit saved", |vm| {
        vm.project
            .as_ref()
            .is_some_and(|p| p.sync == SyncView::Synced && p.confirmed_seq > before)
    })
    .await;
}

async fn app_with_step(server: &TestServer, extra_components: usize) -> App {
    let app = App::new(server.session("alice"), 1360, 860);
    app.online().await;
    app.window.invoke_create_project("Picker".into());
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
    let ecu = ComponentId::new_v7();
    edit(
        &app,
        Edit::AddComponent {
            id: ecu,
            name: "Telematics ECU".into(),
            parent: None,
        },
    )
    .await;
    edit(
        &app,
        Edit::AddInterface {
            id: InterfaceId::new_v7(),
            name: "JTAG".into(),
            component: ecu,
            kind: 0,
        },
    )
    .await;
    for chunk in (0..extra_components).collect::<Vec<_>>().chunks(40) {
        let items = chunk
            .iter()
            .map(|i| Edit::AddComponent {
                id: ComponentId::new_v7(),
                name: format!("Unit {i:03}"),
                parent: None,
            })
            .collect();
        edit(&app, Edit::Batch(items)).await;
    }
    select_step(&app).await;
    app
}

async fn select_step(app: &App) {
    app.window.invoke_show_page(Page::Trees.index() as i32);
    app.until("trees page", |vm| {
        vm.project.as_ref().is_some_and(|p| p.page == Page::Trees)
    })
    .await;
    app.frame();
    app.frame();
    let (x, y) = node_position(app, STEP);
    app.headless.click(x, y);
    app.until("step selected", |vm| {
        vm.project
            .as_ref()
            .is_some_and(|p| p.inspector.as_ref().is_some_and(|i| i.is_step))
    })
    .await;
    app.frame();
    app.frame();
}

/// Presses Tab until `focused` says the field has the keyboard.
fn tab_to(app: &App, focused: impl Fn(&App) -> bool) {
    for _ in 0..60 {
        if focused(app) {
            return;
        }
        press_key(app, Key::Tab, false);
        app.frame();
    }
    panic!("Tab never reached the field");
}

fn inspector(app: &App) -> aegis_client::view::Inspector {
    app.session
        .view()
        .project
        .as_ref()
        .and_then(|p| p.inspector.clone())
        .expect("inspector")
}

#[tokio::test(flavor = "current_thread")]
async fn both_pickers_work_from_the_keyboard_alone() {
    let server = TestServer::start().await;
    let app = app_with_step(&server, 0).await;

    // Through: Tab to it, open, type, pick the first match.
    tab_to(&app, |a| a.window.get_through_focused());
    press_key(&app, Key::Return, false);
    app.frame();
    assert!(app.window.get_through_open());
    assert_eq!(
        app.window.get_through_shown().row_count(),
        3,
        "None and two items"
    );
    type_text(&app, "jtag");
    app.frame();
    let shown = app.window.get_through_shown();
    assert_eq!(shown.row_count(), 2, "None and the match");
    assert_eq!(
        shown.row_data(1).expect("row").as_str(),
        "Telematics ECU / JTAG"
    );
    save(&app.frame(), "through-picker-filtered");
    press_key(&app, Key::Return, false);
    app.until("through saved", |vm| {
        vm.project
            .as_ref()
            .and_then(|p| p.inspector.as_ref())
            .is_some_and(|i| i.through == Some(1))
    })
    .await;
    app.frame();
    assert!(!app.window.get_through_open());

    // Attack type: Tab on, open, type, pick.
    tab_to(&app, |a| a.window.get_attack_type_focused());
    press_key(&app, Key::Return, false);
    app.frame();
    assert!(app.window.get_attack_type_open());
    type_text(&app, "FIRM");
    app.frame();
    assert_eq!(app.window.get_attack_type_shown().row_count(), 2);
    press_key(&app, Key::Return, false);
    app.until("type saved", |vm| {
        vm.project
            .as_ref()
            .and_then(|p| p.inspector.as_ref())
            .is_some_and(|i| i.attack_type == Some(0))
    })
    .await;

    // Escape closes and changes nothing; Down then Return picks the next row.
    tab_to(&app, |a| a.window.get_through_focused());
    press_key(&app, Key::Return, false);
    app.frame();
    press_key(&app, Key::Escape, false);
    app.frame();
    assert!(!app.window.get_through_open());
    assert_eq!(inspector(&app).through, Some(1));
    press_key(&app, Key::Space, false);
    app.frame();
    assert!(app.window.get_through_open());
    press_key(&app, Key::UpArrow, false);
    press_key(&app, Key::UpArrow, false);
    press_key(&app, Key::Return, false);
    app.until("cleared", |vm| {
        vm.project
            .as_ref()
            .and_then(|p| p.inspector.as_ref())
            .is_some_and(|i| i.through.is_none())
    })
    .await;
    server.stop().await;
}

#[tokio::test(flavor = "current_thread")]
async fn a_read_only_user_cannot_open_the_pickers() {
    let server = TestServer::start().await;
    let editor = app_with_step(&server, 0).await;
    let id = editor
        .session
        .view()
        .project
        .as_ref()
        .expect("project")
        .entry
        .id
        .clone();
    let reader = App::new(server.session("bob"), 1360, 860);
    reader.online().await;
    reader.window.invoke_open_project(id.into());
    reader.until("open", |vm| vm.project.is_some()).await;
    select_step(&reader).await;
    assert!(!reader.window.get_editing());
    for _ in 0..60 {
        press_key(&reader, Key::Tab, false);
        press_key(&reader, Key::Return, false);
    }
    reader.frame();
    assert!(!reader.window.get_through_open() && !reader.window.get_attack_type_open());
    drop(reader);
    drop(editor);
    server.stop().await;
}

#[tokio::test(flavor = "current_thread")]
async fn two_hundred_components_stay_usable_in_a_small_window() {
    let server = TestServer::start().await;
    let mut app = app_with_step(&server, 200).await;
    app.resize(900, 600);
    app.frame();
    app.frame();
    let all = app.window.get_through_options().row_count();
    assert_eq!(all, 1 + 201 + 1, "None, 201 components and the interface");
    tab_to(&app, |a| a.window.get_through_focused());
    press_key(&app, Key::Return, false);
    app.frame();
    assert!(app.window.get_through_open());
    assert_eq!(app.window.get_through_shown().row_count(), all);
    let frame = app.frame();
    assert!(frame.rgb.iter().any(|b| *b != frame.rgb[0]));
    save(&frame, "through-picker-200-open");
    type_text(&app, "unit 17");
    app.frame();
    // Unit 170 to 179, and None.
    assert_eq!(app.window.get_through_shown().row_count(), 11);
    for _ in 0..20 {
        press_key(&app, Key::DownArrow, false);
    }
    app.frame();
    save(&app.frame(), "through-picker-200-filtered");
    press_key(&app, Key::Return, false);
    app.until("picked", |vm| {
        vm.project
            .as_ref()
            .and_then(|p| p.inspector.as_ref())
            .is_some_and(|i| i.through.is_some())
    })
    .await;
    server.stop().await;
}
