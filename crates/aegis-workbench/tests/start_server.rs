//! The start screen's "Company server" column: the five connection states,
//! Enter to connect, wrong/right password, creating and reopening a project
//! read-only, Back to Start, sign out and change server (job 208, slice 208e).
#![cfg(any(target_os = "macos", target_os = "windows"))]

use std::fs;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use aegis_client::commands::Command;
use aegis_client::i18n::{Catalog, Language};
use aegis_client::provider::memory_secrets;
use aegis_client::settings::Appearance;
use aegis_client::signin::{LoginForm, SignInView, Stage, status_text};
use aegis_license::TrustedKeys;
use aegis_server::accounts::service::{AccountsService, LoginOptions};
use aegis_server::accounts::{AccountBackend, Accounts, AccountsConfig, MemoryAccounts};
use aegis_server::auth_layer::{AuthLayer, PasswordAuthn};
use aegis_server::config::ServerConfig;
use aegis_server::methodologies::Methodologies;
use aegis_server::{AegisService, MemoryStore, Registry, server};
use aegis_workbench::app::{
    ServerPane, ServerSetup, Shell, StartConfig, Workbench, apply_appearance, bind_all,
};
use gpui::{Entity, TestAppContext, VisualTestContext};
use tokio::net::TcpListener;
use tokio::runtime::{Builder, Runtime};
use tokio::task::JoinHandle;
use tokio_stream::wrappers::TcpListenerStream;

fn runtime() -> Runtime {
    Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("runtime")
}

/// An in-process password server ("Test server") with one admin user.
async fn password_server() -> (String, tokio::sync::oneshot::Sender<()>, JoinHandle<()>) {
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
    let public = AccountsService::new(
        Some(Arc::clone(&accounts)),
        LoginOptions::password("Test server"),
    );
    let auth = AuthLayer::new(Arc::new(PasswordAuthn(Arc::clone(&accounts))), "");
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let task = tokio::spawn(async move {
        server::router(service, public, auth)
            .serve_with_incoming_shutdown(TcpListenerStream::new(listener), async {
                let _ = stopped.await;
                registry.shutdown().await;
            })
            .await
            .expect("serve");
    });
    (url, stop, task)
}

fn scratch(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("aegis-start-server-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    dir
}

struct Window<'a> {
    shell: Entity<Shell>,
    cx: &'a mut VisualTestContext,
    dir: PathBuf,
}

