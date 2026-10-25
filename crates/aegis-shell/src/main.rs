//! The `aegis` desktop application.
//!
//! The user types the server address once; the server says how to log in
//! (password, company login, or development), and the app remembers the
//! address in its settings file (`aegis_client::settings`).
//!
//! Environment variables for development and tests (a launcher or a
//! shortcut can set them):
//!
//! | Variable | Meaning |
//! |---|---|
//! | `AEGIS_URL` | The server, for example `https://aegis.example.com`. Overrides the settings. |
//! | `AEGIS_TRANSPORT` | `auto` (default), `grpc` or `grpc-web`. |
//! | `AEGIS_DEV_USER` | Development login against a server run with `AEGIS_DEV_AUTH=1`, skipping the screens. |
//! | `AEGIS_DEV_ADMIN` | With `AEGIS_DEV_USER`: also an admin. |
//!
//! The window always uses Slint's software renderer: it needs no GPU, so it
//! behaves the same on a laptop, a virtual desktop, and a remote session.

use std::cell::RefCell;
use std::process::ExitCode;
use std::rc::Rc;
use std::sync::Arc;

use aegis_client::commands::Command;
use aegis_client::transport::TransportKind;
use aegis_shell::AppWindow;
use aegis_shell::launcher::Launcher;
use slint::ComponentHandle;

thread_local! {
    /// The launcher of this (UI) thread, reachable from `invoke_from_event_loop`.
    static LAUNCHER: RefCell<Option<Rc<Launcher>>> = const { RefCell::new(None) };
}

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("aegis: {message}");
            ExitCode::FAILURE
        }
    }
}

/// Asks the UI thread to handle what the network brought.
fn wake() {
    let _ = slint::invoke_from_event_loop(|| {
        LAUNCHER.with(|l| {
            if let Some(launcher) = l.borrow().as_ref() {
                launcher.pump();
            }
        });
    });
}

fn run() -> Result<(), String> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .thread_name("aegis-net")
        .build()
        .map_err(|e| format!("cannot start the network runtime: {e}"))?;
    let _enter = runtime.enter();

    // Software rendering on every platform (see the module docs).
    slint::BackendSelector::new()
        .backend_name("winit".into())
        .renderer_name("software".into())
        .select()
        .map_err(|e| format!("cannot open a window: {e}"))?;

    let window = AppWindow::new().map_err(|e| format!("cannot create the window: {e}"))?;
    let settings_path =
        aegis_client::settings::config_dir().map(|d| aegis_client::settings::Settings::path_in(&d));
    let launcher = Launcher::new(
        &window,
        runtime.handle().clone(),
        settings_path,
        Arc::new(wake),
    );
    LAUNCHER.with(|l| *l.borrow_mut() = Some(Rc::clone(&launcher)));

    // Every new view model of a session wakes the UI thread once; bursts
    // collapse into one redraw because the watch channel only keeps the
    // latest value.
    let handle = runtime.handle().clone();
    let watcher: Rc<RefCell<Option<tokio::task::JoinHandle<()>>>> = Rc::new(RefCell::new(None));
    {
        let watcher = Rc::clone(&watcher);
        launcher.on_session(move |session| {
            let mut changes = session.subscribe();
            let task = handle.spawn(async move {
                while changes.changed().await.is_ok() {
                    wake();
                }
            });
            if let Some(old) = watcher.borrow_mut().replace(task) {
                old.abort();
            }
        });
    }

    if let Some(url) = env("AEGIS_URL") {
        window.set_server_url(url.as_str().into());
    }
    let transport = env("AEGIS_TRANSPORT").unwrap_or_default();
    match (env("AEGIS_DEV_USER"), env("AEGIS_URL")) {
        (Some(user), url) => {
            tracing::warn!("development login as {user}: only for a development server");
            let url = url.unwrap_or_else(|| "http://127.0.0.1:50051".to_owned());
            let kind = TransportKind::parse(&transport, &url)
                .map_err(|e| format!("AEGIS_TRANSPORT: {e}"))?;
            launcher.start_dev(&url, &user, env("AEGIS_DEV_ADMIN").is_some(), kind);
        }
        (None, Some(url)) => {
            let position = match transport.as_str() {
                "grpc" => 1,
                "grpc-web" => 2,
                _ => 0,
            };
            launcher.connect(&url, position);
        }
        (None, None) => launcher.start(),
    }

    window
        .run()
        .map_err(|e| format!("the window failed: {e}"))?;
    if let Some(session) = launcher.controller().session() {
        let _ = session.send(Command::Shutdown);
    }
    Ok(())
}
