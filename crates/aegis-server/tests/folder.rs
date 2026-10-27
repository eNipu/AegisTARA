//! Folder-backed ProjectStore conformance and recovery tests.

mod common;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use aegis_server::ProjectId;
use aegis_server::store::{
    BaselineMeta, FolderStore, ProjectMeta, ProjectStore, StoreFault, StoredOp, StoredSnapshot,
};
use uuid::Uuid;

struct TempFolder(PathBuf);

impl TempFolder {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("aegis-folder-test-{}", Uuid::new_v4()));
        Self(path)
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

fn meta() -> ProjectMeta {
    ProjectMeta {
        id: ProjectId::new_v7(),
        name: "Brake".into(),
        methodology_key: "iso21434-r155".into(),
        methodology_version: "0.1.0".into(),
        created_ms: 1_700_000_000_000,
    }
}

fn op(seq: u64) -> StoredOp {
    StoredOp {
        seq,
        actor: if seq == 2 { "bob" } else { "alice" }.into(),
        timestamp_ms: 1_700_000_000_000 + seq,
        op: format!("{{\"op\":\"remove_asset\",\"id\":\"0199c82c-c001-7000-8000-{seq:012}\"}}")
            .into_bytes(),
    }
}

#[tokio::test]
async fn active_editors_is_empty_in_a_new_folder() {
    let folder = TempFolder::new();
    let store = FolderStore::open(folder.path()).expect("open empty folder");
    assert_eq!(
        store.active_editors(0).await.expect("active editors"),
        vec![]
    );
}

#[tokio::test]
async fn append_is_atomic_and_preserves_the_oplog_v1_bytes() {
    let folder = TempFolder::new();
    let m = meta();
    let store = FolderStore::open(folder.path()).expect("open");
    store.create(&m, b"s0").await.expect("create");
    let before = std::fs::read(folder.path().join("ops.jsonl")).expect("initial log");

    assert!(matches!(
        store.append(m.id, &[op(1), op(3)]).await,
        Err(StoreFault::Sequence {
            expected: 2,
            found: 3
        })
    ));
    assert_eq!(
        std::fs::read(folder.path().join("ops.jsonl")).expect("log"),
        before
    );
    assert!(
        store
            .load(m.id)
            .await
            .expect("load")
            .expect("project")
            .ops
            .is_empty()
    );

    store.append(m.id, &[op(1), op(2)]).await.expect("append");
    let expected = format!(
        "{{\"format\":\"aegis-oplog\",\"version\":1,\"methodology\":{{\"key\":\"iso21434-r155\",\"version\":\"0.1.0\"}}}}\n\
         {{\"seq\":1,\"op\":{}}}\n\
         {{\"seq\":2,\"op\":{}}}\n",
        String::from_utf8(op(1).op).expect("utf8"),
        String::from_utf8(op(2).op).expect("utf8")
    );
    assert_eq!(
        std::fs::read(folder.path().join("ops.jsonl")).expect("log"),
        expected.as_bytes()
    );
    assert_eq!(
        store.load(m.id).await.expect("load").expect("project").ops,
        vec![op(1), op(2)]
    );
    drop(store);

    let store = FolderStore::open(folder.path()).expect("reopen");
    assert_eq!(
        store.load(m.id).await.expect("load").expect("project").ops,
        vec![op(1), op(2)]
    );
}

#[tokio::test]
async fn history_recent_editors_and_audit_survive_reopen_and_detect_tampering() {
    let folder = TempFolder::new();
    let m = meta();
    let store = FolderStore::open(folder.path()).expect("open");
    store.create(&m, b"s0").await.expect("create");
    store
        .append(m.id, &[op(1), op(2), op(3)])
        .await
        .expect("append");

    let recent = store.recent_ops(m.id, 2).await.expect("recent");
    assert_eq!(recent, vec![op(3), op(2)]);
    assert!(
        store
            .recent_ops(m.id, 0)
            .await
            .expect("recent zero")
            .is_empty()
    );
    assert_eq!(
        store
            .active_editors(1_700_000_000_002)
            .await
            .expect("editors"),
        vec![
            ("alice".to_owned(), 1_700_000_000_003),
            ("bob".to_owned(), 1_700_000_000_002),
        ]
    );
    let report = store.verify_chain(m.id).await.expect("verify");
    assert!(report.is_intact());
    assert_eq!(report.verified, 3);
    drop(store);

    let store = FolderStore::open(folder.path()).expect("reopen");
    assert!(store.verify_chain(m.id).await.expect("verify").is_intact());
    drop(store);

    let path = folder.path().join("ops.jsonl");
    let log = String::from_utf8(std::fs::read(&path).expect("log")).expect("utf8");
    let original = String::from_utf8(op(2).op).expect("utf8");
    let changed = original.replace("000000000002", "000000000009");
    std::fs::write(&path, log.replacen(&original, &changed, 1)).expect("tamper");
    let store = FolderStore::open(folder.path()).expect("reopen tampered");
    let report = store.verify_chain(m.id).await.expect("verify tampering");
    assert_eq!(report.first_bad, Some(2));
    assert_eq!(report.verified, 1);
}

