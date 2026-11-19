//! The start screen on this computer: a license file and a project folder,
//! New workspace, Try the sample and Back to Start (job 208, slice 208d).
#![cfg(any(target_os = "macos", target_os = "windows"))]

use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use aegis_client::explorer::{ExplorerKey, Folder, Owner};
use aegis_client::folder::{WorkspaceFolder, is_workspace};
use aegis_client::i18n::{Catalog, Language};
use aegis_client::license_file::now_secs;
use aegis_client::settings::{Appearance, Settings};
use aegis_client::systems::system_rows;
use aegis_client::view::Selection;
use aegis_license::{Issuer, License, TrustedKeys};
use aegis_model::{Methodology, Project};
use aegis_workbench::app::{
    Shell, StartConfig, StartScreen, Workbench, apply_appearance, bind_all,
};
use aegis_workbench::sample::SAMPLE_NAME;
use gpui::{Entity, TestAppContext, VisualTestContext};

const DAY: u64 = 86_400;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("aegis-start-local-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    dir
}

/// A signing key and the trusted keys that hold only it.
fn issuer() -> (Issuer, TrustedKeys) {
    let (pkcs8, public) = Issuer::generate().expect("key");
    let issuer = Issuer::from_pkcs8(&pkcs8).expect("issuer");
    (issuer, TrustedKeys::single("test", public).expect("keys"))
}

/// A license for "Start Test" ending `expires_at`, signed by `issuer`,
/// written to `<dir>/license.json`.
fn license_file(issuer: &Issuer, dir: &Path, expires_at: u64) -> PathBuf {
    let now = now_secs();
    let record = License {
        version: 1,
        id: "start-test".into(),
        kid: "test".into(),
        customer: "Start Test".into(),
        seats: 1,
        issued_at: now - 100 * DAY,
        expires_at,
        grace_days: 0,
        features: vec!["core".into()],
    };
    fs::create_dir_all(dir).expect("dir");
    let path = dir.join("license.json");
    fs::write(&path, issuer.sign(&record).expect("signed")).expect("write");
    path
}

/// A new workspace folder, its lock released at once.
fn workspace(name: &str) -> PathBuf {
    let dir = scratch(name);
    let methodology = Arc::new(Methodology::iso21434_r155().expect("builtin"));
    drop(WorkspaceFolder::create(&dir, &Project::new(methodology)).expect("create"));
    dir
}

struct Window<'a> {
    shell: Entity<Shell>,
    cx: &'a mut VisualTestContext,
    settings: PathBuf,
}

fn open<'a>(cx: &'a mut TestAppContext, name: &str, keys: TrustedKeys) -> Window<'a> {
    cx.update(|cx| {
        gpui_component::init(cx);
        bind_all(cx);
    });
    let settings = scratch(&format!("{name}-settings")).join("settings.json");
    let config = StartConfig {
        catalog: Rc::new(Catalog::load(Language::En)),
        appearance: Appearance::Light,
        settings_path: Some(settings.clone()),
        keys,
        server: None,
    };
    let (shell, cx) = cx.add_window_view(|window, cx| {
        apply_appearance(Appearance::Light, window, cx);
        Shell::new(config, window, cx)
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    Window {
        shell,
        cx,
        settings,
    }
}

impl Window<'_> {
    fn start(&mut self) -> Entity<StartScreen> {
        let shell = self.shell.clone();
        self.cx
            .update(|_, cx| shell.read(cx).start_screen().cloned())
            .expect("start screen")
    }

    fn workbench(&mut self) -> Option<Entity<Workbench>> {
        let shell = self.shell.clone();
        self.cx.update(|_, cx| shell.read(cx).workbench().cloned())
    }

    fn fill(&mut self, folder: &Path, license: &Path) {
        let start = self.start();
        start.update_in(self.cx, |s, window, cx| {
            s.set_folder(folder, window, cx);
            s.set_license_path(license, window, cx);
        });
    }

    fn open_local(&mut self, folder: &Path, license: &Path) {
        self.fill(folder, license);
        let start = self.start();
        start.update(self.cx, |s, cx| s.open_local(cx));
        self.cx.run_until_parked();
    }

    fn error(&mut self) -> Option<String> {
        let start = self.start();
        self.cx
            .update(|_, cx| start.read(cx).error().map(str::to_owned))
    }

    fn folder(&mut self) -> Option<PathBuf> {
        let wb = self.workbench()?;
        self.cx
            .update(|_, cx| wb.read(cx).folder().map(Path::to_path_buf))
    }

    fn read_only(&mut self) -> Option<String> {
        let wb = self.workbench()?;
        self.cx
            .update(|_, cx| wb.read(cx).read_only().map(str::to_owned))
    }

    fn run(&mut self, id: &str) {
        let wb = self.workbench().expect("workbench");
        wb.update_in(self.cx, |w, window, cx| w.run(id, window, cx));
        self.cx.run_until_parked();
    }

    fn on_start(&mut self) -> bool {
        let shell = self.shell.clone();
        self.cx
            .update(|_, cx| shell.read(cx).start_screen().is_some())
    }
}

