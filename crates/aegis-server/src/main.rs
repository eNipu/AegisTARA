//! The `aegis-server` binary.
//!
//! Configuration comes from environment variables (12-factor style, so the
//! same container image runs everywhere):
//!
//! | Variable | Default | Meaning |
//! |---|---|---|
//! | `AEGIS_LISTEN` | `127.0.0.1:50051` | Address to serve gRPC on |
//! | `AEGIS_PACKS_DIR` | unset | Directory of extra methodology pack files |
//! | `AEGIS_DEV_AUTH` | unset | `1` trusts the `x-aegis-dev-user` header. **Development only.** |
//! | `RUST_LOG` | `info` | Log filter |
//!
//! TLS ends at the load balancer in the reference deployment (direction
//! section 1), so the server speaks plain HTTP/2 behind it.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use aegis_server::auth::dev_interceptor;
use aegis_server::config::ServerConfig;
use aegis_server::methodologies::Methodologies;
use aegis_server::{AegisService, MemoryStore, Registry, server};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            tracing::error!("{message}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<(), String> {
    let listen: SocketAddr = std::env::var("AEGIS_LISTEN")
        .unwrap_or_else(|_| "127.0.0.1:50051".to_owned())
        .parse()
        .map_err(|e| format!("AEGIS_LISTEN is not an address: {e}"))?;

    if std::env::var("AEGIS_DEV_AUTH").as_deref() != Ok("1") {
        return Err(
            "no authentication is configured; set AEGIS_DEV_AUTH=1 for development".to_owned(),
        );
    }
    tracing::warn!("AEGIS_DEV_AUTH=1: every caller is trusted. Development only.");

    let mut methodologies = Methodologies::builtin().map_err(|e| e.to_string())?;
    if let Some(dir) = std::env::var_os("AEGIS_PACKS_DIR") {
        methodologies
            .add_dir(&PathBuf::from(dir))
            .map_err(|e| e.to_string())?;
    }

    tracing::warn!("projects are kept in memory only and are lost on exit");
    let registry = Arc::new(Registry::new(
        Arc::new(MemoryStore::new()),
        Arc::new(methodologies),
        Arc::new(ServerConfig::default()),
    ));
    let service = AegisService::new(Arc::clone(&registry)).into_server();

    tracing::info!(%listen, "serving");
    let on_stop = Arc::clone(&registry);
    server::builder()
        .layer(tonic::service::InterceptorLayer::new(dev_interceptor))
        .add_service(service)
        .serve_with_shutdown(listen, async move {
            stop_signal().await;
            tracing::info!("shutting down");
            // Unloading the projects writes their snapshots and ends every
            // `Watch` stream, which lets the graceful drain finish.
            on_stop.shutdown().await;
        })
        .await
        .map_err(|e| format!("server failed: {e}"))
}

/// Resolves on Ctrl-C, or on SIGTERM where it exists.
async fn stop_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        if let Ok(mut term) = signal(SignalKind::terminate()) {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {}
                _ = term.recv() => {}
            }
            return;
        }
    }
    let _ = tokio::signal::ctrl_c().await;
}
