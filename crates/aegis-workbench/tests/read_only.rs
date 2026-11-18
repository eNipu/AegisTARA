//! The read-only workbench (job 208, slice 208c): an expired license or a
//! server project shows the project but changes nothing.
#![cfg(any(target_os = "macos", target_os = "windows"))]

use std::fs;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use aegis_client::explorer::{ExplorerKey, Folder, Owner};
use aegis_client::folder::WorkspaceFolder;
use aegis_client::i18n::{Catalog, Language};
use aegis_client::license_file::LocalLicense;
use aegis_client::settings::Appearance;
use aegis_client::systems::system_rows;
use aegis_client::view::Selection;
use aegis_model::{Methodology, Project};
use aegis_workbench::app::{Workbench, apply_appearance, bind_all};
use aegis_workbench::sample::workbench_sample;
use gpui::{Entity, TestAppContext, VisualTestContext};

const REASON: &str = "Read-only: test";

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("aegis-read-only-{name}-{}", std::process::id()));
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

fn project_bytes(view: &Entity<Workbench>, cx: &mut VisualTestContext) -> Vec<u8> {
    cx.update(|_, cx| saved(view.read(cx).workspace().project()))
}

fn note(view: &Entity<Workbench>, cx: &mut VisualTestContext) -> Option<String> {
    cx.update(|_, cx| view.read(cx).note().map(str::to_owned))
}

fn read_only(view: &Entity<Workbench>, cx: &mut VisualTestContext) -> Option<String> {
    cx.update(|_, cx| view.read(cx).read_only().map(str::to_owned))
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

/// "Telematics unit" and the explorer keys of the "Gateway" system and of
/// the system it is in now.
fn tcu(
    view: &Entity<Workbench>,
    cx: &mut VisualTestContext,
) -> (aegis_model::ComponentId, ExplorerKey, ExplorerKey) {
    cx.update(|_, cx| {
        let ws = view.read(cx).workspace();
        let p = ws.project();
        let (idx, id, _) = p
            .components()
            .find(|(_, _, c)| c.name() == "Telematics unit")
            .expect("component");
        let was = p
            .component(idx)
            .and_then(aegis_model::Component::system)
            .map_or(ExplorerKey::Unassigned, ExplorerKey::System);
        let gateway = system_rows(ws)
            .into_iter()
            .find(|r| r.name == "Gateway")
            .expect("Gateway")
            .id;
        (id, ExplorerKey::System(gateway), was)
    })
}

#[gpui::test]
fn read_only_refuses_every_change_and_every_save(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let (id, gateway, was) = tcu(&view, cx);
    let start = project_bytes(&view, cx);
    move_to(&view, cx, id, gateway);
    let before = project_bytes(&view, cx);
    assert_ne!(before, start, "the first move must change the project");

    view.update(cx, |w, cx| w.set_read_only(Some(REASON.into()), cx));
    view.update_in(cx, |w, window, cx| w.run("undo", window, cx));
    assert_eq!(
        project_bytes(&view, cx),
        before,
        "undo changed a read-only project"
    );
    view.update_in(cx, |w, window, cx| w.run("redo", window, cx));
    move_to(&view, cx, id, was);
    assert_eq!(
        project_bytes(&view, cx),
        before,
        "a move changed a read-only project"
    );
    assert_eq!(note(&view, cx).as_deref(), Some(REASON));

    let res = view.update(cx, |w, cx| w.save(cx));
    assert_eq!(res, Err(REASON.to_owned()));
    let dir = scratch("save-as");
    let res = view.update_in(cx, |w, window, cx| w.save_as(&dir, window, cx));
    assert_eq!(res, Err(REASON.to_owned()));
    assert!(!dir.exists(), "Save As wrote a read-only project");
    assert_eq!(project_bytes(&view, cx), before);
}

#[gpui::test]
fn show_project_shows_a_project_that_is_not_a_folder(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let (id, gateway, _) = tcu(&view, cx);
    move_to(&view, cx, id, gateway);
    // A project unlike the one shown, so a no-op `show_project` fails.
    let p = workbench_sample().project().clone();
    let bytes = saved(&p);
    assert_ne!(project_bytes(&view, cx), bytes);

    let reason = "Read-only: a server project".to_owned();
    let r = reason.clone();
    view.update_in(cx, |w, window, cx| {
        w.show_project(p, "Gateway", Some(r), window, cx);
    });
    cx.update(|_, cx| {
        let w = view.read(cx);
        assert_eq!(w.project_name(), "Gateway");
        assert!(w.folder().is_none());
    });
    assert_eq!(read_only(&view, cx), Some(reason));
    assert_eq!(project_bytes(&view, cx), bytes);
}

fn license(editing: bool) -> LocalLicense {
    LocalLicense {
        customer: "ACME".into(),
        editing,
        grace: false,
        days_left: editing.then_some(30),
    }
}

#[gpui::test]
fn an_expired_license_opens_a_folder_read_only(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    view.update(cx, |w, _| w.set_license(Some(license(false))));
    let dir = other_folder("expired");
    view.update_in(cx, |w, window, cx| w.open_folder(&dir, window, cx))
        .expect("opens");
    let reason = read_only(&view, cx).expect("read-only");
    assert!(reason.contains("ACME"), "{reason}");
    drop(view);
    let _ = fs::remove_dir_all(&dir);
}

#[gpui::test]
fn an_active_license_opens_a_folder_for_editing(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    view.update(cx, |w, cx| w.set_read_only(Some(REASON.into()), cx));
    view.update(cx, |w, _| w.set_license(Some(license(true))));
    let dir = other_folder("active");
    view.update_in(cx, |w, window, cx| w.open_folder(&dir, window, cx))
        .expect("opens");
    assert_eq!(read_only(&view, cx), None);
    drop(view);
    let _ = fs::remove_dir_all(&dir);
}

#[gpui::test]
fn open_folder_without_a_license_points_to_the_start_screen(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    view.update_in(cx, |w, window, cx| w.run("open-folder", window, cx));
    let expected = Catalog::load(Language::En).tr("workbench-needs-license");
    assert_eq!(note(&view, cx), Some(expected));
}
