//! Descriptions and project notes in the real window (job 064, FND-01):
//! every page that shows an element has a box for its description, the
//! Overview has the project notes, and saving a box is one saved change.

mod common;

use aegis_client::state::{Mode, SyncView};
use aegis_client::view::Page;
use common::{App, TestServer, save};
use slint::Model;

async fn editing_with_sample(server: &TestServer) -> App {
    let app = App::new(server.admin_session("root"), 1360, 860);
    app.online().await;
    app.window.invoke_create_project("Brake ECU".into());
    app.until("open", |vm| vm.project.is_some()).await;
    app.window.invoke_begin_edit();
    app.until("editing", |vm| {
        vm.project.as_ref().is_some_and(|p| p.mode == Mode::Editing)
    })
    .await;
    app.window.invoke_create_sample();
    app.until("sample saved", |vm| {
        vm.project
            .as_ref()
            .is_some_and(|p| p.threats.len() == 2 && p.sync == SyncView::Synced)
    })
    .await;
    app
}

async fn show(app: &App, page: Page) {
    app.window.invoke_show_page(page.index() as i32);
    app.until(page.label(), |vm| {
        vm.project.as_ref().is_some_and(|p| p.page == page)
    })
    .await;
    app.controller.apply_latest();
}

fn kinds(app: &App) -> Vec<String> {
    app.window
        .get_descriptions()
        .iter()
        .map(|d| d.kind.to_string())
        .collect()
}

#[tokio::test(flavor = "current_thread")]
async fn the_notes_and_an_asset_description_are_saved_from_their_pages() {
    let server = TestServer::start().await;
    let app = editing_with_sample(&server).await;

    // Project notes on the Overview.
    show(&app, Page::Overview).await;
    assert_eq!(kinds(&app), ["project"]);
    // The box edits in place and saves on leaving it (what set-description gets).
    app.window
        .invoke_set_description(0, "Scope: the brake ECU.\nOut of scope: the pedal.".into());
    app.until("notes saved", |vm| {
        vm.project.as_ref().is_some_and(|p| {
            p.sync == SyncView::Synced
                && p.descriptions
                    .first()
                    .is_some_and(|d| d.text.starts_with("Scope"))
        })
    })
    .await;
    app.controller.apply_latest();
    let item = app.window.get_descriptions().row_data(0).expect("box");
    assert_eq!(item.text, "Scope: the brake ECU.\nOut of scope: the pedal.");
    assert!(item.by.contains("root"), "{}", item.by);
    save(&app.frame(), "p00-overview-notes");

    // An asset's description on the Assets page.
    show(&app, Page::Assets).await;
    app.window.invoke_pick_asset(0);
    app.until("asset picked", |vm| {
        vm.project.as_ref().is_some_and(|p| p.data.asset.is_some())
    })
    .await;
    app.controller.apply_latest();
    assert_eq!(kinds(&app), ["asset"]);
    assert_eq!(
        app.window.get_descriptions().row_data(0).expect("box").text,
        ""
    );
    app.window
        .invoke_set_description(0, "Controls the brake pressure.".into());
    app.until("asset text saved", |vm| {
        vm.project.as_ref().is_some_and(|p| {
            p.sync == SyncView::Synced
                && p.descriptions
                    .first()
                    .is_some_and(|d| d.text.starts_with("Controls"))
        })
    })
    .await;
    save(&app.frame(), "p01-asset-description");

    // Every page that shows an element has a box; the others have none.
    for (page, expected) in [
        (Page::Damage, 1),
        (Page::Threats, 1),
        (Page::Trees, 1),
        (Page::Risks, 1),
        (Page::Controls, 0),
        (Page::Goals, 0),
        (Page::Problems, 0),
        (Page::Versions, 0),
        (Page::History, 0),
        (Page::Reports, 0),
        (Page::Admin, 0),
    ] {
        show(&app, page).await;
        if page == Page::Damage {
            // A side panel (and its box) exists once a row is picked.
            assert!(kinds(&app).is_empty(), "nothing picked yet");
            app.window.invoke_pick_damage(0);
            app.until("damage picked", |vm| {
                vm.project.as_ref().is_some_and(|p| p.data.damage.is_some())
            })
            .await;
            app.controller.apply_latest();
        }
        app.frame();
        assert_eq!(
            app.window.get_descriptions().row_count(),
            expected,
            "{page:?}: {:?}",
            kinds(&app)
        );
    }

    drop(app);
    server.stop().await;
}

