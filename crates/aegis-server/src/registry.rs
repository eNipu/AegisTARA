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
use crate::auth::Principal;
use crate::config::ServerConfig;
use crate::error::ServerError;
use crate::ids::ProjectId;
use crate::license::LicenseGate;
use crate::methodologies::Methodologies;
use crate::store::{
    ChainReport, FigureError, FigureMeta, ProjectMeta, ProjectStore, StoreFault,
    validate_and_sanitize_figure, validate_caption, validate_target,
};

type Cell = Arc<OnceCell<ProjectHandle>>;

/// Hosts projects over one store.
pub struct Registry<S> {
    store: Arc<S>,
    methodologies: Arc<Methodologies>,
    config: Arc<ServerConfig>,
    license: Option<Arc<LicenseGate>>,
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
            license: None,
            open: Mutex::new(HashMap::new()),
        }
    }

    /// Puts the registry behind a license gate. Without one, nothing is
    /// checked (library use and tests); `main` always sets one.
    #[must_use]
    pub fn with_license(mut self, gate: Arc<LicenseGate>) -> Self {
        self.license = Some(gate);
        self
    }

    /// The license gate, if the server has one.
    #[must_use]
    pub fn license(&self) -> Option<&LicenseGate> {
        self.license.as_deref()
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
        let name = self.check_name(name)?;
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

    /// Trims and checks a project or baseline name.
    ///
    /// # Errors
    ///
    /// [`ServerError::InvalidRequest`] for an empty or too long name, or one
    /// with control characters.
    pub fn check_name<'a>(&self, name: &'a str) -> Result<&'a str, ServerError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(ServerError::InvalidRequest("name is empty"));
        }
        if name.chars().count() > self.config.max_project_name_chars {
            return Err(ServerError::InvalidRequest("name is too long"));
        }
        if name.chars().any(char::is_control) {
            return Err(ServerError::InvalidRequest("name has control characters"));
        }
        Ok(name)
    }

    /// The store, for calls that need no actor (history, baselines).
    #[must_use]
    pub fn store(&self) -> &S {
        &self.store
    }

    /// Deletes a project: its actor stops and its
    /// history and baselines are removed.
    ///
    /// # Errors
    ///
    /// [`ServerError::UnknownProject`], [`ServerError::LockHeld`], or a
    /// storage failure.
    pub async fn delete(&self, id: ProjectId) -> Result<(), ServerError> {
        self.with(id, |h| async move { h.delete().await }).await?;
        let mut map = self.lock();
        if map
            .get(&id)
            .is_some_and(|c| c.get().is_none_or(ProjectHandle::is_closed))
        {
            map.remove(&id);
        }
        Ok(())
    }

    /// Freezes the project's current state as a baseline. Reads the state
    /// through the actor, so it is consistent with the accepted ops.
    ///
    /// # Errors
    ///
    /// [`ServerError::InvalidRequest`] for a bad name or too many
    /// baselines, [`ServerError::UnknownProject`], or a storage failure.
    pub async fn create_baseline(
        &self,
        id: ProjectId,
        name: &str,
        who: &str,
    ) -> Result<crate::store::BaselineMeta, ServerError> {
        let name = self.check_name(name)?.to_owned();
        let existing = self
            .store
            .list_baselines(id)
            .await
            .map_err(|e| missing(e, id))?;
        if existing.len() >= self.config.max_baselines {
            return Err(ServerError::InvalidRequest("too many baselines"));
        }
        let data = self.with(id, |h| async move { h.open().await }).await?;
        let meta = crate::store::BaselineMeta {
            id: uuid::Uuid::now_v7(),
            project: id,
            name,
            seq: data.last_seq,
            created_by: who.to_owned(),
            created_ms: (self.config.wall_clock)(),
            snapshot_bytes: data.snapshot.len() as u64,
        };
        self.store
            .create_baseline(&meta, &data.snapshot)
            .await
            .map_err(|e| missing(e, id))?;
        Ok(meta)
    }

    /// Validates, sanitizes (when SVG), and stores a figure attachment for `id`
    /// (`ARC-01`). Requires the edit lease.
    ///
    /// # Errors
    ///
    /// [`ServerError::InvalidRequest`], [`ServerError::QuotaExceeded`], lease
    /// errors, [`ServerError::UnknownProject`], or a storage failure.
    #[allow(clippy::too_many_arguments)]
    pub async fn put_figure(
        &self,
        id: ProjectId,
        who: Principal,
        token: String,
        caption: &str,
        target: &str,
        media_type: &str,
        raw: &[u8],
    ) -> Result<FigureMeta, ServerError> {
        let caption = validate_caption(caption).map_err(figure_error)?;
        let target = validate_target(target).map_err(figure_error)?;
        let max_bytes = self.config.max_figure_bytes;
        let media_owned = media_type.to_owned();
        let raw_owned = raw.to_vec();
        let validated = tokio::task::spawn_blocking(move || {
            validate_and_sanitize_figure(&media_owned, &raw_owned, max_bytes)
        })
        .await
        .map_err(|_| ServerError::Internal)?
        .map_err(figure_error)?;

        let who_sub = who.subject.to_string();
        self.with(id, |h| {
            let (who, token) = (who.clone(), token.clone());
            async move { h.check_lease(who, token).await }
        })
        .await?;

        let meta = FigureMeta {
            id: uuid::Uuid::now_v7(),
            project: id,
            caption,
            target,
            media_type: validated.media.as_str().to_owned(),
            width: validated.width,
            height: validated.height,
            byte_len: validated.bytes.len() as u64,
            sha256: validated.sha256,
            created_by: who_sub,
            created_ms: (self.config.wall_clock)(),
        };
        self.store
            .put_figure(
                &meta,
                &validated.bytes,
                self.config.max_figures_per_project,
                self.config.max_project_figures_bytes,
            )
            .await
            .map_err(|e| missing(e, id))?;
        Ok(meta)
    }

    /// Updates a figure's caption and optional element target (`ARC-01`).
    /// Requires the edit lease.
    pub async fn update_figure(
        &self,
        id: ProjectId,
        who: Principal,
        token: String,
        figure_id: uuid::Uuid,
        caption: &str,
        target: &str,
    ) -> Result<FigureMeta, ServerError> {
        let caption = validate_caption(caption).map_err(figure_error)?;
        let target = validate_target(target).map_err(figure_error)?;
        self.with(id, |h| {
            let (who, token) = (who.clone(), token.clone());
            async move { h.check_lease(who, token).await }
        })
        .await?;
        self.store
            .update_figure(id, figure_id, &caption, &target)
            .await
            .map_err(|e| missing(e, id))?
            .ok_or(ServerError::InvalidRequest("unknown figure"))
    }

    /// Deletes a figure attachment (`ARC-01`). Requires the edit lease.
    pub async fn delete_figure(
        &self,
        id: ProjectId,
        who: Principal,
        token: String,
        figure_id: uuid::Uuid,
    ) -> Result<(), ServerError> {
        self.with(id, |h| {
            let (who, token) = (who.clone(), token.clone());
            async move { h.check_lease(who, token).await }
        })
        .await?;
        let removed = self
            .store
            .delete_figure(id, figure_id)
            .await
            .map_err(|e| missing(e, id))?;
        if !removed {
            return Err(ServerError::InvalidRequest("unknown figure"));
        }
        Ok(())
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

    /// Rebuilds the license seat ledger from the stored history: everyone
    /// whose ops were accepted in the seat window holds a seat again, so a
    /// restart does not hand out seats twice (backlog 039).
    ///
    /// # Errors
    ///
    /// A storage failure.
    pub async fn restore_seats(&self) -> Result<usize, ServerError> {
        let Some(gate) = self.license.as_ref() else {
            return Ok(0);
        };
        let now_ms = (self.config.wall_clock)();
        let since = now_ms.saturating_sub(aegis_license::SEAT_WINDOW_SECS * 1000);
        let editors = self.store.active_editors(since).await?;
        let n = editors.len();
        gate.restore(editors.into_iter().map(|(who, ms)| (who, ms / 1000)));
        Ok(n)
    }

    /// Recomputes the audit hash chain of a project's stored history.
    ///
    /// # Errors
    ///
    /// [`ServerError::UnknownProject`] or a storage failure.
    pub async fn verify_audit(&self, id: ProjectId) -> Result<ChainReport, ServerError> {
        self.store.verify_chain(id).await.map_err(|e| match e {
            StoreFault::Missing => ServerError::UnknownProject(id),
            other => ServerError::Store(other),
        })
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

/// Maps "no such project" from the store to the API error.
pub(crate) fn missing(fault: StoreFault, id: ProjectId) -> ServerError {
    match fault {
        StoreFault::Missing => ServerError::UnknownProject(id),
        StoreFault::QuotaExceeded(msg) => ServerError::QuotaExceeded(msg),
        other => ServerError::Store(other),
    }
}

fn figure_error(err: FigureError) -> ServerError {
    match err {
        FigureError::Empty => ServerError::InvalidRequest("figure file is empty"),
        FigureError::TooLarge { max } => {
            ServerError::QuotaExceeded(format!("figure exceeds maximum size of {max} bytes"))
        }
        FigureError::UnsupportedMediaType => ServerError::InvalidRequest(
            "figure media type must be image/png, image/jpeg, or image/svg+xml",
        ),
        FigureError::InvalidCaption(msg) => ServerError::InvalidRequest(msg),
        FigureError::InvalidTarget(msg) => ServerError::InvalidRequest(msg),
        FigureError::InvalidImage(msg) => ServerError::InvalidRequest(msg),
        FigureError::InvalidDimensions { .. } => {
            ServerError::InvalidRequest("figure dimensions are out of range")
        }
        FigureError::UnsafeSvg(msg) => ServerError::InvalidRequest(msg),
    }
}
