//! Open Folder, Save and Save As: the workbench over a project folder on
//! disk. Reading and writing go through `WorkspaceFolder` and `convert`.

use std::path::Path;
use std::sync::Arc;

use aegis_client::explorer::ExplorerState;
use aegis_client::folder::WorkspaceFolder;
use aegis_client::settings::default_export_dir;
use aegis_client::workspace::Workspace;
use gpui::{Context, PathPromptOptions, SharedString, Window};

use super::workbench::{Body, OVERVIEW, Workbench, overview_lines, remove};
use crate::convert::{convert_legacy, is_legacy};

impl Workbench {
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
        if self.folder.as_ref().is_some_and(|f| f.holds(path)) {
            return Ok(());
        }
        if self.folder.is_some() && self.has_unsaved_changes() {
            self.save(cx)?;
        }
        let converted = is_legacy(path);
        let methodology = Arc::clone(&self.methodology);
        let (folder, project) = if converted {
            convert_legacy(path, methodology)?
        } else {
            WorkspaceFolder::open(path, methodology).map_err(|e| e.to_string())?
        };
        self.workspace = Workspace::new(project, 0);
        self.saved_seq = 0;
        self.name = folder.name();
        self.folder = Some(folder);
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
        if converted {
            self.note = Some(self.catalog.tr("workbench-converted"));
        }
        Ok(())
    }

    /// Saves the changes to the open folder.
    ///
    /// # Errors
    ///
    /// When no folder is open (use Save As), or the text of the failed
    /// save.
    pub fn save(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
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
    /// Why the folder cannot be created or written.
    pub fn save_as(
        &mut self,
        path: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
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

    /// Saves the layout and any unsaved changes before the window closes;
    /// `false` (and the error in the status bar) when the save failed.
    pub fn before_close(&mut self, cx: &mut Context<Self>) -> bool {
        self.save_layout(cx);
        if self.folder.is_none() || !self.has_unsaved_changes() {
            return true;
        }
        match self.save(cx) {
            Ok(()) => true,
            Err(e) => {
                self.note = Some(e);
                cx.notify();
                false
            }
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

    /// Asks the OS for a folder and opens it.
    pub(super) fn prompt_open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
                    if let Err(e) = w.open_folder(p, window, cx) {
                        w.note = Some(e);
                        cx.notify();
                    }
                });
            }
        })
        .detach();
    }

    /// Asks the OS for a new folder and saves the project into it.
    pub(super) fn prompt_save_as(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_new_path(&default_export_dir(), Some(&self.name));
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(path))) = rx.await {
                let _ = this.update_in(cx, |w, window, cx| {
                    if let Err(e) = w.save_as(&path, window, cx) {
                        w.note = Some(e);
                        cx.notify();
                    }
                });
            }
        })
        .detach();
    }
}
