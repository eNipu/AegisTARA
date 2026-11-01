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
