//! Open Folder, Save, Save As and leaving with unsaved changes in the
//! workbench window (job 164, slice 164d; job 164e, slice 164eb).
#![cfg(any(target_os = "macos", target_os = "windows"))]

use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use aegis_client::commands::Edit;
use aegis_client::explorer::{ExplorerKey, Folder, Owner};
use aegis_client::folder::{WorkspaceFolder, is_workspace};
use aegis_client::i18n::{Catalog, Language};
use aegis_client::pages::{RiskRow, risk_rows};
use aegis_client::settings::Appearance;
use aegis_client::systems::system_rows;
use aegis_client::view::Selection;
use aegis_client::workspace::Workspace;
use aegis_model::{Methodology, OpCodec, Project};
use aegis_server::store::{ProjectMeta, StoredOp};
use aegis_server::{FolderStore, ProjectId, ProjectStore};
use aegis_workbench::app::{Leave, Workbench, apply_appearance, bind_all};
use aegis_workbench::sample::workbench_sample;
use gpui::{Entity, TestAppContext, VisualContext as _, VisualTestContext};

const OVERVIEW: ExplorerKey = ExplorerKey::Folder(Owner::Project, Folder::Overview);

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("aegis-folder-view-{name}-{}", std::process::id()));
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

/// A new empty workspace folder, its lock released at once.
fn other_folder(name: &str) -> PathBuf {
    let dir = scratch(name);
    drop(WorkspaceFolder::create(&dir, &Project::new(methodology())).expect("create"));
    dir
}

