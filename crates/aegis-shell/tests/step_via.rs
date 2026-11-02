//! A step's "through" item and attack type (job 122, CFG-01, CFG-02) in the
//! real window: the pickers in the step inspector, the via chip on the
//! canvas, "Steps through this" in the bottom slot, Japanese text, a
//! read-only user, and a small window.

mod common;

use aegis_client::commands::{Command, Edit};
use aegis_client::state::{Mode, SyncView};
use aegis_client::view::Page;
use aegis_model::{ComponentId, InterfaceId};
use common::{App, TestServer, node_position, save};
use slint::Model;

const STEP: &str = "Exploit the debug port";

fn seq(app: &App) -> u64 {
    app.session
        .view()
        .project
        .as_ref()
        .map_or(0, |p| p.confirmed_seq)
}

async fn edit(app: &App, edit: Edit) {
    let before = seq(app);
    app.send(Command::Edit(edit));
    app.until("edit saved", |vm| {
        vm.project
            .as_ref()
            .is_some_and(|p| p.sync == SyncView::Synced && p.confirmed_seq > before)
    })
    .await;
}

async fn app_with_item(server: &TestServer) -> (App, ComponentId) {
    let app = App::new(server.session("alice"), 1360, 860);
    app.online().await;
    app.window.invoke_create_project("Through".into());
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
    let (ecu, jtag) = (ComponentId::new_v7(), InterfaceId::new_v7());
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
            id: jtag,
            name: "JTAG".into(),
            component: ecu,
            kind: 0,
        },
    )
    .await;
    select_step(&app).await;
    (app, ecu)
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
}

#[tokio::test(flavor = "current_thread")]
async fn pick_through_and_attack_type_see_the_chip_list_the_steps_and_read_japanese() {
    let server = TestServer::start().await;
    let (mut app, ecu) = app_with_item(&server).await;

    // The pickers list "None", components, then their interfaces indented.
    let options = app.window.get_through_options();
    assert_eq!(options.row_count(), 3);
    assert_eq!(options.row_data(0).expect("row").as_str(), "None");
    assert_eq!(options.row_data(1).expect("row").as_str(), "Telematics ECU");
    assert!(options.row_data(2).expect("row").as_str().ends_with("JTAG"));
    assert!(app.window.get_attack_type_options().row_count() > 1);

    app.window.invoke_set_through(2);
    app.until("via saved", |vm| {
        vm.project
            .as_ref()
            .and_then(|p| p.inspector.as_ref())
            .is_some_and(|i| i.through == Some(1))
    })
    .await;
    app.window.invoke_set_attack_type(3);
    app.until("type saved", |vm| {
        vm.project
            .as_ref()
            .and_then(|p| p.inspector.as_ref())
            .is_some_and(|i| i.attack_type == Some(2))
    })
    .await;
    app.frame();
    assert_eq!(app.window.get_through_index(), 2);

    // The canvas node carries the chip, and says so for screen readers.
    {
        let view = app.session.view();
        let scene = view
            .project
            .as_ref()
            .expect("project")
            .scene()
            .expect("scene");
        let node = (0..scene.len())
            .find(|i| scene.label(*i as u32).starts_with(STEP))
            .expect("node");
        let style = scene.style(node as u32).expect("style");
        assert_eq!(style.via.as_deref(), Some("Telematics ECU / JTAG"));
    }
    save(&app.frame(), "step-via-inspector");

    // The component and the interface list the step in the bottom slot.
    app.send(Command::Select {
        selection: aegis_client::view::Selection::Component(ecu),
        navigate: true,
    });
    app.until("component shown", |vm| {
        vm.project.as_ref().is_some_and(|p| {
            p.data
                .component
                .as_ref()
                .is_some_and(|c| c.steps.len() == 1)
        })
    })
    .await;
    app.controller.apply_latest();
    app.frame();
    assert_eq!(app.window.get_component_steps_count(), 1);
    app.window.set_steps_shown(true);
    app.frame();
    app.frame();
    assert_eq!(app.window.get_steps_rows().row_count(), 1);
    save(&app.frame(), "step-via-list");

    // Enter on the row opens the step on the canvas.
    app.window.invoke_steps_open(0);
    app.until("step opened", |vm| {
        vm.project.as_ref().is_some_and(|p| {
            p.page == Page::Trees
                && p.inspector
                    .as_ref()
                    .is_some_and(|i| i.title.starts_with(STEP))
        })
    })
    .await;

    // 900 x 600: the layout still draws, with the list open.
    app.resize(900, 600);
    app.frame();
    let frame = app.frame();
    assert!(frame.rgb.iter().any(|b| *b != frame.rgb[0]));
    save(&frame, "step-via-900x600");

    // Japanese labels (the language is process-wide, so this goes last).
    if slint::select_bundled_translation("ja").is_ok() {
        assert_eq!(
            app.window.get_steps_heading().as_str(),
            "この要素を経由するステップ"
        );
        let _ = slint::select_bundled_translation("");
    }
    server.stop().await;
}

#[tokio::test(flavor = "current_thread")]
async fn a_read_only_user_cannot_change_either_field() {
    let server = TestServer::start().await;
    let (editor, _) = app_with_item(&server).await;
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
    assert!(!reader.window.get_editing());
    select_step(&reader).await;
    let before = seq(&reader);
    reader.window.invoke_set_attack_type(1);
    reader
        .until("refused", |vm| {
            vm.notices.iter().any(|n| n.text.contains("Edit"))
        })
        .await;
    assert_eq!(seq(&reader), before);
    drop(reader);
    drop(editor);
    server.stop().await;
}