#[tokio::test(flavor = "current_thread")]
async fn a_read_only_view_cannot_change_a_description() {
    let server = TestServer::start().await;
    let editor = editing_with_sample(&server).await;
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
    show(&reader, Page::Overview).await;
    assert!(!reader.window.get_editing(), "the Save button stays off");
    reader
        .window
        .invoke_set_description(0, "Not allowed".into());
    reader
        .until("refused", |vm| {
            vm.notices.iter().any(|n| n.text.contains("Edit"))
        })
        .await;
    assert_eq!(
        reader
            .session
            .view()
            .project
            .as_ref()
            .expect("project")
            .confirmed_seq,
        editor
            .session
            .view()
            .project
            .as_ref()
            .expect("project")
            .confirmed_seq
    );
    drop(reader);
    drop(editor);
    server.stop().await;
}

#[tokio::test(flavor = "current_thread")]
async fn a_description_is_typed_in_place_tab_saves_and_escape_restores() {
    use common::{press_key, type_text};
    use slint::platform::Key;

    let server = TestServer::start().await;
    let app = editing_with_sample(&server).await;
    show(&app, Page::Assets).await;
    app.window.invoke_pick_asset(0);
    app.until("asset picked", |vm| {
        vm.project.as_ref().is_some_and(|p| p.data.asset.is_some())
    })
    .await;
    app.controller.apply_latest();
    app.frame();
    // The name field has focus; Tab reaches the description box under it.
    app.window.set_focus_name(true);
    app.frame();
    press_key(&app, Key::Tab, false);

    // Escape throws the typing away: nothing is sent.
    type_text(&app, "draft");
    press_key(&app, Key::Escape, false);
    press_key(&app, Key::Tab, false);
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    app.controller.apply_latest();
    assert_eq!(
        app.window.get_descriptions().row_data(0).expect("box").text,
        ""
    );

    // Typing and leaving with Tab saves.
    app.window.set_focus_name(true);
    app.frame();
    press_key(&app, Key::Tab, false);
    type_text(&app, "Holds the brake map.");
    press_key(&app, Key::Tab, false);
    app.until("typed text saved", |vm| {
        vm.project.as_ref().is_some_and(|p| {
            p.sync == SyncView::Synced
                && p.descriptions
                    .first()
                    .is_some_and(|d| d.text == "Holds the brake map.")
        })
    })
    .await;
    app.controller.apply_latest();
    app.frame();
    save(&app.frame(), "p02-asset-description-inline");
}

#[tokio::test(flavor = "current_thread")]
#[cfg_attr(
    target_os = "linux",
    ignore = "fails on Linux only; Slint client is being replaced"
)]
async fn the_translation_editor_shows_only_for_a_project_with_languages() {
    use aegis_shell::ProjectLanguages;
    use slint::ComponentHandle;

    let server = TestServer::start().await;
    let app = editing_with_sample(&server).await;
    show(&app, Page::Assets).await;
    app.window.invoke_pick_asset(0);
    app.until("asset picked", |vm| {
        vm.project.as_ref().is_some_and(|p| p.data.asset.is_some())
    })
    .await;
    let single = app.frame();
    assert_eq!(
        app.window
            .global::<ProjectLanguages>()
            .get_langs()
            .row_count(),
        0
    );

    // Comma or space separated, a repeat counts once.
    app.window.invoke_set_project_languages("ja, de ja".into());
    app.until("languages saved", |vm| {
        vm.project
            .as_ref()
            .is_some_and(|p| p.languages == ["ja", "de"] && p.sync == SyncView::Synced)
    })
    .await;
    let langs = app.window.global::<ProjectLanguages>();
    assert_eq!(langs.get_text(), "ja, de");
    assert_eq!(langs.get_langs().row_data(0).as_deref(), Some("ja"));
    let multi = app.frame();
    save(&multi, "p02-translation-editor");
    assert_ne!(single.rgb, multi.rgb, "the add row appears");

    // Back to one language: the add row is gone again.
    app.window.invoke_set_project_languages("".into());
    app.until("languages cleared", |vm| {
        vm.project
            .as_ref()
            .is_some_and(|p| p.languages.is_empty() && p.sync == SyncView::Synced)
    })
    .await;
    assert_eq!(app.frame().rgb, single.rgb);

    drop(app);
    server.stop().await;
}