#[tokio::test]
async fn create_list_and_reopen_an_empty_project() {
    let folder = TempFolder::new();
    let m = meta();
    let store = FolderStore::open(folder.path()).expect("open empty folder");
    assert_eq!(store.count().await.expect("count"), 0);
    store.create(&m, b"snapshot-zero\n").await.expect("create");
    assert_eq!(store.count().await.expect("count"), 1);
    assert_eq!(store.list(10).await.expect("list"), vec![m.clone()]);
    assert!(store.list(0).await.expect("limited list").is_empty());
    drop(store);

    let store = FolderStore::open(folder.path()).expect("reopen project folder");
    let loaded = store
        .load(m.id)
        .await
        .expect("load")
        .expect("project exists");
    assert_eq!(loaded.meta, m);
    assert_eq!(loaded.snapshot.seq, 0);
    assert_eq!(loaded.snapshot.bytes, b"snapshot-zero\n");
    assert!(loaded.ops.is_empty());
}

fn baseline(project: ProjectId, name: &str, seq: u64) -> BaselineMeta {
    BaselineMeta {
        id: Uuid::now_v7(),
        project,
        name: name.into(),
        seq,
        created_by: "alice".into(),
        created_ms: 5,
        snapshot_bytes: 4,
    }
}

#[tokio::test]
async fn a_folder_server_restarts_with_identical_projects() {
    let folder = TempFolder::new();
    let store = Arc::new(FolderStore::open(folder.path()).expect("open"));
    common::restart_keeps_every_project_identical(store).await;
}

#[tokio::test]
async fn audit_verification_works_through_the_api_on_a_folder() {
    let folder = TempFolder::new();
    let store = Arc::new(FolderStore::open(folder.path()).expect("open"));
    common::audit_is_verifiable_by_admins_only(store).await;
}

#[tokio::test]
async fn the_files_are_the_canonical_oplog_and_snapshot_formats() {
    let folder = TempFolder::new();
    let m = common::methodology();
    let codec = aegis_model::OpCodec::new(m.clone());
    let fx = common::fixture(&m);
    let store = Arc::new(FolderStore::open(folder.path()).expect("open"));
    let server = common::TestServer::start_on(Arc::clone(&store), common::fast_config()).await;
    let mut alice = server.client("alice").await;
    let id = common::create_project(&mut alice, "p").await;
    let lease = common::lock(&mut alice, &id).await;
    common::submit(&mut alice, &id, &lease, 0, &fx.ops, &codec)
        .await
        .expect("accepted");
    server.stop().await;

    let log = std::fs::File::open(folder.path().join("ops.jsonl")).expect("log");
    let reader =
        aegis_model::OpLogReader::open(std::io::BufReader::new(log), m.clone()).expect("header");
    let replayed: Vec<_> = reader.map(|r| r.expect("record").op).collect();
    assert_eq!(replayed, fx.ops);

    let snapshots: Vec<_> = std::fs::read_dir(folder.path().join("snapshots"))
        .expect("dir")
        .map(|e| e.expect("entry").path())
        .collect();
    assert_eq!(snapshots.len(), 1, "the old snapshot is removed");
    let bytes = std::fs::read(&snapshots[0]).expect("snapshot");
    let project = aegis_model::Project::load(bytes.as_slice(), m.clone()).expect("loads");
    assert_eq!(project, common::expected_project(&m, &fx.ops));
}

