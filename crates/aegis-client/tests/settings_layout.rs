//! The workbench dock layout is remembered per workspace (job 182).

use std::path::PathBuf;

use aegis_client::settings::{MAX_REMEMBERED_VIEWS, Settings, WorkbenchLayout};

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "aegis-settings-layout-{}-{name}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn layout(key: &str) -> WorkbenchLayout {
    WorkbenchLayout {
        key: key.to_owned(),
        left_open: false,
        right_open: true,
        bottom_open: false,
        left_width: 300,
        right_width: 340,
        bottom_height: 220,
    }
}

#[test]
fn a_layout_survives_a_save_and_a_load() {
    let dir = temp_dir("round-trip");
    let path = Settings::path_in(&dir);
    let mut settings = Settings::default();
    settings.set_workbench_layout(layout("sample"));
    settings.save(&path).expect("saves");
    let loaded = Settings::load(&path);
    assert_eq!(loaded, settings);
    assert_eq!(loaded.workbench_layout("sample"), Some(layout("sample")));
    assert_eq!(loaded.workbench_layout("other"), None);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn seventeen_keys_keep_the_last_sixteen() {
    let mut settings = Settings::default();
    for n in 0..=MAX_REMEMBERED_VIEWS {
        settings.set_workbench_layout(layout(&format!("ws-{n}")));
    }
    assert_eq!(settings.workbench_layouts.len(), MAX_REMEMBERED_VIEWS);
    assert_eq!(settings.workbench_layout("ws-0"), None);
    for n in 1..=MAX_REMEMBERED_VIEWS {
        assert!(
            settings.workbench_layout(&format!("ws-{n}")).is_some(),
            "{n}"
        );
    }
    // Setting a known key again replaces it and makes it the newest.
    let mut changed = layout("ws-1");
    changed.left_width = 500;
    settings.set_workbench_layout(changed.clone());
    assert_eq!(settings.workbench_layouts.len(), MAX_REMEMBERED_VIEWS);
    assert_eq!(settings.workbench_layouts.last(), Some(&changed));
}

#[test]
fn sizes_out_of_range_come_back_clamped() {
    let mut settings = Settings::default();
    let mut small = layout("small");
    (small.left_width, small.right_width, small.bottom_height) = (10, 0, 5);
    let mut big = layout("big");
    (big.left_width, big.right_width, big.bottom_height) = (5000, 801, 601);
    settings.set_workbench_layout(small);
    settings.set_workbench_layout(big);
    let small = settings.workbench_layout("small").expect("stored");
    assert_eq!(
        (small.left_width, small.right_width, small.bottom_height),
        (160, 160, 80)
    );
    let big = settings.workbench_layout("big").expect("stored");
    assert_eq!(
        (big.left_width, big.right_width, big.bottom_height),
        (800, 800, 600)
    );
}

#[test]
fn a_key_over_128_bytes_gives_none() {
    let mut settings = Settings::default();
    let long = "k".repeat(129);
    settings.workbench_layouts.push(layout(&long));
    assert_eq!(settings.workbench_layout(&long), None);
    let edge = "k".repeat(128);
    settings.workbench_layouts.push(layout(&edge));
    assert!(settings.workbench_layout(&edge).is_some());
}

#[test]
fn an_old_file_without_layouts_loads_an_empty_list() {
    let dir = temp_dir("old-file");
    let path = Settings::path_in(&dir);
    std::fs::create_dir_all(&dir).expect("folder");
    std::fs::write(&path, r#"{"username": "ana", "appearance": "dark"}"#).expect("writes");
    let loaded = Settings::load(&path);
    assert_eq!(loaded.username, "ana");
    assert!(loaded.workbench_layouts.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}
