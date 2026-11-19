//! The GPUI window: theme set-up, the `Workbench` view and its panels.

mod commands;
mod files;
mod icons;
mod inspector;
mod layout;
mod panels;
mod picker_view;
mod server;
mod shell;
mod start;
mod workbench;

use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use aegis_client::i18n::Catalog;
use aegis_client::provider::secrets;
use aegis_client::settings::{Appearance, Settings, config_dir};
use aegis_license::TrustedKeys;
use gpui::{
    App, AppContext as _, Bounds, Hsla, TitlebarOptions, Window, WindowBounds, WindowOptions, px,
    size,
};
use gpui_component::{Root, Theme, ThemeMode};

use crate::theme::{Tokens, dark, light, parse_hex};

pub use commands::{bind_all, bindings, dispatch_id};
pub use files::Leave;
pub use icons::icon;
pub use inspector::{InspectorPanel, InspectorRelationRow};
pub use picker_view::MultiLinkPickerView;
pub use server::{METHODOLOGIES, ServerEvent, ServerPane};
pub use shell::{Screen, Shell};
pub use start::{ServerSetup, StartConfig, StartEvent, StartScreen};
pub use workbench::{Workbench, WorkbenchEvent};

/// Opens the window on the start screen, opening the project folder
/// `folder` under the remembered license when given, and runs until it
/// closes.
pub fn run(folder: Option<PathBuf>) {
    gpui_platform::application().run(move |cx| {
        gpui_component::init(cx);
        bind_all(cx);
        let path = config_dir().map(|dir| Settings::path_in(&dir));
        let settings = path.as_deref().map(Settings::load).unwrap_or_default();
        let appearance = settings.appearance();
        let catalog = Rc::new(Catalog::load(settings.language()));
        commands::set_menus(&catalog, cx);
        let server = match tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
        {
            Ok(rt) => Some(ServerSetup {
                runtime: rt.handle().clone(),
                secrets: secrets(),
                keep: Some(Arc::new(rt)),
            }),
            Err(err) => {
                eprintln!("could not start the server runtime: {err}");
                None
            }
        };
        let bounds = Bounds::centered(None, size(px(1400.), px(900.)), cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions {
                title: Some("AegisTARA".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let opened = cx.open_window(options, |window, cx| {
            apply_appearance(appearance, window, cx);
            let config = StartConfig {
                catalog,
                appearance,
                settings_path: path,
                keys: TrustedKeys::vendor(),
                server,
            };
            let shell = cx.new(|cx| Shell::new(config, window, cx));
            shell.update(cx, |s, cx| s.start_polling(window, cx));
            if let Some(dir) = &folder {
                shell.update(cx, |s, cx| s.open_path(dir, window, cx));
            }
            cx.new(|cx| Root::new(shell, window, cx))
        });
        if let Err(err) = opened {
            eprintln!("could not open the window: {err:#}");
            cx.quit();
            return;
        }
        cx.activate(true);
    });
}

/// Switches GPUI's theme to the mode the setting asks for (System follows
/// the window) and paints it with our tokens.
pub fn apply_appearance(appearance: Appearance, window: &mut Window, cx: &mut App) {
    match appearance {
        Appearance::System => Theme::sync_system_appearance(Some(window), cx),
        Appearance::Light => Theme::change(ThemeMode::Light, Some(window), cx),
        Appearance::Dark => Theme::change(ThemeMode::Dark, Some(window), cx),
    }
    let tokens = if Theme::global(cx).is_dark() {
        dark()
    } else {
        light()
    };
    apply_tokens(&tokens, cx);
}

fn color(hex: &str) -> Hsla {
    let [r, g, b] = parse_hex(hex).unwrap_or([0, 0, 0]);
    gpui::rgb(u32::from_be_bytes([0, r, g, b])).into()
}

/// Writes our palette over the gpui-component colours. Call after a mode
/// change: loading a mode replaces the colours.
fn apply_tokens(tokens: &Tokens, cx: &mut App) {
    Theme::update(cx, |theme| {
        let c = &mut theme.colors;
        let (surface, ink, muted) = (
            color(&tokens.surface),
            color(&tokens.ink),
            color(&tokens.ink_muted),
        );
        let (line, accent, wash) = (
            color(&tokens.line),
            color(&tokens.accent),
            color(&tokens.accent_wash),
        );
        let raised = color(&tokens.raised);
        c.background = surface;
        c.foreground = ink;
        c.muted_foreground = muted;
        c.border = line;
        c.sidebar = color(&tokens.sidebar);
        c.sidebar_foreground = ink;
        c.sidebar_border = line;
        c.primary = accent;
        c.ring = accent;
        c.accent = wash;
        c.list_active = wash;
        c.selection = wash;
        c.tab_bar = raised;
        c.title_bar = raised;
        c.status_bar = raised;
        c.danger = color(&tokens.error);
        c.warning = color(&tokens.warning);
    });
}
