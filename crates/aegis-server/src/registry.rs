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
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

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

type Cell = Arc<OnceCell<LoadOutcome>>;

/// The settled outcome of loading one project's actor. Once a cell holds an
/// outcome it never changes, so a cell removed from the map (for example a
/// failed load, or an actor that stopped) cannot be re-initialized by a
/// waiter that still holds a clone of it.
#[derive(Clone)]
enum LoadOutcome {
    Loaded(ProjectHandle),
    Failed(LoadFailure),
}

/// Why a load failed, in a form every waiter can share without cloning the
/// full [`ServerError`] (which carries non-`Clone` storage and model errors).
/// The loader logs the original error; waiters reconstruct a same-code one.
#[derive(Clone)]
enum LoadFailure {
    UnknownProject(ProjectId),
    /// A storage failure: retriable (`UNAVAILABLE`).
    Unavailable,
    /// The server began shutting down before the load finished.
    ShuttingDown,
    /// A bug or corrupted state: not retriable.
    Internal,
}

impl From<ServerError> for LoadFailure {
    fn from(error: ServerError) -> Self {
        match error {
            ServerError::UnknownProject(id) => LoadFailure::UnknownProject(id),
            ServerError::Store(fault) => {
                tracing::error!(%fault, "project load failed");
                LoadFailure::Unavailable
            }
            ServerError::ShuttingDown => LoadFailure::ShuttingDown,
            other => {
                tracing::error!(error = %other, "project load failed");
                LoadFailure::Internal
            }
        }
    }
}

impl LoadFailure {
    fn into_server_error(self) -> ServerError {
        match self {
            LoadFailure::UnknownProject(id) => ServerError::UnknownProject(id),
            LoadFailure::Unavailable => ServerError::Store(StoreFault::Backend(
                "the project could not be loaded".into(),
            )),
            LoadFailure::ShuttingDown => ServerError::ShuttingDown,
            LoadFailure::Internal => ServerError::Internal,
        }
    }
}

/// Hosts projects over one store.
pub struct Registry<S> {
    store: Arc<S>,
    methodologies: Arc<Methodologies>,
    config: Arc<ServerConfig>,
    license: Option<Arc<LicenseGate>>,
    open: Mutex<HashMap<ProjectId, Cell>>,
    /// Set once shutdown starts: no new project is admitted or loaded after.
    closing: AtomicBool,
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
            closing: AtomicBool::new(false),
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
        if map.get(&id).is_some_and(|c| match c.get() {
            None => true,
            Some(LoadOutcome::Loaded(handle)) => handle.is_closed(),
            Some(LoadOutcome::Failed(_)) => true,
        }) {
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
        let max_count = self.config.max_figures_per_project;
        let max_total_bytes = self.config.max_project_figures_bytes;
        let data = Arc::new(validated.bytes);
        let meta = FigureMeta {
            id: uuid::Uuid::now_v7(),
            project: id,
            caption,
            target,
            media_type: validated.media.as_str().to_owned(),
            width: validated.width,
            height: validated.height,
            byte_len: data.len() as u64,
            sha256: validated.sha256,
            created_by: who_sub,
            created_ms: (self.config.wall_clock)(),
        };
        self.with(id, |handle| {
            let (who, token, data, meta) =
                (who.clone(), token.clone(), Arc::clone(&data), meta.clone());
            async move {
                handle
                    .put_figure(who, token, meta, data, max_count, max_total_bytes)
                    .await
            }
        })
        .await
        .map_err(|error| match error {
            ServerError::Store(fault) => missing(fault, id),
            other => other,
        })?;
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
        self.with(id, |handle| {
            let (who, token) = (who.clone(), token.clone());
            let (caption, target) = (caption.clone(), target.clone());
            async move {
                handle
                    .update_figure(who, token, figure_id, caption, target)
                    .await
            }
        })
        .await
        .map_err(|error| match error {
            ServerError::Store(fault) => missing(fault, id),
            other => other,
        })
    }

