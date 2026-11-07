//! ARC-05 in the real window: the "Description and scope" page sits in the
//! item definition group and saves both texts. The frame is saved for the UI
//! review (`target/shell-screenshots/item-text.png`).

mod common;

use aegis_client::state::{Mode, ProjectView, ViewModel};
use aegis_client::view::Page;
use aegis_model::ElementRef;
use common::{App, TestServer, save};
use slint::Model;

fn p(vm: &ViewModel) -> &ProjectView {
    vm.project.as_ref().expect("project")
}

fn text(vm: &ViewModel, target: ElementRef) -> String {
    p(vm)
        .descriptions
        .iter()
        .find(|d| d.target == target)
        .map_or_else(String::new, |d| d.text.clone())
}

#[tokio::test(flavor = "current_thread")]
async fn description_and_scope_page_saves_both_texts() {
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
    app.window.invoke_show_page(Page::Scope.index() as i32);
    app.until("scope page", |vm| {
        p(vm).page == Page::Scope && p(vm).descriptions.len() == 2
    })
    .await;
    let kinds: Vec<_> = app
        .window
        .get_descriptions()
        .iter()
        .map(|d| d.kind.to_string())
        .collect();
    assert_eq!(kinds, ["item-description", "item-scope"]);

    app.window
        .invoke_set_description(0, "A brake-by-wire ECU with two buses.".into());
    app.window
        .invoke_set_description(1, "In scope: the ECU. Out of scope: the vehicle.".into());
    app.until("saved", |vm| {
        text(vm, ElementRef::ItemDescription) == "A brake-by-wire ECU with two buses."
            && text(vm, ElementRef::ItemScope) == "In scope: the ECU. Out of scope: the vehicle."
    })
    .await;
    app.frame();
    save(&app.frame(), "item-text");
}
