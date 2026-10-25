//! The `aegis-server` binary.
//!
//! Configuration comes from environment variables (12-factor style, so the
//! same container image runs everywhere):
//!
//! | Variable | Default | Meaning |
//! |---|---|---|
//! | `AEGIS_LISTEN` | `127.0.0.1:50051` | Address to serve gRPC on |
//! | `AEGIS_PUBLIC_URL` | (required with OIDC) | The URL clients use, for DPoP proofs |
//! | `AEGIS_OIDC_ISSUER` | unset | Identity provider issuer URL; enables login |
//! | `AEGIS_OIDC_AUDIENCE` | (required with OIDC) | Expected `aud` of access tokens |
//! | `AEGIS_DPOP` | `required` | `optional` accepts unbound bearer tokens (weaker) |
//! | `AEGIS_ADMIN_CLAIM` / `AEGIS_ADMIN_VALUE` | `roles` / `aegis-admin` | Token claim that grants admin |
//! | `AEGIS_DEV_AUTH` | unset | `1` trusts the `x-aegis-dev-user` header. **Development only.** |
//! | `AEGIS_DATABASE_URL` | unset | PostgreSQL connection string. Unset keeps projects in memory only. |
//! | `AEGIS_DB_POOL` | `16` | Database connections held open |
//! | `AEGIS_PACKS_DIR` | unset | Directory of extra methodology pack files |
//! | `RUST_LOG` | `info` | Log filter |
//!
//! TLS ends at the load balancer in the reference deployment (direction
//! section 1), so the server speaks plain HTTP/2 behind it.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use aegis_auth::dpop::DpopVerifier;
use aegis_auth::http::HyperClient;
use aegis_auth::jwks::JwksCache;
use aegis_auth::validate::{AdminRule, Authenticator, DpopPolicy, ValidatorConfig};
use aegis_auth::{oidc, system_clock};
use aegis_server::auth::Principal;
use aegis_server::auth_layer::{AuthLayer, Authn, DevAuthn, OidcAuthn, RequestInfo};
use aegis_server::config::ServerConfig;
use aegis_server::methodologies::Methodologies;
use aegis_server::store::PostgresOptions;
use aegis_server::{AegisService, MemoryStore, PostgresStore, ProjectStore, Registry, server};
use tracing_subscriber::EnvFilter;

/// How callers are authenticated, chosen at startup.
enum Mode {
    Oidc(OidcAuthn<HyperClient>),
    Dev(DevAuthn),
}

impl Authn for Mode {
    async fn authenticate(&self, request: &RequestInfo) -> Result<Principal, String> {
        match self {
            Mode::Oidc(a) => a.authenticate(request).await,
            Mode::Dev(a) => a.authenticate(request).await,
        }
    }
}

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

