//! The login screens in the real window, against a server with password
//! login (`AEGIS_AUTH=password`), driven through the launcher exactly as the
//! app runs it: type the address, the server asks for a password, log in,
//! work, log out.

use std::sync::Arc;
use std::time::{Duration, Instant};

use aegis_server::accounts::service::{AccountsService, LoginOptions};
use aegis_server::accounts::{AccountBackend, Accounts, AccountsConfig, MemoryAccounts};
use aegis_server::auth_layer::{AuthLayer, PasswordAuthn};
use aegis_server::config::ServerConfig;
use aegis_server::methodologies::Methodologies;
use aegis_server::pb::get_login_options_response::Method;
use aegis_server::{AegisService, MemoryStore, Registry, server};
use aegis_shell::AppWindow;
use aegis_shell::headless::Headless;
use aegis_shell::launcher::Launcher;
use slint::{ComponentHandle, Model};
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;

async fn password_server() -> (String, tokio::sync::oneshot::Sender<()>) {
    server_with(LoginOptions::password("Test server"), false)
        .await
        .0
}

/// A server whose public login is `options`. With `company_login` the admin
/// may turn password login off (job 045). Returns the accounts too.
async fn server_with(
    options: LoginOptions,
    company_login: bool,
) -> ((String, tokio::sync::oneshot::Sender<()>), Arc<Accounts>) {
    let registry = Arc::new(Registry::new(
        Arc::new(MemoryStore::new()),
        Arc::new(Methodologies::builtin().expect("builtin")),
        Arc::new(ServerConfig::default()),
    ));
    let accounts = Arc::new(
        Accounts::open(
            AccountBackend::Memory(MemoryAccounts::new()),
            AccountsConfig {
                company_login,
                ..AccountsConfig::default()
            },
        )
        .await
        .expect("accounts"),
    );
    accounts
        .bootstrap_admin("admin", "a long admin password")
        .await
        .expect("admin");
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let url = format!("http://{}", listener.local_addr().expect("addr"));
    let service = AegisService::new(Arc::clone(&registry)).with_accounts(Arc::clone(&accounts));
    let public = AccountsService::new(Some(Arc::clone(&accounts)), options);
    let auth = AuthLayer::new(Arc::new(PasswordAuthn(Arc::clone(&accounts))), "");
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    tokio::spawn(async move {
        server::router(service, public, auth)
            .serve_with_incoming_shutdown(TcpListenerStream::new(listener), async {
                let _ = stopped.await;
                registry.shutdown().await;
            })
            .await
            .expect("serve");
    });
    ((url, stop), accounts)
}

async fn until(
    headless: &Headless,
    launcher: &std::rc::Rc<Launcher>,
    what: &str,
    ready: impl Fn() -> bool,
) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        launcher.pump();
        headless.tick();
        if ready() {
            return;
        }
        assert!(Instant::now() < deadline, "timed out waiting for: {what}");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[tokio::test(flavor = "current_thread")]
