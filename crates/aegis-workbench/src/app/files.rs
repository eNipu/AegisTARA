//! Open Folder, Save and Save As: the workbench over a project folder on
//! disk. Reading and writing go through `WorkspaceFolder` and `convert`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use aegis_client::explorer::ExplorerState;
use aegis_client::folder::WorkspaceFolder;
use aegis_client::license_file::LocalLicense;
use aegis_client::settings::default_export_dir;
use aegis_client::workspace::Workspace;
use aegis_model::Project;
use gpui::{Context, PathPromptOptions, PromptButton, PromptLevel, SharedString, Window};

use super::workbench::{Body, OVERVIEW, Workbench, WorkbenchEvent, overview_lines, remove};
use crate::convert::{convert_legacy, is_legacy};

/// What happens once the project may be left.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Leave {
    /// Close the window.
    Close,
    /// Quit the app.
    Quit,
    /// Open the project folder at the path.
    Open(PathBuf),
    /// Go back to the start screen.
    Start,
}

impl Workbench {
    /// Makes the project unchangeable for `reason`, shown in the status
    /// bar, or changeable again with `None`.
    pub fn set_read_only(&mut self, reason: Option<String>, cx: &mut Context<Self>) {
        self.read_only = reason;
        cx.notify();
    }

    /// Why the project cannot be changed, if it cannot.
    pub fn read_only(&self) -> Option<&str> {
        self.read_only.as_deref()
    }

    /// The license checked on the start screen; Open Folder needs one.
    pub fn set_license(&mut self, license: Option<LocalLicense>) {
        self.license = license;
    }

    /// The license checked on the start screen, if any.
    pub fn license(&self) -> Option<&LocalLicense> {
        self.license.as_ref()
    }

    /// When read-only, puts the reason in the status bar and returns `true`.
    pub(super) fn refuse_read_only(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(reason) = self.read_only.clone() else {
            return false;
        };
        self.note = Some(reason);
        cx.notify();
        true
    }

    /// Shows a project that is not a folder (a server project's downloaded
    /// version).
    pub fn show_project(
        &mut self,
        project: Project,
        name: &str,
        read_only: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.replace(project, name.to_owned(), None, window, cx);
        self.read_only = read_only;
        cx.notify();
    }

    /// The open project folder; `None` for the in-memory sample.
    pub fn folder(&self) -> Option<&Path> {
        self.folder.as_ref().map(WorkspaceFolder::root)
    }

    /// The project name: the folder's name, or the sample's.
    pub fn project_name(&self) -> &str {
        &self.name
    }

    /// The message shown in the status bar instead of the status, if any.
    pub fn note(&self) -> Option<&str> {
        self.note.as_deref()
    }

    /// Whether the project changed since it was opened or saved. Undoing
    /// back to the saved state still counts; the next save then writes
    /// nothing.
    pub fn has_unsaved_changes(&self) -> bool {
        self.workspace.confirmed_seq() != self.saved_seq
    }

    /// Opens the project folder `path`, converting an old project folder
    /// first. Unsaved changes of the open folder are saved before. Opening
    /// the folder that is already open does nothing.
    ///
    /// # Errors
    ///
    /// The text of the failed save, or why `path` cannot be opened; the
    /// project shown stays as it was.
    pub fn open_folder(
        &mut self,
        path: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.open_inner(path, true, window, cx)
    }

    /// `open_folder`, saving the unsaved changes of the open folder first
    /// only when `save_first`.
    fn open_inner(
        &mut self,
        path: &Path,
        save_first: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if self.folder.as_ref().is_some_and(|f| f.holds(path)) {
            return Ok(());
        }
        if save_first && self.folder.is_some() && self.has_unsaved_changes() {
            self.save(cx)?;
        }
        let converted = is_legacy(path);
        let methodology = Arc::clone(&self.methodology);
        let (folder, project) = if converted {
            convert_legacy(path, methodology)?
        } else {
            WorkspaceFolder::open(path, methodology).map_err(|e| e.to_string())?
        };
        let name = folder.name();
        self.replace(project, name, Some(folder), window, cx);
        self.read_only = self.license.as_ref().filter(|l| !l.editing).map(|l| {
            self.catalog.tr_with(
                "workbench-read-only-expired",
                &[("customer", l.customer.as_str())],
            )
        });
        if converted {
            self.note = Some(self.catalog.tr("workbench-converted"));
        }
        Ok(())
    }

