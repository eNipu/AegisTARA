//! Where projects live between requests.
//!
//! A project is stored as a **snapshot** (the `aegis-project` v1 bytes, as
//! of some sequence number) plus the **ops** accepted after it (one
//! canonical JSON op per sequence number, from `OpCodec`). Loading replays
//! the ops onto the snapshot, so the op log is the source of truth and a
//! snapshot is a cache that bounds replay time (D10).
//!
//! The trait is the only thing the rest of the server knows about storage.
//! [`MemoryStore`] backs tests and development; the PostgreSQL backend
//! (job 030) implements the same contract.
//!
//! Contract every backend must keep:
//! - `append` is atomic for the whole slice: all ops are stored or none.
//! - `append` refuses a slice that does not continue the stored sequence
//!   exactly (`StoreFault::Sequence`), so two writers can never interleave.
//! - `load` returns ops with `seq > snapshot.seq`, in order, gap free.
//! - Nothing is acknowledged before it is durable.
//! - Every stored op is linked into a per-project hash chain (`audit.rs`),
//!   and `verify_chain` recomputes it.

mod audit;
mod memory;
mod postgres;

use std::future::Future;

pub use audit::{ChainReport, ChainVerifier, Hash, genesis, link};
pub use memory::MemoryStore;
pub use postgres::{DbTls, PostgresOptions, PostgresStore};

use crate::ids::ProjectId;

/// Static facts about a project.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectMeta {
    /// Identity.
    pub id: ProjectId,
    /// Display name.
    pub name: String,
    /// Methodology key, fixed for the project's lifetime.
    pub methodology_key: String,
    /// Methodology version the project was created with.
    pub methodology_version: String,
    /// Creation time, milliseconds since the Unix epoch.
    pub created_ms: u64,
}

/// One accepted op as stored: canonical bytes plus who and when.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredOp {
    /// Position in the project's history, from 1, gap free.
    pub seq: u64,
    /// Subject that submitted it.
    pub actor: String,
    /// Acceptance time, milliseconds since the Unix epoch.
    pub timestamp_ms: u64,
    /// `OpCodec::encode` output.
    pub op: Vec<u8>,
}

/// A snapshot and the sequence number it is current as of.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredSnapshot {
    /// The snapshot holds the effect of ops `1..=seq`.
    pub seq: u64,
    /// `Project::save` output.
    pub bytes: Vec<u8>,
}

/// Everything needed to rebuild a project.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoadedProject {
    /// Static facts.
    pub meta: ProjectMeta,
    /// The latest snapshot.
    pub snapshot: StoredSnapshot,
    /// Ops after the snapshot, in order.
    pub ops: Vec<StoredOp>,
}

/// A named, frozen project state (job 039): the snapshot after ops
/// `1..=seq`. Baselines are never changed, only created and listed, and
/// deleted with their project.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BaselineMeta {
    /// Identity (UUIDv7, so ordering by ID is creation order).
    pub id: uuid::Uuid,
    /// The project.
    pub project: ProjectId,
    /// Display name, for example `B3 SOP gate`.
    pub name: String,
    /// The baseline holds the effect of ops `1..=seq`.
    pub seq: u64,
    /// Subject that created it.
    pub created_by: String,
    /// Milliseconds since the Unix epoch.
    pub created_ms: u64,
    /// Size of the snapshot.
    pub snapshot_bytes: u64,
}

/// A storage failure. Details stay in the server log.
#[derive(Debug, thiserror::Error)]
pub enum StoreFault {
    /// The slice does not continue the stored history.
    #[error("sequence conflict: expected {expected}, got {found}")]
    Sequence {
        /// The next sequence number the store expected.
        expected: u64,
        /// The one it was given.
        found: u64,
    },
    /// A project with this ID already exists.
    #[error("project already exists")]
    Exists,
    /// The project is not in the store.
    #[error("project not found")]
    Missing,
    /// Stored data is not what the server wrote (for example a gap).
    #[error("stored data is corrupt: {0}")]
    Corrupt(String),
    /// The backend failed (connection, disk, constraint).
    #[error("backend failure: {0}")]
    Backend(String),
}

