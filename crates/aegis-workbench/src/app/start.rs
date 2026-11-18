//! The start screen: a project folder and a license file on this computer,
//! Open, New workspace and Try the sample. The license is checked with
//! `read_license`; the folder is opened by the shell's workbench.

use std::path::{Path, PathBuf};
use std::rc::Rc;

use aegis_client::i18n::Catalog;
use aegis_client::license_file::{LocalLicense, now_secs, read_license};
use aegis_client::settings::{Appearance, Settings, default_export_dir};
use aegis_license::TrustedKeys;
use gpui::{
    AppContext as _, Context, Entity, EventEmitter, IntoElement, ParentElement as _,
    PathPromptOptions, Render, SharedString, Styled as _, Subscription, Window, div, px,
};
use gpui_component::ActiveTheme as _;
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::input::{Input, InputEvent, InputState};

/// What the start screen and the shell around it share.
#[derive(Clone)]
pub struct StartConfig {
    /// The texts.
    pub catalog: Rc<Catalog>,
    /// The appearance the workbench starts with.
    pub appearance: Appearance,
    /// The settings file; `None` remembers nothing.
    pub settings_path: Option<PathBuf>,
    /// The keys a license file must be signed with.
    pub keys: TrustedKeys,
}

/// What the start screen asks the shell to do.
#[derive(Clone, Debug)]
pub enum StartEvent {
    /// Open the workspace in `folder` under `license`.
    Open {
        /// The project folder.
        folder: PathBuf,
        /// The checked license.
        license: LocalLicense,
    },
    /// Create a new workspace in `folder`, then open it under `license`.
    Create {
        /// The new project folder.
        folder: PathBuf,
        /// The checked license.
        license: LocalLicense,
    },
    /// Open the in-memory sample; no license needed.
    Sample,
}

/// The start screen view.
pub struct StartScreen {
    config: StartConfig,
    folder: Entity<InputState>,
    license_path: Entity<InputState>,
    error: Option<String>,
    license: Option<LocalLicense>,
    _subscriptions: [Subscription; 2],
}

impl EventEmitter<StartEvent> for StartScreen {}

/// What a Browse… button asks for.
#[derive(Clone, Copy)]
enum Pick {
    Folder,
    License,
}

impl StartScreen {
    /// The start screen, its fields filled from the remembered settings.
    pub fn new(config: &StartConfig, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let settings = config
            .settings_path
            .as_deref()
            .map(Settings::load)
            .unwrap_or_default();
        let catalog = &config.catalog;
        let mut input = |placeholder: String, value: String, cx: &mut Context<Self>| {
            cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder(placeholder)
                    .default_value(value)
            })
        };
        let folder = input(catalog.tr("start-folder"), settings.standalone_folder, cx);
        let license_path = input(catalog.tr("start-license"), settings.standalone_license, cx);
        let enter = |this: &mut Self,
                     _: &Entity<InputState>,
                     ev: &InputEvent,
                     _: &mut Window,
                     cx: &mut Context<Self>| {
            if let InputEvent::PressEnter { .. } = ev {
                this.open_local(cx);
            }
        };
        let subscriptions = [
            cx.subscribe_in(&folder, window, enter),
            cx.subscribe_in(&license_path, window, enter),
        ];
        Self {
            config: config.clone(),
            folder,
            license_path,
            error: None,
            license: None,
            _subscriptions: subscriptions,
        }
    }

    /// Puts `path` in the folder field.
    pub fn set_folder(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
        set_text(&self.folder, path, window, cx);
    }

    /// Puts `path` in the license field.
    pub fn set_license_path(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
        set_text(&self.license_path, path, window, cx);
    }

    /// Shows `error` below the buttons, or nothing with `None`.
    pub fn set_error(&mut self, error: Option<String>, cx: &mut Context<Self>) {
        self.error = error;
        cx.notify();
    }

    /// The error shown, if any.
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// The license checked last, if any.
    pub fn license(&self) -> Option<&LocalLicense> {
        self.license.as_ref()
    }

    /// The license file path in the field.
    pub fn license_file(&self, cx: &gpui::App) -> PathBuf {
        PathBuf::from(self.license_path.read(cx).value().trim())
    }

    /// Moves the keyboard focus to the folder field.
    pub fn focus_folder(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.folder.update(cx, |f, cx| f.focus(window, cx));
    }

    /// The folder and the checked license, or the error shown.
    fn checked(&mut self, cx: &mut Context<Self>) -> Option<(PathBuf, LocalLicense)> {
        let folder = self.folder.read(cx).value().trim().to_owned();
        let license_file = self.license_file(cx);
        let tr = |id: &str| self.config.catalog.tr(id);
        let res = if folder.is_empty() {
            Err(tr("start-needs-folder"))
        } else if license_file.as_os_str().is_empty() {
            Err(tr("start-needs-license"))
        } else {
            read_license(
                &license_file,
                &self.config.keys,
                now_secs(),
                &self.config.catalog,
            )
        };
        let checked = match res {
            Ok(license) => {
                self.license = Some(license.clone());
                Some((PathBuf::from(folder), license))
            }
            Err(e) => {
                self.error = Some(e);
                None
            }
        };
        cx.notify();
        checked
    }

    /// Checks the license and asks to open the folder.
    pub fn open_local(&mut self, cx: &mut Context<Self>) {
        if let Some((folder, license)) = self.checked(cx) {
            self.error = None;
            cx.emit(StartEvent::Open { folder, license });
        }
    }

    /// Checks the license and asks to create a new workspace in the folder,
    /// which must be missing or an empty directory.
    pub fn create_local(&mut self, cx: &mut Context<Self>) {
        let Some((folder, license)) = self.checked(cx) else {
            return;
        };
        let empty = !folder.exists()
            || std::fs::read_dir(&folder).is_ok_and(|mut entries| entries.next().is_none());
        if empty {
            self.error = None;
            cx.emit(StartEvent::Create { folder, license });
        } else {
            let path = folder.display().to_string();
            self.error = Some(
                self.config
                    .catalog
                    .tr_with("start-folder-not-empty", &[("path", &path)]),
            );
        }
        cx.notify();
    }

    /// Asks to open the in-memory sample.
    pub fn open_sample(&mut self, cx: &mut Context<Self>) {
        self.error = None;
        cx.emit(StartEvent::Sample);
    }

    /// Asks the OS for a folder or a license file and fills its field.
    fn browse(&mut self, pick: Pick, window: &mut Window, cx: &mut Context<Self>) {
        let folder = matches!(pick, Pick::Folder);
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: !folder,
            directories: folder,
            multiple: false,
            prompt: Some(self.config.catalog.tr("start-browse").into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = rx.await
                && let Some(p) = paths.first()
            {
                let _ = this.update_in(cx, |s, window, cx| match pick {
                    Pick::Folder => s.set_folder(p, window, cx),
                    Pick::License => s.set_license_path(p, window, cx),
                });
            }
        })
        .detach();
    }

    /// Asks the OS for a new folder, then creates the workspace there.
    fn prompt_new(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_new_path(&default_export_dir(), None);
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(path))) = rx.await {
                let _ = this.update_in(cx, |s, window, cx| {
                    s.set_folder(&path, window, cx);
                    s.create_local(cx);
                });
            }
        })
        .detach();
    }

    /// The license line: who is licensed and for how long.
    fn license_text(&self) -> Option<String> {
        let l = self.license.as_ref()?;
        let days = l.days_left.unwrap_or(0).to_string();
        let args = [("customer", l.customer.as_str()), ("days", days.as_str())];
        let id = match (l.editing, l.grace) {
            (false, _) => "start-license-expired",
            (true, true) => "start-license-grace",
            (true, false) => "start-license-ok",
        };
        Some(self.config.catalog.tr_with(id, &args))
    }
}

