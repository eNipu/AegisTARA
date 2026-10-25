//! The set of projects a server hosts and the actors that are loaded.
//!
//! Creating and listing go straight to the store. Anything that touches a
//! project's content goes through its actor, which the registry loads on
//! demand and keeps until the actor unloads itself when idle.
//!
//! The map lock is a plain mutex held only to find or insert a cell, never
//! across I/O (C2). Loading happens inside the cell, so two requests for
//! the same cold project share one load, and requests for other projects
//! are not blocked.

use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, Mutex};

use aegis_model::Project;
use tokio::sync::OnceCell;

use crate::actor::{self, Loaded, ProjectHandle};
use crate::config::ServerConfig;
use crate::error::ServerError;
use crate::ids::ProjectId;
use crate::methodologies::Methodologies;
use crate::store::{ProjectMeta, ProjectStore};

type Cell = Arc<OnceCell<ProjectHandle>>;

/// Hosts projects over one store.
pub struct Registry<S> {
    store: Arc<S>,
    methodologies: Arc<Methodologies>,
    config: Arc<ServerConfig>,
    open: Mutex<HashMap<ProjectId, Cell>>,
}

impl<S: ProjectStore> Registry<S> {
    /// A registry with nothing loaded.
    #[must_use]
    pub fn new(
        store: Arc<S>,
        methodologies: Arc<Methodologies>,
        config: Arc<ServerConfig>,
    ) -> Self {
        Registry {
            store,
            methodologies,
            config,
            open: Mutex::new(HashMap::new()),
        }
    }

    /// The server's limits.
    #[must_use]
    pub fn config(&self) -> &ServerConfig {
        &self.config
    }

    /// The hosted methodologies.
    #[must_use]
    pub fn methodologies(&self) -> &Methodologies {
        &self.methodologies
    }

    /// Creates an empty project.
    ///
    /// # Errors
    ///
    /// [`ServerError::InvalidRequest`] for a bad name,
    /// [`ServerError::UnknownMethodology`], [`ServerError::Busy`] when the
    /// server hosts its maximum, or a storage failure.
    pub async fn create(
        &self,
        name: &str,
        methodology_key: &str,
    ) -> Result<ProjectMeta, ServerError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(ServerError::InvalidRequest("project name is empty"));
        }
        if name.chars().count() > self.config.max_project_name_chars {
            return Err(ServerError::InvalidRequest("project name is too long"));
        }
        if name.chars().any(char::is_control) {
            return Err(ServerError::InvalidRequest(
                "project name has control characters",
            ));
        }
        let hosted = self
            .methodologies
            .get(methodology_key)
            .ok_or(ServerError::UnknownMethodology)?;
        if self.store.count().await? >= self.config.max_projects {
            return Err(ServerError::Busy);
        }
        let methodology = Arc::clone(&hosted.methodology);
        let snapshot = tokio::task::spawn_blocking(move || {
            let mut out = Vec::new();
            Project::new(methodology).save(&mut out).map(|()| out)
        })
        .await
        .map_err(|_| ServerError::Internal)?
        .map_err(|e| {
            tracing::error!(error = %e, "empty project does not serialize");
            ServerError::Internal
        })?;
        let meta = ProjectMeta {
            id: ProjectId::new_v7(),
            name: name.to_owned(),
            methodology_key: hosted.methodology.key().to_owned(),
            methodology_version: hosted.methodology.version().to_owned(),
            created_ms: (self.config.wall_clock)(),
        };
        self.store.create(&meta, &snapshot).await?;
        Ok(meta)
    }

    /// Lists projects, oldest first, up to `max_projects`.
    ///
    /// # Errors
    ///
    /// A storage failure.
    pub async fn list(&self) -> Result<Vec<ProjectMeta>, ServerError> {
        Ok(self.store.list(self.config.max_projects).await?)
    }

    /// Runs `f` against the project's actor, loading it first if needed.
    /// If the actor unloads while `f` is in flight, loads it again and
    /// retries once. A retried `SubmitOps` stays safe: its base sequence
    /// number no longer matches if the first try was applied.
    ///
    /// # Errors
    ///
    /// [`ServerError::UnknownProject`], load failures, or whatever `f`
    /// returns.
    pub async fn with<T, F, Fut>(&self, id: ProjectId, f: F) -> Result<T, ServerError>
    where
        F: Fn(ProjectHandle) -> Fut,
        Fut: Future<Output = Result<T, ServerError>>,
    {
        let mut attempt = 0;
        loop {
            let handle = self.handle(id).await?;
            match f(handle).await {
                Err(ServerError::Unloaded) if attempt == 0 => attempt += 1,
                result => return result,
            }
        }
    }

    async fn handle(&self, id: ProjectId) -> Result<ProjectHandle, ServerError> {
        loop {
            let cell = self.cell(id)?;
            let handle = match cell.get_or_try_init(|| self.load(id)).await {
                Ok(handle) => handle,
                Err(e) => {
                    // A failed load must not hold a slot in the table.
                    let mut map = self.lock();
                    if map
                        .get(&id)
                        .is_some_and(|c| Arc::ptr_eq(c, &cell) && c.get().is_none())
                    {
                        map.remove(&id);
                    }
                    return Err(e);
                }
            };
            if !handle.is_closed() {
                return Ok(handle.clone());
            }
            // The actor stopped; drop its cell (unless someone already did).
            let mut map = self.lock();
            if map.get(&id).is_some_and(|c| Arc::ptr_eq(c, &cell)) {
                map.remove(&id);
            }
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<ProjectId, Cell>> {
        self.open
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn cell(&self, id: ProjectId) -> Result<Cell, ServerError> {
        let mut map = self.lock();
        if let Some(cell) = map.get(&id) {
            return Ok(Arc::clone(cell));
        }
        if map.len() >= self.config.max_open_projects {
            // Forget cells whose actors already stopped before refusing.
            map.retain(|_, c| c.get().is_none_or(|h| !h.is_closed()));
            if map.len() >= self.config.max_open_projects {
                return Err(ServerError::Busy);
            }
        }
        let cell = Cell::default();
        map.insert(id, Arc::clone(&cell));
        Ok(cell)
    }

    async fn load(&self, id: ProjectId) -> Result<ProjectHandle, ServerError> {
        let stored = self
            .store
            .load(id)
            .await?
            .ok_or(ServerError::UnknownProject(id))?;
        let Some(hosted) = self.methodologies.get(&stored.meta.methodology_key) else {
            tracing::error!(project = %id, key = %stored.meta.methodology_key, "stored project uses a methodology this server does not host");
            return Err(ServerError::Internal);
        };
        let methodology = Arc::clone(&hosted.methodology);
        let rebuilt = tokio::task::spawn_blocking(move || Loaded::rebuild(stored, methodology))
            .await
            .map_err(|_| ServerError::Internal)??;
        Ok(actor::spawn(rebuilt, Arc::clone(&self.store), &self.config))
    }

    /// Asks every loaded project to write a snapshot and stop. Call it
    /// when the server shuts down.
    pub async fn shutdown(&self) {
        let handles: Vec<ProjectHandle> = {
            let mut map = self.lock();
            map.drain().filter_map(|(_, c)| c.get().cloned()).collect()
        };
        for handle in handles {
            // Already stopped is fine.
            let _ = handle.shutdown().await;
        }
    }

    /// How many projects are loaded right now.
    #[must_use]
    pub fn loaded_count(&self) -> usize {
        self.lock()
            .values()
            .filter(|c| c.get().is_some_and(|h| !h.is_closed()))
            .count()
    }
}
