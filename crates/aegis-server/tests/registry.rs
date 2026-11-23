//! Registry lifecycle tests: a failed load is shared by its waiters and a
//! shutdown closes admission before a blocked load can publish an actor.

use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use aegis_model::{Methodology, Project};
use aegis_server::config::ServerConfig;
use aegis_server::methodologies::Methodologies;
use aegis_server::store::{
    BaselineMeta, ChainReport, FigureMeta, LoadedProject, ProjectMembership, ProjectMeta,
    ProjectStore, StoreFault, StoredOp, StoredSnapshot,
};
use aegis_server::{MemoryStore, ProjectId, Registry, ServerError};
use tokio::sync::Notify;

/// Generates the methods that simply delegate to the wrapped MemoryStore.
macro_rules! delegate_inner {
    ($($name:ident($($arg:ident: $ty:ty),*) -> $ret:ty;)*) => {
        $(
            fn $name(&self, $($arg: $ty),*) -> impl Future<Output = $ret> + Send {
                self.inner.$name($($arg),*)
            }
        )*
    };
}

/// A store that can fail the next load and hold a load on a pair of notifies.
struct FaultStore {
    inner: MemoryStore,
    load_calls: AtomicUsize,
    failures_left: AtomicUsize,
    entered: Notify,
    release: Notify,
    block: bool,
}

impl FaultStore {
    fn new(block: bool) -> Self {
        Self {
            inner: MemoryStore::new(),
            load_calls: AtomicUsize::new(0),
            failures_left: AtomicUsize::new(0),
            entered: Notify::new(),
            release: Notify::new(),
            block,
        }
    }

    fn fail_next(&self) {
        self.failures_left.store(1, Ordering::SeqCst);
    }
}

impl ProjectStore for FaultStore {
    delegate_inner! {
        create(meta: &ProjectMeta, snapshot: &[u8]) -> Result<(), StoreFault>;
        list(limit: usize) -> Result<Vec<ProjectMeta>, StoreFault>;
        count() -> Result<usize, StoreFault>;
        append(id: ProjectId, ops: &[StoredOp]) -> Result<(), StoreFault>;
        verify_chain(id: ProjectId) -> Result<ChainReport, StoreFault>;
        save_snapshot(id: ProjectId, snapshot: &StoredSnapshot) -> Result<(), StoreFault>;
        rename(id: ProjectId, name: &str) -> Result<(), StoreFault>;
        delete(id: ProjectId) -> Result<(), StoreFault>;
        recent_ops(id: ProjectId, limit: usize) -> Result<Vec<StoredOp>, StoreFault>;
        active_editors(since_ms: u64) -> Result<Vec<(String, u64)>, StoreFault>;
        create_baseline(meta: &BaselineMeta, snapshot: &[u8]) -> Result<(), StoreFault>;
        list_baselines(project: ProjectId) -> Result<Vec<BaselineMeta>, StoreFault>;
        load_baseline(project: ProjectId, id: uuid::Uuid) -> Result<Option<(BaselineMeta, Vec<u8>)>, StoreFault>;
        all_memberships() -> Result<Vec<ProjectMembership>, StoreFault>;
        project_memberships(project: ProjectId) -> Result<Vec<ProjectMembership>, StoreFault>;
        memberships_for(members: &[String]) -> Result<Vec<ProjectMembership>, StoreFault>;
        set_membership(membership: &ProjectMembership) -> Result<(), StoreFault>;
        remove_membership(project: ProjectId, member: &str) -> Result<bool, StoreFault>;
        put_figure(meta: &FigureMeta, data: &[u8], max_count: usize, max_total_bytes: u64) -> Result<(), StoreFault>;
        list_figures(project: ProjectId) -> Result<Vec<FigureMeta>, StoreFault>;
        load_figure(project: ProjectId, id: uuid::Uuid) -> Result<Option<(FigureMeta, Vec<u8>)>, StoreFault>;
        update_figure(project: ProjectId, id: uuid::Uuid, caption: &str, target: &str) -> Result<Option<FigureMeta>, StoreFault>;
        delete_figure(project: ProjectId, id: uuid::Uuid) -> Result<bool, StoreFault>;
    }

