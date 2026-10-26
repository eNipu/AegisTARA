//! The `aegis-server` binary.
//!
//! Configuration comes from environment variables (12-factor style, so the
//! same container image runs everywhere):
//!
//! | Variable | Default | Meaning |
//! |---|---|---|
//! | `AEGIS_LISTEN` | `127.0.0.1:50051` | Address to serve on |
//! | `PORT` | unset | Set by hosting platforms (Railway): serve on `0.0.0.0:$PORT` when `AEGIS_LISTEN` is unset |
//! | `AEGIS_AUTH` | from the variables below | `password`, `oidc`, `both` or `dev`: how callers log in. `both` is company login plus password login, which an admin can turn off in the app |
//! | `AEGIS_PASSWORD_LOGIN` | unset | With `both`: `on` or `off` forces the switch at start (the way back in when the company login is broken) |
//! | `AEGIS_ADMIN_USERNAME` | `admin` | Password login: name of the first administrator |
//! | `AEGIS_ADMIN_PASSWORD` | (required while no user exists) | Password login: password of the first administrator, used only when the user table is empty |
//! | `AEGIS_SESSION_HOURS` | `12` | Password login: how long a session lasts |
//! | `AEGIS_SERVER_NAME` | `AegisTARA` | Shown on the login screen |
//! | `AEGIS_PUBLIC_URL` | (required with OIDC) | The URL clients use, for DPoP proofs |
//! | `AEGIS_OIDC_ISSUER` | unset | Identity provider issuer URL; selects OIDC when `AEGIS_AUTH` is unset |
//! | `AEGIS_OIDC_AUDIENCE` | (required with OIDC) | Expected `aud` of access tokens |
//! | `AEGIS_OIDC_CLIENT_ID` | unset | The desktop app's client ID, told to clients by `GetLoginOptions` |
//! | `AEGIS_OIDC_RESOURCE` | unset | The API resource, told to clients |
//! | `AEGIS_DPOP` | `required` | `optional` accepts unbound bearer tokens (weaker) |
//! | `AEGIS_ADMIN_CLAIM` / `AEGIS_ADMIN_VALUE` | `roles` / `aegis-admin` | Token claim that grants admin |
//! | `AEGIS_DEV_AUTH` | unset | `1` trusts the `x-aegis-dev-user` header. **Development only.** |
//! | `AEGIS_DATABASE_URL` | unset | PostgreSQL connection string. Unset keeps everything in memory only. `DATABASE_URL` is read when this is unset. |
//! | `AEGIS_DB_TLS` | `off` | `off`, `encrypt` (libpq `require`) or `verify` (libpq `verify-full`) |
//! | `AEGIS_LICENSE_FILE` | (required) | Path of the signed license file. The server does not start without a valid one. |
//! | `AEGIS_DEV_LICENSE` | unset | `1` skips the check. Only in builds with the `dev-license` feature. |
//! | `AEGIS_DB_POOL` | `16` | Database connections held open |
//! | `AEGIS_EVENT_RETENTION_DAYS` | `365` | Events older than this are removed from the event log (hourly) |
//! | `AEGIS_EVENT_MAX_ROWS` | `1000000` | Most events kept; the oldest go first, so a flood of failed logins cannot fill the disk |
//! | `AEGIS_PACKS_DIR` | unset | Directory of extra methodology pack files |
//! | `RUST_LOG` | `info` | Log filter |
//!
//! TLS ends at the load balancer in the reference deployment (direction
//! section 1), so the server speaks plain HTTP/2 and HTTP/1.1 (gRPC-Web)
//! behind it. `GET /healthz` answers `200` for health probes.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use aegis_auth::dpop::DpopVerifier;
use aegis_auth::http::HyperClient;
use aegis_auth::jwks::JwksCache;
use aegis_auth::validate::{AdminRule, Authenticator, DpopPolicy, ValidatorConfig};
use aegis_auth::{oidc, system_clock};
use aegis_license::{DAY, LicenseState, TrustedKeys};
use aegis_server::accounts::events::spawn_trimmer;
use aegis_server::accounts::service::{AccountsService, LoginOptions};
use aegis_server::accounts::{
    AccountBackend, Accounts, AccountsConfig, EventLog, MemoryAccounts, PostgresAccounts,
    Retention, spawn_purger,
};
use aegis_server::auth::Principal;
use aegis_server::auth_layer::{
    AuthLayer, Authn, DevAuthn, HybridAuthn, OidcAuthn, PasswordAuthn, RequestInfo,
};
use aegis_server::config::ServerConfig;
use aegis_server::license::system_clock_arc;
use aegis_server::methodologies::Methodologies;
use aegis_server::pb::get_login_options_response::Method;
use aegis_server::store::{DbTls, PostgresOptions};
use aegis_server::{
    AegisService, LicenseGate, MemoryStore, PostgresStore, ProjectStore, Registry, server,
};
use tracing_subscriber::EnvFilter;

