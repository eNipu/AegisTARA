//! FND-10 in the real window: the threat inspector rates whether a threat
//! scales to many vehicles with a reasoning text, the Threats table shows a
//! Scales column, and each change is one undo step. The frame is saved for
//! the UI review (`target/shell-screenshots/threat-scaling.png`).

mod common;

use aegis_client::commands::{Command, Edit};
use aegis_client::state::{Mode, ProjectView, ViewModel};
use aegis_client::view::Page;
use aegis_model::{AssetId, ThreatId};
use common::{App, TestServer, save};
use slint::Model;

fn p(vm: &ViewModel) -> &ProjectView {
    vm.project.as_ref().expect("project")
}

fn detail(vm: &ViewModel) -> Option<(Option<bool>, String)> {
    p(vm)
        .data
        .threat
        .as_ref()
        .map(|d| (d.scales, d.scaling_reason.clone()))
}

#[tokio::test(flavor = "current_thread")]
async fn scaling_is_rated_explained_shown_and_undone() {
    let server = TestServer::start().await;
    let app = App::new(server.admin_session("root"), 1360, 860);
    app.online().await;
    app.window.invoke_create_project("Keyless entry".into());
    app.until("open", |vm| vm.project.is_some()).await;
    app.window.invoke_begin_edit();
    app.until("editing", |vm| {
        vm.project.as_ref().is_some_and(|p| p.mode == Mode::Editing)
    })
    .await;
    let asset = AssetId::new_v7();
    app.send(Command::Edit(Edit::AddAsset {
        id: asset,
        name: "Key fob".into(),
        properties: (0..6).collect(),
    }));
    app.send(Command::Edit(Edit::AddThreat {
        id: ThreatId::new_v7(),
        name: "Relay attack".into(),
        asset,
        category: 0,
    }));
    app.window.invoke_show_page(Page::Threats.index() as i32);
    app.until("threats page", |vm| {
        p(vm).page == Page::Threats && p(vm).threats.len() == 1
    })
    .await;
    app.window.invoke_select_threat(0);
    app.until("detail", |vm| detail(vm) == Some((None, String::new())))
        .await;
    assert_eq!(app.window.get_td_scales(), 0);

    app.window.invoke_set_threat_scales(1);
    app.until("scales", |vm| {
        detail(vm) == Some((Some(true), String::new()))
    })
    .await;
    app.window
        .invoke_set_threat_scaling_reason("One relay kit opens every car".into());
    app.until("reason", |vm| {
        detail(vm) == Some((Some(true), "One relay kit opens every car".into()))
    })
    .await;
    app.frame();
    assert_eq!(app.window.get_td_scales(), 1);
    assert_eq!(
        app.window.get_td_scaling_reason(),
        "One relay kit opens every car"
    );
    let row = app.window.get_threat_rows().row_data(0).expect("row");
    assert_eq!(row.row_data(3).expect("cell").text, "Yes");
    assert!(
        app.window
            .get_undo_label()
            .starts_with("change scaling of threat scenario"),
        "{}",
        app.window.get_undo_label()
    );
    save(&app.frame(), "threat-scaling");

    // Undo takes back the reasoning only, then the rating.
    app.window.invoke_undo();
    app.until("reason undone", |vm| {
        detail(vm) == Some((Some(true), String::new()))
    })
    .await;
    app.window.invoke_undo();
    app.until("rating undone", |vm| {
        detail(vm) == Some((None, String::new()))
    })
    .await;
    let row = app.window.get_threat_rows().row_data(0).expect("row");
    assert_eq!(row.row_data(3).expect("cell").text, "");
}