#[gpui::test]
fn a_valid_license_opens_the_folder_and_is_remembered(cx: &mut TestAppContext) {
    let (issuer, keys) = issuer();
    let dir = workspace("valid");
    let lic = license_file(&issuer, &scratch("valid-lic"), now_secs() + 30 * DAY);
    let mut w = open(cx, "valid", keys);
    w.open_local(&dir, &lic);
    assert!(w.workbench().is_some());
    assert_eq!(w.folder().as_deref(), Some(dir.as_path()));
    assert_eq!(w.read_only(), None);
    let settings = Settings::load(&w.settings);
    assert_eq!(settings.standalone_folder, dir.display().to_string());
    assert_eq!(settings.standalone_license, lic.display().to_string());
    // The next start screen reads the remembered paths.
    w.run("back-to-start");
    assert!(w.on_start());
    let start = w.start();
    start.update(w.cx, |s, cx| s.open_local(cx));
    w.cx.run_until_parked();
    assert_eq!(w.folder().as_deref(), Some(dir.as_path()));
    let _ = fs::remove_dir_all(&dir);
}

#[gpui::test]
fn an_expired_license_opens_read_only(cx: &mut TestAppContext) {
    let (issuer, keys) = issuer();
    let dir = workspace("expired");
    let lic = license_file(&issuer, &scratch("expired-lic"), now_secs() - 10 * DAY);
    let mut w = open(cx, "expired", keys);
    w.open_local(&dir, &lic);
    assert_eq!(w.folder().as_deref(), Some(dir.as_path()));
    assert!(w.read_only().expect("read-only").contains("Start Test"));
    let _ = fs::remove_dir_all(&dir);
}

#[gpui::test]
fn a_license_of_another_key_is_refused(cx: &mut TestAppContext) {
    let (issuer, _) = issuer();
    let (_, keys) = self::issuer();
    let dir = workspace("foreign");
    let lic = license_file(&issuer, &scratch("foreign-lic"), now_secs() + 30 * DAY);
    let mut w = open(cx, "foreign", keys);
    w.open_local(&dir, &lic);
    assert!(w.workbench().is_none());
    let error = w.error().expect("error");
    assert!(
        error.starts_with("The license file cannot be used"),
        "{error}"
    );
    assert!(
        !Settings::load(&w.settings)
            .standalone_folder
            .contains("foreign")
    );
    let _ = fs::remove_dir_all(&dir);
}

#[gpui::test]
fn a_folder_that_is_not_a_workspace_stays_on_start(cx: &mut TestAppContext) {
    let (issuer, keys) = issuer();
    let dir = scratch("not-workspace");
    fs::create_dir_all(&dir).expect("dir");
    fs::write(dir.join("notes.txt"), "unrelated").expect("write");
    let lic = license_file(
        &issuer,
        &scratch("not-workspace-lic"),
        now_secs() + 30 * DAY,
    );
    let mut w = open(cx, "not-workspace", keys);
    w.open_local(&dir, &lic);
    assert!(w.on_start());
    assert!(w.error().is_some());
    let _ = fs::remove_dir_all(&dir);
}

#[gpui::test]
fn empty_fields_ask_for_a_folder_and_a_license(cx: &mut TestAppContext) {
    let (_, keys) = issuer();
    let mut w = open(cx, "empty-fields", keys);
    w.open_local(Path::new(""), Path::new(""));
    assert_eq!(w.error().as_deref(), Some("Choose a project folder."));
    w.open_local(Path::new("/somewhere"), Path::new(""));
    assert_eq!(w.error().as_deref(), Some("Choose your license file."));
}

