//! The sign-in controller (job 208) against in-process servers: choose a
//! server, log in with a password or a development user name, keep the
//! login, sign out and change server. No window.

use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use aegis_client::commands::Command;
use aegis_client::i18n::{Catalog, Language};
use aegis_client::provider::memory_secrets;
use aegis_client::settings::Settings;
use aegis_client::signin::{LoginForm, SignIn, SignInView, Stage};
use aegis_server::accounts::service::{AccountsService, LoginOptions};
use aegis_server::accounts::{AccountBackend, Accounts, AccountsConfig, MemoryAccounts};
use aegis_server::auth_layer::{AuthLayer, Authn, DevAuthn, PasswordAuthn};
use aegis_server::config::ServerConfig;
use aegis_server::methodologies::Methodologies;
use aegis_server::{AegisService, MemoryStore, Registry, server};
use tokio::net::TcpListener;
use tokio::runtime::Handle;
use tokio_stream::wrappers::TcpListenerStream;

/// A server whose public login is `options` and whose calls are checked by
/// the `Authn` that `authn` makes from its accounts.
async fn server_with<A: Authn>(
    options: LoginOptions,
    authn: impl FnOnce(Arc<Accounts>) -> A,
) -> (String, tokio::sync::oneshot::Sender<()>) {
    let registry = Arc::new(Registry::new(
        Arc::new(MemoryStore::new()),
        Arc::new(Methodologies::builtin().expect("builtin")),
        Arc::new(ServerConfig::default()),
    ));
    let accounts = Arc::new(
        Accounts::open(
            AccountBackend::Memory(MemoryAccounts::new()),
            AccountsConfig::default(),
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
    let auth = AuthLayer::new(Arc::new(authn(Arc::clone(&accounts))), "");
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
    (url, stop)
}

async fn until(signin: &mut SignIn, what: &str, ready: impl Fn(&SignInView) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        signin.pump();
        let view = signin.view();
        if ready(&view) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for: {what}; last view: {view:?}"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "aegis-signin-{name}-{}-{}",
        std::process::id(),
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

fn sign_in(path: &Path, secrets: &aegis_client::provider::Secrets) -> SignIn {
    SignIn::new(
        Handle::current(),
        Rc::new(Catalog::load(Language::En)),
        Some(path.to_owned()),
        Arc::clone(secrets),
    )
}

#[tokio::test(flavor = "current_thread")]
async fn no_settings_file_waits_on_the_server_form() {
    let dir = temp_dir("fresh");
    let catalog = Catalog::load(Language::En);
    let mut signin = sign_in(&dir.join("settings.json"), &memory_secrets());
    signin.start();
    signin.pump();
    let view = signin.view();
    assert_eq!(view.stage, Stage::ChooseServer);
    assert_eq!(view.status, catalog.tr("signin-choose-server"));
    assert_eq!(view.transport, "auto");
    assert!(signin.session().is_none());
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test(flavor = "current_thread")]
async fn password_login_is_kept_signed_out_and_left() {
    let (url, _stop) = server_with(LoginOptions::password("Test server"), PasswordAuthn).await;
    let dir = temp_dir("password");
    let path = dir.join("settings.json");
    let secrets = memory_secrets();
    let catalog = Catalog::load(Language::En);

    let mut first = sign_in(&path, &secrets);
    first.connect(&format!("  {url}/ "), "auto");
    assert_eq!(first.view().stage, Stage::Connecting);
    until(&mut first, "the password form", |v| {
        v.stage == Stage::LoginRequired && v.form == Some(LoginForm::Password)
    })
    .await;
    assert_eq!(first.view().server_name, "Test server");
    assert_eq!(first.view().server_url, url);

    first.password_log_in("admin", "wrong", false);
    let wrong = catalog.tr("signin-error-credentials");
    until(&mut first, "the wrong password refused", |v| {
        !v.busy && v.error.as_deref() == Some(wrong.as_str())
    })
    .await;
    assert_eq!(first.view().stage, Stage::LoginRequired);

    first.password_log_in(" admin ", "a long admin password", true);
    until(&mut first, "online", |v| v.stage == Stage::Online).await;
    assert_eq!(first.view().error, None);
    first
        .session()
        .expect("session")
        .send(Command::CreateProject {
            name: "Gateway".into(),
            methodology_key: "iso21434-r155".into(),
        })
        .expect("send");
    until(&mut first, "the new project", |v| {
        v.projects.iter().any(|p| p.name == "Gateway")
    })
    .await;
    let saved = Settings::load(&path);
    assert_eq!(saved.server_url, url);
    assert_eq!(saved.username, "admin");
    assert_eq!(saved.transport, "auto");
    drop(first);

    // The kept login: the same settings and keychain, no password typed.
    let mut second = sign_in(&path, &secrets);
    assert_eq!(second.view().server_url, url);
    second.start();
    until(&mut second, "online again without a password", |v| {
        v.stage == Stage::Online
    })
    .await;
    assert_eq!(second.view().user_name, "admin");

    second.sign_out();
    until(&mut second, "signed out", |v| {
        v.stage == Stage::LoginRequired
    })
    .await;
    assert_eq!(second.view().status, catalog.tr("signin-login-password"));

    second.change_server();
    let view = second.view();
    assert_eq!(view.stage, Stage::ChooseServer);
    assert_eq!(view.form, None);
    assert!(second.session().is_none());

    // Signing out forgot the kept login.
    drop(second);
    let mut third = sign_in(&path, &secrets);
    third.start();
    until(&mut third, "a login asked again", |v| {
        v.stage == Stage::LoginRequired
    })
    .await;
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test(flavor = "current_thread")]
async fn development_server_takes_a_user_name() {
    let (url, _stop) = server_with(LoginOptions::development("Dev server"), |_| DevAuthn).await;
    let dir = temp_dir("dev");
    let mut signin = sign_in(&dir.join("settings.json"), &memory_secrets());
    signin.connect(&url, "grpc-web");
    until(&mut signin, "the user name form", |v| {
        v.stage == Stage::LoginRequired && v.form == Some(LoginForm::Development)
    })
    .await;
    assert!(signin.session().is_none());
    assert_eq!(signin.view().transport, "grpc-web");

    let before = signin.view();
    signin.dev_log_in("  ");
    signin.pump();
    assert_eq!(signin.view(), before);
    assert!(signin.session().is_none());

    signin.dev_log_in("dana");
    until(&mut signin, "online as dana", |v| v.stage == Stage::Online).await;
    assert_eq!(signin.settings().username, "dana");
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test(flavor = "current_thread")]
async fn bad_addresses_stay_on_the_server_form() {
    let dir = temp_dir("bad");
    let mut signin = sign_in(&dir.join("settings.json"), &memory_secrets());

    signin.connect("not a url", "auto");
    let view = signin.view();
    assert_eq!(view.stage, Stage::ChooseServer);
    assert!(view.error.is_some());
    assert!(!view.busy);

    signin.connect("http://127.0.0.1:1", "grpc");
    until(&mut signin, "the unreachable server refused", |v| {
        v.stage == Stage::ChooseServer && !v.busy && v.error.is_some()
    })
    .await;
    let error = signin.view().error.unwrap_or_default();
    assert!(error.starts_with("Could not reach"), "{error}");
    assert!(signin.session().is_none());
    assert!(signin.settings().server_url.is_empty());
    let _ = std::fs::remove_dir_all(dir);
}
