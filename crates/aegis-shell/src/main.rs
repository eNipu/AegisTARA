//! The `aegis` desktop application.
//!
//! Configuration (environment variables, so a launcher or a shortcut can set
//! them; a settings dialog comes later):
//!
//! | Variable | Meaning |
//! |---|---|
//! | `AEGIS_URL` | The server, for example `https://aegis.example.com`. Default `http://127.0.0.1:50051`. |
//! | `AEGIS_OIDC_ISSUER`, `AEGIS_OIDC_CLIENT_ID` | Company login. |
//! | `AEGIS_OIDC_RESOURCE` | The API the access token is for, if the provider needs it. |
//! | `AEGIS_DEV_USER` | Development login against a server run with `AEGIS_DEV_AUTH=1`. |
//!
//! The window always uses Slint's software renderer: it needs no GPU, so it
//! behaves the same on a laptop, a virtual desktop, and a remote session.

use std::cell::RefCell;
use std::process::ExitCode;
use std::rc::Rc;
use std::sync::Arc;

use aegis_client::commands::Command;
use aegis_client::connection::DevLogin;
use aegis_client::session::{Session, SessionConfig};
use aegis_shell::AppWindow;
use aegis_shell::controller::Controller;
use aegis_shell::provider::{AppProvider, OidcProvider, OidcSettings};
use slint::ComponentHandle;

thread_local! {
    /// The controller of this (UI) thread, reachable from `invoke_from_event_loop`.
    static CONTROLLER: RefCell<Option<Rc<Controller>>> = const { RefCell::new(None) };
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

fn run() -> Result<(), String> {
    let url = env("AEGIS_URL").unwrap_or_else(|| "http://127.0.0.1:50051".to_owned());
    let (provider, user) = if let Some(user) = env("AEGIS_DEV_USER") {
        tracing::warn!("development login as {user}: only for a development server");
        (
            AppProvider::Dev(DevLogin::new(&user, env("AEGIS_DEV_ADMIN").is_some())),
            user,
        )
    } else if let (Some(issuer), Some(client_id)) =
        (env("AEGIS_OIDC_ISSUER"), env("AEGIS_OIDC_CLIENT_ID"))
    {
        let account = url.trim_end_matches('/').to_string();
        let settings = OidcSettings {
            issuer,
            client_id,
            resource: env("AEGIS_OIDC_RESOURCE"),
            account,
        };
        (
            AppProvider::Oidc(OidcProvider::new(settings)),
            String::from("you"),
        )
    } else {
        return Err(
            "no login is configured; set AEGIS_OIDC_ISSUER and AEGIS_OIDC_CLIENT_ID \
                    (or AEGIS_DEV_USER for a development server)"
                .to_owned(),
        );
    };

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

    let provider = Arc::new(provider);
    let (session, _task) = Session::start(&url, Arc::clone(&provider), SessionConfig::new(&user));
    let window = AppWindow::new().map_err(|e| format!("cannot create the window: {e}"))?;

    let log_in = {
        let (provider, session, handle) = (
            Arc::clone(&provider),
            session.clone(),
            runtime.handle().clone(),
        );
        move || {
            let (provider, session) = (Arc::clone(&provider), session.clone());
            handle.spawn(async move {
                if let AppProvider::Oidc(oidc) = &*provider {
                    match oidc.log_in().await {
                        Ok(()) => {
                            let _ = session.send(Command::Reconnect);
                        }
                        Err(e) => tracing::warn!(error = %e, "login failed"),
                    }
                }
            });
        }
    };
    let controller = Controller::attach(&window, session.clone(), log_in);
    CONTROLLER.with(|c| *c.borrow_mut() = Some(Rc::clone(&controller)));
    controller.apply_latest();

    // Saved login: try the refresh token before showing the login button.
    if let AppProvider::Oidc(_) = &*provider {
        let (provider, session) = (Arc::clone(&provider), session.clone());
        runtime.spawn(async move {
            if let AppProvider::Oidc(oidc) = &*provider
                && matches!(oidc.restore().await, Ok(true))
            {
                let _ = session.send(Command::Reconnect);
            }
        });
    }

    // Every new view model wakes the UI thread once; bursts collapse into one
    // redraw because the watch channel only keeps the latest value.
    let mut changes = session.subscribe();
    runtime.spawn(async move {
        while changes.changed().await.is_ok() {
            let _ = slint::invoke_from_event_loop(|| {
                CONTROLLER.with(|c| {
                    if let Some(controller) = c.borrow().as_ref() {
                        controller.apply_latest();
                    }
                });
            });
        }
    });

    window
        .run()
        .map_err(|e| format!("the window failed: {e}"))?;
    let _ = session.send(Command::Shutdown);
    Ok(())
}
