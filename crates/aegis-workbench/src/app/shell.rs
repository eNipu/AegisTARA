//! The window's root: the start screen until a project is opened, then the
//! workbench, and the start screen again after Back to Start.

use std::path::Path;
use std::sync::Arc;

use aegis_client::folder::WorkspaceFolder;
use aegis_client::license_file::LocalLicense;
use aegis_client::settings::Settings;
use aegis_model::{Methodology, Project};
use gpui::{
    AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render, Styled as _,
    Subscription, Window, div,
};

use super::start::{StartConfig, StartEvent, StartScreen};
use super::workbench::{Workbench, WorkbenchEvent};

/// What the window shows.
pub enum Screen {
    /// The start screen.
    Start(Entity<StartScreen>),
    /// The workbench over a project.
    Workbench(Entity<Workbench>),
}

/// The window's root view.
pub struct Shell {
    config: StartConfig,
    screen: Screen,
    _subscription: Subscription,
}

impl Shell {
    /// The shell on the start screen.
    pub fn new(config: StartConfig, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (screen, subscription) = start(&config, window, cx);
        Self {
            config,
            screen,
            _subscription: subscription,
        }
    }

    /// What the window shows.
    pub fn screen(&self) -> &Screen {
        &self.screen
    }

    /// The start screen, when shown.
    pub fn start_screen(&self) -> Option<&Entity<StartScreen>> {
        match &self.screen {
            Screen::Start(s) => Some(s),
            Screen::Workbench(_) => None,
        }
    }

    /// The workbench, when shown.
    pub fn workbench(&self) -> Option<&Entity<Workbench>> {
        match &self.screen {
            Screen::Workbench(w) => Some(w),
            Screen::Start(_) => None,
        }
    }

    /// Puts `folder` in the start screen's folder field and opens it under
    /// the license in the license field.
    pub fn open_path(&mut self, folder: &Path, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s) = self.start_screen().cloned() {
            s.update(cx, |s, cx| {
                s.set_folder(folder, window, cx);
                s.open_local(cx);
            });
        }
    }

    fn on_start(&mut self, ev: &StartEvent, window: &mut Window, cx: &mut Context<Self>) {
        let res = match ev {
            StartEvent::Open { folder, license } => self.open(folder, license, window, cx),
            StartEvent::Create { folder, license } => {
                create(folder).and_then(|()| self.open(folder, license, window, cx))
            }
            StartEvent::Sample => {
                let wb = self.workbench_view(window, cx);
                self.show_workbench(wb, window, cx);
                Ok(())
            }
        };
        if let Err(e) = res
            && let Some(s) = self.start_screen()
        {
            s.update(cx, |s, cx| s.set_error(Some(e), cx));
        }
    }

    /// Opens `folder` in a new workbench under `license` and remembers
    /// both paths; the start screen stays on an error.
    fn open(
        &mut self,
        folder: &Path,
        license: &LocalLicense,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let wb = self.workbench_view(window, cx);
        wb.update(cx, |w, cx| {
            w.set_license(Some(license.clone()));
            w.open_folder(folder, window, cx)
        })?;
        if let (Some(path), Some(s)) = (&self.config.settings_path, self.start_screen()) {
            // Reload first: other code may have written other fields.
            let mut settings = Settings::load(path);
            settings.standalone_folder = folder.display().to_string();
            settings.standalone_license = s.read(cx).license_file(cx).display().to_string();
            if let Err(err) = settings.save(path) {
                eprintln!("could not save the settings: {err}");
            }
        }
        self.show_workbench(wb, window, cx);
        Ok(())
    }

    fn workbench_view(&self, window: &mut Window, cx: &mut Context<Self>) -> Entity<Workbench> {
        let c = &self.config;
        let (catalog, appearance, path) =
            (c.catalog.clone(), c.appearance, c.settings_path.clone());
        cx.new(|cx| Workbench::new(catalog, appearance, path, window, cx))
    }

    fn show_workbench(
        &mut self,
        wb: Entity<Workbench>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self._subscription = cx.subscribe_in(&wb, window, |this, _, ev, window, cx| match ev {
            WorkbenchEvent::BackToStart => {
                let (screen, subscription) = start(&this.config, window, cx);
                this.screen = screen;
                this._subscription = subscription;
                window.set_window_title("AegisTARA");
                cx.notify();
            }
        });
        self.screen = Screen::Workbench(wb);
        cx.notify();
    }
}

/// A new start screen and the shell's subscription to it.
fn start(
    config: &StartConfig,
    window: &mut Window,
    cx: &mut Context<Shell>,
) -> (Screen, Subscription) {
    let s = cx.new(|cx| StartScreen::new(config, window, cx));
    let sub = cx.subscribe_in(&s, window, |this, _, ev, window, cx| {
        this.on_start(ev, window, cx);
    });
    (Screen::Start(s), sub)
}

/// Creates an empty ISO 21434 workspace in `folder` and releases its lock.
fn create(folder: &Path) -> Result<(), String> {
    let methodology = Methodology::iso21434_r155().map_err(|e| e.to_string())?;
    let project = Project::new(Arc::new(methodology));
    drop(WorkspaceFolder::create(folder, &project).map_err(|e| e.to_string())?);
    Ok(())
}

impl Render for Shell {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let body = div().size_full();
        match &self.screen {
            Screen::Start(s) => body.child(s.clone()),
            Screen::Workbench(w) => body.child(w.clone()),
        }
    }
}