    async fn load(&self, id: ProjectId) -> Result<Option<LoadedProject>, StoreFault> {
        self.load_calls.fetch_add(1, Ordering::SeqCst);
        if self.failures_left.swap(0, Ordering::SeqCst) > 0 {
            return Err(StoreFault::Backend("injected load failure".into()));
        }
        if self.block {
            self.entered.notify_one();
            self.release.notified().await;
        }
        self.inner.load(id).await
    }
}

fn config() -> Arc<ServerConfig> {
    Arc::new(ServerConfig {
        idle_unload: Duration::from_secs(3600),
        lease: Duration::from_secs(60),
        wall_clock: || 1_700_000_000_000,
        ..ServerConfig::default()
    })
}

async fn seeded(block: bool) -> (Arc<FaultStore>, Arc<Registry<FaultStore>>, ProjectId) {
    let m = Arc::new(Methodology::iso21434_r155().expect("builtin"));
    let project = Project::new(Arc::clone(&m));
    let mut snapshot = Vec::new();
    project.save(&mut snapshot).expect("save");
    let id = ProjectId::new_v7();
    let meta = ProjectMeta {
        id,
        name: "p".into(),
        methodology_key: "iso21434-r155".into(),
        methodology_version: m.version().to_owned(),
        created_ms: 0,
    };
    let store = Arc::new(FaultStore::new(block));
    store.inner.create(&meta, &snapshot).await.expect("seed");
    let registry = Arc::new(Registry::new(
        Arc::clone(&store),
        Arc::new(Methodologies::builtin().expect("builtin")),
        config(),
    ));
    (store, registry, id)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failed_load_is_shared_by_waiters_and_retried_by_a_new_caller() {
    let (store, registry, id) = seeded(false).await;
    store.fail_next();

    let mut tasks = Vec::new();
    for _ in 0..8 {
        let registry = Arc::clone(&registry);
        tasks.push(tokio::spawn(async move {
            registry
                .with(id, |_| async { Ok::<(), ServerError>(()) })
                .await
        }));
    }
    for task in tasks {
        let _ = task.await.expect("task");
    }
    // A request that arrives after the failed cell was dropped legitimately
    // retries; the invariant is one load per failed cell plus one retry, and
    // never more than one actor.
    assert!(store.load_calls.load(Ordering::SeqCst) <= 2);
    assert!(registry.loaded_count() <= 1);

    registry
        .with(id, |_| async { Ok::<(), ServerError>(()) })
        .await
        .expect("a new request retries and succeeds");
    assert_eq!(store.load_calls.load(Ordering::SeqCst), 2);
    assert_eq!(
        registry.loaded_count(),
        1,
        "exactly one actor after the retry"
    );
    registry.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn shutdown_closes_admission_before_a_blocked_load_can_spawn() {
    let (store, registry, id) = seeded(true).await;

    let load = tokio::spawn({
        let registry = Arc::clone(&registry);
        async move {
            registry
                .with(id, |_| async { Ok::<(), ServerError>(()) })
                .await
        }
    });
    store.entered.notified().await; // the load is inside FaultStore::load

    registry.shutdown().await; // drains while the load is blocked
    store.release.notify_one();

    let result = load.await.expect("task");
    assert!(result.is_err(), "the load sees the closed admission");
    assert_eq!(
        registry.loaded_count(),
        0,
        "no actor was published after the drain"
    );

    assert!(
        matches!(
            registry
                .with(id, |_| async { Ok::<(), ServerError>(()) })
                .await,
            Err(ServerError::ShuttingDown)
        ),
        "new requests are refused after shutdown"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_live_actor_is_drained_and_reports_shutting_down() {
    let (_store, registry, id) = seeded(false).await;
    registry
        .with(id, |_| async { Ok::<(), ServerError>(()) })
        .await
        .expect("load");
    assert_eq!(registry.loaded_count(), 1);

    registry.shutdown().await;
    assert_eq!(
        registry.loaded_count(),
        0,
        "shutdown drains every live actor"
    );
    assert!(matches!(
        registry
            .with(id, |_| async { Ok::<(), ServerError>(()) })
            .await,
        Err(ServerError::ShuttingDown)
    ));
}