/// How callers are authenticated, chosen at startup.
enum Mode {
    Oidc(OidcAuthn<HyperClient>),
    Password(PasswordAuthn),
    Both(HybridAuthn<HyperClient>),
    Dev(DevAuthn),
}

impl Authn for Mode {
    async fn authenticate(&self, request: &RequestInfo) -> Result<Principal, String> {
        match self {
            Mode::Oidc(a) => a.authenticate(request).await,
            Mode::Password(a) => a.authenticate(request).await,
            Mode::Both(a) => a.authenticate(request).await,
            Mode::Dev(a) => a.authenticate(request).await,
        }
    }
}

/// The login method named by the environment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AuthKind {
    Password,
    Oidc,
    /// Company login plus password login, which an admin may turn off.
    Both,
    Dev,
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

fn auth_kind() -> Result<AuthKind, String> {
    match var("AEGIS_AUTH").as_deref() {
        Some("password") => Ok(AuthKind::Password),
        Some("oidc") => Ok(AuthKind::Oidc),
        Some("both") => Ok(AuthKind::Both),
        Some("dev" | "development") => Ok(AuthKind::Dev),
        Some(_) => Err("AEGIS_AUTH must be password, oidc, both or dev".to_owned()),
        None if var("AEGIS_OIDC_ISSUER").is_some() => Ok(AuthKind::Oidc),
        None if var("AEGIS_DEV_AUTH").as_deref() == Some("1") => Ok(AuthKind::Dev),
        None => Err(
            "no authentication is configured; set AEGIS_AUTH=password (test servers), \
             AEGIS_OIDC_ISSUER (company login), or AEGIS_DEV_AUTH=1 (development)"
                .to_owned(),
        ),
    }
}

async fn oidc_mode() -> Result<(Mode, String, LoginOptions), String> {
    let issuer = required("AEGIS_OIDC_ISSUER")?;
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
        issuer: issuer.clone(),
        audience,
        dpop: policy,
        admin: Some(AdminRule {
            claim: var("AEGIS_ADMIN_CLAIM").unwrap_or_else(|| "roles".to_owned()),
            value: var("AEGIS_ADMIN_VALUE").unwrap_or_else(|| "aegis-admin".to_owned()),
        }),
    };
    let authenticator = Authenticator::new(config, jwks, DpopVerifier::standard(), system_clock);
    tracing::info!("login: OIDC with DPoP");
    let options = LoginOptions {
        method: Method::Oidc,
        oidc_issuer: issuer,
        oidc_client_id: var("AEGIS_OIDC_CLIENT_ID").unwrap_or_default(),
        oidc_resource: var("AEGIS_OIDC_RESOURCE").unwrap_or_default(),
        server_name: server_name(),
    };
    Ok((
        Mode::Oidc(OidcAuthn(Arc::new(authenticator))),
        public_url,
        options,
    ))
}

fn server_name() -> String {
    var("AEGIS_SERVER_NAME").unwrap_or_else(|| "AegisTARA".to_owned())
}

/// Opens the password accounts and makes sure an administrator exists.
async fn password_accounts(
    backend: AccountBackend,
    company_login: bool,
) -> Result<Arc<Accounts>, String> {
    let hours: u64 = match var("AEGIS_SESSION_HOURS") {
        Some(v) => v
            .parse()
            .ok()
            .filter(|h| (1..=24 * 30).contains(h))
            .ok_or_else(|| "AEGIS_SESSION_HOURS must be 1 to 720".to_owned())?,
        None => 12,
    };
    let config = AccountsConfig {
        session_ttl: Duration::from_secs(hours * 3600),
        company_login,
        ..AccountsConfig::default()
    };
    let accounts = Accounts::open(backend, config)
        .await
        .map_err(|e| format!("cannot load the user accounts: {e}"))?;
    if accounts.user_count() == 0 {
        let username = var("AEGIS_ADMIN_USERNAME").unwrap_or_else(|| "admin".to_owned());
        let password = var("AEGIS_ADMIN_PASSWORD").ok_or_else(|| {
            "password login has no user yet; set AEGIS_ADMIN_PASSWORD (and optionally \
             AEGIS_ADMIN_USERNAME) to create the first administrator"
                .to_owned()
        })?;
        accounts
            .bootstrap_admin(&username, &password)
            .await
            .map_err(|e| format!("cannot create the first administrator: {e}"))?;
    } else if var("AEGIS_ADMIN_PASSWORD").is_some() {
        tracing::info!(
            "AEGIS_ADMIN_PASSWORD is ignored: users exist already. Change passwords in the app."
        );
    }
    if company_login {
        match var("AEGIS_PASSWORD_LOGIN").as_deref() {
            None => {}
            Some("on") => accounts
                .set_password_login("startup", true)
                .await
                .map_err(|e| e.to_string())?,
            Some("off") => accounts
                .set_password_login("startup", false)
                .await
                .map_err(|e| e.to_string())?,
            Some(_) => return Err("AEGIS_PASSWORD_LOGIN must be on or off".to_owned()),
        }
    }
    tracing::info!(
        users = accounts.user_count(),
        enabled = accounts.password_login_enabled(),
        "login: password"
    );
    Ok(Arc::new(accounts))
}

