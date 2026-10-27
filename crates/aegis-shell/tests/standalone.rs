mod common;

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use aegis_client::connection::{Connection, DevLogin};
use aegis_client::session::{Session, SessionConfig};
use aegis_client::state::Mode;
use aegis_client::transport::TransportKind;
use aegis_client::view::Page;
use aegis_license::{Issuer, License, TrustedKeys};
use aegis_server::LicenseGate;
use aegis_server::license::system_clock_arc;
use aegis_shell::AppWindow;
use aegis_shell::headless::Headless;
use aegis_shell::launcher::Launcher;
use aegis_shell::standalone::Standalone;
use std::sync::Arc;

fn scratch(name: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "aegis-standalone-{name}-{}-{nonce}",
        std::process::id()
    ))
}

fn test_gate() -> Arc<LicenseGate> {
    let (pkcs8, public) = Issuer::generate().expect("key");
    let issuer = Issuer::from_pkcs8(&pkcs8).expect("issuer");
    let keys = TrustedKeys::single("test", public).expect("keys");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_secs();
    let record = License {
        version: 1,
        id: "standalone-test".into(),
        kid: "test".into(),
        customer: "Standalone Test".into(),
        seats: 1,
        issued_at: now - 1,
        expires_at: now + 3_600,
        grace_days: 0,
        features: vec!["core".into()],
    };
    let license = License::verify(&issuer.sign(&record).expect("signed"), &keys).expect("verified");
    Arc::new(LicenseGate::new(license, system_clock_arc()))
}

#[tokio::test]
async fn a_missing_license_is_refused_before_a_server_starts() {
    let root = scratch("missing-license");
    let err = Standalone::open(&root, &root.join("missing.aegis"))
        .await
        .err()
        .expect("a missing license must not start standalone mode");
    assert!(err.contains("license"), "{err}");
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn a_verified_license_starts_only_a_loopback_server() {
    let root = scratch("loopback");
    let server = Standalone::open_with_gate(&root, test_gate())
        .await
        .expect("standalone server");
    assert!(server.url().starts_with("http://127.0.0.1:"));
    assert!(!server.url().ends_with(":0"));

    let connection = Connection::connect_with(
        server.url(),
        TransportKind::Grpc,
        Arc::new(DevLogin::new("standalone", true)),
    )
    .await
    .expect("connect");
    let info = connection.info().await.expect("server info");
    assert_eq!(info.license.expect("license").customer, "Standalone Test");
    server.stop().await.expect("stop");
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test(flavor = "current_thread")]
async fn the_launcher_reports_a_license_failure_without_showing_login() {
    let root = scratch("launcher-license");
    let headless = Headless::install(1100, 760);
    let window = AppWindow::new().expect("window");
    let launcher = Launcher::new(
        &window,
        tokio::runtime::Handle::current(),
        None,
        Arc::new(|| {}),
    );
    launcher.open_standalone(root.clone(), root.join("missing.aegis"));
    for _ in 0..100 {
        launcher.pump();
        headless.tick();
        if !window.get_login_error().is_empty() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert!(window.get_login_error().contains("license"));
    assert_eq!(window.get_connection(), 4);
    assert!(launcher.controller().session().is_none());
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test(flavor = "current_thread")]
async fn the_launcher_shows_the_projects_of_a_standalone_folder_and_frees_it_again() {
    let root = scratch("launcher-ok");
    let headless = Headless::install(1100, 760);
    let window = AppWindow::new().expect("window");
    let launcher = Launcher::new(
        &window,
        tokio::runtime::Handle::current(),
        None,
        Arc::new(|| {}),
    );
    let server = Standalone::open_with_gate(&root, test_gate())
        .await
        .expect("standalone server");
    launcher.attach_standalone(server, root.clone(), root.join("license.aegis"));
    for _ in 0..300 {
        launcher.pump();
        headless.tick();
        if window.get_connection() == 1 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert_eq!(window.get_connection(), 1, "{}", window.get_login_error());
    assert!(window.get_standalone_mode());
    assert!(window.get_login_error().is_empty());

    // Leaving standalone mode stops the embedded server, which frees the
    // folder for the next one.
    launcher.change_server();
    assert!(!window.get_standalone_mode());
    let mut reopened = None;
    for _ in 0..300 {
        match Standalone::open_with_gate(&root, test_gate()).await {
            Ok(server) => {
                reopened = Some(server);
                break;
            }
            Err(_) => tokio::time::sleep(std::time::Duration::from_millis(10)).await,
        }
    }
    reopened
        .expect("the folder is free after leaving standalone mode")
        .stop()
        .await
        .expect("stop");
    let _ = std::fs::remove_dir_all(root);
}

async fn show_every_page(app: &common::App) {
    for page in Page::ALL {
        app.window.invoke_show_page(page.index() as i32);
        app.until(page.label(), |vm| {
            vm.project.as_ref().is_some_and(|p| p.page == page)
        })
        .await;
        app.frame();
    }
}

#[tokio::test(flavor = "current_thread")]
async fn every_page_works_and_the_folder_reopens_with_history() {
    let root = scratch("all-pages");
    let server = Standalone::open_with_gate(&root, test_gate())
        .await
        .expect("standalone");
    let session = Session::start(
        server.url(),
        Arc::new(DevLogin::new("standalone", true)),
        SessionConfig::new("standalone"),
    )
    .0;
    let app = common::App::new(session, 1360, 860);
    app.online().await;
    app.window.invoke_create_project("Brake ECU".into());
    app.until("created", |vm| vm.project.is_some()).await;
    app.window.invoke_begin_edit();
    app.until("editing", |vm| {
        vm.project.as_ref().is_some_and(|p| p.mode == Mode::Editing)
    })
    .await;
    app.window.invoke_create_sample();
    app.until("sample saved", |vm| {
        vm.project
            .as_ref()
            .is_some_and(|p| p.threats.len() == 2 && p.confirmed_seq > 0)
    })
    .await;
    show_every_page(&app).await;
    let project_id = app
        .session
        .view()
        .project
        .as_ref()
        .expect("project")
        .entry
        .id
        .clone();
    drop(app);
    server.stop().await.expect("stop");

    let server = Standalone::open_with_gate(&root, test_gate())
        .await
        .expect("reopen");
    let session = Session::start(
        server.url(),
        Arc::new(DevLogin::new("standalone", true)),
        SessionConfig::new("standalone"),
    )
    .0;
    let app = common::App::new(session, 1360, 860);
    app.online().await;
    app.window.invoke_open_project(project_id.into());
    app.until("reopened project", |vm| {
        vm.project.as_ref().is_some_and(|p| p.threats.len() == 2)
    })
    .await;
    show_every_page(&app).await;
    // The history page loads its rows after it is shown.
    let view = app
        .until("history from the folder", |vm| {
            vm.project.as_ref().is_some_and(|p| !p.history.is_empty())
        })
        .await;
    let history = &view.project.as_ref().expect("project").history;
    assert!(history.iter().all(|row| row.actor == "standalone"));
    drop(app);
    server.stop().await.expect("stop after reopen");
    let _ = std::fs::remove_dir_all(root);
}