fn open<'a>(cx: &'a mut TestAppContext, rt: &Runtime, name: &str) -> Window<'a> {
    cx.update(|cx| {
        gpui_component::init(cx);
        bind_all(cx);
    });
    let dir = scratch(name);
    let config = StartConfig {
        catalog: Rc::new(Catalog::load(Language::En)),
        appearance: Appearance::Light,
        settings_path: Some(dir.join("settings.json")),
        keys: TrustedKeys::vendor(),
        server: Some(ServerSetup {
            runtime: rt.handle().clone(),
            secrets: memory_secrets(),
            keep: None,
        }),
    };
    let (shell, cx) = cx.add_window_view(|window, cx| {
        apply_appearance(Appearance::Light, window, cx);
        Shell::new(config, window, cx)
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    Window { shell, cx, dir }
}

impl Drop for Window<'_> {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

impl Window<'_> {
    fn pane(&mut self) -> Entity<ServerPane> {
        let shell = self.shell.clone();
        self.cx
            .update(|_, cx| shell.read(cx).server_pane().cloned())
            .expect("server pane")
    }

    fn view(&mut self) -> SignInView {
        let pane = self.pane();
        self.cx.update(|_, cx| pane.read(cx).view())
    }

    fn workbench(&mut self) -> Option<Entity<Workbench>> {
        let shell = self.shell.clone();
        self.cx.update(|_, cx| shell.read(cx).workbench().cloned())
    }

    fn on_start(&mut self) -> bool {
        let shell = self.shell.clone();
        self.cx
            .update(|_, cx| shell.read(cx).start_screen().is_some())
    }

    fn pump(&mut self) {
        let shell = self.shell.clone();
        shell.update_in(self.cx, |s, window, cx| s.pump(window, cx));
        self.cx.run_until_parked();
    }

    fn draw(&mut self) {
        self.cx.update(|window, cx| window.draw(cx).clear(cx));
    }

    fn until(&mut self, what: &str, ready: impl Fn(&mut Self) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            self.pump();
            if ready(self) {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "timed out waiting for: {what}; last view: {:?}",
                self.view()
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn connect(&mut self, url: &str) {
        let pane = self.pane();
        pane.update_in(self.cx, |p, window, cx| {
            p.set_address(url, window, cx);
            p.connect(cx);
        });
        self.cx.run_until_parked();
    }

    fn log_in(&mut self, user: &str, password: &str) {
        let pane = self.pane();
        pane.update_in(self.cx, |p, window, cx| {
            p.set_user(user, window, cx);
            p.set_password(password, window, cx);
            p.log_in(window, cx);
        });
        self.cx.run_until_parked();
    }

    fn run(&mut self, id: &str) {
        let wb = self.workbench().expect("workbench");
        wb.update_in(self.cx, |w, window, cx| w.run(id, window, cx));
        self.cx.run_until_parked();
    }
}

#[gpui::test]
fn the_five_connection_states_and_their_status_lines(cx: &mut TestAppContext) {
    let rt = runtime();
    let (url, stop, task) = rt.block_on(password_server());
    let catalog = Catalog::load(Language::En);
    let mut w = open(cx, &rt, "states");

    // 1. ChooseServer
    let view = w.view();
    assert_eq!(view.stage, Stage::ChooseServer);
    assert_eq!(
        view.status,
        status_text(Stage::ChooseServer, None, "", None, "", &catalog)
    );
    w.draw();

    // 2. Connecting
    w.connect(&url);
    let view = w.view();
    assert_eq!(view.stage, Stage::Connecting);
    assert_eq!(
        view.status,
        status_text(Stage::Connecting, None, &url, None, "", &catalog)
    );
    w.draw();

    // 3. LoginRequired (Password)
    w.until("password form", |w| {
        let v = w.view();
        v.stage == Stage::LoginRequired && v.form == Some(LoginForm::Password)
    });
    let view = w.view();
    assert_eq!(
        view.status,
        status_text(
            Stage::LoginRequired,
            Some(LoginForm::Password),
            &url,
            None,
            "",
            &catalog,
        )
    );
    w.draw();

    // 4. Online
    w.log_in("admin", "a long admin password");
    w.until("online", |w| w.view().stage == Stage::Online);
    let view = w.view();
    assert_eq!(
        view.status,
        status_text(
            Stage::Online,
            Some(LoginForm::Password),
            &url,
            None,
            &view.user,
            &catalog,
        )
    );
    w.draw();

    // 5. Offline after stopping the server
    let _ = stop.send(());
    let _ = rt.block_on(task);
    let pane = w.pane();
    w.cx.update(|_, cx| {
        pane.read(cx)
            .session()
            .expect("session")
            .send(Command::Reconnect)
            .expect("reconnect");
    });
    w.until("offline", |w| w.view().stage == Stage::Offline);
    let view = w.view();
    let conn = w.cx.update(|_, cx| {
        pane.read(cx)
            .session()
            .expect("session")
            .view()
            .connection
            .clone()
    });
    assert_eq!(
        view.status,
        status_text(Stage::Offline, view.form, &url, Some(&conn), "", &catalog)
    );
    w.draw();
}

#[gpui::test]
fn enter_in_the_address_field_connects(cx: &mut TestAppContext) {
    let rt = runtime();
    let (url, stop, _task) = rt.block_on(password_server());
    let mut w = open(cx, &rt, "enter");

    assert_eq!(w.view().stage, Stage::ChooseServer);
    let pane = w.pane();
    pane.update_in(w.cx, |p, window, cx| p.focus_address(window, cx));
    w.cx.run_until_parked();
    w.cx.simulate_input(&url);
    w.cx.simulate_keystrokes("enter");
    w.cx.run_until_parked();

    w.until("left ChooseServer", |w| {
        w.view().stage != Stage::ChooseServer
    });
    let _ = stop.send(());
}

#[gpui::test]
fn wrong_password_shows_credentials_error_and_right_password_logs_in(cx: &mut TestAppContext) {
    let rt = runtime();
    let (url, stop, _task) = rt.block_on(password_server());
    let catalog = Catalog::load(Language::En);
    let mut w = open(cx, &rt, "password");

    w.connect(&url);
    w.until("password form", |w| {
        w.view().stage == Stage::LoginRequired && w.view().form == Some(LoginForm::Password)
    });

    w.log_in("admin", "wrong");
    let expected = catalog.tr("signin-error-credentials");
    w.until("credentials error", |w| {
        let v = w.view();
        !v.busy && v.error.is_some()
    });
    assert_eq!(w.view().error, Some(expected));
    assert_eq!(w.view().stage, Stage::LoginRequired);

    w.log_in("admin", "a long admin password");
    w.until("online", |w| w.view().stage == Stage::Online);
    assert_eq!(w.view().error, None);
    let _ = stop.send(());
}

#[gpui::test]
fn creating_and_reopening_a_project_shows_read_only_workbench(cx: &mut TestAppContext) {
    let rt = runtime();
    let (url, stop, _task) = rt.block_on(password_server());
    let mut w = open(cx, &rt, "project");

    w.connect(&url);
    w.until("password form", |w| {
        w.view().stage == Stage::LoginRequired && w.view().form == Some(LoginForm::Password)
    });
    w.log_in("admin", "a long admin password");
    w.until("online", |w| w.view().stage == Stage::Online);

    let pane = w.pane();
    pane.update_in(w.cx, |p, window, cx| {
        p.set_new_name("Gateway", window, cx);
        p.create_project(window, cx);
    });
    w.until("workbench shown for Gateway", |w| w.workbench().is_some());
    let wb = w.workbench().expect("workbench");
    let (name, read_only) = w.cx.update(|_, cx| {
        let r = wb.read(cx);
        (
            r.project_name().to_owned(),
            r.read_only().map(str::to_owned),
        )
    });
    assert_eq!(name, "Gateway");
    let reason = read_only.expect("read-only reason");
    assert!(reason.contains("Test server"), "{reason}");

    w.run("back-to-start");
    assert!(w.on_start());
    w.until("project closed and Gateway listed", |w| {
        let closed = w.cx.update(|_, cx| {
            pane.read(cx)
                .session()
                .is_some_and(|s| s.view().project.is_none())
        });
        let listed = w.view().projects.iter().any(|p| p.name == "Gateway");
        closed && listed
    });
    assert!(w.on_start());

    let id = w
        .view()
        .projects
        .into_iter()
        .find(|p| p.name == "Gateway")
        .expect("Gateway in projects")
        .id;
    pane.update(w.cx, |p, cx| p.open_project(&id, cx));
    w.until("workbench shown again", |w| w.workbench().is_some());
    let wb = w.workbench().expect("workbench");
    let (name, read_only) = w.cx.update(|_, cx| {
        let r = wb.read(cx);
        (
            r.project_name().to_owned(),
            r.read_only().map(str::to_owned),
        )
    });
    assert_eq!(name, "Gateway");
    assert!(
        read_only.expect("read-only reason").contains("Test server"),
        "expected server name in read-only reason"
    );
    let _ = stop.send(());
}

#[gpui::test]
fn sign_out_and_change_server(cx: &mut TestAppContext) {
    let rt = runtime();
    let (url, stop, _task) = rt.block_on(password_server());
    let mut w = open(cx, &rt, "sign-out");

    w.connect(&url);
    w.until("password form", |w| {
        w.view().stage == Stage::LoginRequired && w.view().form == Some(LoginForm::Password)
    });
    w.log_in("admin", "a long admin password");
    w.until("online", |w| w.view().stage == Stage::Online);

    let pane = w.pane();
    pane.update(w.cx, |p, cx| p.sign_out(cx));
    w.until("login required after sign out", |w| {
        w.view().stage == Stage::LoginRequired
    });

    pane.update(w.cx, |p, cx| p.change_server(cx));
    w.cx.run_until_parked();
    assert_eq!(w.view().stage, Stage::ChooseServer);
    assert!(w.cx.update(|_, cx| pane.read(cx).session().is_none()));
    let _ = stop.send(());
}