async fn a_user_logs_in_with_a_password_and_out_again() {
    let (url, stop) = password_server().await;
    let headless = Headless::install(1100, 760);
    let window = AppWindow::new().expect("window");
    window
        .window()
        .set_size(slint::PhysicalSize::new(1100, 760));
    let launcher = Launcher::new(
        &window,
        tokio::runtime::Handle::current(),
        None,
        Arc::new(|| {}),
    );
    window.show().expect("show");

    // Nothing remembered: the server screen.
    launcher.start();
    assert_eq!(window.get_connection(), 4);
    let frame = headless.render();
    common_save(&frame, "l01-server");

    // A bad address is refused before any connection.
    window.invoke_connect_server("not a url".into(), 0);
    assert_eq!(window.get_connection(), 4);
    assert!(window.get_connection_text().contains("http"));

    // The server asks for a password.
    window.invoke_connect_server(url.as_str().into(), 2);
    until(&headless, &launcher, "password form", || {
        window.get_connection() == 3 && window.get_login_method() == 1
    })
    .await;
    assert_eq!(window.get_server_name().as_str(), "Test server");
    common_save(&headless.render(), "l02-password");

    // A wrong password says so and stays on the form.
    window.invoke_password_log_in("admin".into(), "wrong password!".into(), false);
    until(&headless, &launcher, "error shown", || {
        !window.get_login_error().is_empty()
    })
    .await;
    assert!(
        window.get_login_error().contains("wrong"),
        "{}",
        window.get_login_error()
    );
    assert_eq!(window.get_connection(), 3);

    // The right one connects, and the server says who we are.
    window.invoke_password_log_in("Admin".into(), "a long admin password".into(), false);
    until(&headless, &launcher, "online", || {
        window.get_connection() == 1
    })
    .await;
    until(&headless, &launcher, "who am i", || {
        window.get_is_admin() && window.get_is_password()
    })
    .await;
    assert_eq!(window.get_user().as_str(), "admin");
    common_save(&headless.render(), "l03-projects");

    // Work: create a project and take the edit lock (the lock holder is the
    // server's subject, so the band must say "editing", not "someone else").
    window.invoke_create_project("Gateway".into());
    until(&headless, &launcher, "project", || window.get_has_project()).await;
    window.invoke_begin_edit();
    until(&headless, &launcher, "editing", || window.get_editing()).await;
    assert_eq!(window.get_band_kind(), 2);

    // The administration page shows the event log in words, a filter, and
    // the license facts (this test server runs without a license).
    window.invoke_show_page(12);
    until(&headless, &launcher, "events", || {
        window.get_event_rows().row_count() >= 2
    })
    .await;
    let cell = |row: usize, col: usize| {
        window
            .get_event_rows()
            .row_data(row)
            .expect("row")
            .row_data(col)
            .expect("cell")
            .text
            .to_string()
    };
    assert_eq!(cell(0, 1), "admin");
    assert_eq!(cell(0, 2), "Logged in");
    window.invoke_set_event_filter(2); // exports: none yet
    until(&headless, &launcher, "filtered", || {
        window.get_event_rows().row_count() == 0
    })
    .await;
    assert!(
        window.get_events_text().contains("No events"),
        "{}",
        window.get_events_text()
    );
    window.invoke_set_event_filter(3); // admin actions: the first admin's creation
    until(&headless, &launcher, "admin actions", || {
        window.get_event_rows().row_count() >= 1
    })
    .await;
    assert_eq!(cell(0, 2), "Created a user");
    window.invoke_set_event_filter(0);
    assert!(
        window.get_license_text().contains("No license"),
        "{}",
        window.get_license_text()
    );
    window.set_admin_tab(1);
    common_save(&headless.render(), "l03b-admin-events");

    // The sessions tab (job 055): the admin's own session is listed; ending
    // it is a log out, so only look and select here.
    window.set_admin_tab(2);
    window.invoke_refresh_sessions();
    until(&headless, &launcher, "sessions", || {
        window.get_session_rows().row_count() >= 1
    })
    .await;
    let first = window
        .get_session_rows()
        .row_data(0)
        .expect("row")
        .row_data(0)
        .expect("cell")
        .text
        .to_string();
    assert_eq!(first, "admin");
    window.invoke_pick_session(0);
    until(&headless, &launcher, "selected", || {
        window.get_session_selected() == 0
    })
    .await;
    common_save(&headless.render(), "l03c-admin-sessions");
    window.set_admin_tab(0);

    // Log out: back to the password form, and the old token is gone.
    window.invoke_log_out();
    until(&headless, &launcher, "logged out", || {
        window.get_connection() == 3
    })
    .await;
    assert!(!window.get_has_project());

    // Change server: back to the server screen.
    window.invoke_change_server();
    assert_eq!(window.get_connection(), 4);

    drop(launcher);
    let _ = stop.send(());
}

