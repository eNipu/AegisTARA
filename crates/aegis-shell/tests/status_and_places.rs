//! Job 130 (UI-17) in the real window: the status bar shows when the last
//! edit was saved and the problem counts by kind; back and forward walk the
//! pages and selections the user visited.

mod common;

use aegis_client::commands::{Command, Edit};
use aegis_client::state::{Mode, SyncView, ViewModel};
use aegis_client::view::{Page, Selection};
use common::{App, TestServer, save};

fn p(vm: &ViewModel) -> &aegis_client::state::ProjectView {
    vm.project.as_ref().expect("project")
}

#[tokio::test(flavor = "current_thread")]
async fn the_status_bar_and_back_and_forward() {
    use aegis_model::{AssetId, ThreatId};
    let server = TestServer::start().await;
    let app = App::new(server.admin_session("root"), 1360, 860);
    app.online().await;
    app.window.invoke_create_project("Brake ECU".into());
    app.until("open", |vm| vm.project.is_some()).await;
    assert_eq!(app.window.get_saved_text(), "", "nothing saved yet");
    app.window.invoke_begin_edit();
    app.until("editing", |vm| {
        vm.project.as_ref().is_some_and(|p| p.mode == Mode::Editing)
    })
    .await;
    let (a, b) = (AssetId::new_v7(), AssetId::new_v7());
    for (id, name) in [(a, "A"), (b, "B")] {
        app.send(Command::Edit(Edit::AddAsset {
            id,
            name: name.into(),
            properties: (0..6).collect(),
        }));
    }
    // A threat with no feasibility is a blocking problem.
    app.send(Command::Edit(Edit::AddThreat {
        id: ThreatId::new_v7(),
        name: "Spoof".into(),
        asset: a,
        category: 0,
    }));
    let vm = app
        .until("saved", |vm| {
            let p = p(vm);
            p.sync == SyncView::Synced && p.saved_ms.is_some() && p.threats.len() == 1
        })
        .await;
    app.frame();
    assert!(app.window.get_saved_text().ends_with(" UTC"));
    let (errors, warnings) = p(&vm).problem_counts;
    assert!(errors > 0, "a threat with no feasibility blocks");
    assert_eq!(app.window.get_problem_errors() as usize, errors);
    assert_eq!(app.window.get_problem_warnings() as usize, warnings);
    save(&app.frame(), "status-bar");

    // Visit: assets with A, assets with B, the threats page.
    app.window.invoke_show_page(Page::Assets.index() as i32);
    app.until("assets", |vm| p(vm).page == Page::Assets).await;
    for (id, name) in [(a, "A"), (b, "B")] {
        app.send(Command::Select {
            selection: Selection::Asset(id),
            navigate: false,
        });
        app.until(name, |vm| {
            p(vm).data.asset.as_ref().is_some_and(|x| x.name == name)
        })
        .await;
        app.frame();
    }
    app.window.invoke_show_page(Page::Threats.index() as i32);
    app.until("threats", |vm| p(vm).page == Page::Threats).await;
    app.frame();
    assert!(app.window.get_can_go_back());
    assert!(!app.window.get_can_go_forward());

    // Back to B on the assets page, back again to A, forward to B.
    let at = |page: Page, name: &'static str| {
        move |vm: &ViewModel| {
            p(vm).page == page && p(vm).data.asset.as_ref().is_some_and(|x| x.name == name)
        }
    };
    app.window.invoke_go_back();
    app.until("back to B", at(Page::Assets, "B")).await;
    app.frame();
    app.window.invoke_go_back();
    app.until("back to A", at(Page::Assets, "A")).await;
    app.frame();
    assert!(app.window.get_can_go_forward());
    app.window.invoke_go_forward();
    app.until("forward to B", at(Page::Assets, "B")).await;
    app.frame();
    app.window.invoke_go_forward();
    app.until("forward to threats", |vm| p(vm).page == Page::Threats)
        .await;
    app.frame();
    assert!(!app.window.get_can_go_forward(), "the end of the list");
}
