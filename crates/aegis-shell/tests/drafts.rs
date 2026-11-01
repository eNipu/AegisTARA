//! UI-08 in the real window with real key events: a draft in the inspector,
//! created by Enter or Ctrl/Cmd+Enter, discarded by Escape, for assets,
//! damage scenarios and controls. A cancelled draft sends no op.

mod common;

use aegis_client::state::{Mode, ViewModel};
use aegis_client::view::{DraftKind, Page};
use common::{App, TestServer, press_key, save, type_text};
use slint::Model;
use slint::platform::Key;

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

fn p(vm: &ViewModel) -> &aegis_client::state::ProjectView {
    vm.project.as_ref().expect("project")
}

async fn settle(app: &App) -> std::sync::Arc<ViewModel> {
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    app.frame();
    app.until("settled", |_| true).await
}

#[tokio::test(flavor = "current_thread")]
async fn an_asset_is_created_from_a_draft_with_the_keyboard_alone() {
    let server = TestServer::start().await;
    let app = editing_app(&server).await;
    app.window.invoke_show_page(Page::Assets.index() as i32);
    app.window.invoke_begin_draft("asset".into());
    app.until("draft", |vm| p(vm).draft == Some(DraftKind::Asset))
        .await;
    app.frame();
    app.frame();
    assert_eq!(app.window.get_draft_kind(), "asset");
    // The draft row is in the table and selected.
    assert_eq!(app.window.get_asset_rows().row_count(), 1);
    assert_eq!(app.window.get_asset_selected(), 0);
    type_text(&app, "Brake ECU");
    save(&app.frame(), "draft-asset");

    press_key(&app, Key::Return, false);
    let vm = app
        .until("created", |vm| {
            p(vm).data.assets.len() == 1 && p(vm).draft.is_none()
        })
        .await;
    assert_eq!(p(&vm).data.assets[0].name, "Brake ECU");
    app.frame();
    assert_eq!(app.window.get_draft_kind(), "");
}

#[tokio::test(flavor = "current_thread")]
async fn escape_discards_a_draft_and_sends_no_op() {
    let server = TestServer::start().await;
    let app = editing_app(&server).await;
    let before = settle(&app).await;
    let start = p(&before).confirmed_seq;
    app.window.invoke_show_page(Page::Controls.index() as i32);
    app.window.invoke_begin_draft("control".into());
    app.until("draft", |vm| p(vm).draft == Some(DraftKind::Control))
        .await;
    app.frame();
    app.frame();
    type_text(&app, "Secure boot");
    press_key(&app, Key::Escape, false);
    app.until("discarded", |vm| p(vm).draft.is_none()).await;
    let vm = settle(&app).await;
    assert_eq!(p(&vm).data.controls.len(), 0);
    assert_eq!(p(&vm).confirmed_seq, start, "nothing was sent");
    assert!(!p(&vm).can_undo, "nothing is in the history");
}

#[tokio::test(flavor = "current_thread")]
async fn control_enter_creates_and_starts_the_next_draft() {
    let server = TestServer::start().await;
    let app = editing_app(&server).await;
    app.window.invoke_show_page(Page::Damage.index() as i32);
    app.window.invoke_begin_draft("damage".into());
    app.until("draft", |vm| p(vm).draft == Some(DraftKind::Damage))
        .await;
    app.frame();
    app.frame();
    for name in ["Loss of braking", "Data leak", "Theft"] {
        type_text(&app, name);
        press_key(&app, Key::Return, true);
        app.until(name, |vm| {
            p(vm).data.damages.iter().any(|d| d.name == name) && p(vm).draft.is_some()
        })
        .await;
        app.frame();
        app.frame();
    }
    let vm = settle(&app).await;
    assert_eq!(
        p(&vm).data.damages.len(),
        3,
        "three created, one draft open"
    );
    assert_eq!(p(&vm).draft, Some(DraftKind::Damage));
    save(&app.frame(), "draft-damage-next");
    press_key(&app, Key::Escape, false);
    app.until("last draft discarded", |vm| p(vm).draft.is_none())
        .await;
    let vm = settle(&app).await;
    assert_eq!(p(&vm).data.damages.len(), 3);
}

