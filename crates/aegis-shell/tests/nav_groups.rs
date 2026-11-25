//! UI-13 in the real window: the sidebar lists pages by TARA phase, item
//! definition first, under group headers that close and open; a closed phase
//! shows the number of elements on its pages. The frame is saved for the UI
//! review (`target/shell-screenshots/nav-groups-*.png`).

mod common;

use aegis_client::commands::{Command, Edit};
use aegis_client::state::{Mode, ViewModel};
use aegis_client::view::{Page, PageGroup};
use common::{App, TestServer, save};
use slint::Model;

fn p(vm: &ViewModel) -> &aegis_client::state::ProjectView {
    vm.project.as_ref().expect("project")
}

#[tokio::test(flavor = "current_thread")]
async fn groups_close_open_and_count_their_pages() {
    use aegis_model::{AssetId, ThreatId};
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
        name: "Bus".into(),
        properties: (0..6).collect(),
    }));
    app.send(Command::Edit(Edit::AddThreat {
        id: ThreatId::new_v7(),
        name: "Spoof".into(),
        asset,
        category: 0,
    }));
    app.until("counts", |vm| {
        p(vm).nav_counts[Page::Threats.index()] == Some(1)
    })
    .await;
    app.frame();

    // Item definition comes first, right after the overview.
    assert_eq!(Page::ALL[1], Page::Item);
    let g = PageGroup::AssetsThreats.index();
    let open = |app: &App| -> Vec<bool> { app.window.get_nav_group_open().iter().collect() };
    assert_eq!(open(&app), vec![true; PageGroup::ALL.len()]);

    // Closing assets and threats keeps the others open and shows 1 + 1.
    app.window.invoke_toggle_nav_group(g as i32);
    let now = open(&app);
    assert!(!now[g] && now.iter().filter(|o| !**o).count() == 1);
    let counts: Vec<_> = app.window.get_nav_group_counts().iter().collect();
    assert_eq!(counts[g], "2");
    assert_eq!(counts[PageGroup::Project.index()], "", "not a phase");
    save(&app.frame(), "nav-groups-closed");

    // A page of a closed group still opens from the palette or a link.
    app.window.invoke_show_page(Page::Threats.index() as i32);
    app.until("threats", |vm| p(vm).page == Page::Threats).await;

    app.window.invoke_toggle_nav_group(g as i32);
    assert_eq!(open(&app), vec![true; PageGroup::ALL.len()]);
    // A stale index changes nothing.
    app.window.invoke_toggle_nav_group(99);
    app.window.invoke_toggle_nav_group(-1);
    assert_eq!(open(&app), vec![true; PageGroup::ALL.len()]);
    save(&app.frame(), "nav-groups-open");
}

#[tokio::test(flavor = "current_thread")]
async fn attacker_profile_nav_dot_tracks_non_default_reset_and_undo() {
    use aegis_model::{AttackFilter, ScalabilityFilter};

    assert_eq!(Page::ALL.len(), 19);
    assert_eq!(Page::ALL[9], Page::SharedBlocks);
    assert_eq!(Page::ALL[10], Page::AttackerProfile);
    assert_eq!(Page::AttackerProfile.group(), Some(PageGroup::Feasibility));

    let server = TestServer::start().await;
    let app = App::new(server.admin_session("root"), 1360, 860);
    app.online().await;
    app.window.invoke_create_project("Profile ECU".into());
    app.until("open", |vm| vm.project.is_some()).await;
    app.window.invoke_begin_edit();
    app.until("editing", |vm| {
        vm.project.as_ref().is_some_and(|p| p.mode == Mode::Editing)
    })
    .await;

    let idx = Page::AttackerProfile.index();
    let dots = |app: &App| -> Vec<bool> { app.window.get_nav_dots().iter().collect() };
    let v = app.session.view();
    assert_eq!(p(&v).nav_counts[idx], None);
    assert_eq!(p(&v).nav_dots, vec![false; Page::ALL.len()]);
    assert_eq!(dots(&app), vec![false; Page::ALL.len()]);

    // Opening the Attacker profile page populates data.attacker_profile.
    app.window.invoke_show_page(idx as i32);
    let opened = app
        .until("attacker profile page", |vm| {
            p(vm).page == Page::AttackerProfile && p(vm).data.attacker_profile.is_some()
        })
        .await;
    assert!(
        opened
            .project
            .as_ref()
            .expect("project")
            .data
            .attacker_profile
            .as_ref()
            .expect("view")
            .is_default
    );

    // Non-default attacker profile lights up only Page::AttackerProfile's nav dot.
    app.send(Command::Edit(Edit::SetAttackFilter {
        filter: AttackFilter::default().with_scalability(ScalabilityFilter::ScaledOnly),
    }));
    app.until("dot on", |vm| p(vm).nav_dots[idx]).await;
    app.frame();
    let mut expected = vec![false; Page::ALL.len()];
    expected[idx] = true;
    assert_eq!(dots(&app), expected);

    // Resetting back to default clears the dot.
    app.send(Command::Edit(Edit::SetAttackFilter {
        filter: AttackFilter::default(),
    }));
    app.until("dot off", |vm| !p(vm).nav_dots[idx]).await;
    app.frame();
    assert_eq!(dots(&app), vec![false; Page::ALL.len()]);

    // Undo restores the non-default profile and lights the dot back up; second undo clears it.
    app.window.invoke_undo();
    app.until("dot restored by undo", |vm| p(vm).nav_dots[idx])
        .await;
    app.frame();
    assert_eq!(dots(&app), expected);

    app.window.invoke_undo();
    app.until("dot cleared by second undo", |vm| !p(vm).nav_dots[idx])
        .await;
    app.frame();
    assert_eq!(dots(&app), vec![false; Page::ALL.len()]);
}