/// How long events stay in the event log and how many are kept.
fn retention() -> Result<Retention, String> {
    let mut retention = Retention::default();
    if let Some(v) = var("AEGIS_EVENT_RETENTION_DAYS") {
        let days: u64 = v
            .parse()
            .ok()
            .filter(|d| (1..=3650).contains(d))
            .ok_or_else(|| "AEGIS_EVENT_RETENTION_DAYS must be 1 to 3650".to_owned())?;
        retention.max_age = Duration::from_secs(days * 86_400);
    }
    if let Some(v) = var("AEGIS_EVENT_MAX_ROWS") {
        retention.max_events = v
            .parse()
            .ok()
            .filter(|n| (1000..=100_000_000).contains(n))
            .ok_or_else(|| "AEGIS_EVENT_MAX_ROWS must be 1000 to 100000000".to_owned())?;
    }
    Ok(retention)
}

/// Loads and checks the license. The server does not start without one.
fn license() -> Result<Option<Arc<LicenseGate>>, String> {
    #[cfg(feature = "dev-license")]
    if var("AEGIS_DEV_LICENSE").as_deref() == Some("1") {
        tracing::warn!("AEGIS_DEV_LICENSE=1: no license is checked. Development only.");
        return Ok(None);
    }
    let path = required("AEGIS_LICENSE_FILE")?;
    let text = std::fs::read_to_string(&path)
        .map_err(|e| format!("cannot read the license file {path}: {e}"))?;
    let gate = LicenseGate::load(&text, &TrustedKeys::vendor(), system_clock_arc())
        .map_err(|e| format!("the license file {path} cannot be used: {e}"))?;
    let info = gate.info();
    match gate.state() {
        LicenseState::Grace { ends_in } => tracing::warn!(
            customer = %info.customer,
            days_left = ends_in / DAY,
            "the license has expired and is in its grace period; renew it"
        ),
        LicenseState::Active { expires_in } if expires_in < 30 * DAY => tracing::warn!(
            customer = %info.customer,
            days_left = expires_in / DAY,
            "the license expires soon"
        ),
        _ => {}
    }
    tracing::info!(customer = %info.customer, seats = info.seats, "licensed");
    Ok(Some(Arc::new(gate)))
}

fn listen_address() -> Result<SocketAddr, String> {
    if let Some(listen) = var("AEGIS_LISTEN") {
        return listen
            .parse()
            .map_err(|e| format!("AEGIS_LISTEN is not an address: {e}"));
    }
    if let Some(port) = var("PORT") {
        let port: u16 = port
            .parse()
            .map_err(|_| "PORT is not a port number".to_owned())?;
        return Ok(SocketAddr::from(([0, 0, 0, 0], port)));
    }
    Ok(SocketAddr::from(([127, 0, 0, 1], 50051)))
}

/// Everything decided at startup, before the store is known.
struct Setup {
    listen: SocketAddr,
    license: Option<Arc<LicenseGate>>,
    kind: AuthKind,
    methodologies: Arc<Methodologies>,
}

