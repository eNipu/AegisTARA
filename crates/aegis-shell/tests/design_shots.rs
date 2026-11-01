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

fn click(app: &App, x: f32, y: f32) {
    use slint::ComponentHandle;
    use slint::platform::{PointerEventButton, WindowEvent};
    let position = slint::LogicalPosition::new(x, y);
    let w = app.window.window();
    w.dispatch_event(WindowEvent::PointerMoved { position });
    w.dispatch_event(WindowEvent::PointerPressed {
        position,
        button: PointerEventButton::Left,
    });
    // A list holds a press back for 100 ms to see whether it becomes a drag.
    std::thread::sleep(std::time::Duration::from_millis(150));
    app.headless.tick();
    w.dispatch_event(WindowEvent::PointerReleased {
        position,
        button: PointerEventButton::Left,
    });
}

/// The drop-down is the app's own (ui/widgets.slint): a click opens its list,
/// a click on an entry picks it, and the arrow keys change it while closed.
#[tokio::test(flavor = "current_thread")]
async fn the_drop_down_opens_and_picks_with_mouse_and_keys() {
    let server = TestServer::start().await;
    let app = App::new(server.admin_session("root"), 1360, 860);
    app.online().await;
    app.window.set_appearance(2);
    app.frame();
    assert_eq!(app.window.get_new_project_methodology(), 0);

    // The assessment method box of the new-project row.
    click(&app, 747.0, 627.0);
    save(&app.frame(), "design-dark-dropdown-open");
    // The second entry of the list under the box.
    click(&app, 747.0, 692.0);
    save(&app.frame(), "design-dark-dropdown-picked");
    assert_eq!(
        app.window.get_new_project_methodology(),
        1,
        "picked by a click"
    );

    // The box kept the focus: Down picks the next one, Up goes back.
    common::press_key(&app, slint::platform::Key::DownArrow, false);
    app.frame();
    assert_eq!(app.window.get_new_project_methodology(), 2, "Down");
    common::press_key(&app, slint::platform::Key::UpArrow, false);
    app.frame();
    assert_eq!(app.window.get_new_project_methodology(), 1, "Up");
    server.stop().await;
}