fn var(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

fn required(name: &str) -> Result<String, String> {
    var(name).ok_or_else(|| format!("{name} is required"))
}

async fn authentication() -> Result<(Mode, String), String> {
    if let Some(issuer) = var("AEGIS_OIDC_ISSUER") {
        let audience = required("AEGIS_OIDC_AUDIENCE")?;
        let public_url = required("AEGIS_PUBLIC_URL")?;
        let policy = match var("AEGIS_DPOP").as_deref() {
            None | Some("required") => DpopPolicy::Required,
            Some("optional") => {
                tracing::warn!("AEGIS_DPOP=optional: unbound bearer tokens are accepted");
                DpopPolicy::Optional
            }
            Some(_) => return Err("AEGIS_DPOP must be required or optional".to_owned()),
        };
        let http = Arc::new(HyperClient::new());
        let discovery = oidc::discover(&*http, &issuer)
            .await
            .map_err(|e| format!("cannot reach the identity provider: {e}"))?;
        let jwks = JwksCache::new(Arc::clone(&http), &discovery.jwks_uri, system_clock);
        let config = ValidatorConfig {
            issuer,
            audience,
            dpop: policy,
            admin: Some(AdminRule {
                claim: var("AEGIS_ADMIN_CLAIM").unwrap_or_else(|| "roles".to_owned()),
                value: var("AEGIS_ADMIN_VALUE").unwrap_or_else(|| "aegis-admin".to_owned()),
            }),
        };
        let authenticator =
            Authenticator::new(config, jwks, DpopVerifier::standard(), system_clock);
        tracing::info!("login: OIDC with DPoP");
        return Ok((Mode::Oidc(OidcAuthn(Arc::new(authenticator))), public_url));
    }
    if var("AEGIS_DEV_AUTH").as_deref() == Some("1") {
        tracing::warn!("AEGIS_DEV_AUTH=1: every caller is trusted. Development only.");
        return Ok((Mode::Dev(DevAuthn), String::new()));
    }
    Err("no authentication is configured; set AEGIS_OIDC_ISSUER (or AEGIS_DEV_AUTH=1 for development)".to_owned())
}

async fn run() -> Result<(), String> {
    let listen: SocketAddr = var("AEGIS_LISTEN")
        .unwrap_or_else(|| "127.0.0.1:50051".to_owned())
        .parse()
        .map_err(|e| format!("AEGIS_LISTEN is not an address: {e}"))?;
    let (mode, public_url) = authentication().await?;

    let mut methodologies = Methodologies::builtin().map_err(|e| e.to_string())?;
    if let Some(dir) = var("AEGIS_PACKS_DIR") {
        methodologies
            .add_dir(&PathBuf::from(dir))
            .map_err(|e| e.to_string())?;
    }
    let methodologies = Arc::new(methodologies);
    let config = Arc::new(ServerConfig::default());
    let layer = AuthLayer::new(Arc::new(mode), &public_url);

    match var("AEGIS_DATABASE_URL") {
        Some(url) => {
            let pool_size = match var("AEGIS_DB_POOL") {
                Some(v) => v
                    .parse()
                    .map_err(|_| "AEGIS_DB_POOL is not a number".to_owned())?,
                None => PostgresOptions::default().pool_size,
            };
            let options = PostgresOptions {
                pool_size,
                ..PostgresOptions::default()
            };
            let store = PostgresStore::connect(&url, &options)
                .await
                .map_err(|e| format!("cannot open the database: {e}"))?;
            tracing::info!("using PostgreSQL");
            serve(
                Arc::new(Registry::new(Arc::new(store), methodologies, config)),
                listen,
                layer,
            )
            .await
        }
        None => {
            tracing::warn!(
                "AEGIS_DATABASE_URL is unset: projects are kept in memory and are lost on exit"
            );
            serve(
                Arc::new(Registry::new(
                    Arc::new(MemoryStore::new()),
                    methodologies,
                    config,
                )),
                listen,
                layer,
            )
            .await
        }
    }
}

async fn serve<S: ProjectStore>(
    registry: Arc<Registry<S>>,
    listen: SocketAddr,
    layer: AuthLayer<Mode>,
) -> Result<(), String> {
    let service = AegisService::new(Arc::clone(&registry)).into_server();
    tracing::info!(%listen, "serving");
    let (stopping, stop_requested) = tokio::sync::oneshot::channel::<()>();
    let serving = server::builder()
        .layer(layer)
        .add_service(service)
        .serve_with_shutdown(listen, async move {
            stop_signal().await;
            tracing::info!("shutting down");
            // Unloading the projects writes their snapshots and ends every
            // `Watch` stream, which lets the graceful drain finish.
            registry.shutdown().await;
            let _ = stopping.send(());
        });
    // A client that keeps a stream open must not keep the process alive for
    // ever (R6): after the grace period the server exits anyway. Everything
    // acknowledged is already stored, so nothing is lost.
    tokio::select! {
        result = serving => result.map_err(|e| format!("server failed: {e}")),
        () = async {
            let _ = stop_requested.await;
            tokio::time::sleep(SHUTDOWN_GRACE).await;
        } => {
            tracing::warn!("connections did not close in time; exiting");
            Ok(())
        }
    }
}

/// How long open connections get to finish after a stop signal.
const SHUTDOWN_GRACE: std::time::Duration = std::time::Duration::from_secs(10);

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
