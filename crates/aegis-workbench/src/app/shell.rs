//! The window's root: the start screen until a project is opened, then the
//! workbench, and the start screen again after Back to Start.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use aegis_client::folder::WorkspaceFolder;
use aegis_client::license_file::LocalLicense;
use aegis_client::settings::Settings;
use aegis_model::{Methodology, Project};
use gpui::{
    AppContext as _, Context, Entity, IntoElement, ParentElement as _, Render, Styled as _,
    Subscription, Task, Window, div,
};

use super::server::{ServerEvent, ServerPane};
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
    server: Option<Entity<ServerPane>>,
    server_project: bool,
    _subscription: Subscription,
    _server_subscription: Option<Subscription>,
    _poll: Option<Task<()>>,
}

impl Shell {
    /// The shell on the start screen.
    pub fn new(config: StartConfig, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (server, server_subscription) = match &config.server {
            Some(setup) => {
                let pane = cx.new(|cx| ServerPane::new(&config, setup, window, cx));
                let sub =
                    cx.subscribe_in(&pane, window, |this, _, ev: &ServerEvent, window, cx| {
                        this.on_server(ev, window, cx);
                    });
                (Some(pane), Some(sub))
            }
            None => (None, None),
        };
        let (screen, subscription) = start(&config, server.clone(), window, cx);
        Self {
            config,
            screen,
            server,
            server_project: false,
            _subscription: subscription,
            _server_subscription: server_subscription,
            _poll: None,
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

    /// The company-server pane, when configured.
    pub fn server_pane(&self) -> Option<&Entity<ServerPane>> {
        self.server.as_ref()
    }

    /// Drains pending updates on the server pane.
    pub fn pump(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(pane) = &self.server {
            pane.update(cx, |p, cx| {
                p.pump(cx);
            });
        }
    }

    /// Starts a task that pumps the server pane every 50 ms for the life of
    /// the shell.
    pub fn start_polling(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.server.is_none() {
            return;
        }
        self._poll = Some(cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(50))
                    .await;
                if this
                    .update_in(cx, |shell, window, cx| shell.pump(window, cx))
                    .is_err()
                {
                    break;
                }
            }
        }));
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

    fn on_server(&mut self, ev: &ServerEvent, window: &mut Window, cx: &mut Context<Self>) {
        let ServerEvent::ShowProject {
            project,
            name,
            reason,
        } = ev;
        let (wb, reuse) = match &self.screen {
            Screen::Workbench(w) if self.server_project => (w.clone(), true),
            _ => (self.workbench_view(window, cx), false),
        };
        wb.update(cx, |w, cx| {
            w.show_project((**project).clone(), name, Some(reason.clone()), window, cx);
        });
        if !reuse {
            self.show_workbench(wb, true, window, cx);
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
                self.show_workbench(wb, false, window, cx);
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
        self.show_workbench(wb, false, window, cx);
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
        server_project: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.server_project = server_project;
        self._subscription = cx.subscribe_in(&wb, window, |this, _, ev, window, cx| match ev {
            WorkbenchEvent::BackToStart => {
                if this.server_project
                    && let Some(pane) = &this.server
                {
                    pane.update(cx, |p, cx| p.close_project(cx));
                }
                this.server_project = false;
                let (screen, subscription) = start(&this.config, this.server.clone(), window, cx);
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
    server: Option<Entity<ServerPane>>,
    window: &mut Window,
    cx: &mut Context<Shell>,
) -> (Screen, Subscription) {
    let s = cx.new(|cx| StartScreen::new(config, server, window, cx));
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
