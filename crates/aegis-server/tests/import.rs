//! Importing a standalone project folder into a server (backlog 107).
//!
//! The import copies history; it never replays it as new edits. Everything a
//! reviewer can see about the project afterwards (IDs, actors, times, the
//! audit chain, baselines) must be what the folder held.

mod common;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use aegis_model::{Op, OpCodec, Provenance};
use aegis_server::config::ServerConfig;
use aegis_server::methodologies::Methodologies;
use aegis_server::store::{FolderStore, ProjectMeta, ProjectStore, StoreFault};
use aegis_server::{ImportError, MemoryStore, ProjectId, Registry, pb};
use uuid::Uuid;

const RENAMES: u64 = 1_300;

static CLOCK: AtomicU64 = AtomicU64::new(1_700_000_000_000);

struct TempFolder(PathBuf);

impl TempFolder {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!("aegis-import-test-{}", Uuid::new_v4())))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempFolder {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn clock_config() -> ServerConfig {
    ServerConfig {
        wall_clock: || CLOCK.fetch_add(7, Ordering::SeqCst),
        ..common::fast_config()
    }
}

struct Built {
    id: ProjectId,
    asset: aegis_model::AssetId,
    ops: Vec<Op>,
}

/// A standalone folder as the app leaves it: a project edited by two people,
/// a baseline, a rename, and a snapshot written at shutdown.
async fn build_folder(path: &Path) -> Built {
    let m = common::methodology();
    let codec = OpCodec::new(m.clone());
    let fx = common::fixture(&m);
    let store = Arc::new(FolderStore::open(path).expect("open"));
    let server = common::TestServer::start_on(Arc::clone(&store), clock_config()).await;
    let mut alice = server.client("alice").await;
    let mut bob = server.client("bob").await;
    let text = common::create_project(&mut alice, "Brake ECU").await;
    let id: ProjectId = text.parse().expect("id");

    let lease = common::lock(&mut alice, &text).await;
    common::submit(&mut alice, &text, &lease, 0, &fx.ops, &codec)
        .await
        .expect("accepted");
    server
        .registry
        .create_baseline(id, "B1 concept", "alice")
        .await
        .expect("baseline");
    alice
        .release_lock(pb::ReleaseLockRequest {
            project_id: text.clone(),
            token: lease.token.clone(),
        })
        .await
        .expect("released");

    let mut all = fx.ops.clone();
    let lease = common::lock(&mut bob, &text).await;
    let mut seq = fx.ops.len() as u64;
    let renames: Vec<Op> = (0..RENAMES)
        .map(|i| Op::RenameAsset {
            id: fx.asset,
            name: format!("Brake ECU {i}").into(),
            provenance: Provenance::default(),
        })
        .collect();
    for chunk in renames.chunks(500) {
        seq = common::submit(&mut bob, &text, &lease, seq, chunk, &codec)
            .await
            .expect("accepted");
    }
    all.extend(renames);
    server
        .registry
        .create_baseline(id, "B2 later", "bob")
        .await
        .expect("baseline");
    server.stop().await;
    drop(store);
    Built {
        id,
        asset: fx.asset,
        ops: all,
    }
}

fn target(config: ServerConfig) -> (Arc<MemoryStore>, Registry<MemoryStore>) {
    let store = Arc::new(MemoryStore::new());
    let registry = Registry::new(
        Arc::clone(&store),
        Arc::new(Methodologies::builtin().expect("builtin")),
        Arc::new(config),
    );
    (store, registry)
}

async fn all_ops<S: ProjectStore>(store: &S, id: ProjectId) -> Vec<aegis_server::store::StoredOp> {
    let mut ops = store.recent_ops(id, usize::MAX).await.expect("history");
    ops.reverse();
    ops
}

#[tokio::test]
async fn an_imported_project_keeps_ids_history_actors_times_chain_and_baselines() {
    let folder = TempFolder::new();
    let built = build_folder(folder.path()).await;
    let source = FolderStore::open_existing(folder.path()).expect("reopen");
    let (store, registry) = target(common::fast_config());

    let report = registry.import_folder(&source).await.expect("imports");
    let total = built.ops.len() as u64;
    assert_eq!(report.project.id, built.id);
    assert_eq!(report.ops, total);
    assert_eq!(report.baselines, 2);

    let from = source.list(1).await.expect("list").remove(0);
    assert_eq!(store.list(10).await.expect("list"), vec![from.clone()]);
    assert_eq!(report.project, from);

    let theirs = all_ops(&source, built.id).await;
    let ours = all_ops(&*store, built.id).await;
    assert_eq!(ours.len() as u64, total);
    assert_eq!(
        ours, theirs,
        "ops, actors and times are copied, not replayed"
    );
    let actors: std::collections::BTreeSet<_> = ours.iter().map(|o| o.actor.as_str()).collect();
    assert_eq!(actors, ["alice", "bob"].into_iter().collect());
    // A batch is accepted at one time, so times rise per batch.
    assert!(
        ours.windows(2)
            .all(|w| w[0].timestamp_ms <= w[1].timestamp_ms)
    );
    let times: std::collections::BTreeSet<_> = ours.iter().map(|o| o.timestamp_ms).collect();
    assert!(
        times.len() >= 3,
        "several distinct acceptance times survive"
    );

    let a = source.verify_chain(built.id).await.expect("source chain");
    let b = store.verify_chain(built.id).await.expect("target chain");
    assert!(a.is_intact() && b.is_intact());
    assert_eq!(
        (b.verified, b.head),
        (a.verified, a.head),
        "the audit chain is identical"
    );
    assert_eq!(report.head, a.head);

    let baselines = source.list_baselines(built.id).await.expect("baselines");
    assert_eq!(
        store.list_baselines(built.id).await.expect("baselines"),
        baselines
    );
    for b in &baselines {
        let ours = store.load_baseline(built.id, b.id).await.expect("load");
        assert_eq!(
            ours,
            source.load_baseline(built.id, b.id).await.expect("load")
        );
    }
    let (s, t) = (
        source.load(built.id).await.expect("load").expect("project"),
        store.load(built.id).await.expect("load").expect("project"),
    );
    assert_eq!(t.snapshot, s.snapshot, "the folder's snapshot is reused");
    assert_eq!(t.ops, s.ops);

    // The server serves it and the next edit continues the same history.
    let m = common::methodology();
    let server = common::TestServer::start_on(Arc::clone(&store), clock_config()).await;
    let mut carol = server.client("carol").await;
    let text = built.id.to_string();
    let (seq, project) = common::open(&mut carol, &text, &m).await;
    assert_eq!(seq, total);
    assert_eq!(project, common::expected_project(&m, &built.ops));
    let lease = common::lock(&mut carol, &text).await;
    let codec = OpCodec::new(m.clone());
    let next = Op::RenameAsset {
        id: built.asset,
        name: "After import".into(),
        provenance: Provenance::default(),
    };
    let last = common::submit(&mut carol, &text, &lease, seq, &[next], &codec)
        .await
        .expect("accepted");
    assert_eq!(last, total + 1);
    server.stop().await;
    assert!(
        store
            .verify_chain(built.id)
            .await
            .expect("chain")
            .is_intact()
    );
}

#[tokio::test]
async fn an_empty_folder_and_a_project_that_is_already_there_are_refused() {
    let empty = TempFolder::new();
    let source = FolderStore::open(empty.path()).expect("open");
    let (_, registry) = target(common::fast_config());
    assert!(matches!(
        registry.import_folder(&source).await,
        Err(ImportError::NoProject)
    ));
    assert!(matches!(
        FolderStore::open_existing(empty.path().join("nothing-here")),
        Err(StoreFault::Backend(_))
    ));
    assert!(
        !empty.path().join("nothing-here").exists(),
        "looking at a path must not create it"
    );

    let folder = TempFolder::new();
    let built = build_folder(folder.path()).await;
    let source = FolderStore::open_existing(folder.path()).expect("reopen");
    let (store, registry) = target(common::fast_config());
    registry.import_folder(&source).await.expect("first import");
    let before = all_ops(&*store, built.id).await;
    assert!(matches!(
        registry.import_folder(&source).await,
        Err(ImportError::Exists)
    ));
    assert_eq!(
        all_ops(&*store, built.id).await,
        before,
        "the project is untouched"
    );
    assert_eq!(store.count().await.expect("count"), 1);
}

#[tokio::test]
async fn a_full_server_unknown_methodologies_and_other_versions_are_refused() {
    let folder = TempFolder::new();
    let built = build_folder(folder.path()).await;
    let source = FolderStore::open_existing(folder.path()).expect("reopen");

    let (store, registry) = target(ServerConfig {
        max_projects: 1,
        ..common::fast_config()
    });
    let other = ProjectMeta {
        id: ProjectId::new_v7(),
        name: "other".into(),
        methodology_key: "iso21434-r155".into(),
        methodology_version: "0.1.0".into(),
        created_ms: 1,
    };
    store.create(&other, b"x").await.expect("create");
    assert!(matches!(
        registry.import_folder(&source).await,
        Err(ImportError::Busy)
    ));
    assert!(store.load(built.id).await.expect("load").is_none());

    for (key, version, unknown) in [
        ("no-such-method", "1", true),
        ("iso21434-r155", "99.0.0", false),
    ] {
        let odd = TempFolder::new();
        let store = FolderStore::open(odd.path()).expect("open");
        let meta = ProjectMeta {
            id: ProjectId::new_v7(),
            name: "odd".into(),
            methodology_key: key.into(),
            methodology_version: version.into(),
            created_ms: 1,
        };
        store.create(&meta, b"x").await.expect("create");
        let (target_store, registry) = target(common::fast_config());
        match registry.import_folder(&store).await {
            Err(ImportError::UnknownMethodology(k)) if unknown => assert_eq!(k, key),
            Err(ImportError::MethodologyVersion { .. }) if !unknown => {}
            other => panic!("unexpected {other:?}"),
        }
        assert_eq!(target_store.count().await.expect("count"), 0);
    }
}

#[tokio::test]
async fn a_folder_whose_history_was_changed_is_refused_and_leaves_nothing_behind() {
    let folder = TempFolder::new();
    let built = build_folder(folder.path()).await;
    let log = folder.path().join("ops.jsonl");
    let text = std::fs::read_to_string(&log).expect("log");
    assert!(text.contains("Brake ECU 7"));
    std::fs::write(&log, text.replacen("Brake ECU 7", "Brake ECU 8", 1)).expect("tamper");
    let source = FolderStore::open_existing(folder.path()).expect("reopen");
    let (store, registry) = target(common::fast_config());
    assert!(matches!(
        registry.import_folder(&source).await,
        Err(ImportError::Damaged(_))
    ));
    assert!(store.load(built.id).await.expect("load").is_none());
    assert_eq!(store.count().await.expect("count"), 0);
}

#[tokio::test]
async fn a_failure_after_the_history_was_copied_removes_the_half_imported_project() {
    let folder = TempFolder::new();
    let built = build_folder(folder.path()).await;
    // Damage a baseline snapshot: the ops and chain are fine, so the import
    // only fails once it reaches the baselines.
    let baseline = {
        let source = FolderStore::open_existing(folder.path()).expect("open");
        source
            .list_baselines(built.id)
            .await
            .expect("list")
            .remove(0)
    };
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(folder.path().join("manifest.json")).expect("m"))
            .expect("json");
    let file = manifest["project"]["baselines"]
        .as_array()
        .expect("baselines")
        .iter()
        .find(|b| b["id"] == baseline.id.to_string())
        .expect("found")["snapshot_id"]
        .as_str()
        .expect("id")
        .to_owned();
    std::fs::write(
        folder
            .path()
            .join("snapshots")
            .join(format!("{file}.aegis")),
        b"x",
    )
    .expect("damage");

    let source = FolderStore::open_existing(folder.path()).expect("reopen");
    let (store, registry) = target(common::fast_config());
    assert!(registry.import_folder(&source).await.is_err());
    assert!(store.load(built.id).await.expect("load").is_none());
    assert_eq!(store.count().await.expect("count"), 0);
}
