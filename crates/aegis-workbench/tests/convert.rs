//! Converting an old standalone project folder into workspace files
//! (job 164, slice 164c): same project, old files untouched, and the
//! refusals.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use aegis_client::commands::Edit;
use aegis_client::folder::WorkspaceFolder;
use aegis_model::{Methodology, OpCodec, Project};
use aegis_server::store::{ProjectMeta, StoredOp};
use aegis_server::{FolderStore, ProjectId, ProjectStore};
use aegis_workbench::convert::{convert_legacy, is_legacy};
use aegis_workbench::sample::workbench_sample;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("aegis-convert-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    dir
}

fn methodology() -> Arc<Methodology> {
    Arc::new(Methodology::iso21434_r155().expect("builtin"))
}

fn saved(project: &Project) -> Vec<u8> {
    let mut out = Vec::new();
    project.save(&mut out).expect("save");
    out
}

/// Writes an old folder in `dir` holding the workbench sample plus one
/// rename after the snapshot, and returns the project it should hold.
fn old_folder(dir: &Path, m: &Arc<Methodology>, version: &str) -> Project {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    runtime.block_on(async {
        let store = FolderStore::open(dir).expect("store");
        let meta = ProjectMeta {
            id: ProjectId::new_v7(),
            name: "Old".into(),
            methodology_key: m.key().into(),
            methodology_version: version.into(),
            created_ms: 1,
        };
        let mut expected = workbench_sample().project().clone();
        store
            .create(&meta, &saved(&expected))
            .await
            .expect("create");
        let (_, asset, _) = expected.assets().next().expect("asset");
        let edit = Edit::RenameAsset {
            asset,
            name: "Renamed ECU".into(),
        };
        let codec = OpCodec::new(Arc::clone(m));
        let mut stored = Vec::new();
        for (i, op) in edit
            .to_ops(&expected, "old", 2)
            .expect("ops")
            .into_iter()
            .enumerate()
        {
            stored.push(StoredOp {
                seq: i as u64 + 1,
                actor: "old".into(),
                timestamp_ms: 2,
                op: codec.encode(&op).expect("encode"),
            });
            expected.apply(op).expect("applies");
        }
        store.append(meta.id, &stored).await.expect("append");
        drop(store);
        expected
    })
}

fn old_files(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut files: Vec<(PathBuf, Vec<u8>)> = ["manifest.json", "ops.jsonl", "ops.meta"]
        .iter()
        .map(|name| (dir.join(name), fs::read(dir.join(name)).expect("read")))
        .collect();
    for entry in fs::read_dir(dir.join("snapshots")).expect("snapshots") {
        let path = entry.expect("entry").path();
        let bytes = fs::read(&path).expect("read");
        files.push((path, bytes));
    }
    files.sort();
    files
}

#[test]
fn an_old_folder_converts_to_the_same_project() {
    let dir = scratch("same");
    let m = methodology();
    let expected = old_folder(&dir, &m, m.version());
    let before = old_files(&dir);
    assert!(
        before
            .iter()
            .any(|(p, _)| p.extension().is_some_and(|e| e == "aegis"))
    );
    assert!(is_legacy(&dir));

    let (folder, project) = convert_legacy(&dir, Arc::clone(&m)).expect("converts");
    assert!(project == expected);
    assert_eq!(saved(&project), saved(&expected));
    drop(folder);

    assert_eq!(old_files(&dir), before);
    assert!(!is_legacy(&dir));
    let (_folder, reopened) = WorkspaceFolder::open(&dir, m).expect("opens");
    assert!(reopened == expected);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn another_methodology_version_is_refused() {
    let dir = scratch("version");
    let m = methodology();
    old_folder(&dir, &m, "0.0-test");
    let Err(message) = convert_legacy(&dir, m) else {
        panic!("another version must be refused");
    };
    assert!(message.contains("0.0-test"), "{message}");
    assert!(!dir.join("aegis.json").exists());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn an_empty_folder_is_not_legacy() {
    let dir = scratch("empty");
    fs::create_dir_all(&dir).expect("dir");
    fs::write(dir.join("notes.txt"), b"notes").expect("write");
    assert!(!is_legacy(&dir));
    let _ = fs::remove_dir_all(&dir);
}