#[gpui::test]
fn new_workspace_creates_and_opens_an_empty_folder(cx: &mut TestAppContext) {
    let (issuer, keys) = issuer();
    let dir = scratch("create");
    let full = scratch("create-full");
    fs::create_dir_all(&full).expect("dir");
    fs::write(full.join("notes.txt"), "unrelated").expect("write");
    let lic = license_file(&issuer, &scratch("create-lic"), now_secs() + 30 * DAY);
    let mut w = open(cx, "create", keys);

    w.fill(&full, &lic);
    let start = w.start();
    start.update(w.cx, |s, cx| s.create_local(cx));
    w.cx.run_until_parked();
    assert!(w.on_start());
    let error = w.error().expect("error");
    assert!(error.contains("is not empty"), "{error}");
    assert!(!is_workspace(&full));

    w.fill(&dir, &lic);
    start.update(w.cx, |s, cx| s.create_local(cx));
    w.cx.run_until_parked();
    assert!(is_workspace(&dir));
    assert_eq!(w.folder().as_deref(), Some(dir.as_path()));
    for d in [&dir, &full] {
        let _ = fs::remove_dir_all(d);
    }
}

#[gpui::test]
fn the_sample_needs_no_license(cx: &mut TestAppContext) {
    let (_, keys) = issuer();
    let mut w = open(cx, "sample", keys);
    let start = w.start();
    start.update(w.cx, |s, cx| s.open_sample(cx));
    w.cx.run_until_parked();
    let wb = w.workbench().expect("workbench");
    let name = w.cx.update(|_, cx| wb.read(cx).project_name().to_owned());
    assert_eq!(name, SAMPLE_NAME);
    assert_eq!(w.folder(), None);
}

#[gpui::test]
fn back_to_start_asks_before_changes_of_the_sample_are_lost(cx: &mut TestAppContext) {
    let (_, keys) = issuer();
    let mut w = open(cx, "back", keys);
    let start = w.start();
    start.update(w.cx, |s, cx| s.open_sample(cx));
    w.cx.run_until_parked();
    let wb = w.workbench().expect("workbench");
    let (tcu, gateway) = w.cx.update(|_, cx| {
        let ws = wb.read(cx).workspace();
        let tcu = ws
            .project()
            .components()
            .find(|(_, _, c)| c.name() == "Telematics unit")
            .map(|(_, id, _)| id)
            .expect("component");
        let gw = system_rows(ws).into_iter().find(|r| r.name == "Gateway");
        (tcu, gw.expect("Gateway").id)
    });
    let dragged = ExplorerKey::Element(
        Owner::Project,
        Folder::Components,
        Selection::Component(tcu),
    );
    wb.update(w.cx, |w, cx| {
        w.drop_on(dragged, ExplorerKey::System(gateway), cx)
    });
    assert!(w.cx.update(|_, cx| wb.read(cx).has_unsaved_changes()));

    w.run("back-to-start");
    assert!(w.cx.has_pending_prompt());
    assert!(!w.on_start());
    w.cx.simulate_prompt_answer("Don't save");
    w.cx.run_until_parked();
    assert!(w.on_start());
}

#[gpui::test]
fn back_to_start_leaves_an_unchanged_folder_at_once(cx: &mut TestAppContext) {
    let (issuer, keys) = issuer();
    let dir = workspace("back-folder");
    let lic = license_file(&issuer, &scratch("back-folder-lic"), now_secs() + 30 * DAY);
    let mut w = open(cx, "back-folder", keys);
    w.open_local(&dir, &lic);
    assert!(w.workbench().is_some());
    w.run("back-to-start");
    assert!(!w.cx.has_pending_prompt());
    assert!(w.on_start());
    // The folder's lock is released: it opens again.
    let methodology = Arc::new(Methodology::iso21434_r155().expect("builtin"));
    assert!(WorkspaceFolder::open(&dir, methodology).is_ok());
    let _ = fs::remove_dir_all(&dir);
}

#[gpui::test]
fn enter_in_the_folder_field_opens_the_folder(cx: &mut TestAppContext) {
    let (issuer, keys) = issuer();
    let dir = workspace("enter");
    let lic = license_file(&issuer, &scratch("enter-lic"), now_secs() + 30 * DAY);
    let mut w = open(cx, "enter", keys);
    w.fill(Path::new(""), &lic);
    let start = w.start();
    start.update_in(w.cx, |s, window, cx| s.focus_folder(window, cx));
    w.cx.run_until_parked();
    w.cx.simulate_input(&dir.display().to_string());
    w.cx.simulate_keystrokes("enter");
    w.cx.run_until_parked();
    assert_eq!(w.folder().as_deref(), Some(dir.as_path()));
    let _ = fs::remove_dir_all(&dir);
}