/// A server with company login and a password form: the company login leads,
/// the password form is one click away, and after an admin turns it off the
/// app shows only the company login (job 045).
#[tokio::test(flavor = "current_thread")]
async fn the_password_form_disappears_when_an_admin_turns_it_off() {
    let options = LoginOptions {
        method: Method::Oidc,
        // Nothing listens here: the test never signs in with the company login.
        oidc_issuer: "http://127.0.0.1:1".into(),
        oidc_client_id: "app".into(),
        oidc_resource: String::new(),
        server_name: "Both ways".into(),
    };
    let ((url, stop), accounts) = server_with(options, true).await;
    let headless = Headless::install(1100, 760);
    let window = AppWindow::new().expect("window");
    window
        .window()
        .set_size(slint::PhysicalSize::new(1100, 760));
    let launcher = Launcher::new(
        &window,
        tokio::runtime::Handle::current(),
        None,
        Arc::new(|| {}),
    );
    window.show().expect("show");
    launcher.start();

    // The company login leads, with a way to the password form.
    window.invoke_connect_server(url.as_str().into(), 2);
    until(&headless, &launcher, "company login", || {
        window.get_connection() == 3 && window.get_login_method() == 0
    })
    .await;
    assert!(window.get_password_also());
    common_save(&headless.render(), "l04-company-with-password");

    // One click opens the password form; it can go back.
    window.invoke_use_password_login();
    assert_eq!(window.get_login_method(), 1);
    assert!(window.get_password_also());
    common_save(&headless.render(), "l05-password-with-company");
    window.invoke_use_company_login();
    until(&headless, &launcher, "back to company", || {
        window.get_connection() == 3 && window.get_login_method() == 0
    })
    .await;

    // Log in with the password and turn the form off from the admin page.
    window.invoke_use_password_login();
    window.invoke_password_log_in("admin".into(), "a long admin password".into(), false);
    until(&headless, &launcher, "online as admin", || {
        window.get_connection() == 1 && window.get_is_admin()
    })
    .await;
    window.invoke_create_project("Gateway".into());
    until(&headless, &launcher, "project", || window.get_has_project()).await;
    window.invoke_show_page(12);
    until(&headless, &launcher, "settings loaded", || {
        window.get_settings_known() && window.get_password_login_on()
    })
    .await;
    assert!(window.get_company_login_available());
    common_save(&headless.render(), "l06-admin-login-switch");
    window.invoke_set_password_login(false);
    until(&headless, &launcher, "switched off", || {
        !accounts.password_login_enabled()
    })
    .await;
    // The admin's own password session ended with it.
    until(&headless, &launcher, "login required", || {
        window.get_connection() == 3
    })
    .await;

    // Trying the password form now says why, and shows the company login only.
    window.invoke_password_log_in("admin".into(), "a long admin password".into(), false);
    until(&headless, &launcher, "company login only", || {
        window.get_login_method() == 0 && !window.get_password_also()
    })
    .await;
    assert!(
        window.get_login_error().contains("turned off"),
        "{}",
        window.get_login_error()
    );
    common_save(&headless.render(), "l07-company-only");

    // Turned on again, a new connection offers the password form once more.
    accounts.set_password_login("test", true).await.expect("on");
    window.invoke_change_server();
    window.invoke_connect_server(url.as_str().into(), 2);
    until(&headless, &launcher, "both again", || {
        window.get_connection() == 3 && window.get_password_also()
    })
    .await;

    drop(launcher);
    let _ = stop.send(());
}

fn common_save(frame: &aegis_shell::headless::Frame, name: &str) {
    let dir =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/shell-screenshots");
    std::fs::create_dir_all(&dir).expect("dir");
    let file = std::fs::File::create(dir.join(format!("{name}.png"))).expect("file");
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), frame.width, frame.height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .expect("header")
        .write_image_data(&frame.rgb)
        .expect("png");
}