#[tokio::test(flavor = "current_thread")]
async fn a_threat_draft_waits_for_its_asset_and_category() {
    use aegis_client::commands::{Command, Edit};
    use aegis_model::AssetId;
    let server = TestServer::start().await;
    let app = editing_app(&server).await;
    app.send(Command::Edit(Edit::AddAsset {
        id: AssetId::new_v7(),
        name: "ECU".into(),
        properties: (0..6).collect(),
    }));
    app.window.invoke_show_page(Page::Assets.index() as i32);
    app.until("asset", |vm| p(vm).data.assets.len() == 1).await;
    app.window.invoke_show_page(Page::Threats.index() as i32);
    app.window.invoke_begin_draft("threat".into());
    app.until("draft", |vm| p(vm).draft == Some(DraftKind::Threat))
        .await;
    app.frame();
    app.frame();
    assert_eq!(app.window.get_threat_rows().row_count(), 1, "the draft row");
    assert_eq!(app.window.get_threat_selected(), 0, "and it is selected");
    assert!(app.window.get_draft_incomplete(), "the category is missing");
    assert_eq!(
        app.window.get_draft_asset(),
        0,
        "the only asset is the context"
    );
    assert_eq!(app.window.get_draft_category(), -1);

    // Enter without a category creates nothing.
    type_text(&app, "Spoofed brake frame");
    press_key(&app, Key::Return, false);
    let vm = settle(&app).await;
    assert_eq!(p(&vm).threats.len(), 0);
    assert!(p(&vm).draft.is_some(), "the draft stays");
    save(&app.frame(), "draft-threat-incomplete");

    // Choosing a category completes it; Enter in the name field creates it.
    app.window.invoke_set_draft_category(0);
    app.until("complete", |vm| !p(vm).draft_incomplete).await;
    app.frame();
    assert!(!app.window.get_draft_incomplete());
    press_key(&app, Key::Return, false);
    let vm = app.until("created", |vm| p(vm).threats.len() == 1).await;
    assert_eq!(p(&vm).threats[0].name, "Spoofed brake frame");
}

#[tokio::test(flavor = "current_thread")]
async fn goal_claim_and_tree_drafts_are_created_with_the_keyboard() {
    let server = TestServer::start().await;
    let app = editing_app(&server).await;
    for (page, key, kind, name) in [
        (Page::Goals, "goal", DraftKind::Goal, "No remote braking"),
        (
            Page::Goals,
            "claim",
            DraftKind::Claim,
            "Debug port is closed",
        ),
        (Page::Trees, "tree", DraftKind::Tree, "Reflash the ECU"),
    ] {
        app.window.invoke_show_page(page.index() as i32);
        app.window.invoke_begin_draft(key.into());
        app.until(key, |vm| p(vm).draft == Some(kind)).await;
        app.frame();
        app.frame();
        type_text(&app, name);
        press_key(&app, Key::Return, false);
        // Each page fills only its own data: count on the page of the draft.
        app.until(name, |vm| {
            let d = &p(vm).data;
            p(vm).draft.is_none()
                && match kind {
                    DraftKind::Goal => d.goals.len() == 1,
                    DraftKind::Claim => d.claims.len() == 1,
                    _ => d.trees.len() == 1,
                }
        })
        .await;
    }
}

#[tokio::test(flavor = "current_thread")]
async fn item_definition_drafts_are_created_with_the_keyboard() {
    let server = TestServer::start().await;
    let app = editing_app(&server).await;
    app.window.invoke_show_page(Page::Item.index() as i32);

    // Components and a trust boundary: a name and Enter.
    for (key, kind, name) in [
        ("component", DraftKind::Component, "Gateway"),
        ("component", DraftKind::Component, "Brake ECU"),
        ("trust-boundary", DraftKind::TrustBoundary, "Safety domain"),
    ] {
        app.window.invoke_begin_draft(key.into());
        app.until(key, |vm| p(vm).draft == Some(kind)).await;
        app.frame();
        app.frame();
        type_text(&app, name);
        press_key(&app, Key::Return, false);
        app.until(name, |vm| p(vm).draft.is_none()).await;
    }
    app.until("components and boundary", |vm| {
        p(vm).data.components.len() == 2 && p(vm).data.trust_boundaries.len() == 1
    })
    .await;

    // Interface: the component is the context; the kind is missing.
    app.window.invoke_begin_draft("interface".into());
    app.until("interface draft", |vm| {
        p(vm).draft == Some(DraftKind::Interface)
    })
    .await;
    app.frame();
    app.frame();
    assert_eq!(app.window.get_interface_rows().row_count(), 1, "draft row");
    assert!(app.window.get_draft_incomplete());
    assert!(app.window.get_draft_component() >= 0);
    assert_eq!(app.window.get_draft_interface_kind(), -1);
    type_text(&app, "CAN-2 port");
    press_key(&app, Key::Return, false);
    let vm = settle(&app).await;
    assert_eq!(
        p(&vm).data.interfaces.len(),
        0,
        "kind missing: nothing created"
    );
    app.window.invoke_set_draft_interface_kind(0);
    app.until("complete", |vm| !p(vm).draft_incomplete).await;
    app.frame();
    press_key(&app, Key::Return, false);
    app.until("interface", |vm| p(vm).data.interfaces.len() == 1)
        .await;

    // Data flow: from is the context, to must be another component.
    app.window.invoke_begin_draft("data-flow".into());
    app.until("flow draft", |vm| p(vm).draft == Some(DraftKind::DataFlow))
        .await;
    app.frame();
    app.frame();
    assert!(app.window.get_draft_incomplete());
    let from = app.window.get_draft_component();
    assert!(from >= 0);
    app.window.invoke_set_draft_to(from);
    let vm = settle(&app).await;
    assert!(p(&vm).draft_incomplete, "from and to must differ");
    app.window.invoke_set_draft_to(1 - from);
    app.until("complete", |vm| !p(vm).draft_incomplete).await;
    app.frame();
    type_text(&app, "Brake frame");
    press_key(&app, Key::Return, false);
    app.until("flow", |vm| p(vm).data.data_flows.len() == 1)
        .await;
    save(&app.frame(), "draft-item-definition");
}

