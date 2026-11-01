//! Screenshots for the visual design review, in both themes: the project
//! list, the Assets page with an asset selected, and a dialog
//! (`target/shell-screenshots/design-{light,dark}-*.png`).

mod common;

use aegis_client::state::Mode;
use aegis_client::view::Page;
use common::{App, TestServer, save};

#[tokio::test(flavor = "current_thread")]
async fn the_main_screens_are_saved_in_both_themes() {
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
    app.window.invoke_show_page(Page::Assets.index() as i32);
    app.window.invoke_add_asset("Brake request".into());
    app.until("asset selected", |vm| {
        vm.project.as_ref().is_some_and(|p| p.data.asset.is_some())
    })
    .await;
    for (appearance, theme) in [(1, "light"), (2, "dark")] {
        app.window.set_appearance(appearance);
        save(&app.frame(), &format!("design-{theme}-assets"));
    }
    app.window.invoke_ask(
        "rename".into(),
        "Rename project".into(),
        "Name".into(),
        "Brake ECU".into(),
    );
    for (appearance, theme) in [(1, "light"), (2, "dark")] {
        app.window.set_appearance(appearance);
        save(&app.frame(), &format!("design-{theme}-dialog"));
    }
    app.window.set_prompt_open(false);
    app.window.invoke_close_project();
    app.until("project list", |vm| vm.project.is_none()).await;
    for (appearance, theme) in [(1, "light"), (2, "dark")] {
        app.window.set_appearance(appearance);
        save(&app.frame(), &format!("design-{theme}-projects"));
    }
    server.stop().await;
}