async fn run() -> Result<(), String> {
    let listen = listen_address()?;
    let license = license()?;
    let kind = auth_kind()?;

    let mut methodologies = Methodologies::builtin().map_err(|e| e.to_string())?;
    if let Some(dir) = var("AEGIS_PACKS_DIR") {
        methodologies
            .add_dir(&PathBuf::from(dir))
            .map_err(|e| e.to_string())?;
    }
    let setup = Setup {
        listen,
        license,
        kind,
        methodologies: Arc::new(methodologies),
    };

    match var("AEGIS_DATABASE_URL").or_else(|| var("DATABASE_URL")) {
        Some(url) => {
            let pool_size = match var("AEGIS_DB_POOL") {
                Some(v) => v
                    .parse()
                    .map_err(|_| "AEGIS_DB_POOL is not a number".to_owned())?,
                None => PostgresOptions::default().pool_size,
            };
            let tls: DbTls = match var("AEGIS_DB_TLS") {
                Some(v) => v.parse().map_err(|e| format!("AEGIS_DB_TLS: {e}"))?,
                None => DbTls::Off,
            };
            let options = PostgresOptions {
                pool_size,
                tls,
                ..PostgresOptions::default()
            };
            let store = PostgresStore::connect(&url, &options)
                .await
                .map_err(|e| format!("cannot open the database: {e}"))?;
            tracing::info!(?tls, "using PostgreSQL");
            let accounts =
                AccountBackend::Postgres(PostgresAccounts::new(store.pool(), store.org()));
            start(setup, Arc::new(store), accounts).await
        }
        None => {
            tracing::warn!(
                "AEGIS_DATABASE_URL is unset: projects and accounts are kept in memory and are lost on exit"
            );
            start(
                setup,
                Arc::new(MemoryStore::new()),
                AccountBackend::Memory(MemoryAccounts::new()),
            )
            .await
        }
    }
}

async fn start<S: ProjectStore>(
    setup: Setup,
    store: Arc<S>,
    account_backend: AccountBackend,
) -> Result<(), String> {
    let config = Arc::new(ServerConfig::default());
    let mut registry = Registry::new(store, Arc::clone(&setup.methodologies), config);
    if let Some(gate) = setup.license {
        registry = registry.with_license(gate);
    }
    let registry = Arc::new(registry);
    match registry.restore_seats().await {
        Ok(0) => {}
        Ok(n) => tracing::info!(n, "seats restored from the history"),
        Err(e) => tracing::warn!(error = %e, "seats could not be restored; they count from zero"),
    }

    let mut service = AegisService::new(Arc::clone(&registry));
    let (mode, public_url, options, accounts) = match setup.kind {
        AuthKind::Oidc => {
            let (mode, url, options) = oidc_mode().await?;
            // No password accounts, but the event log still works (job 052).
            service = service.with_event_log(EventLog::standalone(account_backend));
            (mode, url, options, None)
        }
        AuthKind::Both => {
            let (Mode::Oidc(oidc), url, options) = oidc_mode().await? else {
                return Err("internal: oidc_mode returned another mode".to_owned());
            };
            let accounts = password_accounts(account_backend, true).await?;
            service = service.with_accounts(Arc::clone(&accounts));
            tracing::info!("login: company login and password login");
            (
                Mode::Both(HybridAuthn {
                    password: PasswordAuthn(Arc::clone(&accounts)),
                    oidc,
                }),
                url,
                options,
                Some(accounts),
            )
        }
        AuthKind::Password => {
            let accounts = password_accounts(account_backend, false).await?;
            service = service.with_accounts(Arc::clone(&accounts));
            (
                Mode::Password(PasswordAuthn(Arc::clone(&accounts))),
                String::new(),
                LoginOptions::password(&server_name()),
                Some(accounts),
            )
        }
        AuthKind::Dev => {
            tracing::warn!("development login: every caller is trusted. Development only.");
            service = service.with_event_log(EventLog::standalone(account_backend));
            (
                Mode::Dev(DevAuthn),
                String::new(),
                LoginOptions::development(&server_name()),
                None,
            )
        }
    };
    let purger = accounts
        .as_ref()
        .map(|a| spawn_purger(Arc::clone(a), Duration::from_secs(600)));
    let trimmer = match service.event_log() {
        Some(log) => Some(spawn_trimmer(
            log.clone(),
            retention()?,
            Duration::from_secs(3600),
        )),
        None => None,
    };
    let public = AccountsService::new(accounts, options);
    let layer = AuthLayer::new(Arc::new(mode), &public_url);
    let result = serve(registry, service, public, setup.listen, layer).await;
    for task in [purger, trimmer].into_iter().flatten() {
        task.abort();
    }
    result
}

async fn serve<S: ProjectStore>(
    registry: Arc<Registry<S>>,
    service: AegisService<S>,
    public: AccountsService,
    listen: SocketAddr,
    layer: AuthLayer<Mode>,
) -> Result<(), String> {
    tracing::info!(%listen, "serving gRPC and gRPC-Web");
    let (stopping, stop_requested) = tokio::sync::oneshot::channel::<()>();
    let serving = server::router(service, public, layer).serve_with_shutdown(listen, async move {
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
