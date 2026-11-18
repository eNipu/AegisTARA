//! A workspace folder on disk (job 164, slice 164a): open, create, convert
//! beside and save, and the refusals of an untrusted folder.

use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use aegis_client::commands::Edit;
use aegis_client::folder::{FolderError, SaveReport, WorkspaceFolder, is_workspace};
use aegis_client::pages::risk_rows;
use aegis_client::sample::sample_ops;
use aegis_client::workspace::{Origin, Workspace};
use aegis_model::{
    MAX_WORKSPACE_FILE_BYTES, MAX_WORKSPACE_FILES, Methodology, Project, StoreError,
};

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("aegis-folder-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    dir
}

fn sample() -> (Arc<Methodology>, Workspace) {
    let m = Arc::new(Methodology::iso21434_r155().expect("builtin"));
    let mut project = Project::new(Arc::clone(&m));
    for op in sample_ops(&m, "t", 1).expect("sample") {
        project.apply(op).expect("applies");
    }
    (m, Workspace::new(project, 0))
}

fn edit(ws: &mut Workspace, edit: &Edit) {
    let ops = edit.to_ops(ws.project(), "t", 2).expect("ops");
    ws.apply(ops, Origin::Edit).expect("applies");
}

fn rename_asset(ws: &mut Workspace) {
    let (_, asset, _) = ws.project().assets().next().expect("asset");
    edit(
        ws,
        &Edit::RenameAsset {
            asset,
            name: "Renamed ECU".into(),
        },
    );
}

fn remove_tree(ws: &mut Workspace) {
    let (_, tree, _) = ws.project().attack_trees().next().expect("tree");
    edit(ws, &Edit::RemoveTree { tree });
}

fn saved(project: &Project) -> Vec<u8> {
    let mut out = Vec::new();
    project.save(&mut out).expect("save");
    out
}

