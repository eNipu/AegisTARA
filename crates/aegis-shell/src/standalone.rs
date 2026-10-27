//! An in-process server for the single-user desktop mode.
//!
//! The client still uses its normal gRPC path. The listener is bound only
//! to loopback on an operating-system-assigned port, and project data lives
//! in the selected folder. The signed license is read by the app before the
//! listener starts; the gate remains attached for expiry and seat checks.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use aegis_license::TrustedKeys;
use aegis_server::accounts::events::EventLog;
use aegis_server::accounts::service::{AccountsService, LoginOptions};
use aegis_server::accounts::{AccountBackend, MemoryAccounts};
use aegis_server::auth_layer::{AuthLayer, DevAuthn};
use aegis_server::license::system_clock_arc;
use aegis_server::methodologies::Methodologies;
use aegis_server::store::FolderStore;
use aegis_server::{AegisService, LicenseGate, ProjectStore, Registry, ServerConfig, server};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tokio_stream::wrappers::TcpListenerStream;

const SHUTDOWN_GRACE: Duration = Duration::from_secs(10);

/// Owns the embedded server. Dropping it requests shutdown; [`stop`](Self::stop)
/// also waits for project snapshots to be written.
pub struct Standalone {
    url: String,
    stop: Option<oneshot::Sender<()>>,
    done: Option<JoinHandle<Result<(), String>>>,
}

impl Standalone {
    /// Opens `project_folder` and checks `license_path` before binding a port.
    ///
    /// # Errors
    ///
    /// Returns a human-readable error when the license, folder, methodology,
    /// listener, or embedded server cannot be started.
    pub async fn open(project_folder: &Path, license_path: &Path) -> Result<Self, String> {
        let gate = Self::load_gate(license_path).await?;
        Self::start(project_folder, gate, false).await
    }

    /// Creates a standalone server for a folder that does not already hold a
    /// project, after checking the signed license.
    ///
    /// # Errors
    ///
    /// As [`open`](Self::open), or when the folder already holds a project.
    pub async fn create(project_folder: &Path, license_path: &Path) -> Result<Self, String> {
        let gate = Self::load_gate(license_path).await?;
        Self::start(project_folder, gate, true).await
    }

    async fn load_gate(license_path: &Path) -> Result<Arc<LicenseGate>, String> {
        let license_path = license_path.to_owned();
        tokio::task::spawn_blocking(move || {
            let text = std::fs::read_to_string(&license_path).map_err(|e| {
                format!(
                    "cannot read the license file {}: {e}",
                    license_path.display()
                )
            })?;
            LicenseGate::load(&text, &TrustedKeys::vendor(), system_clock_arc())
                .map(Arc::new)
                .map_err(|e| {
                    format!(
                        "the license file {} cannot be used: {e}",
                        license_path.display()
                    )
                })
        })
        .await
        .map_err(|e| format!("the license check failed: {e}"))?
    }

    /// Opens a folder with an already verified gate. This keeps license parsing
    /// separate from the server lifecycle and lets integration tests use their
    /// own signing keys.
    ///
    /// # Errors
    ///
    /// As [`open`](Self::open).
    pub async fn open_with_gate(
        project_folder: &Path,
        gate: Arc<LicenseGate>,
    ) -> Result<Self, String> {
        Self::start(project_folder, gate, false).await
    }

    async fn start(
        project_folder: &Path,
        gate: Arc<LicenseGate>,
        require_empty: bool,
    ) -> Result<Self, String> {
        let folder = project_folder.to_owned();
        let store = tokio::task::spawn_blocking(move || FolderStore::open(&folder))
            .await
            .map_err(|e| format!("cannot open the project folder: {e}"))?
            .map_err(|e| format!("cannot open the project folder: {e}"))?;
        let store = Arc::new(store);
        if require_empty && !store.list(1).await.map_err(|e| e.to_string())?.is_empty() {
            return Err("the selected folder already contains a project".to_owned());
        }
        let methodologies = Methodologies::builtin().map_err(|e| e.to_string())?;
        let config = ServerConfig {
            max_projects: 1,
            ..ServerConfig::default()
        };
        let registry = Arc::new(
            Registry::new(store, Arc::new(methodologies), Arc::new(config)).with_license(gate),
        );
        registry
            .restore_seats()
            .await
            .map_err(|e| format!("cannot restore the license seats: {e}"))?;

        // Binding happens only after every local check above, so a bad license
        // or damaged project never opens a listener.
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|e| format!("cannot bind the standalone server: {e}"))?;
        let address = listener
            .local_addr()
            .map_err(|e| format!("cannot read the standalone address: {e}"))?;
        debug_assert!(address.ip().is_loopback() && address.port() != 0);
        let url = format!("http://{address}");

        let service = AegisService::new(Arc::clone(&registry)).with_event_log(
            EventLog::standalone(AccountBackend::Memory(MemoryAccounts::new())),
        );
        let public = AccountsService::new(None, LoginOptions::development("Standalone"));
        let layer = AuthLayer::new(Arc::new(DevAuthn), &url);
        let (stop, stopped) = oneshot::channel::<()>();
        let serving_registry = Arc::clone(&registry);
        let done = tokio::spawn(async move {
            let result = server::router(service, public, layer)
                .serve_with_incoming_shutdown(TcpListenerStream::new(listener), async {
                    let _ = stopped.await;
                    serving_registry.shutdown().await;
                })
                .await
                .map_err(|e| format!("standalone server failed: {e}"));
            // Do not let an abandoned project download prevent shutdown forever.
            match tokio::time::timeout(SHUTDOWN_GRACE, async { result }).await {
                Ok(result) => result,
                Err(_) => Err("standalone server did not stop in time".to_owned()),
            }
        });
        Ok(Standalone {
            url,
            stop: Some(stop),
            done: Some(done),
        })
    }

    /// The loopback URL for the normal client session.
    #[must_use]
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Stops the listener and waits for all open projects to be snapshotted.
    ///
    /// # Errors
    ///
    /// Returns a server task failure or a shutdown timeout.
    pub async fn stop(mut self) -> Result<(), String> {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(done) = self.done.take() {
            return tokio::time::timeout(SHUTDOWN_GRACE + Duration::from_secs(1), done)
                .await
                .map_err(|_| "timed out stopping the standalone server".to_owned())?
                .map_err(|e| format!("the standalone server task failed: {e}"))?;
        }
        Ok(())
    }
}

impl Drop for Standalone {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        // Dropping a JoinHandle detaches rather than cancels the task, so the
        // server can still finish its graceful snapshot flush.
    }
}