#[tokio::test]
async fn snapshots_move_forward_only_and_keep_the_whole_history() {
    let folder = TempFolder::new();
    let m = meta();
    let store = FolderStore::open(folder.path()).expect("open");
    store.create(&m, b"s0").await.expect("create");
    store
        .append(m.id, &[op(1), op(2), op(3)])
        .await
        .expect("append");
    let ahead = StoredSnapshot {
        seq: 4,
        bytes: b"s4".to_vec(),
    };
    assert!(matches!(
        store.save_snapshot(m.id, &ahead).await,
        Err(StoreFault::Sequence {
            expected: 3,
            found: 4
        })
    ));
    store
        .save_snapshot(
            m.id,
            &StoredSnapshot {
                seq: 2,
                bytes: b"s2".to_vec(),
            },
        )
        .await
        .expect("snapshot");
    let back = StoredSnapshot {
        seq: 1,
        bytes: b"s1".to_vec(),
    };
    assert!(store.save_snapshot(m.id, &back).await.is_err());
    assert!(matches!(
        store.save_snapshot(ProjectId::new_v7(), &ahead).await,
        Err(StoreFault::Missing)
    ));
    let loaded = store.load(m.id).await.expect("load").expect("project");
    assert_eq!(
        (loaded.snapshot.seq, loaded.snapshot.bytes.as_slice()),
        (2, b"s2".as_slice())
    );
    assert_eq!(loaded.ops, vec![op(3)]);
    assert_eq!(store.verify_chain(m.id).await.expect("verify").verified, 3);
    assert_eq!(store.recent_ops(m.id, 10).await.expect("recent").len(), 3);
    drop(store);

    let store = FolderStore::open(folder.path()).expect("reopen");
    let loaded = store.load(m.id).await.expect("load").expect("project");
    assert_eq!((loaded.snapshot.seq, loaded.ops), (2, vec![op(3)]));
    let files = std::fs::read_dir(folder.path().join("snapshots"))
        .expect("dir")
        .count();
    assert_eq!(files, 1);
}

#[tokio::test]
async fn rename_baselines_and_delete_survive_reopen() {
    let folder = TempFolder::new();
    let m = meta();
    let store = FolderStore::open(folder.path()).expect("open");
    let wrong = ProjectId::new_v7();
    assert!(matches!(
        store.rename(wrong, "x").await,
        Err(StoreFault::Missing)
    ));
    assert!(matches!(
        store.delete(wrong).await,
        Err(StoreFault::Missing)
    ));
    assert!(matches!(
        store.list_baselines(wrong).await,
        Err(StoreFault::Missing)
    ));
    assert!(matches!(
        store.recent_ops(wrong, 1).await,
        Err(StoreFault::Missing)
    ));
    assert!(matches!(
        store.verify_chain(wrong).await,
        Err(StoreFault::Missing)
    ));

    store.create(&m, b"s0").await.expect("create");
    assert!(matches!(
        store.create(&m, b"s0").await,
        Err(StoreFault::Exists)
    ));
    store
        .append(m.id, &[op(1), op(2), op(3)])
        .await
        .expect("append");
    store.rename(m.id, "Renamed").await.expect("rename");
    let b = baseline(m.id, "B1", 3);
    store.create_baseline(&b, b"snap").await.expect("baseline");
    assert!(matches!(
        store.create_baseline(&b, b"snap").await,
        Err(StoreFault::Exists)
    ));
    let other = baseline(wrong, "B2", 0);
    assert!(matches!(
        store.create_baseline(&other, b"x").await,
        Err(StoreFault::Missing)
    ));
    drop(store);

    let store = FolderStore::open(folder.path()).expect("reopen");
    assert_eq!(store.list(10).await.expect("list")[0].name, "Renamed");
    assert_eq!(
        store.list_baselines(m.id).await.expect("list"),
        vec![b.clone()]
    );
    let (got, bytes) = store
        .load_baseline(m.id, b.id)
        .await
        .expect("load")
        .expect("found");
    assert_eq!((got, bytes.as_slice()), (b.clone(), b"snap".as_slice()));
    assert!(
        store
            .load_baseline(m.id, Uuid::now_v7())
            .await
            .expect("load")
            .is_none()
    );

    store.delete(m.id).await.expect("delete");
    assert!(store.load(m.id).await.expect("load").is_none());
    assert_eq!(store.count().await.expect("count"), 0);
    assert!(matches!(store.delete(m.id).await, Err(StoreFault::Missing)));
    assert!(matches!(
        store.list_baselines(m.id).await,
        Err(StoreFault::Missing)
    ));
    assert!(
        !folder.path().join("snapshots").exists() || {
            std::fs::read_dir(folder.path().join("snapshots"))
                .expect("dir")
                .count()
                == 0
        }
    );
    drop(store);

    let store = FolderStore::open(folder.path()).expect("reopen deleted");
    assert_eq!(store.count().await.expect("count"), 0);
    let fresh = meta();
    store
        .create(&fresh, b"again")
        .await
        .expect("a new project may follow");
    assert!(
        store
            .load(fresh.id)
            .await
            .expect("load")
            .expect("project")
            .ops
            .is_empty()
    );
}

