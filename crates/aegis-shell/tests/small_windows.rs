//! The main pages in small windows (job 056). The window refuses to be
//! smaller than 980 x 600 (`min-width`, `min-height` in `app.slint`), so that
//! is the smallest size a user can reach: its screenshots are for the UI
//! review (`target/shell-screenshots/s*.png`). Smaller sizes cannot happen on
//! a desktop but must still draw without a panic (a platform that ignores the
//! minimum, a remote desktop, a test).

mod common;

use aegis_client::state::{Mode, SyncView};
use aegis_client::view::Page;
use common::{App, TestServer, save};

const PAGES: [(Page, &str); 9] = [
    (Page::Overview, "overview"),
    (Page::Item, "item"),
    (Page::Assets, "assets"),
    (Page::Threats, "threats"),
    (Page::Trees, "trees"),
    (Page::Risks, "risks"),
    (Page::Versions, "versions"),
    (Page::Reports, "reports"),
    (Page::Admin, "admin"),
];

#[tokio::test(flavor = "current_thread")]
async fn every_main_page_draws_in_small_windows() {
    let server = TestServer::start().await;
    for (w, h) in [(980u32, 600u32), (1100, 760), (800, 600), (640, 480)] {
        let mut app = App::new(server.admin_session("mira"), 1360, 860);
        app.online().await;
        app.window
            .invoke_create_project(format!("Small {w}").into());
        app.until("project", |vm| vm.project.is_some()).await;
        app.window.invoke_begin_edit();
        app.until("editing", |vm| {
            vm.project.as_ref().is_some_and(|p| p.mode == Mode::Editing)
        })
        .await;
        app.window.invoke_create_sample();
        app.until("sample saved", |vm| {
            vm.project
                .as_ref()
                .is_some_and(|p| p.tree.is_some() && p.sync == SyncView::Synced)
        })
        .await;
        app.resize(w, h);
        for (page, name) in PAGES {
            app.window.invoke_show_page(page.index() as i32);
            // The page is the session's state: wait until it has taken it,
            // then two frames (the first lays the canvas out).
            app.until(name, |vm| {
                vm.project.as_ref().is_some_and(|p| p.page == page)
            })
            .await;
            app.frame();
            let frame = app.frame();
            assert_eq!((frame.width, frame.height), (w, h));
            assert!(
                frame.rgb.iter().any(|b| *b != frame.rgb[0]),
                "{name} at {w}x{h} is blank"
            );
            save(&frame, &format!("s{w}-{name}"));
        }
    }
    server.stop().await;
}
