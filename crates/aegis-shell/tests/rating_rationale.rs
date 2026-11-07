//! FND-09 in the real window: the damage and threat inspectors take a
//! rationale next to the impact and feasibility ratings, each one undo step.
//! The frame is saved for the UI review
//! (`target/shell-screenshots/rating-rationale.png`).

mod common;

use aegis_client::commands::{Command, Edit};
use aegis_client::state::{Mode, ProjectView, ViewModel};
use aegis_client::view::Page;
use aegis_model::{AssetId, DamageId, ThreatId};
use common::{App, TestServer, save};

fn p(vm: &ViewModel) -> &ProjectView {
    vm.project.as_ref().expect("project")
}

#[tokio::test(flavor = "current_thread")]
async fn impact_and_feasibility_rationale_are_saved_and_undone() {
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
    let asset = AssetId::new_v7();
    app.send(Command::Edit(Edit::AddAsset {
        id: asset,
        name: "Brake controller".into(),
        properties: (0..6).collect(),
    }));
    app.send(Command::Edit(Edit::AddDamage {
        id: DamageId::new_v7(),
        name: "Brakes fail".into(),
    }));
    app.send(Command::Edit(Edit::AddThreat {
        id: ThreatId::new_v7(),
        name: "Spoof brake command".into(),
        asset,
        category: 0,
    }));

    app.window.invoke_show_page(Page::Damage.index() as i32);
    app.until("damage", |vm| {
        p(vm).page == Page::Damage && p(vm).data.damages.len() == 1
    })
    .await;
    app.window.invoke_pick_damage(0);
    app.until("detail", |vm| p(vm).data.damage.is_some()).await;
    app.window
        .invoke_set_impact_rationale("Loss of braking is life threatening".into());
    app.until("impact rationale", |vm| {
        p(vm)
            .data
            .damage
            .as_ref()
            .is_some_and(|d| d.impact_rationale == "Loss of braking is life threatening")
    })
    .await;
    app.frame();
    assert_eq!(
        app.window.get_damage_impact_rationale(),
        "Loss of braking is life threatening"
    );
    assert!(
        app.window
            .get_undo_label()
            .starts_with("change impact rationale of damage scenario"),
        "{}",
        app.window.get_undo_label()
    );
    save(&app.frame(), "rating-rationale");

    app.window.invoke_show_page(Page::Threats.index() as i32);
    app.until("threats", |vm| {
        p(vm).page == Page::Threats && p(vm).threats.len() == 1
    })
    .await;
    app.window.invoke_select_threat(0);
    app.until("threat", |vm| p(vm).data.threat.is_some()).await;
    app.window
        .invoke_set_feasibility_rationale("Needs physical bus access".into());
    app.until("feasibility rationale", |vm| {
        p(vm)
            .data
            .threat
            .as_ref()
            .is_some_and(|t| t.feasibility_rationale == "Needs physical bus access")
    })
    .await;
    app.frame();
    assert_eq!(
        app.window.get_td_feasibility_rationale(),
        "Needs physical bus access"
    );
    app.window.invoke_undo();
    app.until("undone", |vm| {
        p(vm)
            .data
            .threat
            .as_ref()
            .is_some_and(|t| t.feasibility_rationale.is_empty())
    })
    .await;
}