fn list(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .expect("read dir")
        .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn save_edit_save_reopen_gives_equal_bytes_and_risks() {
    let dir = scratch("reopen");
    let (m, mut ws) = sample();
    let mut folder = WorkspaceFolder::create(&dir, ws.project()).expect("create");
    rename_asset(&mut ws);
    remove_tree(&mut ws);
    folder.save(ws.project()).expect("save");
    drop(folder);

    let (_folder, reopened) = WorkspaceFolder::open(&dir, m).expect("open");
    assert_eq!(saved(&reopened), saved(ws.project()));
    assert_eq!(risk_rows(&Workspace::new(reopened, 0)), risk_rows(&ws));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn only_changed_files_are_written() {
    let dir = scratch("changed");
    let (_, mut ws) = sample();
    let mut folder = WorkspaceFolder::create(&dir, ws.project()).expect("create");
    let damage = dir.join("shared/damage.json");
    let mut planted = fs::read(&damage).expect("damage file");
    assert_eq!(planted.pop(), Some(b'\n'));
    planted.extend_from_slice(b" \n");
    fs::write(&damage, &planted).expect("plant");

    rename_asset(&mut ws);
    let report = folder.save(ws.project()).expect("save");
    assert_eq!(report.written, vec!["systems/main/assets.json".to_owned()]);
    assert!(report.removed.is_empty());
    assert_eq!(fs::read(&damage).expect("damage file"), planted);
    assert_eq!(
        folder.save(ws.project()).expect("save"),
        SaveReport::default()
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_removed_tree_file_is_deleted() {
    let dir = scratch("tree");
    let (_, mut ws) = sample();
    let mut folder = WorkspaceFolder::create(&dir, ws.project()).expect("create");
    let trees = dir.join("systems/main/trees");
    let names = list(&trees);
    assert_eq!(names.len(), 1, "{names:?}");

    remove_tree(&mut ws);
    let report = folder.save(ws.project()).expect("save");
    assert_eq!(
        report.removed,
        vec![format!("systems/main/trees/{}", names[0])]
    );
    assert!(!trees.join(&names[0]).exists());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_second_open_is_locked() {
    let dir = scratch("locked");
    let (m, ws) = sample();
    let first = WorkspaceFolder::create(&dir, ws.project()).expect("create");
    let second = WorkspaceFolder::open(&dir, Arc::clone(&m));
    assert!(
        matches!(second, Err(FolderError::Locked { .. })),
        "{second:?}"
    );
    drop(first);
    let (folder, project) = WorkspaceFolder::open(&dir, m).expect("open");
    assert_eq!(saved(&project), saved(ws.project()));
    assert_eq!(folder.root(), dir.as_path());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn holds_names_only_this_folder() {
    let (dir, other) = (scratch("holds"), scratch("holds-other"));
    let (_, ws) = sample();
    let folder = WorkspaceFolder::create(&dir, ws.project()).expect("create");
    fs::create_dir_all(&other).expect("other");
    assert!(folder.holds(&dir));
    assert!(folder.holds(&dir.join("shared").join("..")));
    assert!(!folder.holds(&other));
    assert!(!folder.holds(&dir.join("missing")));
    #[cfg(unix)]
    {
        let link = scratch("holds-link");
        std::os::unix::fs::symlink(&dir, &link).expect("symlink");
        assert!(folder.holds(&link));
        let _ = fs::remove_file(&link);
    }
    drop(folder);
    let _ = fs::remove_dir_all(&dir);
    let _ = fs::remove_dir_all(&other);
}

#[test]
fn create_refuses_a_non_empty_folder() {
    let (_, ws) = sample();
    let dir = scratch("not-empty");
    fs::create_dir_all(&dir).expect("dir");
    fs::write(dir.join("notes.txt"), b"x").expect("notes");
    let result = WorkspaceFolder::create(&dir, ws.project());
    assert!(
        matches!(result, Err(FolderError::NotEmpty { .. })),
        "{result:?}"
    );
    let _ = fs::remove_dir_all(&dir);

    let dir = scratch("dot-only");
    fs::create_dir_all(&dir).expect("dir");
    fs::write(dir.join(".DS_Store"), b"x").expect("ds store");
    WorkspaceFolder::create(&dir, ws.project()).expect("create");
    assert!(is_workspace(&dir));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn convert_beside_keeps_other_files() {
    let dir = scratch("convert");
    let (m, ws) = sample();
    fs::create_dir_all(&dir).expect("dir");
    fs::write(dir.join("manifest.json"), b"{\"old\": true}").expect("manifest");
    fs::write(dir.join("ops.jsonl"), b"line\n").expect("ops");
    let folder = WorkspaceFolder::convert_beside(&dir, ws.project()).expect("convert");
    drop(folder);
    assert_eq!(
        fs::read(dir.join("manifest.json")).expect("manifest"),
        b"{\"old\": true}"
    );
    assert_eq!(fs::read(dir.join("ops.jsonl")).expect("ops"), b"line\n");
    assert!(is_workspace(&dir));
    let (folder, project) = WorkspaceFolder::open(&dir, m).expect("open");
    assert_eq!(saved(&project), saved(ws.project()));
    drop(folder);

    let again = WorkspaceFolder::convert_beside(&dir, ws.project());
    assert!(
        matches!(again, Err(FolderError::AlreadyWorkspace { .. })),
        "{again:?}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// A workspace of the sample, written and closed, for planting bad files.
fn planted(name: &str) -> (PathBuf, Arc<Methodology>) {
    let dir = scratch(name);
    let (m, ws) = sample();
    drop(WorkspaceFolder::create(&dir, ws.project()).expect("create"));
    (dir, m)
}

fn open_err(dir: &Path, m: Arc<Methodology>) -> FolderError {
    let err = WorkspaceFolder::open(dir, m).expect_err("refused");
    let _ = fs::remove_dir_all(dir);
    err
}

fn sized(path: &Path, len: u64) {
    File::create(path)
        .expect("file")
        .set_len(len)
        .expect("set len");
}

#[test]
fn a_folder_without_aegis_json_is_not_a_workspace() {
    let (dir, m) = planted("no-aegis");
    fs::remove_file(dir.join("aegis.json")).expect("remove");
    let err = open_err(&dir, m);
    assert!(matches!(err, FolderError::NotAWorkspace { .. }), "{err:?}");
}

#[test]
fn an_unknown_file_is_refused_by_path() {
    let (dir, m) = planted("unknown");
    fs::write(dir.join("systems/main/notes.txt"), b"x").expect("notes");
    let err = open_err(&dir, m);
    assert!(
        matches!(err, FolderError::Store(StoreError::WorkspacePath { .. })),
        "{err:?}"
    );
}

#[test]
fn bad_json_is_refused_naming_the_file() {
    let (dir, m) = planted("bad-json");
    fs::write(dir.join("systems/main/assets.json"), b"{").expect("assets");
    let err = open_err(&dir, m);
    assert!(
        matches!(
            err,
            FolderError::Store(StoreError::WorkspaceEnvelope { .. })
        ),
        "{err:?}"
    );
    assert!(
        err.to_string().contains("systems/main/assets.json"),
        "{err}"
    );
}

#[test]
fn a_file_over_its_cap_is_too_large() {
    let (dir, m) = planted("too-large");
    sized(
        &dir.join("systems/main/trees/big.json"),
        MAX_WORKSPACE_FILE_BYTES + 1,
    );
    let err = open_err(&dir, m);
    assert!(
        matches!(err, FolderError::TooLarge { max, .. } if max == MAX_WORKSPACE_FILE_BYTES),
        "{err:?}"
    );
}

#[test]
fn files_over_the_total_cap_are_too_big() {
    let (dir, m) = planted("too-big");
    for i in 0..5 {
        sized(
            &dir.join(format!("systems/main/trees/big{i}.json")),
            14 * 1024 * 1024,
        );
    }
    let err = open_err(&dir, m);
    assert!(matches!(err, FolderError::TooBig { .. }), "{err:?}");
}

#[test]
fn too_many_files_are_refused() {
    let (dir, m) = planted("too-many");
    let trees = dir.join("systems/main/trees");
    for i in 0..=MAX_WORKSPACE_FILES {
        File::create(trees.join(format!("x{i}.json"))).expect("file");
    }
    let err = open_err(&dir, m);
    assert!(
        matches!(err, FolderError::TooManyFiles { max } if max == MAX_WORKSPACE_FILES),
        "{err:?}"
    );
}

#[test]
fn a_folder_seven_levels_down_is_too_deep() {
    let (dir, m) = planted("too-deep");
    let deep = dir.join("systems/a/b/c/d/e/f");
    fs::create_dir_all(&deep).expect("dirs");
    fs::write(deep.join("x.json"), b"{}").expect("file");
    let err = open_err(&dir, m);
    assert!(
        matches!(&err, FolderError::TooDeep { path } if path == "systems/a/b/c/d/e/f"),
        "{err:?}"
    );
}

#[cfg(unix)]
#[test]
fn a_symbolic_link_is_refused() {
    let (dir, m) = planted("symlink");
    let outside = scratch("symlink-target");
    fs::create_dir_all(&outside).expect("dir");
    let target = outside.join("x.json");
    fs::write(&target, b"{}").expect("target");
    std::os::unix::fs::symlink(&target, dir.join("systems/main/trees/x.json")).expect("link");
    let err = open_err(&dir, m);
    let _ = fs::remove_dir_all(&outside);
    assert!(
        matches!(&err, FolderError::Symlink { path } if path == "systems/main/trees/x.json"),
        "{err:?}"
    );
}
