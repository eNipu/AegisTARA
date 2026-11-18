//! A save cut off part way and the recovery on the next open (job 164,
//! slice 164b), and the refusal of a save record that cannot be trusted.

use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use aegis_client::commands::Edit;
use aegis_client::folder::{FolderError, SaveStop, WorkspaceFolder};
use aegis_client::sample::sample_ops;
use aegis_client::workspace::{Origin, Workspace};
use aegis_model::{Methodology, Project};

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("aegis-recovery-{name}-{}", std::process::id()));
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

/// Renames the asset and removes the tree; returns the tree file's path.
fn change(ws: &mut Workspace, dir: &Path) -> PathBuf {
    let (_, asset, _) = ws.project().assets().next().expect("asset");
    edit(
        ws,
        &Edit::RenameAsset {
            asset,
            name: "Renamed ECU".into(),
        },
    );
    let (_, tree, _) = ws.project().attack_trees().next().expect("tree");
    edit(ws, &Edit::RemoveTree { tree });
    let trees = dir.join("systems/main/trees");
    let names: Vec<_> = fs::read_dir(&trees)
        .expect("trees")
        .map(|e| e.expect("entry").path())
        .collect();
    assert_eq!(names.len(), 1, "{names:?}");
    names[0].clone()
}

fn saved(project: &Project) -> Vec<u8> {
    let mut out = Vec::new();
    project.save(&mut out).expect("save");
    out
}

/// Every staged file and journal anywhere in `root`.
fn leftovers(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for entry in fs::read_dir(root).expect("read dir") {
        let path = entry.expect("entry").path();
        let name = path
            .file_name()
            .expect("name")
            .to_string_lossy()
            .into_owned();
        if fs::symlink_metadata(&path).expect("meta").is_dir() {
            found.extend(leftovers(&path));
        } else if name.ends_with(".aegis-new")
            || name.ends_with(".aegis-save.json")
            || name.ends_with(".aegis-save.json.tmp")
        {
            found.push(path);
        }
    }
    found
}

/// Creates the sample, applies [`change`] and stops its save at `stop`.
/// Returns the folder, the old and new `save()` bytes and the tree file.
fn stopped(name: &str, stop: SaveStop) -> (PathBuf, Arc<Methodology>, Vec<u8>, Vec<u8>, PathBuf) {
    let dir = scratch(name);
    let (m, mut ws) = sample();
    let old = saved(ws.project());
    let mut folder = WorkspaceFolder::create(&dir, ws.project()).expect("create");
    let tree = change(&mut ws, &dir);
    let new = saved(ws.project());
    assert_ne!(old, new);
    folder.save_until(ws.project(), stop).expect("save until");
    drop(folder);
    (dir, m, old, new, tree)
}

#[test]
fn a_save_stopped_before_the_journal_opens_as_the_old_project() {
    let (dir, m, old, _, tree) = stopped("staging", SaveStop::AfterStaging);
    assert!(!leftovers(&dir).is_empty());
    assert!(!dir.join(".aegis-save.json").exists());
    let (folder, project) = WorkspaceFolder::open(&dir, m).expect("open");
    assert_eq!(saved(&project), old);
    assert!(tree.exists());
    assert_eq!(leftovers(&dir), Vec::<PathBuf>::new());
    drop(folder);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_save_stopped_after_the_journal_opens_as_the_new_project() {
    let (dir, m, _, new, tree) = stopped("journal", SaveStop::AfterJournal);
    assert!(dir.join(".aegis-save.json").is_file());
    assert!(tree.exists());
    let (folder, project) = WorkspaceFolder::open(&dir, m).expect("open");
    assert_eq!(saved(&project), new);
    assert_eq!(leftovers(&dir), Vec::<PathBuf>::new());
    assert!(!tree.exists());
    drop(folder);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_finished_save_leaves_nothing_behind() {
    let dir = scratch("finished");
    let (_, mut ws) = sample();
    let mut folder = WorkspaceFolder::create(&dir, ws.project()).expect("create");
    change(&mut ws, &dir);
    let report = folder.save(ws.project()).expect("save");
    assert!(!report.written.is_empty() && !report.removed.is_empty());
    assert_eq!(leftovers(&dir), Vec::<PathBuf>::new());
    drop(folder);
    let _ = fs::remove_dir_all(&dir);
}

fn journal(replace: &[&str], remove: &[&str]) -> String {
    serde_json::json!({
        "format": "aegis-save-journal",
        "version": 1,
        "replace": replace,
        "remove": remove,
    })
    .to_string()
}

#[test]
fn a_hostile_journal_is_refused_and_touches_nothing() {
    let hostile = [
        "../victim.json",
        "/tmp/victim.json",
        "shared\\damage.json",
        "Shared/damage.json",
        "shared/../../victim.json",
    ];
    for (i, bad) in hostile.iter().enumerate() {
        for in_replace in [false, true] {
            let base = scratch(&format!("hostile-{i}-{in_replace}"));
            let dir = base.join("ws");
            let (m, ws) = sample();
            drop(WorkspaceFolder::create(&dir, ws.project()).expect("create"));
            let victim = base.join("victim.json");
            fs::write(&victim, b"victim").expect("victim");
            let damage = dir.join("shared/damage.json");
            let before = fs::read(&damage).expect("damage");
            // A valid entry first: refusing must come before acting on it.
            let record = if in_replace {
                fs::write(dir.join("shared/damage.json.aegis-new"), b"{}").expect("staged");
                journal(&["shared/damage.json", bad], &[])
            } else {
                journal(&[], &["shared/damage.json", bad])
            };
            fs::write(dir.join(".aegis-save.json"), record).expect("journal");

            let result = WorkspaceFolder::open(&dir, m);
            assert!(
                matches!(result, Err(FolderError::Journal { .. })),
                "{bad} {in_replace}: {result:?}"
            );
            assert_eq!(fs::read(&victim).expect("victim"), b"victim");
            assert_eq!(fs::read(&damage).expect("damage"), before);
            let _ = fs::remove_dir_all(&base);
        }
    }
}

#[cfg(unix)]
#[test]
fn a_journal_through_a_link_is_refused() {
    let dir = scratch("link");
    let outside = scratch("link-target");
    let (m, ws) = sample();
    drop(WorkspaceFolder::create(&dir, ws.project()).expect("create"));
    fs::create_dir_all(&outside).expect("outside");
    let victim = outside.join("victim.json");
    fs::write(&victim, b"victim").expect("victim");
    std::os::unix::fs::symlink(&outside, dir.join("systems/link")).expect("link");
    fs::write(
        dir.join(".aegis-save.json"),
        journal(&[], &["systems/link/victim.json"]),
    )
    .expect("journal");

    let result = WorkspaceFolder::open(&dir, m);
    assert!(
        matches!(&result, Err(FolderError::Symlink { path }) if path == "systems/link"),
        "{result:?}"
    );
    assert!(victim.exists());
    let _ = fs::remove_dir_all(&dir);
    let _ = fs::remove_dir_all(&outside);
}

#[test]
fn an_oversized_journal_is_refused() {
    let dir = scratch("oversized");
    let (m, ws) = sample();
    drop(WorkspaceFolder::create(&dir, ws.project()).expect("create"));
    File::create(dir.join(".aegis-save.json"))
        .expect("journal")
        .set_len(8 * 1024 * 1024 + 1)
        .expect("set len");
    let result = WorkspaceFolder::open(&dir, m);
    assert!(
        matches!(&result, Err(FolderError::Journal { problem }) if problem.contains("larger")),
        "{result:?}"
    );
    let _ = fs::remove_dir_all(&dir);
}
