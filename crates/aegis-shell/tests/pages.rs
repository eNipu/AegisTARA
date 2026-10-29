//! Every TARA page in the real window, driven through the window's own
//! callbacks (what a click on a button or a row calls), against a real
//! server. Each page is also saved as a screenshot for the UI review.

mod common;

use std::time::Duration;

use aegis_client::state::{Mode, NumbersCheck, SyncView, ViewModel};
use aegis_client::view::Page;
use common::{App, TestServer, save};
use slint::Model;

async fn editing_admin(server: &TestServer) -> App {
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

async fn page(app: &App, page: Page) {
    app.window.invoke_show_page(page.index() as i32);
    app.until(page.label(), |vm| {
        vm.project.as_ref().is_some_and(|p| p.page == page)
    })
    .await;
    app.frame();
}

async fn saved(app: &App) -> std::sync::Arc<ViewModel> {
    app.until("saved", |vm| {
        vm.project
            .as_ref()
            .is_some_and(|p| p.sync == SyncView::Synced)
    })
    .await
}

fn project(vm: &ViewModel) -> &aegis_client::state::ProjectView {
    vm.project.as_ref().expect("project")
}

#[tokio::test(flavor = "current_thread")]
async fn a_tara_is_built_page_by_page_from_the_window() {
    let server = TestServer::start().await;
    let app = editing_admin(&server).await;

    // Item definition: add two components, an interface, a data flow, and a trust boundary.
    page(&app, Page::Item).await;
    app.window.invoke_add_component("Brake ECU".into());
    app.until("component 1", |vm| project(vm).data.components.len() == 1)
        .await;
    app.window.invoke_add_component("Telematics gateway".into());
    app.until("component 2", |vm| project(vm).data.components.len() == 2)
        .await;
    app.window.invoke_add_interface("CAN-2 port".into(), 0, 0);
    app.until("interface", |vm| project(vm).data.interfaces.len() == 1)
        .await;
    app.window
        .invoke_add_data_flow("Brake command frame".into(), 1, 0);
    app.until("data flow", |vm| project(vm).data.data_flows.len() == 1)
        .await;
    app.window
        .invoke_add_trust_boundary("Safety-critical domain".into());
    app.until("trust boundary", |vm| {
        project(vm).data.trust_boundaries.len() == 1
    })
    .await;
    app.controller.apply_latest();
    app.window.invoke_toggle_trust_boundary_member(0, true);
    app.until("boundary member", |vm| {
        project(vm)
            .data
            .trust_boundary
            .as_ref()
            .is_some_and(|tb| tb.members.iter().any(|m| m.member))
    })
    .await;
    save(&app.frame(), "p00-item");

    // Assets: add, select, rename, bind to Brake ECU, turn a property off.
    page(&app, Page::Assets).await;
    app.window.invoke_add_asset("Brake request".into());
    app.until("asset", |vm| project(vm).data.assets.len() == 1)
        .await;
    app.until("asset selected", |vm| project(vm).data.asset.is_some())
        .await;
    app.controller.apply_latest();
    assert!(app.window.get_has_asset());
    app.window.invoke_set_asset_component(1);
    app.until("asset component bound", |vm| {
        project(vm)
            .data
            .asset
            .as_ref()
            .is_some_and(|a| a.component.is_some())
    })
    .await;
    let properties = app.window.get_asset_properties();
    assert!(properties.row_count() >= 3);
    app.window.invoke_toggle_asset_property(0, false);
    app.window
        .invoke_rename_asset("Brake request on CAN-2".into());
    app.until("renamed", |vm| {
        project(vm)
            .data
            .asset
            .as_ref()
            .is_some_and(|a| a.name == "Brake request on CAN-2" && !a.properties[0])
    })
    .await;
    // The header says the numbers were checked against the server's (G15).
    app.until("numbers checked", |vm| {
        project(vm).numbers == NumbersCheck::Checked
    })
    .await;
    app.controller.apply_latest();
    assert_eq!(app.window.get_numbers_kind(), 0);
    assert_eq!(
        app.window.get_numbers_text().as_str(),
        "Numbers checked with the server"
    );
    save(&app.frame(), "p01-assets");

    // Damage: add and rate safety at the highest level.
    page(&app, Page::Damage).await;
    app.window
        .invoke_add_damage("Vehicle brakes hard at speed".into());
    app.until("damage", |vm| project(vm).data.damage.is_some())
        .await;
    app.controller.apply_latest();
    let impacts = app.window.get_damage_impacts();
    let levels = impacts
        .row_data(0)
        .expect("first category")
        .options
        .row_count() as i32;
    app.window.invoke_set_impact(0, levels - 1);
    app.until("rated", |vm| {
        project(vm)
            .data
            .damage
            .as_ref()
            .is_some_and(|d| d.impacts[0] == Some((levels - 2) as usize))
    })
    .await;
    save(&app.frame(), "p02-damage");

    // Threats: add on the asset, link the damage, rate it directly.
    page(&app, Page::Threats).await;
    app.window
        .invoke_add_threat("Spoofed brake request".into(), 0, 0);
    app.until("threat", |vm| project(vm).data.threat.is_some())
        .await;
    app.controller.apply_latest();
    app.window.invoke_toggle_threat_damage(0, true);
    app.window.invoke_set_threat_feasibility(4);
    app.until("risk", |vm| {
        project(vm)
            .data
            .threat
            .as_ref()
            .is_some_and(|t| t.inherent.is_some())
    })
    .await;
    app.window.invoke_add_catalog_ref(0, "4.1".into());
    app.until("ref", |vm| {
        project(vm)
            .data
            .threat
            .as_ref()
            .is_some_and(|t| t.catalog_refs.len() == 1)
    })
    .await;
    save(&app.frame(), "p03-threats");

    // A tree for the threat, edited as an outline.
    app.window.invoke_new_tree_for_threat();
    app.until("tree page", |vm| {
        let p = project(vm);
        p.page == Page::Trees && p.tree.is_some()
    })
    .await;
    app.window.invoke_apply_outline(
        "OR Spoofed brake request\n  SEQ Via OBD\n    - Reach the OBD port\n    - Open a diagnostic session\n  - Via telematics\n"
            .into(),
    );
    app.until("outline applied", |vm| {
        project(vm)
            .tree
            .as_ref()
            .is_some_and(|t| t.scene.len() == 5)
    })
    .await;
    app.frame();
    save(&app.frame(), "p04-tree");

    // Controls and goals.
    page(&app, Page::Controls).await;
    app.window
        .invoke_add_control("Authenticated diagnostics".into());
    app.until("control", |vm| project(vm).data.control.is_some())
        .await;
    save(&app.frame(), "p05-controls");
    page(&app, Page::Goals).await;
    app.window
        .invoke_add_goal("Only testers open sessions".into());
    app.until("goal", |vm| project(vm).data.goal.is_some())
        .await;
    app.controller.apply_latest();
    app.window.invoke_toggle_goal_control(0, true);
    app.until("goal control", |vm| {
        project(vm)
            .data
            .goal
            .as_ref()
            .is_some_and(|g| g.controls.iter().any(|c| c.linked))
    })
    .await;
    app.window
        .invoke_add_claim("Physical access is out of scope".into());
    app.until("claim", |vm| project(vm).data.claim.is_some())
        .await;
    save(&app.frame(), "p06-goals");

    // Risks: decide Reduce with the goal.
    page(&app, Page::Risks).await;
    app.window.invoke_pick_risk(0);
    app.until("risk selected", |vm| project(vm).data.threat.is_some())
        .await;
    app.window.invoke_set_treatment(2);
    app.window.invoke_set_threat_goal(1);
    app.until("treated", |vm| {
        project(vm)
            .data
            .threat
            .as_ref()
            .is_some_and(|t| t.treatment == Some(1) && t.goal.is_some())
    })
    .await;
    save(&app.frame(), "p07-risks");

    // Overview and problems.
    page(&app, Page::Overview).await;
    save(&app.frame(), "p08-overview");
    page(&app, Page::Problems).await;
    let vm = app.session.view();
    assert!(!project(&vm).data.problems.is_empty());
    save(&app.frame(), "p09-problems");

    // Versions: a baseline, a change, the comparison.
    saved(&app).await;
    page(&app, Page::Versions).await;
    app.window.invoke_create_baseline("B1 Concept".into());
    app.until("baseline", |vm| project(vm).baselines.len() == 1)
        .await;
    page(&app, Page::Assets).await;
    app.window.invoke_pick_asset(0);
    app.until("asset again", |vm| project(vm).data.asset.is_some())
        .await;
    app.window.invoke_rename_asset("Brake request".into());
    saved(&app).await;
    page(&app, Page::Versions).await;
    app.window.invoke_pick_baseline(0);
    app.window.invoke_compare_baseline();
    app.until("compared", |vm| project(vm).comparison.is_some())
        .await;
    app.frame();
    let vm = app.session.view();
    let c = project(&vm).comparison.as_ref().expect("comparison");
    assert!(c.lines.iter().any(|l| l.kind == "Changed"), "{c:?}");
    save(&app.frame(), "p10-versions");

    // History.
    page(&app, Page::History).await;
    app.until("history", |vm| !project(vm).history.is_empty())
        .await;
    save(&app.frame(), "p11-history");

    // Reports: all four exports (HTML, CSV, PDF, XLSX) land in the chosen folder,
    // plus figure attachment and Excel dry-run import.
    let dir = std::env::temp_dir().join(format!("aegis-ui-exports-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("dir");
    page(&app, Page::Reports).await;
    let svg_path = dir.join("diagram.svg");
    std::fs::write(
        &svg_path,
        b"<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 120 60\"><rect width=\"120\" height=\"60\" fill=\"#e2e8f0\"/></svg>",
    )
    .expect("write svg");
    app.window.invoke_add_figure(
        svg_path.display().to_string().into(),
        "Architecture diagram".into(),
    );
    app.until("figure attached", |vm| project(vm).figures.len() == 1)
        .await;
    std::fs::remove_file(&svg_path).expect("remove temp svg");

    app.window.set_export_dir(dir.display().to_string().into());
    app.window.invoke_export_report();
    app.until("report", |vm| {
        project(vm)
            .last_export
            .as_deref()
            .is_some_and(|p| p.ends_with(".html"))
    })
    .await;
    app.window.invoke_export_csv();
    app.until("csv", |vm| {
        project(vm)
            .last_export
            .as_deref()
            .is_some_and(|p| p.ends_with(".csv"))
    })
    .await;
    app.window.invoke_export_pdf();
    app.until("pdf", |vm| {
        project(vm)
            .last_export
            .as_deref()
            .is_some_and(|p| p.ends_with(".pdf"))
    })
    .await;
    app.window.invoke_export_xlsx();
    app.until("xlsx", |vm| {
        project(vm)
            .last_export
            .as_deref()
            .is_some_and(|p| p.ends_with(".xlsx"))
    })
    .await;
    let files: Vec<_> = std::fs::read_dir(&dir)
        .expect("dir")
        .map(|f| f.expect("entry").path())
        .collect();
    assert_eq!(files.len(), 4);
    let pdf = files
        .iter()
        .find(|p| p.extension().is_some_and(|e| e == "pdf"))
        .expect("a pdf file");
    let bytes = std::fs::read(pdf).expect("read");
    assert!(bytes.starts_with(b"%PDF-"), "a real PDF file");
    let xlsx = files
        .iter()
        .find(|p| p.extension().is_some_and(|e| e == "xlsx"))
        .expect("an xlsx file");
    app.window
        .invoke_preview_excel_import(xlsx.display().to_string().into());
    app.until("excel preview", |vm| project(vm).excel_preview.is_some())
        .await;
    app.window.invoke_clear_excel_preview();
    app.until("excel preview cleared", |vm| {
        project(vm).excel_preview.is_none()
    })
    .await;
    save(&app.frame(), "p12-reports");
    let _ = std::fs::remove_dir_all(&dir);

    // Administration (a development admin: users come from elsewhere).
    page(&app, Page::Admin).await;
    app.window.invoke_verify_audit();
    app.until("audit", |vm| {
        vm.admin_text
            .as_deref()
            .is_some_and(|t| t.contains("intact"))
    })
    .await;
    save(&app.frame(), "p13-admin");

    // Removing the threat removes its links; one undo brings it back.
    page(&app, Page::Threats).await;
    app.window.invoke_select_threat(0);
    app.until("threat selected", |vm| project(vm).data.threat.is_some())
        .await;
    app.window.invoke_remove_threat();
    app.until("removed", |vm| project(vm).threats.is_empty())
        .await;
    app.window.invoke_undo();
    app.until("back", |vm| project(vm).threats.len() == 1).await;
    saved(&app).await;

    drop(app);
    tokio::time::sleep(Duration::from_millis(50)).await;
    server.stop().await;
}