    /// Shows `project` named `name` from `folder` (`None`: not a folder),
    /// with no undo history, no open tabs but the overview.
    fn replace(
        &mut self,
        project: Project,
        name: String,
        folder: Option<WorkspaceFolder>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.methodology = Arc::clone(project.methodology());
        self.names = project.methodology().element_names().clone();
        self.workspace = Workspace::new(project, 0);
        self.saved_seq = 0;
        self.name = name;
        self.folder = folder;
        self.picked = None;
        self.state = ExplorerState {
            filter: std::mem::take(&mut self.state.filter),
            ..ExplorerState::default()
        };
        for (key, tab) in std::mem::take(&mut self.tabs) {
            match &tab.body {
                Body::Overview(_) if key == OVERVIEW => {
                    self.tabs.insert(key, tab);
                }
                Body::Overview(e) => remove(&self.area, e, window, cx),
                Body::List(e) => remove(&self.area, e, window, cx),
                Body::Element(e) => remove(&self.area, e, window, cx),
            }
        }
        self.renamed(window, cx);
    }

    /// Saves the changes to the open folder.
    ///
    /// # Errors
    ///
    /// The read-only reason, when no folder is open (use Save As), or the
    /// text of the failed save.
    pub fn save(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        if let Some(reason) = &self.read_only {
            return Err(reason.clone());
        }
        let Some(folder) = &mut self.folder else {
            return Err("no project folder is open".to_owned());
        };
        folder
            .save(self.workspace.project())
            .map_err(|e| e.to_string())?;
        self.saved_seq = self.workspace.confirmed_seq();
        cx.notify();
        Ok(())
    }

    /// Writes the project into the new folder `path` and goes on in it.
    /// The changes and their undo history stay. Save As into the open
    /// folder is a plain Save.
    ///
    /// # Errors
    ///
    /// The read-only reason, or why the folder cannot be created or
    /// written.
    pub fn save_as(
        &mut self,
        path: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if let Some(reason) = self.read_only.clone() {
            self.note = Some(reason.clone());
            cx.notify();
            return Err(reason);
        }
        if self.folder.as_ref().is_some_and(|f| f.holds(path)) {
            return self.save(cx);
        }
        let folder =
            WorkspaceFolder::create(path, self.workspace.project()).map_err(|e| e.to_string())?;
        self.name = folder.name();
        self.folder = Some(folder);
        self.saved_seq = self.workspace.confirmed_seq();
        self.renamed(window, cx);
        Ok(())
    }

    /// Saves the layout and asks the window to close: `true` once leaving
    /// is decided, otherwise the project is left as `leave` says and the
    /// window closes later, or stays.
    pub fn request_close(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        self.save_layout(cx);
        if self.leaving {
            return true;
        }
        self.leave(Leave::Close, window, cx);
        false
    }

    /// Leaves the project, then does `then`. A folder that saves is saved
    /// silently; the sample with changes, or a failed save, asks Save (or
    /// Try again), Don't save or Cancel first. Does nothing while asking.
    pub fn leave(&mut self, then: Leave, window: &mut Window, cx: &mut Context<Self>) {
        if self.asking {
            return;
        }
        if let Leave::Open(p) = &then
            && self.folder.as_ref().is_some_and(|f| f.holds(p))
        {
            return;
        }
        if !self.has_unsaved_changes() {
            self.finish(then, false, window, cx);
        } else if self.folder.is_some() {
            match self.save(cx) {
                Ok(()) => self.finish(then, false, window, cx),
                Err(e) => {
                    self.note = Some(e.clone());
                    cx.notify();
                    self.ask(then, Some(e), window, cx);
                }
            }
        } else {
            self.ask(then, None, window, cx);
        }
    }