/// The storage backend of the server.
///
/// Every method is async because real backends do I/O. Callers never hold
/// a lock across a call (C2).
pub trait ProjectStore: Send + Sync + 'static {
    /// Stores a new project with its empty snapshot at sequence 0.
    ///
    /// # Errors
    ///
    /// [`StoreFault::Exists`] if the ID is taken.
    fn create(
        &self,
        meta: &ProjectMeta,
        snapshot: &[u8],
    ) -> impl Future<Output = Result<(), StoreFault>> + Send;

    /// Lists projects ordered by ID (creation order, since IDs are
    /// time-ordered). At most `limit` rows.
    fn list(
        &self,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<ProjectMeta>, StoreFault>> + Send;

    /// Counts projects.
    fn count(&self) -> impl Future<Output = Result<usize, StoreFault>> + Send;

    /// Loads a project, or `None` if it does not exist.
    fn load(
        &self,
        id: ProjectId,
    ) -> impl Future<Output = Result<Option<LoadedProject>, StoreFault>> + Send;

    /// Appends `ops` atomically. See the module contract.
    fn append(
        &self,
        id: ProjectId,
        ops: &[StoredOp],
    ) -> impl Future<Output = Result<(), StoreFault>> + Send;

    /// Recomputes the project's audit hash chain from its stored ops.
    /// Streams the history; memory use does not grow with its length.
    ///
    /// # Errors
    ///
    /// [`StoreFault::Missing`] if the project does not exist.
    fn verify_chain(
        &self,
        id: ProjectId,
    ) -> impl Future<Output = Result<ChainReport, StoreFault>> + Send;

    /// Replaces the snapshot with a newer one. Ops up to `seq` may then be
    /// dropped from replay (a backend may keep them for audit).
    fn save_snapshot(
        &self,
        id: ProjectId,
        snapshot: &StoredSnapshot,
    ) -> impl Future<Output = Result<(), StoreFault>> + Send;

    /// Changes a project's display name.
    ///
    /// # Errors
    ///
    /// [`StoreFault::Missing`] if the project does not exist.
    fn rename(
        &self,
        id: ProjectId,
        name: &str,
    ) -> impl Future<Output = Result<(), StoreFault>> + Send;

    /// Deletes a project with its history and baselines, atomically.
    ///
    /// # Errors
    ///
    /// [`StoreFault::Missing`] if the project does not exist.
    fn delete(&self, id: ProjectId) -> impl Future<Output = Result<(), StoreFault>> + Send;

    /// The newest `limit` accepted ops of a project, newest first.
    fn recent_ops(
        &self,
        id: ProjectId,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<StoredOp>, StoreFault>> + Send;

    /// Every subject that had an op accepted at or after `since_ms`, with
    /// the time of their latest one, in any project. Rebuilds the seat
    /// ledger after a restart (backlog 039).
    fn active_editors(
        &self,
        since_ms: u64,
    ) -> impl Future<Output = Result<Vec<(String, u64)>, StoreFault>> + Send;

    /// Stores a baseline.
    ///
    /// # Errors
    ///
    /// [`StoreFault::Missing`] if the project does not exist,
    /// [`StoreFault::Exists`] if the baseline ID is taken.
    fn create_baseline(
        &self,
        meta: &BaselineMeta,
        snapshot: &[u8],
    ) -> impl Future<Output = Result<(), StoreFault>> + Send;

    /// A project's baselines, oldest first.
    fn list_baselines(
        &self,
        project: ProjectId,
    ) -> impl Future<Output = Result<Vec<BaselineMeta>, StoreFault>> + Send;

    /// One baseline with its snapshot, or `None`.
    fn load_baseline(
        &self,
        project: ProjectId,
        id: uuid::Uuid,
    ) -> impl Future<Output = Result<Option<(BaselineMeta, Vec<u8>)>, StoreFault>> + Send;
}