fn set_text(input: &Entity<InputState>, path: &Path, window: &mut Window, cx: &mut gpui::App) {
    let text = path.display().to_string();
    input.update(cx, |i, cx| i.set_value(text, window, cx));
}

impl Render for StartScreen {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tr = |id: &str| SharedString::from(self.config.catalog.tr(id));
        let license_line = self.license_text();
        let error = self.error.clone();
        let browse = |id: &'static str, pick: Pick, cx: &mut Context<Self>| {
            Button::new(id)
                .label(tr("start-browse"))
                .on_click(cx.listener(move |s, _, window, cx| s.browse(pick, window, cx)))
        };
        let row = |label: SharedString, input: &Entity<InputState>, button: Button| {
            div().flex().flex_col().gap_1().child(label).child(
                div()
                    .flex()
                    .gap_2()
                    .child(div().flex_1().child(Input::new(input)))
                    .child(button),
            )
        };
        let folder_row = row(
            tr("start-folder"),
            &self.folder,
            browse("start-browse-folder", Pick::Folder, cx),
        );
        let license_row = row(
            tr("start-license"),
            &self.license_path,
            browse("start-browse-license", Pick::License, cx),
        );
        let buttons = div()
            .flex()
            .gap_2()
            .child(
                Button::new("start-open")
                    .primary()
                    .label(tr("start-open"))
                    .on_click(cx.listener(|s, _, _, cx| s.open_local(cx))),
            )
            .child(
                Button::new("start-new")
                    .label(tr("start-new"))
                    .on_click(cx.listener(|s, _, window, cx| s.prompt_new(window, cx))),
            )
            .child(
                Button::new("start-sample")
                    .ghost()
                    .label(tr("start-sample"))
                    .on_click(cx.listener(|s, _, _, cx| s.open_sample(cx))),
            );
        let theme = cx.theme();
        div()
            .size_full()
            .flex()
            .justify_center()
            .pt(px(96.))
            .bg(theme.background)
            .text_color(theme.foreground)
            .child(
                div()
                    .w(px(560.))
                    .flex()
                    .flex_col()
                    .gap_4()
                    .child(div().text_2xl().child("AegisTARA"))
                    .child(div().text_lg().child(tr("start-local")))
                    .child(folder_row)
                    .child(license_row)
                    .children(
                        license_line
                            .map(|t| div().text_sm().text_color(theme.muted_foreground).child(t)),
                    )
                    .child(buttons)
                    .children(error.map(|e| div().text_color(theme.danger).child(e))),
            )
    }
}
