//! The one table of workbench commands (WBX-11).
//!
//! Every menu, toolbar button, key binding, palette entry and shortcut list
//! reads this table, so a command's label, icon and keys are written once.
//! Labels and tooltips are Fluent ids (`cmd-<id>`, `cmd-<id>-tip`); keys use
//! GPUI keystroke spelling (`"cmd-shift-z"`). No UI toolkit here.

/// Which part of the workbench a command belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandGroup {
    /// App-wide commands (palette, shortcuts, quit).
    General,
    /// Opening and saving the project folder.
    File,
    /// Panels, tabs, theme and layout.
    View,
    /// Moving around the explorer.
    Navigate,
    /// Changing the project.
    Edit,
}

impl CommandGroup {
    /// The group name shown next to a command (English, like the element
    /// kinds in the palette).
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            CommandGroup::General => "General",
            CommandGroup::File => "File",
            CommandGroup::View => "View",
            CommandGroup::Navigate => "Navigate",
            CommandGroup::Edit => "Edit",
        }
    }
}

/// One row of the command table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommandSpec {
    /// Stable id, also the suffix of the Fluent ids.
    pub id: &'static str,
    /// Group shown in the palette and the shortcut list.
    pub group: CommandGroup,
    /// Lucide icon file name (without `.svg`).
    pub icon: &'static str,
    /// Key context the binding is limited to (`None` = whole window).
    pub context: Option<&'static str>,
    /// macOS keystroke, GPUI spelling.
    pub keys: Option<&'static str>,
    /// Keystroke off macOS when it is not `keys` with `cmd` as `ctrl`.
    pub keys_off_mac: Option<&'static str>,
    /// Whether the command palette lists it.
    pub in_palette: bool,
}

impl CommandSpec {
    /// Fluent id of the label: `cmd-<id>`.
    #[must_use]
    pub fn label_id(&self) -> String {
        format!("cmd-{}", self.id)
    }

    /// Fluent id of the tooltip: `cmd-<id>-tip`.
    #[must_use]
    pub fn tooltip_id(&self) -> String {
        format!("cmd-{}-tip", self.id)
    }

    /// The keystroke on macOS (`mac`) or elsewhere. Off macOS,
    /// `keys_off_mac` wins when set, else `cmd` becomes `ctrl`.
    #[must_use]
    pub fn keys_for(&self, mac: bool) -> Option<String> {
        if mac {
            return self.keys.map(str::to_owned);
        }
        if let Some(off) = self.keys_off_mac {
            return Some(off.to_owned());
        }
        self.keys.map(|k| {
            k.split('-')
                .map(|part| if part == "cmd" { "ctrl" } else { part })
                .collect::<Vec<_>>()
                .join("-")
        })
    }
}

/// `-` in a table cell means "none".
macro_rules! cell {
    (-) => {
        None
    };
    ($value:literal) => {
        Some($value)
    };
}

/// Builds the command table from one row per command, columns as in
/// [`CommandSpec`]: id, group, icon, context, keys, keys off macOS, palette.
macro_rules! command_table {
    ($($id:literal $group:ident $icon:literal $context:tt $keys:tt $off:tt $palette:literal;)*) => {
        [$(CommandSpec {
            id: $id,
            group: CommandGroup::$group,
            icon: $icon,
            context: cell!($context),
            keys: cell!($keys),
            keys_off_mac: cell!($off),
            in_palette: $palette,
        },)*]
    };
}