fn open(cx: &mut TestAppContext) -> (Entity<Workbench>, &mut VisualTestContext) {
    cx.update(|cx| {
        gpui_component::init(cx);
        bind_all(cx);
    });
    let catalog = Rc::new(Catalog::load(Language::En));
    let (view, cx) = cx.add_window_view(|window, cx| {
        apply_appearance(Appearance::Light, window, cx);
        Workbench::new(catalog, Appearance::Light, None, window, cx)
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    (view, cx)
}

fn save_as(view: &Entity<Workbench>, cx: &mut VisualTestContext, dir: &Path) {
    view.update_in(cx, |w, window, cx| w.save_as(dir, window, cx))
        .expect("save as");
}

fn open_folder(
    view: &Entity<Workbench>,
    cx: &mut VisualTestContext,
    dir: &Path,
) -> Result<(), String> {
    view.update_in(cx, |w, window, cx| w.open_folder(dir, window, cx))
}

fn unsaved(view: &Entity<Workbench>, cx: &mut VisualTestContext) -> bool {
    cx.update(|_, cx| view.read(cx).has_unsaved_changes())
}

fn root_label(view: &Entity<Workbench>, cx: &mut VisualTestContext) -> Option<String> {
    cx.update(|_, cx| view.read(cx).row(ExplorerKey::Root).map(|r| r.label))
}

fn project_bytes(view: &Entity<Workbench>, cx: &mut VisualTestContext) -> Vec<u8> {
    cx.update(|_, cx| saved(view.read(cx).workspace().project()))
}

/// Moves "Telematics unit" to the "Gateway" system; returns the component
/// and the explorer key of the system it was in.
fn move_tcu(
    view: &Entity<Workbench>,
    cx: &mut VisualTestContext,
) -> (aegis_model::ComponentId, ExplorerKey) {
    let (tcu, gateway) = cx.update(|_, cx| {
        let ws = view.read(cx).workspace();
        let tcu = ws
            .project()
            .components()
            .find(|(_, _, c)| c.name() == "Telematics unit")
            .map(|(_, id, _)| id)
            .expect("component");
        let gw = system_rows(ws).into_iter().find(|r| r.name == "Gateway");
        (tcu, gw.expect("Gateway").id)
    });
    let was = cx.update(|_, cx| {
        let p = view.read(cx).workspace().project();
        let system = p.component_idx(&tcu).and_then(|i| p.component(i))?.system();
        Some(system.map_or(ExplorerKey::Unassigned, ExplorerKey::System))
    });
    move_to(view, cx, tcu, ExplorerKey::System(gateway));
    (tcu, was.expect("component"))
}

fn move_to(
    view: &Entity<Workbench>,
    cx: &mut VisualTestContext,
    id: aegis_model::ComponentId,
    target: ExplorerKey,
) {
    let dragged =
        ExplorerKey::Element(Owner::Project, Folder::Components, Selection::Component(id));
    view.update(cx, |w, cx| w.drop_on(dragged, target, cx));
}

fn system_name_of(project: &Project, id: aegis_model::ComponentId) -> Option<String> {
    let ws = Workspace::new(project.clone(), 0);
    let system = project
        .component_idx(&id)
        .and_then(|i| project.component(i))
        .and_then(|c| c.system())?;
    system_rows(&ws)
        .into_iter()
        .find(|r| r.id == system)
        .map(|r| r.name)
}

#[gpui::test]
fn save_as_writes_the_folder_and_names_the_project(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let dir = scratch("save-as");
    save_as(&view, cx, &dir);
    assert!(dir.join("aegis.json").is_file());
    let name = dir
        .file_name()
        .expect("name")
        .to_string_lossy()
        .into_owned();
    assert_eq!(
        cx.update(|_, cx| view.read(cx).project_name().to_owned()),
        name
    );
    assert_eq!(root_label(&view, cx), Some(name));
    assert_eq!(
        cx.update(|_, cx| view.read(cx).folder().map(Path::to_path_buf)),
        Some(dir.clone())
    );
    assert!(!unsaved(&view, cx));
    let release = other_folder("save-as-release");
    open_folder(&view, cx, &release).expect("open other");
    let _ = fs::remove_dir_all(&dir);
    let _ = fs::remove_dir_all(&release);
}

#[gpui::test]
fn an_edit_is_unsaved_until_save_and_reopens_equal(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let dir = scratch("edit");
    save_as(&view, cx, &dir);
    move_tcu(&view, cx);
    assert!(unsaved(&view, cx));
    view.update_in(cx, |w, window, cx| w.run("save", window, cx));
    assert!(!unsaved(&view, cx));
    let bytes = project_bytes(&view, cx);
    let rows: Vec<RiskRow> = cx.update(|_, cx| risk_rows(view.read(cx).workspace()));

    let release = other_folder("edit-release");
    open_folder(&view, cx, &release).expect("open other");
    let (folder, project) = WorkspaceFolder::open(&dir, methodology()).expect("reopen");
    drop(folder);
    assert_eq!(saved(&project), bytes);
    assert_eq!(risk_rows(&Workspace::new(project, 0)), rows);
    let _ = fs::remove_dir_all(&dir);
    let _ = fs::remove_dir_all(&release);
}

#[gpui::test]
fn opening_the_open_folder_does_nothing(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let dir = scratch("open-same");
    save_as(&view, cx, &dir);
    move_tcu(&view, cx);
    let bytes = project_bytes(&view, cx);
    let item = dir.join("systems/main/item.json");
    let on_disk = fs::read(&item).expect("item");
    for again in [dir.clone(), dir.join("shared").join("..")] {
        open_folder(&view, cx, &again).expect("open again");
        assert!(unsaved(&view, cx));
        assert_eq!(project_bytes(&view, cx), bytes);
        assert_eq!(fs::read(&item).expect("item"), on_disk);
    }
    let _ = fs::remove_dir_all(&dir);
}

#[gpui::test]
fn save_as_into_the_open_folder_saves(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let dir = scratch("save-as-same");
    save_as(&view, cx, &dir);
    move_tcu(&view, cx);
    let edited = project_bytes(&view, cx);
    save_as(&view, cx, &dir);
    assert!(!unsaved(&view, cx));
    let release = other_folder("save-as-same-release");
    open_folder(&view, cx, &release).expect("open other");
    let (folder, project) = WorkspaceFolder::open(&dir, methodology()).expect("reopen");
    drop(folder);
    assert_eq!(saved(&project), edited);
    let _ = fs::remove_dir_all(&dir);
    let _ = fs::remove_dir_all(&release);
}

#[gpui::test]
fn opening_a_folder_closes_element_tabs(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let tcu = cx.update(|_, cx| {
        view.read(cx)
            .workspace()
            .project()
            .components()
            .find(|(_, _, c)| c.name() == "Telematics unit")
            .map(|(_, id, _)| id)
            .expect("component")
    });
    view.update(cx, |w, cx| w.reveal(Selection::Component(tcu), cx));
    let key = cx
        .update(|_, cx| view.read(cx).state().selected)
        .expect("selected");
    view.update_in(cx, |w, window, cx| w.open(key, window, cx));
    assert!(cx.update(|_, cx| view.read(cx).has_tab(key, cx)));

    let other = other_folder("tabs");
    open_folder(&view, cx, &other).expect("open");
    assert!(!cx.update(|_, cx| view.read(cx).has_tab(key, cx)));
    assert!(cx.update(|_, cx| view.read(cx).has_tab(OVERVIEW, cx)));
    let name = other
        .file_name()
        .expect("name")
        .to_string_lossy()
        .into_owned();
    assert_eq!(root_label(&view, cx), Some(name));
    let _ = fs::remove_dir_all(&other);
}

#[gpui::test]
fn a_refused_folder_keeps_the_project(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let dir = scratch("refused-kept");
    save_as(&view, cx, &dir);
    let before = project_bytes(&view, cx);
    let bad = scratch("refused-bad");
    fs::create_dir_all(&bad).expect("dir");
    fs::write(bad.join("aegis.json"), "{").expect("write");

    assert!(open_folder(&view, cx, &bad).is_err());
    assert_eq!(
        cx.update(|_, cx| view.read(cx).folder().map(Path::to_path_buf)),
        Some(dir.clone())
    );
    assert_eq!(project_bytes(&view, cx), before);
    let release = other_folder("refused-release");
    open_folder(&view, cx, &release).expect("open other");
    for d in [&dir, &bad, &release] {
        let _ = fs::remove_dir_all(d);
    }
}

fn request_close(view: &Entity<Workbench>, cx: &mut VisualTestContext) -> bool {
    let closed = view.update_in(cx, |w, window, cx| w.request_close(window, cx));
    cx.run_until_parked();
    closed
}

fn leave(view: &Entity<Workbench>, cx: &mut VisualTestContext, then: Leave) {
    view.update_in(cx, |w, window, cx| w.leave(then, window, cx));
    cx.run_until_parked();
}

fn answer(cx: &mut VisualTestContext, button: &str) {
    cx.simulate_prompt_answer(button);
    cx.run_until_parked();
}

fn window_open(cx: &mut VisualTestContext) -> bool {
    let window = cx.window_handle();
    cx.windows().contains(&window)
}

/// The edit state, read without the window (it may be closed).
fn unsaved_after(view: &Entity<Workbench>, cx: &mut VisualTestContext) -> bool {
    cx.cx.update(|cx| view.read(cx).has_unsaved_changes())
}

#[gpui::test]
fn close_saves_a_folder_without_asking(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let dir = scratch("close");
    save_as(&view, cx, &dir);
    let (tcu, _) = move_tcu(&view, cx);
    assert!(unsaved(&view, cx));
    let closed = request_close(&view, cx);
    assert!(!cx.has_pending_prompt());
    assert!(closed || !window_open(cx));
    assert!(!unsaved_after(&view, cx));

    // The closed window's workbench still holds the lock: read a copy.
    let copy = scratch("close-copy");
    copy_folder(&dir, &copy);
    let (folder, project) = WorkspaceFolder::open(&copy, methodology()).expect("reopen");
    drop(folder);
    assert_eq!(system_name_of(&project, tcu).as_deref(), Some("Gateway"));
    let _ = fs::remove_dir_all(&dir);
    let _ = fs::remove_dir_all(&copy);
}

/// Copies the folder `from` into `to`, without its lock file.
fn copy_folder(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("dir");
    for entry in fs::read_dir(from).expect("read") {
        let entry = entry.expect("entry");
        let (path, name) = (entry.path(), entry.file_name());
        if path.is_dir() {
            copy_folder(&path, &to.join(&name));
        } else if name != ".aegis.lock" {
            fs::copy(&path, to.join(&name)).expect("copy");
        }
    }
}

#[cfg(unix)]
#[gpui::test]
fn a_failing_save_asks_and_dont_save_closes(cx: &mut TestAppContext) {
    use std::os::unix::fs::PermissionsExt as _;
    let (view, cx) = open(cx);
    let dir = scratch("close-fails");
    save_as(&view, cx, &dir);
    let (tcu, was) = move_tcu(&view, cx);
    view.update(cx, |w, cx| w.save(cx)).expect("save");
    let held = dir.join("systems").join("main");
    assert!(held.join("item.json").is_file());
    let mode = fs::metadata(&held).expect("meta").permissions();
    fs::set_permissions(&held, fs::Permissions::from_mode(0o555)).expect("read-only");
    fs::set_permissions(held.join("item.json"), fs::Permissions::from_mode(0o444))
        .expect("read-only");
    move_to(&view, cx, tcu, was);
    assert!(unsaved(&view, cx));

    let closed = request_close(&view, cx);
    let asked = cx.has_pending_prompt();
    let note = cx.update(|_, cx| view.read(cx).note().is_some());
    answer(cx, "Cancel");
    let kept = window_open(cx) && unsaved(&view, cx);
    request_close(&view, cx);
    answer(cx, "Try again");
    let asked_again = cx.has_pending_prompt();
    answer(cx, "Don't save");
    let gone = !window_open(cx);
    fs::set_permissions(held.join("item.json"), fs::Permissions::from_mode(0o644))
        .expect("restore");
    fs::set_permissions(&held, mode).expect("restore");

    assert!(!closed);
    assert!(asked);
    assert!(note);
    assert!(kept);
    assert!(asked_again);
    assert!(gone);
    let _ = fs::remove_dir_all(&dir);
}

#[gpui::test]
fn sample_edits_ask_before_close(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    move_tcu(&view, cx);
    assert!(!request_close(&view, cx));
    assert!(cx.has_pending_prompt());
    answer(cx, "Cancel");
    assert!(window_open(cx));
    assert!(unsaved(&view, cx));

    request_close(&view, cx);
    answer(cx, "Save As…");
    assert!(cx.did_prompt_for_new_path());
    let dir = scratch("close-sample");
    cx.simulate_new_path_selection(|_| Some(dir.clone()));
    cx.run_until_parked();
    assert!(is_workspace(&dir));
    assert!(!window_open(cx));
    assert!(!unsaved_after(&view, cx));
    let _ = fs::remove_dir_all(&dir);
}

#[gpui::test]
fn sample_without_edits_closes_without_asking(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let closed = request_close(&view, cx);
    assert!(!cx.has_pending_prompt());
    assert!(closed || !window_open(cx));
}

#[gpui::test]
fn opening_a_folder_over_sample_edits_asks(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    move_tcu(&view, cx);
    let before = project_bytes(&view, cx);
    let other = other_folder("over-sample");
    leave(&view, cx, Leave::Open(other.clone()));
    assert!(cx.has_pending_prompt());
    answer(cx, "Cancel");
    assert_eq!(project_bytes(&view, cx), before);
    assert!(cx.update(|_, cx| view.read(cx).folder().is_none()));

    leave(&view, cx, Leave::Open(other.clone()));
    answer(cx, "Don't save");
    let name = other
        .file_name()
        .expect("name")
        .to_string_lossy()
        .into_owned();
    assert_eq!(root_label(&view, cx), Some(name));
    let release = other_folder("over-sample-release");
    open_folder(&view, cx, &release).expect("open other");
    let _ = fs::remove_dir_all(&other);
    let _ = fs::remove_dir_all(&release);
}

#[gpui::test]
fn a_refused_folder_after_dont_save_keeps_the_edits(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    move_tcu(&view, cx);
    let before = project_bytes(&view, cx);
    let bad = scratch("over-sample-bad");
    fs::create_dir_all(&bad).expect("dir");
    fs::write(bad.join("aegis.json"), "{").expect("write");
    leave(&view, cx, Leave::Open(bad.clone()));
    answer(cx, "Don't save");
    assert!(unsaved(&view, cx));
    assert_eq!(project_bytes(&view, cx), before);
    assert!(cx.update(|_, cx| view.read(cx).note().is_some()));
    let _ = fs::remove_dir_all(&bad);
}

#[gpui::test]
fn quit_with_sample_edits_asks(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    move_tcu(&view, cx);
    view.update_in(cx, |w, window, cx| w.run("quit", window, cx));
    cx.run_until_parked();
    assert!(cx.has_pending_prompt());
    answer(cx, "Cancel");
    assert!(window_open(cx));
    assert!(unsaved(&view, cx));
}

/// Writes an old folder in `dir` holding the workbench sample plus one
/// rename after the snapshot, and returns the project it should hold
/// (copied from `tests/convert.rs`).
fn old_folder(dir: &Path, m: &Arc<Methodology>) -> Project {
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
            methodology_version: m.version().into(),
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

#[gpui::test]
fn an_old_folder_converts_when_opened(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let dir = scratch("old");
    let m = methodology();
    old_folder(&dir, &m);
    open_folder(&view, cx, &dir).expect("converts");
    let text = Catalog::load(Language::En).tr("workbench-converted");
    assert_eq!(
        cx.update(|_, cx| view.read(cx).note().map(str::to_owned)),
        Some(text)
    );
    assert!(is_workspace(&dir));
    let release = other_folder("old-release");
    open_folder(&view, cx, &release).expect("open other");
    let _ = fs::remove_dir_all(&dir);
    let _ = fs::remove_dir_all(&release);
}

#[gpui::test]
fn typing_in_cell_without_enter_marks_dirty_and_commits_on_save(cx: &mut TestAppContext) {
    use aegis_client::explorer::{ExplorerKey, Folder, Owner};
    use aegis_client::grid::GridSelection;
    use gpui::AppContext as _;

    let (view, cx) = open(cx);
    let dir = other_folder("cell-draft-save");
    open_folder(&view, cx, &dir).expect("open");
    view.update(cx, |w, cx| {
        assert!(w.apply_edit(
            &Edit::AddAsset {
                id: aegis_model::AssetId::new_v7(),
                name: "Initial ECU".into(),
                properties: vec![0],
            },
            cx,
        ));
        w.save(cx).expect("initial save");
    });
    assert!(!unsaved(&view, cx));

    let key = ExplorerKey::Folder(Owner::Project, Folder::Assets);
    view.update_in(cx, |w, window, cx| w.open(key, window, cx));
    cx.run_until_parked();
    let tab = cx
        .read_entity(&view, |w, _| w.table_tab(key))
        .expect("assets tab");
    let name_col = cx.read_entity(&tab, |t, _| {
        t.visible_columns()
            .iter()
            .position(|c| c.key == "name")
            .expect("name column")
    });
    tab.update_in(cx, |t, window, cx| {
        t.set_selection(Some(GridSelection::single(0, name_col)), cx);
        assert!(t.start_edit(window, cx));
        assert!(t.set_editor_text("Typed Without Enter", window, cx));
    });
    cx.run_until_parked();

    assert!(
        unsaved(&view, cx),
        "uncommitted cell draft must mark workbench dirty"
    );
    view.update(cx, |w, cx| w.save(cx))
        .expect("save commits active cell draft");
    assert!(!unsaved(&view, cx));
    assert!(!cx.read_entity(&tab, |t, _| t.is_editing()));

    // Reopen from disk in another folder cycle to verify persistence.
    let release = other_folder("cell-draft-release");
    open_folder(&view, cx, &release).expect("release");
    let (_, disk) = WorkspaceFolder::open(&dir, methodology()).expect("reopen");
    assert!(
        disk.assets()
            .any(|(_, _, a)| a.name() == "Typed Without Enter"),
        "typed cell draft must be saved to disk"
    );
    let _ = fs::remove_dir_all(&dir);
    let _ = fs::remove_dir_all(&release);
}

#[gpui::test]
fn invalid_cell_draft_blocks_save_and_cancel_keeps_editor_open(cx: &mut TestAppContext) {
    use aegis_client::explorer::{ExplorerKey, Folder, Owner};
    use aegis_client::grid::GridSelection;
    use gpui::AppContext as _;

    let (view, cx) = open(cx);
    let dir = other_folder("cell-draft-invalid");
    open_folder(&view, cx, &dir).expect("open");
    view.update(cx, |w, cx| {
        assert!(w.apply_edit(
            &Edit::AddAsset {
                id: aegis_model::AssetId::new_v7(),
                name: "Initial ECU".into(),
                properties: vec![0],
            },
            cx,
        ));
        w.save(cx).expect("initial save");
    });

    let key = ExplorerKey::Folder(Owner::Project, Folder::Assets);
    view.update_in(cx, |w, window, cx| w.open(key, window, cx));
    cx.run_until_parked();
    let tab = cx
        .read_entity(&view, |w, _| w.table_tab(key))
        .expect("assets tab");
    let name_col = cx.read_entity(&tab, |t, _| {
        t.visible_columns()
            .iter()
            .position(|c| c.key == "name")
            .expect("name column")
    });
    tab.update_in(cx, |t, window, cx| {
        t.set_selection(Some(GridSelection::single(0, name_col)), cx);
        assert!(t.start_edit(window, cx));
        assert!(t.set_editor_text("   ", window, cx));
    });
    cx.run_until_parked();

    assert!(unsaved(&view, cx));
    assert!(
        view.update(cx, |w, cx| w.save(cx)).is_err(),
        "blank asset name must fail validation on save"
    );
    assert!(
        cx.read_entity(&tab, |t, _| t.is_editing()),
        "editor must stay open after validation failure"
    );
    assert!(unsaved(&view, cx));

    // Closing prompts (Try again / Don't save / Cancel); Cancel keeps window and draft.
    assert!(!request_close(&view, cx));
    let cancel = Catalog::load(Language::En).tr("workbench-leave-cancel");
    answer(cx, &cancel);
    assert!(window_open(cx));
    assert!(cx.read_entity(&tab, |t, _| t.is_editing()));

    let release = other_folder("cell-draft-invalid-release");
    leave(&view, cx, Leave::Open(release.clone()));
    let discard = Catalog::load(Language::En).tr("workbench-leave-discard");
    answer(cx, &discard);
    let _ = fs::remove_dir_all(&dir);
    let _ = fs::remove_dir_all(&release);
}