#[tokio::test]
async fn an_uncommitted_tail_is_discarded_and_the_chain_stays_intact() {
    let folder = TempFolder::new();
    let m = meta();
    let store = FolderStore::open(folder.path()).expect("open");
    store.create(&m, b"s0").await.expect("create");
    store.append(m.id, &[op(1), op(2)]).await.expect("append");
    drop(store);

    // A crash after the log and sidecar were extended but before the
    // manifest named the new ops, plus a torn half-line, an orphan snapshot
    // and a leftover manifest temporary.
    let log = folder.path().join("ops.jsonl");
    let committed = std::fs::read(&log).expect("log");
    let mut torn = committed.clone();
    torn.extend_from_slice(
        &format!(
            "{{\"seq\":3,\"op\":{}}}\n",
            String::from_utf8(op(3).op).expect("utf8")
        )
        .into_bytes(),
    );
    torn.extend_from_slice(b"{\"seq\":4,\"op\":{\"op\":\"remo");
    std::fs::write(&log, torn).expect("tail");
    let sidecar = folder.path().join("ops.meta");
    let mut side = std::fs::read(&sidecar).expect("sidecar");
    side.extend_from_slice(&[7; 40]);
    std::fs::write(&sidecar, side).expect("sidecar tail");
    std::fs::write(
        folder
            .path()
            .join("snapshots")
            .join(format!("{}.aegis", Uuid::new_v4())),
        b"orphan",
    )
    .expect("orphan");
    std::fs::write(folder.path().join(".manifest-x.tmp"), b"{").expect("tmp");

    let store = FolderStore::open(folder.path()).expect("recovers");
    assert_eq!(std::fs::read(&log).expect("log"), committed);
    assert_eq!(
        std::fs::read_dir(folder.path().join("snapshots"))
            .expect("dir")
            .count(),
        1
    );
    assert!(!folder.path().join(".manifest-x.tmp").exists());
    assert_eq!(
        store.load(m.id).await.expect("load").expect("project").ops,
        vec![op(1), op(2)]
    );
    store
        .append(m.id, &[op(3)])
        .await
        .expect("append after recovery");
    let report = store.verify_chain(m.id).await.expect("verify");
    assert!(report.is_intact());
    assert_eq!(report.verified, 3);
}

#[tokio::test]
async fn damaged_committed_data_is_reported_not_repaired() {
    let folder = TempFolder::new();
    let m = meta();
    let store = FolderStore::open(folder.path()).expect("open");
    store.create(&m, b"s0").await.expect("create");
    store.append(m.id, &[op(1), op(2)]).await.expect("append");
    let b = baseline(m.id, "B1", 2);
    store.create_baseline(&b, b"snap").await.expect("baseline");
    drop(store);

    for entry in std::fs::read_dir(folder.path().join("snapshots")).expect("dir") {
        std::fs::write(entry.expect("entry").path(), b"changed").expect("tamper");
    }
    let store = FolderStore::open(folder.path()).expect("open still works");
    assert!(matches!(
        store.load(m.id).await,
        Err(StoreFault::Corrupt(_))
    ));
    assert!(matches!(
        store.load_baseline(m.id, b.id).await,
        Err(StoreFault::Corrupt(_))
    ));
    drop(store);

    let sidecar = folder.path().join("ops.meta");
    let bytes = std::fs::read(&sidecar).expect("sidecar");
    std::fs::write(&sidecar, &bytes[..bytes.len() - 5]).expect("truncate committed data");
    assert!(matches!(
        FolderStore::open(folder.path()),
        Err(StoreFault::Corrupt(_))
    ));
}

