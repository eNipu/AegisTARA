//! Security settings on components and interfaces (job 123, CFG-03 to CFG-05,
//! CFG-08; D26) in the real window: applying a custom setting with a rules
//! table, dimmed inheritance on interfaces, immediate residual feasibility and
//! risk drop with an undo toast, the shield badge and screen-reader description
//! on capped canvas steps, keyboard undo/redo, Japanese text, a read-only user,
//! and a small window.

mod common;

use aegis_client::commands::{Command, Edit};
use aegis_client::state::{Mode, NoticeAction, SyncView};
use aegis_client::view::{Page, Selection};
use aegis_model::{ComponentId, InterfaceId};
use common::{App, TestServer, node_position, press_chord, press_key, save, type_text};
use slint::Model;
use slint::platform::Key;

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

async fn app_with_step_via_interface(server: &TestServer) -> (App, ComponentId, InterfaceId) {
    let app = App::new(server.session("alice"), 1360, 860);
    app.online().await;
    app.window.invoke_create_project("Settings".into());
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
    // Index 2 in through_options is the JTAG interface; index 3 in
    // attack_type_options is "Physical" (attack type index 2).
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
    (app, ecu, jtag)
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
async fn apply_custom_setting_see_inherited_dimmed_toast_shield_and_japanese() {
    let server = TestServer::start().await;
    let (mut app, ecu, jtag) = app_with_step_via_interface(&server).await;

    // Select the component on the Item Definition page and draft a custom setting.
    app.send(Command::Select {
        selection: Selection::Component(ecu),
        navigate: true,
    });
    app.until("component selected", |vm| {
        vm.project
            .as_ref()
            .is_some_and(|p| p.data.component.as_ref().is_some_and(|c| c.id == ecu))
    })
    .await;
    app.controller.apply_latest();
    app.frame();

    app.window.invoke_begin_component_custom_setting();
    app.frame();
    app.frame();
    assert!(app.window.get_component_drafting_custom());
    press_key(&app, Key::Escape, false);
    app.frame();
    assert!(!app.window.get_component_drafting_custom());

    app.window.invoke_begin_component_custom_setting();
    app.frame();
    app.frame();
    // Cap "Physical" (attack type index 2) at "Very Low" (level 0 -> picker index 1).
    app.window.invoke_set_custom_setting_rule(2, 1);
    type_text(&app, "JTAG disabled in production");
    press_key(&app, Key::Return, false);
    app.until("custom setting applied", |vm| {
        vm.project.as_ref().is_some_and(|p| {
            p.sync == SyncView::Synced
                && p.data
                    .component
                    .as_ref()
                    .is_some_and(|c| c.settings.len() == 1)
        })
    })
    .await;
    app.controller.apply_latest();
    app.frame();

    // The component inspector lists the applied setting.
    let comp_settings = app.window.get_component_settings();
    assert_eq!(comp_settings.row_count(), 1);
    let item = comp_settings.row_data(0).expect("row");
    assert!(item.label.as_str().contains("JTAG disabled in production"));
    assert!(!item.inherited);

    // An undo toast names the capped step count and the risk that moved.
    {
        let view = app.session.view();
        let undo_notice = view
            .notices
            .iter()
            .find(|n| n.action == Some(NoticeAction::Undo))
            .expect("undo toast");
        assert!(
            undo_notice.text.contains("1 step capped")
                && undo_notice.text.contains("1 risk lowered")
                && undo_notice.text.contains("T00"),
            "unexpected toast: {}",
            undo_notice.text
        );
    }

    // Selecting the JTAG interface shows the component's setting dimmed (inherited).
    app.send(Command::Select {
        selection: Selection::Interface(jtag),
        navigate: false,
    });
    app.until("interface selected", |vm| {
        vm.project
            .as_ref()
            .is_some_and(|p| p.data.interface.as_ref().is_some_and(|i| i.id == jtag))
    })
    .await;
    app.controller.apply_latest();
    app.frame();
    let iface_settings = app.window.get_interface_settings();
    assert_eq!(iface_settings.row_count(), 1);
    let inherited = iface_settings.row_data(0).expect("row");
    assert!(
        inherited
            .label
            .as_str()
            .contains("JTAG disabled in production")
    );
    assert!(inherited.inherited);
    save(&app.frame(), "control-settings-interface");

    // On the canvas, the step has a shield badge, accessible description, and explain-why ceiling
    // (deciding ceiling first, ahead of the sample project's manual mitigation).
    select_step(&app).await;
    let ceilings = app.window.get_step_ceilings();
    assert_eq!(ceilings.row_count(), 2);
    assert!(
        ceilings
            .row_data(0)
            .expect("ceiling")
            .as_str()
            .contains("JTAG disabled in production (from Telematics ECU)"),
        "unexpected ceiling[0]: {}",
        ceilings.row_data(0).expect("ceiling")
    );
    assert_eq!(app.window.get_step_residual_text().as_str(), "Very low");
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
        assert!(
            style
                .capped
                .as_deref()
                .is_some_and(|c| c.contains("JTAG disabled in production on Telematics ECU")),
            "unexpected style.capped: {:?}",
            style.capped
        );
        let access_nodes = app.window.get_access_nodes();
        let spoken = (0..access_nodes.row_count())
            .filter_map(|i| access_nodes.row_data(i))
            .find(|n| n.label.as_str().contains(STEP))
            .expect("spoken node");
        assert!(
            spoken
                .label
                .as_str()
                .contains("Capped by JTAG disabled in production on Telematics ECU"),
            "unexpected sentence: {}",
            spoken.label
        );
    }
    save(&app.frame(), "control-settings-step");

    // Keyboard Ctrl+Z undoes the custom setting in one batch; Ctrl+Shift+Z redoes it.
    let before_undo = seq(&app);
    press_chord(&app, 'z', false);
    app.until("undone", |vm| {
        vm.project.as_ref().is_some_and(|p| {
            p.sync == SyncView::Synced
                && p.confirmed_seq > before_undo
                && p.inspector.as_ref().is_some_and(|i| i.ceilings.len() == 1)
        })
    })
    .await;
    let before_redo = seq(&app);
    press_chord(&app, 'z', true);
    app.until("redone", |vm| {
        vm.project.as_ref().is_some_and(|p| {
            p.sync == SyncView::Synced
                && p.confirmed_seq > before_redo
                && p.inspector.as_ref().is_some_and(|i| i.ceilings.len() == 2)
        })
    })
    .await;

    // 900 x 600: the layout still draws.
    app.resize(900, 600);
    app.frame();
    let frame = app.frame();
    assert!(frame.rgb.iter().any(|b| *b != frame.rgb[0]));
    save(&frame, "control-settings-900x600");

    // Japanese labels (process-wide, so tested last).
    if slint::select_bundled_translation("ja").is_ok() {
        assert_eq!(
            app.window.get_security_settings_heading().as_str(),
            "セキュリティ設定"
        );
        assert_eq!(app.window.get_no_ceiling_label().as_str(), "上限なし");
        let _ = slint::select_bundled_translation("");
    }
    server.stop().await;
}

#[tokio::test(flavor = "current_thread")]
async fn a_read_only_user_cannot_apply_or_edit_settings() {
    let server = TestServer::start().await;
    let (editor, ecu, _) = app_with_step_via_interface(&server).await;
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

    reader.send(Command::Select {
        selection: Selection::Component(ecu),
        navigate: true,
    });
    reader
        .until("component selected", |vm| {
            vm.project
                .as_ref()
                .is_some_and(|p| p.data.component.as_ref().is_some_and(|c| c.id == ecu))
        })
        .await;
    reader.controller.apply_latest();
    reader.frame();

    let before = seq(&reader);
    reader.window.invoke_begin_component_custom_setting();
    reader
        .window
        .invoke_create_component_custom_setting("Unauthorized".into());
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