static COMMANDS: [CommandSpec; 32] = command_table! {
    "palette"            General  "search"           -          "cmd-k"          -        false;
    "open-folder"        File     "folder-open"      -          "cmd-o"          -        true;
    "save"               File     "hard-drive"       -          "cmd-s"          -        true;
    "save-as"            File     "folder"           -          "cmd-shift-s"    -        true;
    "back-to-start"      File     "arrow-left"       -          -                -        true;
    "undo"               Edit     "undo"             -          "cmd-z"          -        true;
    "redo"               Edit     "redo"             -          "cmd-shift-z"    "ctrl-y" true;
    "toggle-explorer"    View     "panel-left"       -          "cmd-b"          -        true;
    "toggle-inspector"   View     "panel-right"      -          "cmd-alt-b"      -        true;
    "toggle-bottom-dock" View     "panel-bottom"     -          "cmd-j"          -        true;
    "focus-explorer"     View     "folder-open"      -          "cmd-shift-e"    -        true;
    "filter-explorer"    Navigate "case-sensitive"   -          "cmd-alt-f"      -        true;
    "explorer-down"      Navigate "arrow-down"       "Explorer" "down"           -        false;
    "explorer-up"        Navigate "arrow-up"         "Explorer" "up"             -        false;
    "explorer-expand"    Navigate "chevron-right"    "Explorer" "right"          -        false;
    "explorer-collapse"  Navigate "chevron-left"     "Explorer" "left"           -        false;
    "open-selected"      Navigate "external-link"    "Explorer" "enter"          -        false;
    "pick-up"            Edit     "copy"             "Explorer" "cmd-x"          -        false;
    "drop-here"          Edit     "inbox"            "Explorer" "cmd-v"          -        false;
    "close-tab"          View     "close"            -          "cmd-w"          -        true;
    "next-tab"           View     "arrow-right"      -          "ctrl-tab"       -        true;
    "previous-tab"       View     "arrow-left"       -          "ctrl-shift-tab" -        true;
    "toggle-theme"       View     "moon"             -          -                -        true;
    "reset-layout"       View     "layout-dashboard" -          -                -        true;
    "show-shortcuts"     General  "info"             -          "cmd-/"          -        true;
    "quit"               General  "window-close"     -          "cmd-q"          "ctrl-q" true;
    "grid-add-row"            Edit "plus"            "Grid"     "cmd-n"          -             false;
    "grid-delete-rows"        Edit "delete"          "Grid"     "cmd-backspace"  "ctrl-delete" false;
    "grid-fill-down"          Edit "arrow-down"      "Grid"     "cmd-d"          -             false;
    "grid-copy"               Edit "copy"            "Grid"     "cmd-c"          -             false;
    "grid-paste"              Edit "replace"         "Grid"     "cmd-v"          -             false;
    "toggle-developer-source" View "square-terminal" -          -                -             true;
};

/// Every workbench command, in menu order.
#[must_use]
pub fn commands() -> &'static [CommandSpec] {
    &COMMANDS
}

/// The command with this id. Time O(table size).
#[must_use]
pub fn command(id: &str) -> Option<&'static CommandSpec> {
    COMMANDS.iter().find(|c| c.id == id)
}

/// Modifiers in the order macOS writes them, with their symbols.
const MAC_MODIFIERS: [(&str, char); 4] =
    [("ctrl", '⌃'), ("alt", '⌥'), ("shift", '⇧'), ("cmd", '⌘')];

/// Modifiers elsewhere, in the order they are written.
const OTHER_MODIFIERS: [(&str, &str); 3] = [("ctrl", "Ctrl"), ("alt", "Alt"), ("shift", "Shift")];

/// How a key name is written in a label.
fn key_label(key: &str, mac: bool) -> String {
    match key {
        "enter" if mac => "Return".to_owned(),
        "enter" => "Enter".to_owned(),
        "up" => "↑".to_owned(),
        "down" => "↓".to_owned(),
        "left" => "←".to_owned(),
        "right" => "→".to_owned(),
        "tab" => "Tab".to_owned(),
        "escape" => "Esc".to_owned(),
        "backspace" => "⌫".to_owned(),
        "delete" => "Del".to_owned(),
        k if k.chars().count() == 1 => k.to_uppercase(),
        k => k.to_owned(),
    }
}

/// Formats a GPUI keystroke for people: `⇧⌘Z` on macOS, `Ctrl+Shift+Z`
/// elsewhere. Time O(keys length).
#[must_use]
pub fn shortcut_label(keys: &str, mac: bool) -> String {
    let parts: Vec<&str> = keys.split('-').collect();
    // The key is the last part; a trailing `-` means the key is `-` itself.
    let (mods, key) = match parts.split_last() {
        Some((&"", rest)) if !rest.is_empty() => (&rest[..rest.len() - 1], "-"),
        Some((key, rest)) => (rest, *key),
        None => (&parts[..0], ""),
    };
    let key = key_label(key, mac);
    if mac {
        let mut out: String = MAC_MODIFIERS
            .iter()
            .filter(|(name, _)| mods.contains(name))
            .map(|(_, symbol)| *symbol)
            .collect();
        out.push_str(&key);
        out
    } else {
        let mut out: Vec<&str> = OTHER_MODIFIERS
            .iter()
            .filter(|(name, _)| mods.contains(name))
            .map(|(_, label)| *label)
            .collect();
        out.push(&key);
        out.join("+")
    }
}