    /// Shows the one dialog: Save As (sample) or Try again (failed save),
    /// Don't save, Cancel.
    fn ask(
        &mut self,
        then: Leave,
        error: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.asking = true;
        let name = [("name", self.name.as_str())];
        let (title, detail, first) = match &error {
            Some(e) => (
                self.catalog.tr_with("workbench-leave-failed-title", &name),
                self.catalog
                    .tr_with("workbench-leave-failed", &[("error", e.as_str())]),
                self.catalog.tr("workbench-leave-retry"),
            ),
            None => (
                self.catalog.tr_with("workbench-leave-title", &name),
                self.catalog.tr("workbench-leave-sample"),
                self.catalog.tr("workbench-leave-save-as"),
            ),
        };
        let answers = [
            PromptButton::ok(first),
            PromptButton::new(self.catalog.tr("workbench-leave-discard")),
            PromptButton::cancel(self.catalog.tr("workbench-leave-cancel")),
        ];
        let rx = window.prompt(PromptLevel::Warning, &title, Some(&detail), &answers, cx);
        let failed = error.is_some();
        cx.spawn_in(window, async move |this, cx| {
            let answer = rx.await.ok();
            let _ = this.update_in(cx, |w, window, cx| {
                w.asking = false;
                match answer {
                    Some(0) if failed => w.leave(then, window, cx),
                    Some(0) => w.prompt_save_as(Some(then), window, cx),
                    Some(1) => w.finish(then, true, window, cx),
                    _ => {}
                }
            });
        })
        .detach();
    }

    /// Does `then` now; `discard` opens another folder without saving the
    /// open one first. A refused folder goes to the status bar.
    fn finish(&mut self, then: Leave, discard: bool, window: &mut Window, cx: &mut Context<Self>) {
        match then {
            Leave::Close => {
                self.leaving = true;
                window.remove_window();
            }
            Leave::Quit => {
                self.leaving = true;
                cx.quit();
            }
            Leave::Open(p) => {
                if let Err(e) = self.open_inner(&p, !discard, window, cx) {
                    self.note = Some(e);
                    cx.notify();
                }
            }
            Leave::Start => cx.emit(WorkbenchEvent::BackToStart),
        }
    }

    /// Shows the project name in the window title, the Overview and the
    /// explorer after the folder changed.
    fn renamed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.set_window_title(&format!("{} — AegisTARA", self.name));
        if let Some(Body::Overview(e)) = self.tabs.get(&OVERVIEW).map(|t| &t.body) {
            let title = SharedString::from(self.catalog.tr("workbench-overview"));
            let heading = Some(SharedString::from(self.name.clone()));
            let lines = overview_lines(self.workspace.project(), &self.names);
            e.update(cx, |t, cx| t.set(title, heading, lines, cx));
        }
        self.changed(Ok(()), cx);
    }

    /// Asks the OS for a folder and opens it; without a checked license,
    /// only says to use the start screen.
    pub(super) fn prompt_open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.license.is_none() {
            self.note = Some(self.catalog.tr("workbench-needs-license"));
            cx.notify();
            return;
        }
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(self.catalog.tr("cmd-open-folder").into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = rx.await
                && let Some(p) = paths.first()
            {
                let _ = this.update_in(cx, |w, window, cx| {
                    w.leave(Leave::Open(p.clone()), window, cx);
                });
            }
        })
        .detach();
    }

    /// Asks the OS for a new folder and saves the project into it, then
    /// does `then` when given and the save worked.
    pub(super) fn prompt_save_as(
        &mut self,
        then: Option<Leave>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.refuse_read_only(cx) {
            return;
        }
        let rx = cx.prompt_for_new_path(&default_export_dir(), Some(&self.name));
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(path))) = rx.await {
                let _ = this.update_in(cx, |w, window, cx| match w.save_as(&path, window, cx) {
                    Ok(()) => {
                        if let Some(then) = then {
                            w.finish(then, false, window, cx);
                        }
                    }
                    Err(e) => {
                        w.note = Some(e);
                        cx.notify();
                    }
                });
            }
        })
        .detach();
    }
}
