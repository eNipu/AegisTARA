//! Importing a standalone project folder into a server (backlog 107).
//!
//! A consultant works in standalone mode and hands the folder to a client;
//! the client's administrator imports it. The import **copies** the stored
//! history: the same project ID, every op with its original actor and time,
//! the same audit hash chain, the folder's snapshot, and the baselines with
//! their IDs. Replaying the ops as new edits would give them new actors,
//! times and hashes, which would rewrite the history a reviewer relies on.
//!
//! The folder is checked first (its hash chain must verify and its
//! methodology must be hosted here, at the same version). The copy then runs
//! in bounded batches, and its result is checked against the folder: the
//! chain head, the stored state, and a rebuild of the project through the
//! same path the server uses to load one. Any failure after the project
//! exists removes it again, so a half-imported project is never left behind.
//!
//! Imported actors keep their names. If their edits fall inside the license's
//! seat window, they hold a seat after the next server start, as any editor
//! with recent edits does.

use std::sync::Arc;

use aegis_model::Project;

use crate::actor::Loaded;
use crate::registry::Registry;
use crate::store::{
    FigureMeta, FolderStore, Hash, ProjectMeta, ProjectStore, StoreFault,
    validate_and_sanitize_figure, validate_caption, validate_target,
};

/// What an import copied.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportReport {
    /// The project as now stored (the folder's identity, unchanged).
    pub project: ProjectMeta,
    /// Ops copied, which is also the sequence number of the last one.
    pub ops: u64,
    /// Baselines copied.
    pub baselines: usize,
    /// The audit chain head, equal to the folder's.
    pub head: Hash,
}

/// Why a folder was not imported. Nothing is left behind in any case.
#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    /// The folder holds no project.
    #[error("the folder holds no project")]
    NoProject,
    /// The folder's methodology is not hosted by this server.
    #[error("this server does not host the methodology {0:?}; add its pack first")]
    UnknownMethodology(String),
    /// The server hosts the methodology, but not the folder's version.
    #[error(
        "the folder uses {key} version {folder}, but this server hosts version {hosted}; \
         open the folder with a matching app version or host that pack version"
    )]
    MethodologyVersion {
        /// Methodology key.
        key: String,
        /// Version the project was created with.
        folder: String,
        /// Version this server hosts.
        hosted: String,
    },
    /// The server already hosts as many projects as its limit allows.
    #[error("the server already hosts its maximum number of projects")]
    Busy,
    /// A project with this ID is already on the server.
    #[error("a project with this ID is already on the server")]
    Exists,
    /// The folder cannot be trusted or read: a broken audit chain, damaged
    /// files, or a history that does not apply.
    #[error("the project folder is damaged: {0}")]
    Damaged(String),
    /// The server's store failed.
    #[error("the server's storage failed: {0}")]
    Store(#[from] StoreFault),
}

/// A failure while reading the folder: the folder is at fault.
fn damaged(fault: StoreFault) -> ImportError {
    ImportError::Damaged(fault.to_string())
}

impl<S: ProjectStore> Registry<S> {
    /// Imports the project in `source` as described in the module docs.
    ///
    /// Time O(history + snapshot sizes); memory O(one batch + one project),
    /// as for opening the project.
    ///
    /// # Errors
    ///
    /// See [`ImportError`].
    pub async fn import_folder(&self, source: &FolderStore) -> Result<ImportReport, ImportError> {
        let meta = source
            .list(1)
            .await
            .map_err(damaged)?
            .into_iter()
            .next()
            .ok_or(ImportError::NoProject)?;
        let hosted = self
            .methodologies()
            .get(&meta.methodology_key)
            .ok_or_else(|| ImportError::UnknownMethodology(meta.methodology_key.clone()))?;
        if hosted.methodology.version() != meta.methodology_version {
            return Err(ImportError::MethodologyVersion {
                key: meta.methodology_key.clone(),
                folder: meta.methodology_version.clone(),
                hosted: hosted.methodology.version().to_owned(),
            });
        }
        let chain = source.verify_chain(meta.id).await.map_err(damaged)?;
        if let Some(bad) = chain.first_bad {
            return Err(ImportError::Damaged(format!(
                "the audit chain is broken at change {bad}; the history was changed outside AegisTARA"
            )));
        }
        if self.store().count().await? >= self.config().max_projects {
            return Err(ImportError::Busy);
        }

        // The project starts empty, as a new one does; the folder's own
        // history is then appended to it.
        let methodology = Arc::clone(&hosted.methodology);
        let empty = tokio::task::spawn_blocking(move || {
            let mut out = Vec::new();
            Project::new(methodology).save(&mut out).map(|()| out)
        })
        .await
        .map_err(|e| ImportError::Damaged(format!("the import task failed: {e}")))?
        .map_err(|e| ImportError::Damaged(format!("the empty project does not save: {e}")))?;
        match self.store().create(&meta, &empty).await {
            Ok(()) => {}
            Err(StoreFault::Exists) => return Err(ImportError::Exists),
            Err(other) => return Err(other.into()),
        }

        let copied = self
            .copy_into(source, &meta, chain.verified, chain.head)
            .await;
        if copied.is_err()
            && let Err(fault) = self.store().delete(meta.id).await
        {
            tracing::error!(project = %meta.id, error = %fault, "half-imported project could not be removed");
        }
        copied
    }

