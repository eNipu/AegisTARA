//! One GPUI action per command of the command table, and the key bindings
//! and app menu built from that table, so a shortcut or a label is written
//! in one place only (WBX-11).

use std::rc::Rc;

use aegis_client::command_table::{CommandGroup, command, commands};
use aegis_client::i18n::Catalog;
use gpui::{
    Action, App, Context, Div, InteractiveElement as _, KeyBinding, KeyBindingContextPredicate,
    Menu, MenuItem, actions,
};

use super::workbench::Workbench;

/// Defines the actions, [`dispatch_id`] and the handlers of every command.
macro_rules! command_actions {
    ($($id:literal => $action:ident,)*) => {
        actions!(workbench, [$($action),*]);

        /// The action of a command id; `None` for an id the table lacks.
        #[must_use]
        pub fn dispatch_id(id: &str) -> Option<Box<dyn Action>> {
            match id {
                $($id => Some(Box::new($action)),)*
                _ => None,
            }
        }

        /// Hands every command action that reaches `el` to [`Workbench::run`].
        pub(super) fn on_every_command(el: Div, cx: &mut Context<Workbench>) -> Div {
            el $(.on_action(cx.listener(|w, _: &$action, window, cx| w.run($id, window, cx))))*
        }
    };
}

command_actions! {
    "palette" => Palette,
    "undo" => Undo,
    "redo" => Redo,
    "toggle-explorer" => ToggleExplorer,
    "toggle-inspector" => ToggleInspector,
    "toggle-bottom-dock" => ToggleBottomDock,
    "focus-explorer" => FocusExplorer,
    "filter-explorer" => FilterExplorer,
    "explorer-down" => ExplorerDown,
    "explorer-up" => ExplorerUp,
    "explorer-expand" => ExplorerExpand,
    "explorer-collapse" => ExplorerCollapse,
    "open-selected" => OpenSelected,
    "pick-up" => PickUp,
    "drop-here" => DropHere,
    "close-tab" => CloseTab,
    "next-tab" => NextTab,
    "previous-tab" => PreviousTab,
    "toggle-theme" => ToggleTheme,
    "reset-layout" => ResetLayout,
    "show-shortcuts" => ShowShortcuts,
    "quit" => Quit,
}

/// The key binding of every command with keys, on this platform's
/// spelling and in the command's context.
#[must_use]
pub fn bindings(cx: &App) -> Vec<KeyBinding> {
    let mac = cfg!(target_os = "macos");
    commands()
        .iter()
        .filter_map(|spec| {
            let keys = spec.keys_for(mac)?;
            let action = dispatch_id(spec.id)?;
            let context = spec
                .context
                .and_then(|c| KeyBindingContextPredicate::parse(c).ok())
                .map(Rc::new);
            KeyBinding::load(&keys, action, context, false, None, cx)
                .map_err(|err| eprintln!("cannot bind {keys} to {}: {err}", spec.id))
                .ok()
        })
        .collect()
}

/// Binds the keys of every command.
pub fn bind_all(cx: &mut App) {
    let all = bindings(cx);
    cx.bind_keys(all);
}

/// The app menu: AegisTARA, Edit, View and Navigate, labels from `catalog`.
/// The native menu cannot draw icons; the toolbar and palette do.
pub fn set_menus(catalog: &Catalog, cx: &App) {
    let item = |id: &str| {
        Some(MenuItem::Action {
            name: catalog.tr(&command(id)?.label_id()).into(),
            action: dispatch_id(id)?,
            os_action: None,
            checked: false,
            disabled: false,
        })
    };
    let menu = |name: &str, ids: &[&str]| {
        Menu::new(name.to_owned()).items(ids.iter().filter_map(|id| item(id)))
    };
    cx.set_menus([
        menu("AegisTARA", &["show-shortcuts", "toggle-theme", "quit"]),
        menu(CommandGroup::Edit.name(), &["undo", "redo"]),
        menu(
            CommandGroup::View.name(),
            &[
                "toggle-explorer",
                "toggle-inspector",
                "toggle-bottom-dock",
                "focus-explorer",
                "filter-explorer",
                "reset-layout",
                "close-tab",
                "next-tab",
                "previous-tab",
            ],
        ),
        menu(CommandGroup::Navigate.name(), &["palette"]),
    ]);
}