#[tokio::test]
async fn one_folder_has_one_writer() {
    let folder = TempFolder::new();
    let first = FolderStore::open(folder.path()).expect("open");
    assert!(matches!(
        FolderStore::open(folder.path()),
        Err(StoreFault::Backend(_))
    ));
    let clone = first.clone();
    drop(first);
    assert!(
        FolderStore::open(folder.path()).is_err(),
        "a clone keeps the lock"
    );
    drop(clone);
    FolderStore::open(folder.path()).expect("free again");
}

#[tokio::test]
async fn reads_are_bounded_by_their_limits_not_by_the_history() {
    let folder = TempFolder::new();
    let m = meta();
    let store = FolderStore::open(folder.path()).expect("open");
    store.create(&m, b"s0").await.expect("create");
    let ops: Vec<_> = (1..=50).map(op).collect();
    store.append(m.id, &ops).await.expect("append");
    let all = store
        .recent_ops(m.id, usize::MAX)
        .await
        .expect("huge limit");
    assert_eq!(all.len(), 50);
    assert_eq!(all[0].seq, 50);
    let oversized = StoredOp {
        op: vec![b' '; aegis_model::MAX_OP_RECORD_BYTES + 1],
        ..op(51)
    };
    assert!(store.append(m.id, &[op(51), oversized]).await.is_err());
    assert_eq!(
        store
            .load(m.id)
            .await
            .expect("load")
            .expect("project")
            .ops
            .len(),
        50
    );
    let big = vec![0u8; aegis_model::MAX_PROJECT_FILE_BYTES as usize + 1];
    assert!(
        store
            .save_snapshot(
                m.id,
                &StoredSnapshot {
                    seq: 50,
                    bytes: big
                }
            )
            .await
            .is_err()
    );
}

#[tokio::test]
async fn debris_from_a_failed_append_never_reaches_the_next_one() {
    let folder = TempFolder::new();
    let m = meta();
    let store = FolderStore::open(folder.path()).expect("open");
    store.create(&m, b"s0").await.expect("create");
    store.append(m.id, &[op(1)]).await.expect("append");
    // What a write that died half way leaves behind while the process lives.
    for name in ["ops.jsonl", "ops.meta"] {
        use std::io::Write as _;
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(folder.path().join(name))
            .expect("open");
        file.write_all(b"{\"seq\":2,\"op\":{\"op\":\"partial")
            .expect("debris");
    }
    store
        .append(m.id, &[op(2)])
        .await
        .expect("append over debris");
    assert_eq!(
        store.load(m.id).await.expect("load").expect("project").ops,
        vec![op(1), op(2)]
    );
    assert!(store.verify_chain(m.id).await.expect("verify").is_intact());
    drop(store);
    let store = FolderStore::open(folder.path()).expect("reopen");
    assert_eq!(store.verify_chain(m.id).await.expect("verify").verified, 2);
}

#[tokio::test]
async fn racing_writers_cannot_interleave() {
    let folder = TempFolder::new();
    let m = meta();
    let store = FolderStore::open(folder.path()).expect("open");
    store.create(&m, b"s0").await.expect("create");
    let mut tasks = Vec::new();
    for _ in 0..8 {
        let store = store.clone();
        let id = m.id;
        tasks.push(tokio::spawn(async move {
            store.append(id, &[op(1), op(2)]).await
        }));
    }
    let mut won = 0;
    for task in tasks {
        match task.await.expect("task") {
            Ok(()) => won += 1,
            Err(StoreFault::Sequence { .. }) => {}
            Err(other) => panic!("unexpected {other}"),
        }
    }
    assert_eq!(won, 1);
    assert_eq!(store.verify_chain(m.id).await.expect("verify").verified, 2);
}

#[tokio::test]
async fn open_existing_does_not_create_folders() {
    let folder = TempFolder::new();
    assert!(FolderStore::open_existing(folder.path()).is_err());
    assert!(!folder.path().exists());
    let store = FolderStore::open(folder.path()).expect("create");
    assert!(
        FolderStore::open_existing(folder.path()).is_err(),
        "a folder with no project is not a project folder"
    );
    store.create(&meta(), b"s0").await.expect("create");
    drop(store);
    FolderStore::open_existing(folder.path()).expect("now it is a project folder");
}
