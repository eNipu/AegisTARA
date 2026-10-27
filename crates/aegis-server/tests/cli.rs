//! The `aegis-server import-folder` command, up to the point it needs a
//! database. The copy itself is tested in `import.rs`.

use std::process::{Command, Output};

use aegis_server::ProjectId;
use aegis_server::store::{FolderStore, ProjectMeta, ProjectStore};

fn run(args: &[&str]) -> Output {
    // No AEGIS_* variables leak in from the developer's shell.
    Command::new(env!("CARGO_BIN_EXE_aegis-server"))
        .args(args)
        .env_clear()
        .output()
        .expect("runs")
}

fn text(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

#[test]
fn unknown_arguments_print_the_usage() {
    let out = run(&["frobnicate"]);
    assert!(!out.status.success());
    assert!(text(&out).contains("usage: aegis-server [import-folder <project folder>]"));
}

#[test]
fn a_path_that_is_not_a_project_folder_is_named_and_not_created() {
    let path = std::env::temp_dir().join(format!("aegis-cli-missing-{}", uuid::Uuid::new_v4()));
    let out = run(&["import-folder", path.to_str().expect("utf8")]);
    assert!(!out.status.success());
    assert!(
        text(&out).contains("not an AegisTARA project folder"),
        "{}",
        text(&out)
    );
    assert!(!path.exists());
}

#[tokio::test]
async fn a_valid_folder_still_needs_a_license_before_any_database_is_touched() {
    let path = std::env::temp_dir().join(format!("aegis-cli-folder-{}", uuid::Uuid::new_v4()));
    let store = FolderStore::open(&path).expect("open");
    let meta = ProjectMeta {
        id: ProjectId::new_v7(),
        name: "p".into(),
        methodology_key: "iso21434-r155".into(),
        methodology_version: "0.1.0".into(),
        created_ms: 1,
    };
    store.create(&meta, b"x").await.expect("create");
    drop(store);
    let out = run(&["import-folder", path.to_str().expect("utf8")]);
    let _ = std::fs::remove_dir_all(&path);
    assert!(!out.status.success());
    assert!(
        text(&out).contains("AEGIS_LICENSE_FILE is required"),
        "{}",
        text(&out)
    );
}