    async fn copy_into(
        &self,
        source: &FolderStore,
        meta: &ProjectMeta,
        ops: u64,
        head: Hash,
    ) -> Result<ImportReport, ImportError> {
        let store = self.store();
        let mut copied = 0u64;
        let mut stream = source.stream_ops(meta.id);
        while let Some(batch) = stream.recv().await {
            let batch = batch.map_err(damaged)?;
            store.append(meta.id, &batch).await?;
            copied += batch.len() as u64;
        }
        if copied != ops {
            return Err(ImportError::Damaged(format!(
                "the history ended after {copied} of {ops} changes"
            )));
        }

        let from = source
            .load(meta.id)
            .await
            .map_err(damaged)?
            .ok_or(ImportError::NoProject)?;
        if from.snapshot.seq > 0 {
            store.save_snapshot(meta.id, &from.snapshot).await?;
        }
        let mut baselines = 0;
        for baseline in source.list_baselines(meta.id).await.map_err(damaged)? {
            let (baseline, snapshot) = source
                .load_baseline(meta.id, baseline.id)
                .await
                .map_err(damaged)?
                .ok_or_else(|| ImportError::Damaged("a baseline vanished while reading".into()))?;
            store.create_baseline(&baseline, &snapshot).await?;
            baselines += 1;
        }
        for figure in source.list_figures(meta.id).await.map_err(damaged)? {
            let (figure, bytes) = source
                .load_figure(meta.id, figure.id)
                .await
                .map_err(damaged)?
                .ok_or_else(|| ImportError::Damaged("a figure vanished while reading".into()))?;
            let validated = validate_and_sanitize_figure(
                &figure.media_type,
                &bytes,
                self.config().max_figure_bytes,
            )
            .map_err(|e| ImportError::Damaged(format!("invalid figure: {e}")))?;
            if figure.byte_len != bytes.len() as u64
                || figure.sha256 != validated.sha256
                || figure.width != validated.width
                || figure.height != validated.height
            {
                return Err(ImportError::Damaged(
                    "figure metadata does not match its validated bytes".into(),
                ));
            }
            let caption = validate_caption(&figure.caption)
                .map_err(|e| ImportError::Damaged(format!("invalid figure caption: {e}")))?;
            let target = validate_target(&figure.target)
                .map_err(|e| ImportError::Damaged(format!("invalid figure target: {e}")))?;
            let normalized = FigureMeta {
                caption,
                target,
                media_type: validated.media.as_str().to_owned(),
                width: validated.width,
                height: validated.height,
                byte_len: validated.bytes.len() as u64,
                sha256: validated.sha256,
                ..figure
            };
            store
                .put_figure(
                    &normalized,
                    &validated.bytes,
                    self.config().max_figures_per_project,
                    self.config().max_project_figures_bytes,
                )
                .await?;
        }

        // Check what is stored against what the folder held.
        let chain = store.verify_chain(meta.id).await?;
        if !chain.is_intact() || chain.verified != ops || chain.head != head {
            return Err(ImportError::Damaged(
                "the copied audit chain differs from the folder's".into(),
            ));
        }
        let stored = store.load(meta.id).await?.ok_or(StoreFault::Missing)?;
        if stored != from {
            return Err(ImportError::Damaged(
                "the copied project differs from the folder's".into(),
            ));
        }
        let methodology = self
            .methodologies()
            .get(&meta.methodology_key)
            .map(|h| Arc::clone(&h.methodology))
            .ok_or_else(|| ImportError::UnknownMethodology(meta.methodology_key.clone()))?;
        tokio::task::spawn_blocking(move || Loaded::rebuild(stored, methodology).map(|_| ()))
            .await
            .map_err(|e| ImportError::Damaged(format!("the import task failed: {e}")))?
            .map_err(|e| ImportError::Damaged(format!("the history does not apply: {e}")))?;

        Ok(ImportReport {
            project: meta.clone(),
            ops,
            baselines,
            head,
        })
    }
}
