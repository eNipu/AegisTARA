//! The start screen view on this computer: the license check and the events
//! it sends to the window (job 208, slice 208d).
#![cfg(any(target_os = "macos", target_os = "windows"))]

use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use aegis_client::folder::WorkspaceFolder;
use aegis_client::i18n::{Catalog, Language};
use aegis_client::license_file::now_secs;
use aegis_client::settings::{Appearance, Settings};
use aegis_license::{Issuer, License, TrustedKeys};
use aegis_model::{Methodology, Project};
use aegis_workbench::app::{StartConfig, StartEvent, StartScreen};
use gpui::{Entity, TestAppContext, VisualTestContext};

const DAY: u64 = 86_400;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("aegis-start-view-{name}-{}", std::process::id()));
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

/// A start screen as the window root and the events it sent.
struct View<'a> {
    start: Entity<StartScreen>,
    cx: &'a mut VisualTestContext,
    events: Rc<RefCell<Vec<StartEvent>>>,
}

/// The settings file of the test `name`; its folder exists.
fn settings_path(name: &str) -> PathBuf {
    let dir = scratch(&format!("{name}-settings"));
    fs::create_dir_all(&dir).expect("dir");
    dir.join("settings.json")
}

fn open<'a>(cx: &'a mut TestAppContext, settings: PathBuf, keys: TrustedKeys) -> View<'a> {
    cx.update(gpui_component::init);
    let config = StartConfig {
        catalog: Rc::new(Catalog::load(Language::En)),
        appearance: Appearance::Light,
        settings_path: Some(settings),
        keys,
    };
    let (start, cx) = cx.add_window_view(|window, cx| StartScreen::new(&config, window, cx));
    let events: Rc<RefCell<Vec<StartEvent>>> = Rc::default();
    let sink = events.clone();
    cx.update(|_, cx| {
        cx.subscribe(&start, move |_, ev: &StartEvent, _| {
            sink.borrow_mut().push(ev.clone())
        })
        .detach()
    });
    cx.run_until_parked();
    View { start, cx, events }
}

impl View<'_> {
    fn fill(&mut self, folder: &Path, license: &Path) {
        self.start.update_in(self.cx, |s, window, cx| {
            s.set_folder(folder, window, cx);
            s.set_license_path(license, window, cx);
        });
    }

    fn open_local(&mut self) {
        self.start.update(self.cx, |s, cx| s.open_local(cx));
        self.cx.run_until_parked();
    }

    fn create_local(&mut self) {
        self.start.update(self.cx, |s, cx| s.create_local(cx));
        self.cx.run_until_parked();
    }

    fn error(&mut self) -> Option<String> {
        let start = self.start.clone();
        self.cx
            .update(|_, cx| start.read(cx).error().map(str::to_owned))
    }

    fn events(&self) -> Vec<StartEvent> {
        self.events.borrow().clone()
    }

    /// The folder of the one `Open` event sent.
    fn opened(&self) -> PathBuf {
        match self.events().as_slice() {
            [StartEvent::Open { folder, .. }] => folder.clone(),
            other => panic!("expected one Open, got {other:?}"),
        }
    }
}

#[gpui::test]
fn empty_fields_ask_for_a_folder_then_a_license(cx: &mut TestAppContext) {
    let (_, keys) = issuer();
    let mut v = open(cx, settings_path("empty"), keys);
    v.open_local();
    assert_eq!(v.error().as_deref(), Some("Choose a project folder."));
    v.fill(&scratch("empty-folder"), Path::new(""));
    v.open_local();
    assert_eq!(v.error().as_deref(), Some("Choose your license file."));
    assert!(v.events().is_empty());
}