/// UI-08: the new-element shortcut starts the draft of the page on screen.
#[tokio::test(flavor = "current_thread")]
async fn the_new_element_shortcut_starts_a_draft_of_this_page() {
    let server = TestServer::start().await;
    let app = editing_app(&server).await;
    for (page, kind) in [
        (Page::Assets, DraftKind::Asset),
        (Page::Threats, DraftKind::Threat),
        (Page::Controls, DraftKind::Control),
    ] {
        app.window.invoke_show_page(page.index() as i32);
        app.until("page", |vm| p(vm).page == page).await;
        app.frame();
        common::press_chord(&app, 'n', true);
        app.until("draft", |vm| p(vm).draft == Some(kind)).await;
        app.window.invoke_cancel_draft();
        app.until("cancelled", |vm| p(vm).draft.is_none()).await;
    }
}

/// UI-08.5: a user who may not edit gets no draft, and the header buttons say
/// why they are off.
#[tokio::test(flavor = "current_thread")]
async fn a_read_only_user_gets_no_draft_and_a_reason() {
    let server = TestServer::start().await;
    let app = App::new(server.admin_session("root"), 1360, 860);
    app.online().await;
    app.window.invoke_create_project("Brake ECU".into());
    app.until("open", |vm| vm.project.is_some()).await;
    app.window.invoke_show_page(Page::Assets.index() as i32);
    app.until("assets", |vm| p(vm).page == Page::Assets).await;
    app.frame();
    assert!(
        app.window.get_new_reason().starts_with("Read only"),
        "the reason: {}",
        app.window.get_new_reason()
    );
    common::press_chord(&app, 'n', true);
    let vm = settle(&app).await;
    assert!(p(&vm).draft.is_none(), "a read-only user gets no draft");
    app.window.invoke_begin_edit();
    app.until("editing", |vm| p(vm).mode == Mode::Editing).await;
    app.frame();
    assert_eq!(app.window.get_new_reason(), "", "editing: no reason");
}

/// UI-08 done-when: five assets with the keyboard alone, and how many key
/// presses it takes (recorded in the UX review).
#[tokio::test(flavor = "current_thread")]
async fn five_assets_take_the_keyboard_alone() {
    let server = TestServer::start().await;
    let app = editing_app(&server).await;
    app.window.invoke_show_page(Page::Assets.index() as i32);
    app.until("page", |vm| p(vm).page == Page::Assets).await;
    app.frame();
    let names = ["ECU", "Gateway", "Sensor", "Actuator", "Display"];
    let mut presses = 0;
    let started = std::time::Instant::now();
    common::press_chord(&app, 'n', true);
    presses += 1;
    app.until("draft", |vm| p(vm).draft == Some(DraftKind::Asset))
        .await;
    app.frame();
    app.frame();
    for (i, name) in names.iter().enumerate() {
        type_text(&app, name);
        presses += name.chars().count();
        let last = i == names.len() - 1;
        press_key(&app, Key::Return, !last);
        presses += 1;
        app.until(name, |vm| p(vm).data.assets.iter().any(|a| a.name == *name))
            .await;
        app.frame();
        app.frame();
    }
    let vm = settle(&app).await;
    assert_eq!(p(&vm).data.assets.len(), 5);
    assert!(p(&vm).draft.is_none());
    let typed: usize = names.iter().map(|n| n.chars().count()).sum();
    // One chord to start, one chord or Enter per asset, and the letters.
    assert_eq!(presses, 1 + 5 + typed);
    eprintln!(
        "five assets: {presses} key presses ({typed} letters, {} actions), no mouse, {:?} headless",
        presses - typed,
        started.elapsed()
    );
}