    /// Deletes a figure attachment (`ARC-01`). Requires the edit lease.
    pub async fn delete_figure(
        &self,
        id: ProjectId,
        who: Principal,
        token: String,
        figure_id: uuid::Uuid,
    ) -> Result<(), ServerError> {
        self.with(id, |handle| {
            let (who, token) = (who.clone(), token.clone());
            async move { handle.delete_figure(who, token, figure_id).await }
        })
        .await
        .map_err(|error| match error {
            ServerError::Store(fault) => missing(fault, id),
            other => other,
        })
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
            let outcome = cell.get_or_init(|| self.load_outcome(id)).await.clone();
            match outcome {
                LoadOutcome::Loaded(handle) => {
                    if self.closing.load(Ordering::SeqCst) {
                        self.remove_cell_if(id, &cell);
                        let _ = handle.shutdown().await;
                        handle.wait_stopped().await;
                        return Err(ServerError::ShuttingDown);
                    }
                    if !handle.is_closed() {
                        return Ok(handle);
                    }
                    // Wait for the retiring actor to finish its final snapshot
                    // and exit before dropping its cell, so a replacement actor
                    // can never overlap with it on storage.
                    handle.wait_stopped().await;
                }
                LoadOutcome::Failed(failure) => {
                    self.remove_cell_if(id, &cell);
                    return Err(failure.into_server_error());
                }
            }
            // The actor stopped (or the cell was already replaced): drop this
            // cell if it is still the one in the table, then load a fresh one.
            self.remove_cell_if(id, &cell);
        }
    }

    /// Drops `cell` from the table only while it is still the entry for `id`,
    /// so a newer replacement cell is never removed by a stale caller.
    fn remove_cell_if(&self, id: ProjectId, cell: &Cell) {
        let mut map = self.lock();
        if map.get(&id).is_some_and(|c| Arc::ptr_eq(c, cell)) {
            map.remove(&id);
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<ProjectId, Cell>> {
        self.open
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn cell(&self, id: ProjectId) -> Result<Cell, ServerError> {
        let mut map = self.lock();
        if self.closing.load(Ordering::SeqCst) {
            return Err(ServerError::ShuttingDown);
        }
        if let Some(cell) = map.get(&id) {
            return Ok(Arc::clone(cell));
        }
        if map.len() >= self.config.max_open_projects {
            // Forget settled cells that are no longer useful before refusing:
            // actors that already stopped and loads that already failed.
            map.retain(|_, cell| match cell.get() {
                None => true,
                Some(LoadOutcome::Loaded(handle)) => !handle.is_closed(),
                Some(LoadOutcome::Failed(_)) => false,
            });
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
        // Admission closes at the spawn point too, so a load that started
        // before shutdown cannot publish an actor after the drain.
        if self.closing.load(Ordering::SeqCst) {
            return Err(ServerError::ShuttingDown);
        }
        let handle = actor::spawn(rebuilt, Arc::clone(&self.store), &self.config);
        if self.closing.load(Ordering::SeqCst) {
            let _ = handle.shutdown().await;
            handle.wait_stopped().await;
            return Err(ServerError::ShuttingDown);
        }
        Ok(handle)
    }

    async fn load_outcome(&self, id: ProjectId) -> LoadOutcome {
        match self.load(id).await {
            Ok(handle) => LoadOutcome::Loaded(handle),
            Err(error) => LoadOutcome::Failed(LoadFailure::from(error)),
        }
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
    /// when the server shuts down. New loads are refused once this starts.
    pub async fn shutdown(&self) {
        let _ = self.shutdown_within(Duration::from_secs(30)).await;
    }

    /// Closes registry admission, drains all active and in-flight project
    /// cells concurrently, and waits up to `timeout` for every actor to write
    /// its final snapshot and stop. Returns `true` if all actors finished
    /// within the deadline.
    pub async fn shutdown_within(&self, timeout: Duration) -> bool {
        let cells: Vec<Cell> = {
            let mut map = self.lock();
            self.closing.store(true, Ordering::SeqCst);
            map.drain().map(|(_, cell)| cell).collect()
        };
        let mut tasks = tokio::task::JoinSet::new();
        for cell in cells {
            tasks.spawn(async move {
                // If an in-flight loader already entered `get_or_init`, this
                // waits for it to settle; if it had not entered yet, this
                // settles the cell to `ShuttingDown` so it never starts.
                let outcome = cell
                    .get_or_init(|| async { LoadOutcome::Failed(LoadFailure::ShuttingDown) })
                    .await
                    .clone();
                if let LoadOutcome::Loaded(handle) = outcome {
                    let _ = handle.shutdown().await;
                    handle.wait_stopped().await;
                }
            });
        }
        tokio::time::timeout(timeout, async move {
            while tasks.join_next().await.is_some() {}
        })
        .await
        .is_ok()
    }

    /// How many projects are loaded right now.
    #[must_use]
    pub fn loaded_count(&self) -> usize {
        self.lock()
            .values()
            .filter(|cell| {
                matches!(
                    cell.get(),
                    Some(LoadOutcome::Loaded(handle)) if !handle.is_closed()
                )
            })
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

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicUsize;

    use tokio::sync::Notify;

    use super::*;
    use crate::store::{
        BaselineMeta, LoadedProject, MemoryStore, ProjectMembership, StoredOp, StoredSnapshot,
    };

    #[derive(Clone, Copy)]
    enum AppendFault {
        None,
        /// Definite pre-commit rejection: nothing is written to `inner`.
        RejectBeforeCommit,
        /// Commit succeeds in `inner`, then returns `StoreFault::Uncertain`.
        LostAckAfterCommit,
        /// Returns `StoreFault::Sequence` without writing to `inner`.
        SequenceConflict,
    }

    struct GatedStore {
        inner: MemoryStore,
        load_calls: AtomicUsize,
        fail_load: AtomicBool,
        pause_load: AtomicBool,
        entered: Notify,
        release: Notify,
        pause_append: AtomicBool,
        append_entered: Notify,
        append_release: Notify,
        append_fault: Mutex<AppendFault>,
        pause_snapshot: AtomicBool,
        snapshot_entered: Notify,
        snapshot_release: Notify,
    }

    impl GatedStore {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                inner: MemoryStore::new(),
                load_calls: AtomicUsize::new(0),
                fail_load: AtomicBool::new(false),
                pause_load: AtomicBool::new(false),
                entered: Notify::new(),
                release: Notify::new(),
                pause_append: AtomicBool::new(false),
                append_entered: Notify::new(),
                append_release: Notify::new(),
                append_fault: Mutex::new(AppendFault::None),
                pause_snapshot: AtomicBool::new(false),
                snapshot_entered: Notify::new(),
                snapshot_release: Notify::new(),
            })
        }

        fn set_append_fault(&self, fault: AppendFault) {
            *self
                .append_fault
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = fault;
        }
    }

    impl ProjectStore for GatedStore {
        async fn create(&self, meta: &ProjectMeta, snapshot: &[u8]) -> Result<(), StoreFault> {
            self.inner.create(meta, snapshot).await
        }
        async fn list(&self, limit: usize) -> Result<Vec<ProjectMeta>, StoreFault> {
            self.inner.list(limit).await
        }
        async fn count(&self) -> Result<usize, StoreFault> {
            self.inner.count().await
        }
        async fn load(&self, id: ProjectId) -> Result<Option<LoadedProject>, StoreFault> {
            self.load_calls.fetch_add(1, Ordering::SeqCst);
            if self.pause_load.load(Ordering::SeqCst) {
                self.entered.notify_one();
                self.release.notified().await;
            }
            if self.fail_load.load(Ordering::SeqCst) {
                return Err(StoreFault::Backend("transient fault".into()));
            }
            self.inner.load(id).await
        }
        async fn append(&self, id: ProjectId, ops: &[StoredOp]) -> Result<(), StoreFault> {
            if self.pause_append.load(Ordering::SeqCst) {
                self.append_entered.notify_one();
                self.append_release.notified().await;
            }
            let fault = *self
                .append_fault
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            match fault {
                AppendFault::None => self.inner.append(id, ops).await,
                AppendFault::RejectBeforeCommit => {
                    Err(StoreFault::Backend("pre-commit rejection".into()))
                }
                AppendFault::LostAckAfterCommit => {
                    self.inner.append(id, ops).await?;
                    Err(StoreFault::Uncertain("lost commit acknowledgement".into()))
                }
                AppendFault::SequenceConflict => Err(StoreFault::Sequence {
                    expected: ops.first().map_or(1, |o| o.seq + 1),
                    found: ops.first().map_or(0, |o| o.seq),
                }),
            }
        }
        async fn verify_chain(&self, id: ProjectId) -> Result<ChainReport, StoreFault> {
            self.inner.verify_chain(id).await
        }
        async fn save_snapshot(
            &self,
            id: ProjectId,
            snapshot: &StoredSnapshot,
        ) -> Result<(), StoreFault> {
            if self.pause_snapshot.load(Ordering::SeqCst) {
                self.snapshot_entered.notify_one();
                self.snapshot_release.notified().await;
            }
            self.inner.save_snapshot(id, snapshot).await
        }
        async fn rename(&self, id: ProjectId, name: &str) -> Result<(), StoreFault> {
            self.inner.rename(id, name).await
        }
        async fn delete(&self, id: ProjectId) -> Result<(), StoreFault> {
            self.inner.delete(id).await
        }
        async fn recent_ops(
            &self,
            id: ProjectId,
            limit: usize,
        ) -> Result<Vec<StoredOp>, StoreFault> {
            self.inner.recent_ops(id, limit).await
        }
        async fn active_editors(&self, since_ms: u64) -> Result<Vec<(String, u64)>, StoreFault> {
            self.inner.active_editors(since_ms).await
        }
        async fn create_baseline(
            &self,
            meta: &BaselineMeta,
            snapshot: &[u8],
        ) -> Result<(), StoreFault> {
            self.inner.create_baseline(meta, snapshot).await
        }
        async fn list_baselines(
            &self,
            project: ProjectId,
        ) -> Result<Vec<BaselineMeta>, StoreFault> {
            self.inner.list_baselines(project).await
        }
        async fn load_baseline(
            &self,
            project: ProjectId,
            id: uuid::Uuid,
        ) -> Result<Option<(BaselineMeta, Vec<u8>)>, StoreFault> {
            self.inner.load_baseline(project, id).await
        }
        async fn all_memberships(&self) -> Result<Vec<ProjectMembership>, StoreFault> {
            self.inner.all_memberships().await
        }
        async fn project_memberships(
            &self,
            project: ProjectId,
        ) -> Result<Vec<ProjectMembership>, StoreFault> {
            self.inner.project_memberships(project).await
        }
        async fn memberships_for(
            &self,
            members: &[String],
        ) -> Result<Vec<ProjectMembership>, StoreFault> {
            self.inner.memberships_for(members).await
        }
        async fn set_membership(&self, membership: &ProjectMembership) -> Result<(), StoreFault> {
            self.inner.set_membership(membership).await
        }
        async fn remove_membership(
            &self,
            project: ProjectId,
            member: &str,
        ) -> Result<bool, StoreFault> {
            self.inner.remove_membership(project, member).await
        }
        async fn put_figure(
            &self,
            meta: &FigureMeta,
            data: &[u8],
            max_count: usize,
            max_total_bytes: u64,
        ) -> Result<(), StoreFault> {
            self.inner
                .put_figure(meta, data, max_count, max_total_bytes)
                .await
        }
        async fn list_figures(&self, project: ProjectId) -> Result<Vec<FigureMeta>, StoreFault> {
            self.inner.list_figures(project).await
        }
        async fn load_figure(
            &self,
            project: ProjectId,
            id: uuid::Uuid,
        ) -> Result<Option<(FigureMeta, Vec<u8>)>, StoreFault> {
            self.inner.load_figure(project, id).await
        }
        async fn update_figure(
            &self,
            project: ProjectId,
            id: uuid::Uuid,
            caption: &str,
            target: &str,
        ) -> Result<Option<FigureMeta>, StoreFault> {
            self.inner.update_figure(project, id, caption, target).await
        }
        async fn delete_figure(
            &self,
            project: ProjectId,
            id: uuid::Uuid,
        ) -> Result<bool, StoreFault> {
            self.inner.delete_figure(project, id).await
        }
    }

    fn test_registry(store: Arc<GatedStore>) -> (Arc<Registry<GatedStore>>, String) {
        let methodologies = Arc::new(Methodologies::builtin().expect("builtin"));
        let key = methodologies
            .iter()
            .next()
            .expect("at least one builtin methodology")
            .methodology
            .key()
            .to_owned();
        let config = Arc::new(ServerConfig {
            max_open_projects: 1,
            ..ServerConfig::default()
        });
        (Arc::new(Registry::new(store, methodologies, config)), key)
    }

    #[tokio::test]
    async fn failed_load_shares_outcome_with_concurrent_waiters_and_allows_retry() {
        let store = GatedStore::new();
        let (registry, key) = test_registry(Arc::clone(&store));
        let meta = registry.create("Alpha", &key).await.expect("create");

        store.fail_load.store(true, Ordering::SeqCst);
        store.pause_load.store(true, Ordering::SeqCst);

        let r1 = Arc::clone(&registry);
        let t1 = tokio::spawn(async move { r1.with(meta.id, |_| async { Ok(()) }).await });
        store.entered.notified().await;

        let r2 = Arc::clone(&registry);
        let t2 = tokio::spawn(async move { r2.with(meta.id, |_| async { Ok(()) }).await });
        tokio::task::yield_now().await;

        store.release.notify_one();
        let res1 = t1.await.expect("join t1");
        let res2 = t2.await.expect("join t2");

        assert!(matches!(res1, Err(ServerError::Store(_))));
        assert!(matches!(res2, Err(ServerError::Store(_))));
        assert_eq!(
            store.load_calls.load(Ordering::SeqCst),
            1,
            "concurrent waiter must share the settled failed outcome"
        );
        assert_eq!(registry.loaded_count(), 0);

        // Clearing the transient fault allows a fresh load (failed cell was evicted).
        store.fail_load.store(false, Ordering::SeqCst);
        store.pause_load.store(false, Ordering::SeqCst);
        registry
            .with(meta.id, |_| async { Ok(()) })
            .await
            .expect("retry succeeds");
        assert_eq!(registry.loaded_count(), 1);
        registry.shutdown().await;
    }

    fn encode_condition_op(registry: &Registry<GatedStore>, key: &str) -> bytes::Bytes {
        let hosted = registry.methodologies().get(key).expect("methodology");
        let codec = aegis_model::OpCodec::new(Arc::clone(&hosted.methodology));
        let op = aegis_model::Op::AddCondition {
            id: aegis_model::ConditionId::new_v7(),
            name: "Ethernet".into(),
            description: "".into(),
            present: true,
            provenance: aegis_model::Provenance::default(),
        };
        bytes::Bytes::from(codec.encode(&op).expect("encode op"))
    }

    #[tokio::test]
    async fn shutdown_rejects_inflight_and_subsequent_loads() {
        let store = GatedStore::new();
        let (registry, key) = test_registry(Arc::clone(&store));
        let meta = registry.create("Beta", &key).await.expect("create");

        store.pause_load.store(true, Ordering::SeqCst);

        let r1 = Arc::clone(&registry);
        let inflight = tokio::spawn(async move { r1.with(meta.id, |_| async { Ok(()) }).await });
        store.entered.notified().await;

        let r2 = Arc::clone(&registry);
        let shutting =
            tokio::spawn(async move { r2.shutdown_within(Duration::from_secs(5)).await });
        tokio::task::yield_now().await;

        store.release.notify_one();

        let res = inflight.await.expect("join inflight");
        assert!(matches!(res, Err(ServerError::ShuttingDown)));
        assert!(shutting.await.expect("join shutdown"));
        assert_eq!(registry.loaded_count(), 0);

        store.pause_load.store(false, Ordering::SeqCst);
        let after = registry.with(meta.id, |_| async { Ok(()) }).await;
        assert!(matches!(after, Err(ServerError::ShuttingDown)));
        assert_eq!(registry.loaded_count(), 0);
    }

    #[tokio::test]
    async fn retiring_actor_finishes_snapshot_before_replacement_spawns() {
        let store = GatedStore::new();
        let (registry, key) = test_registry(Arc::clone(&store));
        let meta = registry.create("Gamma", &key).await.expect("create");
        let op = encode_condition_op(&registry, &key);
        let editor = crate::auth::Principal::user("alice");

        // Load actor 1 and commit one edit so shutdown writes a snapshot.
        let handle = registry.handle(meta.id).await.expect("initial load");
        let grant = handle.acquire(editor.clone()).await.expect("acquire");
        let seq = handle
            .submit(editor, grant.token.as_str().to_owned(), 0, vec![op])
            .await
            .expect("submit");
        assert_eq!(seq, 1);
        assert_eq!(store.load_calls.load(Ordering::SeqCst), 1);

        // Pause actor 1 in its final snapshot while retiring it.
        store.pause_snapshot.store(true, Ordering::SeqCst);
        let retiring = tokio::spawn(async move { handle.shutdown().await });
        store.snapshot_entered.notified().await;

        // A new caller sees the closed handle and must wait for actor 1's
        // final snapshot before loading a replacement actor.
        let r2 = Arc::clone(&registry);
        let replacement =
            tokio::spawn(
                async move { r2.with(meta.id, |h| async move { h.status().await }).await },
            );
        tokio::task::yield_now().await;
        assert_eq!(
            store.load_calls.load(Ordering::SeqCst),
            1,
            "replacement must not load while retiring actor is still snapshotting"
        );

        store.snapshot_release.notify_one();
        retiring.await.expect("join retiring").expect("retire ok");
        let status = replacement
            .await
            .expect("join replacement")
            .expect("replacement status");
        assert_eq!(status.last_seq, 1);
        assert_eq!(store.load_calls.load(Ordering::SeqCst), 2);
        registry.shutdown().await;
    }

    #[tokio::test]
    async fn shutdown_racing_with_submit_preserves_acknowledged_edits_and_is_bounded() {
        let store = GatedStore::new();
        let (registry, key) = test_registry(Arc::clone(&store));
        let meta = registry.create("Delta", &key).await.expect("create");
        let op = encode_condition_op(&registry, &key);
        let editor = crate::auth::Principal::user("bob");

        let grant = registry
            .with(meta.id, |h| {
                let who = editor.clone();
                async move { h.acquire(who).await }
            })
            .await
            .expect("acquire");

        // Pause the in-flight submit inside store.append, then race shutdown.
        store.pause_append.store(true, Ordering::SeqCst);
        let r1 = Arc::clone(&registry);
        let token = grant.token.as_str().to_owned();
        let submit = tokio::spawn(async move {
            r1.with(meta.id, |h| {
                let who = editor.clone();
                let tok = token.clone();
                let op = op.clone();
                async move { h.submit(who, tok, 0, vec![op]).await }
            })
            .await
        });
        store.append_entered.notified().await;

        let r2 = Arc::clone(&registry);
        let shutting =
            tokio::spawn(async move { r2.shutdown_within(Duration::from_secs(5)).await });
        tokio::task::yield_now().await;

        store.append_release.notify_one();
        let seq = submit.await.expect("join submit").expect("submit ack");
        assert_eq!(seq, 1);
        assert!(shutting.await.expect("join shutdown"));
        assert_eq!(registry.loaded_count(), 0);

        // Reopening from the same store observes the acknowledged edit.
        store.pause_append.store(false, Ordering::SeqCst);
        let (reopened, _) = test_registry(Arc::clone(&store));
        let status = reopened
            .with(meta.id, |h| async move { h.status().await })
            .await
            .expect("reopen status");
        assert_eq!(status.last_seq, 1);

        // A wedged final snapshot is bounded by shutdown_within.
        let op2 = encode_condition_op(&reopened, &key);
        let editor2 = crate::auth::Principal::user("carol");
        let grant2 = reopened
            .with(meta.id, |h| {
                let who = editor2.clone();
                async move { h.acquire(who).await }
            })
            .await
            .expect("acquire 2");
        reopened
            .with(meta.id, |h| {
                let who = editor2.clone();
                let tok = grant2.token.as_str().to_owned();
                let op2 = op2.clone();
                async move { h.submit(who, tok, 1, vec![op2]).await }
            })
            .await
            .expect("submit 2");
        store.pause_snapshot.store(true, Ordering::SeqCst);
        let completed = reopened.shutdown_within(Duration::from_millis(30)).await;
        assert!(!completed, "wedged snapshot must hit the shutdown bound");
        store.snapshot_release.notify_one();
    }

    #[tokio::test]
    async fn uncertain_append_that_committed_reloads_and_reconciles_repeat_submission() {
        let store = GatedStore::new();
        let (registry, key) = test_registry(Arc::clone(&store));
        let meta = registry.create("Epsilon", &key).await.expect("create");
        let op1 = encode_condition_op(&registry, &key);
        let op2 = encode_condition_op(&registry, &key);
        let editor = crate::auth::Principal::user("alice");

        let grant = registry
            .with(meta.id, |h| {
                let who = editor.clone();
                async move { h.acquire(who).await }
            })
            .await
            .expect("acquire");
        assert_eq!(store.load_calls.load(Ordering::SeqCst), 1);

        // Commit succeeds in storage, then returns Uncertain (lost ack).
        store.set_append_fault(AppendFault::LostAckAfterCommit);
        let err = registry
            .with(meta.id, |h| {
                let who = editor.clone();
                let tok = grant.token.as_str().to_owned();
                let op1 = op1.clone();
                async move { h.submit(who, tok, 0, vec![op1]).await }
            })
            .await;
        assert!(matches!(
            err,
            Err(ServerError::Store(StoreFault::Uncertain(_)))
        ));

        // The next call reloads the actor from storage; memory, sequence,
        // history, and risks all reflect the committed op.
        store.set_append_fault(AppendFault::None);
        let status = registry
            .with(meta.id, |h| async move { h.status().await })
            .await
            .expect("status after reload");
        assert_eq!(status.last_seq, 1);
        assert_eq!(store.load_calls.load(Ordering::SeqCst), 2);
        let risks = registry
            .with(meta.id, |h| async move { h.risks().await })
            .await
            .expect("risks after reload");
        assert_eq!(risks.as_of_seq, 1);
        assert_eq!(store.recent_ops(meta.id, 10).await.expect("ops").len(), 1);

        // Re-acquiring the lease and repeating the exact same (base_seq=0, op1)
        // reconciles to Ok(1) without duplicating the op.
        let grant2 = registry
            .with(meta.id, |h| {
                let who = editor.clone();
                async move { h.acquire(who).await }
            })
            .await
            .expect("re-acquire");
        let repeated = registry
            .with(meta.id, |h| {
                let who = editor.clone();
                let tok = grant2.token.as_str().to_owned();
                let op1 = op1.clone();
                async move { h.submit(who, tok, 0, vec![op1]).await }
            })
            .await
            .expect("idempotent repeat succeeds");
        assert_eq!(repeated, 1);
        assert_eq!(store.recent_ops(meta.id, 10).await.expect("ops").len(), 1);

        // Submitting a DIFFERENT op at stale base_seq=0 is still rejected.
        let stale = registry
            .with(meta.id, |h| {
                let who = editor.clone();
                let tok = grant2.token.as_str().to_owned();
                let op2 = op2.clone();
                async move { h.submit(who, tok, 0, vec![op2]).await }
            })
            .await;
        assert!(matches!(
            stale,
            Err(ServerError::StaleBase {
                base: 0,
                current: 1
            })
        ));

        // The next valid edit at base_seq=1 succeeds cleanly.
        let next_seq = registry
            .with(meta.id, |h| {
                let who = editor.clone();
                let tok = grant2.token.as_str().to_owned();
                let op2 = op2.clone();
                async move { h.submit(who, tok, 1, vec![op2]).await }
            })
            .await
            .expect("next valid edit");
        assert_eq!(next_seq, 2);
        assert_eq!(store.recent_ops(meta.id, 10).await.expect("ops").len(), 2);
        registry.shutdown().await;
    }

    #[tokio::test]
    async fn definite_rejection_before_commit_rolls_back_and_retries_cleanly() {
        let store = GatedStore::new();
        let (registry, key) = test_registry(Arc::clone(&store));
        let meta = registry.create("Zeta", &key).await.expect("create");
        let op1 = encode_condition_op(&registry, &key);
        let editor = crate::auth::Principal::user("bob");

        let grant = registry
            .with(meta.id, |h| {
                let who = editor.clone();
                async move { h.acquire(who).await }
            })
            .await
            .expect("acquire");

        store.set_append_fault(AppendFault::RejectBeforeCommit);
        let err = registry
            .with(meta.id, |h| {
                let who = editor.clone();
                let tok = grant.token.as_str().to_owned();
                let op1 = op1.clone();
                async move { h.submit(who, tok, 0, vec![op1]).await }
            })
            .await;
        assert!(matches!(
            err,
            Err(ServerError::Store(StoreFault::Backend(_)))
        ));

        // Actor rolled back in-place without unloading.
        assert_eq!(store.load_calls.load(Ordering::SeqCst), 1);
        let status = registry
            .with(meta.id, |h| async move { h.status().await })
            .await
            .expect("status");
        assert_eq!(status.last_seq, 0);
        assert!(store.recent_ops(meta.id, 10).await.expect("ops").is_empty());

        // Retrying the same batch once the backend recovers succeeds on the same lease.
        store.set_append_fault(AppendFault::None);
        let seq = registry
            .with(meta.id, |h| {
                let who = editor.clone();
                let tok = grant.token.as_str().to_owned();
                let op1 = op1.clone();
                async move { h.submit(who, tok, 0, vec![op1]).await }
            })
            .await
            .expect("retry after definite rejection");
        assert_eq!(seq, 1);
        registry.shutdown().await;
    }

    #[tokio::test]
    async fn store_sequence_conflict_invalidates_actor_and_reconciles_on_reload() {
        let store = GatedStore::new();
        let (registry, key) = test_registry(Arc::clone(&store));
        let meta = registry.create("Eta", &key).await.expect("create");
        let op1 = encode_condition_op(&registry, &key);
        let editor = crate::auth::Principal::user("carol");

        let grant = registry
            .with(meta.id, |h| {
                let who = editor.clone();
                async move { h.acquire(who).await }
            })
            .await
            .expect("acquire");
        assert_eq!(store.load_calls.load(Ordering::SeqCst), 1);

        store.set_append_fault(AppendFault::SequenceConflict);
        let err = registry
            .with(meta.id, |h| {
                let who = editor.clone();
                let tok = grant.token.as_str().to_owned();
                let op1 = op1.clone();
                async move { h.submit(who, tok, 0, vec![op1]).await }
            })
            .await;
        assert!(matches!(
            err,
            Err(ServerError::Store(StoreFault::Sequence { .. }))
        ));

        // Sequence conflict forced actor invalidation; next request reloads cleanly.
        store.set_append_fault(AppendFault::None);
        let status = registry
            .with(meta.id, |h| async move { h.status().await })
            .await
            .expect("status after reload");
        assert_eq!(status.last_seq, 0);
        assert_eq!(store.load_calls.load(Ordering::SeqCst), 2);
        registry.shutdown().await;
    }
}