#[gpui::test]
fn a_valid_license_asks_to_open_the_folder(cx: &mut TestAppContext) {
    let (issuer, keys) = issuer();
    let dir = workspace("valid");
    let lic = license_file(&issuer, &scratch("valid-lic"), now_secs() + 30 * DAY);
    let mut v = open(cx, settings_path("valid"), keys);
    v.fill(&dir, &lic);
    v.open_local();
    match v.events().as_slice() {
        [StartEvent::Open { folder, license }] => {
            assert_eq!(folder, &dir);
            assert_eq!(license.customer, "Start Test");
            assert!(license.editing);
        }
        other => panic!("expected one Open, got {other:?}"),
    }
    let start = v.start.clone();
    assert!(v.cx.update(|_, cx| start.read(cx).license().is_some()));
    assert_eq!(v.error(), None);
    let _ = fs::remove_dir_all(&dir);
}

#[gpui::test]
fn a_license_of_another_key_is_refused(cx: &mut TestAppContext) {
    let (issuer, _) = issuer();
    let (_, keys) = self::issuer();
    let dir = workspace("foreign");
    let lic = license_file(&issuer, &scratch("foreign-lic"), now_secs() + 30 * DAY);
    let mut v = open(cx, settings_path("foreign"), keys);
    v.fill(&dir, &lic);
    v.open_local();
    assert!(v.events().is_empty());
    let error = v.error().expect("error");
    assert!(
        error.starts_with("The license file cannot be used"),
        "{error}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[gpui::test]
fn new_workspace_needs_an_empty_or_new_folder(cx: &mut TestAppContext) {
    let (issuer, keys) = issuer();
    let lic = license_file(&issuer, &scratch("new-lic"), now_secs() + 30 * DAY);
    let full = scratch("new-full");
    fs::create_dir_all(&full).expect("dir");
    fs::write(full.join("notes.txt"), "kept").expect("write");
    let mut v = open(cx, settings_path("new"), keys);
    v.fill(&full, &lic);
    v.create_local();
    assert!(v.events().is_empty());
    let error = v.error().expect("error");
    assert!(error.contains("is not empty"), "{error}");

    let fresh = scratch("new-fresh");
    v.fill(&fresh, &lic);
    v.create_local();
    match v.events().as_slice() {
        [StartEvent::Create { folder, .. }] => assert_eq!(folder, &fresh),
        other => panic!("expected one Create, got {other:?}"),
    }
    let _ = fs::remove_dir_all(&full);
}

#[gpui::test]
fn the_sample_needs_no_license(cx: &mut TestAppContext) {
    let (_, keys) = issuer();
    let v = open(cx, settings_path("sample"), keys);
    v.start.update(v.cx, |s, cx| s.open_sample(cx));
    v.cx.run_until_parked();
    assert!(matches!(v.events().as_slice(), [StartEvent::Sample]));
}

#[gpui::test]
fn the_fields_start_with_the_remembered_paths(cx: &mut TestAppContext) {
    let (issuer, keys) = issuer();
    let dir = workspace("remembered");
    let lic = license_file(&issuer, &scratch("remembered-lic"), now_secs() + 30 * DAY);
    let settings = settings_path("remembered");
    Settings {
        standalone_folder: dir.display().to_string(),
        standalone_license: lic.display().to_string(),
        ..Settings::default()
    }
    .save(&settings)
    .expect("save");
    let mut v = open(cx, settings, keys);
    let start = v.start.clone();
    assert_eq!(v.cx.update(|_, cx| start.read(cx).license_file(cx)), lic);
    v.open_local();
    assert_eq!(v.opened(), dir);
    let _ = fs::remove_dir_all(&dir);
}

#[gpui::test]
fn enter_in_the_folder_field_opens_the_folder(cx: &mut TestAppContext) {
    let (issuer, keys) = issuer();
    let dir = workspace("enter");
    let lic = license_file(&issuer, &scratch("enter-lic"), now_secs() + 30 * DAY);
    let mut v = open(cx, settings_path("enter"), keys);
    v.fill(Path::new(""), &lic);
    v.start
        .update_in(v.cx, |s, window, cx| s.focus_folder(window, cx));
    v.cx.run_until_parked();
    v.cx.simulate_input(&dir.display().to_string());
    v.cx.simulate_keystrokes("enter");
    v.cx.run_until_parked();
    assert_eq!(v.opened(), dir);
    let _ = fs::remove_dir_all(&dir);
}
