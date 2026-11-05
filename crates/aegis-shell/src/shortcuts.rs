//! The single source of keyboard shortcuts (UI-07.6, job 081 and job 113).
//!
//! The command palette, the keyboard cheat sheet, the menus, and the
//! description editor's hint all read this one table. A shortcut is written
//! once as key parts (`["Control", "Shift", "Z"]`), then rendered as
//! `⇧⌘Z` on macOS or `Ctrl+Shift+Z` elsewhere and turned into the same
//! `slint::Keys` binding the menu uses. Nothing in the `.slint` files or in a
//! Rust string hard-codes a `Ctrl+` label.
//!
//! Slint's `Control` means the platform command key for accelerator purposes
//! (Command on macOS), so the table never needs a per-OS key name. `Meta`
//! means the key Slint calls meta (Control on macOS), used for display only.

use slint::Keys;

use crate::AppWindow;

/// One keyboard shortcut.
pub struct Shortcut {
    /// Stable id: the palette dispatches by it and the menu wiring finds it.
    pub id: &'static str,
    /// What the action is called, for the palette and the cheat sheet.
    pub label: &'static str,
    /// A coarse group for the cheat sheet (`Editing`, `View`, `Project`).
    pub group: &'static str,
    /// Modifiers and one key, in Slint's `Keys::from_parts` spelling.
    pub parts: &'static [&'static str],
    /// A different spelling off macOS, where the platform convention differs
    /// (Redo is Ctrl+Y on Windows and Linux, which already worked).
    pub non_mac_parts: Option<&'static [&'static str]>,
    /// Whether the palette offers this as a runnable command. A hint that is
    /// only shown in the cheat sheet (saving a description) is `false`.
    pub in_palette: bool,
}

impl Shortcut {
    /// The key parts for a platform.
    fn parts_for(&self, mac: bool) -> &'static [&'static str] {
        match (mac, self.non_mac_parts) {
            (false, Some(parts)) => parts,
            _ => self.parts,
        }
    }

    const fn off_mac(mut self, parts: &'static [&'static str]) -> Self {
        self.non_mac_parts = Some(parts);
        self
    }

    const fn new(
        id: &'static str,
        label: &'static str,
        group: &'static str,
        parts: &'static [&'static str],
    ) -> Self {
        Self {
            id,
            label,
            group,
            parts,
            non_mac_parts: None,
            in_palette: true,
        }
    }

    const fn hint(
        id: &'static str,
        label: &'static str,
        group: &'static str,
        parts: &'static [&'static str],
    ) -> Self {
        Self {
            id,
            label,
            group,
            parts,
            non_mac_parts: None,
            in_palette: false,
        }
    }
}

/// Modifier symbols on macOS, in the order Apple writes them: Option,
/// Shift, Command. Slint's `Control` is Command on Apple.
const MAC_MODIFIERS: [(&str, char); 3] = [("Alt", '⌥'), ("Shift", '⇧'), ("Control", '⌘')];

/// Modifier names elsewhere, in the same order (Ctrl, Alt, Shift).
const OTHER_MODIFIERS: [(&str, &str); 3] =
    [("Control", "Ctrl"), ("Alt", "Alt"), ("Shift", "Shift")];

/// Whether the app runs on macOS (used to choose the key labels).
#[must_use]
pub const fn is_mac() -> bool {
    cfg!(target_os = "macos")
}

/// How a key is written in a label. `Return` is the one named key in the
/// table; letters and digits keep their spelling.
fn key_label(part: &str, mac: bool) -> &str {
    match (part, mac) {
        ("Return", true) => "Return",
        ("Return", false) => "Enter",
        (other, _) => other,
    }
}

/// Formats a shortcut for the current platform.
#[must_use]
pub fn keys(s: &Shortcut) -> String {
    format_label(s, is_mac())
}

fn format_label(s: &Shortcut, mac: bool) -> String {
    let mut out = String::new();
    let parts = s.parts_for(mac);
    let key = parts
        .iter()
        .rev()
        .find(|part| !MAC_MODIFIERS.iter().any(|(name, _)| *name == **part))
        .copied()
        .unwrap_or_default();
    if mac {
        for (name, symbol) in MAC_MODIFIERS {
            if parts.contains(&name) {
                out.push(symbol);
            }
        }
        out.push_str(key_label(key, true));
    } else {
        let modifiers: Vec<&str> = OTHER_MODIFIERS
            .iter()
            .filter(|(name, _)| parts.contains(name))
            .map(|(_, label)| *label)
            .collect();
        if !modifiers.is_empty() {
            out.push_str(&modifiers.join("+"));
            out.push('+');
        }
        out.push_str(key_label(key, false));
    }
    out
}

/// The table's `Keys` binding for Slint menus and key handlers.
///
/// # Panics
///
/// Only if a table entry is not a valid `Keys::from_parts` spelling, which
/// the table test rules out.
#[must_use]
pub fn slint_keys(s: &Shortcut) -> Keys {
    Keys::from_parts(s.parts_for(is_mac()).iter().copied())
        .unwrap_or_else(|e| panic!("shortcut {} has invalid parts: {e}", s.id))
}

/// The shortcuts, in cheat-sheet order.
#[must_use]
pub fn table() -> &'static [Shortcut] {
    const TABLE: [Shortcut; 17] = [
        Shortcut::new("palette", "Command palette", "General", &["Control", "K"]),
        Shortcut::new("undo", "Undo", "Editing", &["Control", "Z"]),
        Shortcut::new("redo", "Redo", "Editing", &["Control", "Shift", "Z"])
            .off_mac(&["Control", "Y"]),
        Shortcut::new(
            "begin-edit",
            "Take the edit lock",
            "Editing",
            &["Control", "E"],
        ),
        Shortcut::new(
            "end-edit",
            "Release the edit lock",
            "Editing",
            &["Control", "E"],
        ),
        Shortcut::new(
            "new-element",
            "New element on this page",
            "Project",
            &["Control", "Shift", "N"],
        ),
        Shortcut::hint(
            "save-description",
            "Save the description",
            "Editing",
            &["Control", "Return"],
        ),
        Shortcut::new("overview", "Overview", "View", &["Control", "1"]),
        Shortcut::new("assets", "Assets", "View", &["Control", "2"]),
        Shortcut::new("damage", "Damage scenarios", "View", &["Control", "3"]),
        Shortcut::new("threats", "Threat scenarios", "View", &["Control", "4"]),
        Shortcut::new("trees", "Attack trees", "View", &["Control", "5"]),
        Shortcut::new("risks", "Risks and treatment", "View", &["Control", "6"]),
        Shortcut::new("controls", "Security controls", "View", &["Control", "7"]),
        Shortcut::new("goals", "Goals and claims", "View", &["Control", "8"]),
        Shortcut::new("problems", "Problems", "View", &["Control", "9"]),
        Shortcut::new("item", "Item definition", "View", &["Control", "0"]),
    ];
    &TABLE
}

/// Gives `window` every menu binding and the platform flag its key handler
/// needs. Called from `Controller::attach_with`, so the real app and the
/// tests get the same bindings.
pub fn install(window: &AppWindow) {
    window.set_palette_keys(keys_for("palette"));
    window.set_undo_keys(keys_for("undo"));
    window.set_redo_keys(keys_for("redo"));
    window.set_begin_edit_keys(keys_for("begin-edit"));
    window.set_end_edit_keys(keys_for("end-edit"));
    window.set_new_element_keys(keys_for("new-element"));
    window.set_save_description_keys(keys_for("save-description"));
    window.set_overview_keys(keys_for("overview"));
    window.set_assets_keys(keys_for("assets"));
    window.set_damage_keys(keys_for("damage"));
    window.set_threats_keys(keys_for("threats"));
    window.set_trees_keys(keys_for("trees"));
    window.set_risks_keys(keys_for("risks"));
    window.set_controls_keys(keys_for("controls"));
    window.set_goals_keys(keys_for("goals"));
    window.set_problems_keys(keys_for("problems"));
    window.set_item_keys(keys_for("item"));
    window.set_save_description_label(keys_for_label("save-description").into());
    window.set_undo_shortcut(keys_for_label("undo").into());
    window.set_redo_shortcut(keys_for_label("redo").into());
}

/// The platform-spelled label of a shortcut from the table.
fn keys_for_label(id: &str) -> String {
    table()
        .iter()
        .find(|s| s.id == id)
        .map_or_else(String::new, keys)
}

fn keys_for(id: &str) -> Keys {
    table()
        .iter()
        .find(|s| s.id == id)
        .map(slint_keys)
        .unwrap_or_default()
}

/// The page id a `View` shortcut navigates to, or `None` for a non-page
/// shortcut. The ids line up with [`aegis_client::view::Page`].
#[must_use]
pub fn page_for(id: &str) -> Option<aegis_client::view::Page> {
    use aegis_client::view::Page;
    Some(match id {
        "overview" => Page::Overview,
        "assets" => Page::Assets,
        "damage" => Page::Damage,
        "threats" => Page::Threats,
        "trees" => Page::Trees,
        "risks" => Page::Risks,
        "controls" => Page::Controls,
        "goals" => Page::Goals,
        "problems" => Page::Problems,
        "item" => Page::Item,
        _ => return None,
    })
}

/// The palette entries: every page and every command that has a shortcut.
///
/// Each entry is `(id, label, group, keys, name)`; the palette filters by
/// label, group, keys and (for element entries) the bare element name, and
/// dispatches by id. Pages that are not commands (Versions, History, Reports,
/// Administration) are listed too, so the palette really does reach every
/// page. Hint-only shortcuts (saving a description) stay in the cheat sheet
/// but are not offered as commands.
pub struct PaletteEntry {
    pub id: String,
    pub label: String,
    pub group: String,
    pub keys: String,
    pub name: String,
    pub target: Option<aegis_client::view::Selection>,
}

/// Builds the palette entries once: the pages, then the commands.
#[must_use]
pub fn palette_entries() -> Vec<PaletteEntry> {
    let mut out = Vec::new();
    for page in aegis_client::view::Page::ALL {
        out.push(PaletteEntry {
            id: format!("page:{}", page.index()),
            label: page.label().to_owned(),
            group: "Pages".to_owned(),
            keys: String::new(),
            name: String::new(),
            target: None,
        });
    }
    for s in table().iter().filter(|s| s.in_palette) {
        out.push(PaletteEntry {
            id: format!("cmd:{}", s.id),
            label: s.label.to_owned(),
            group: s.group.to_owned(),
            keys: keys(s),
            name: String::new(),
            target: None,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn every_id_is_unique_and_every_binding_parses() {
        let mut ids: Vec<&str> = table().iter().map(|s| s.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), table().len(), "shortcut ids must be unique");
        for s in table() {
            assert_ne!(
                slint_keys(s),
                Keys::default(),
                "{} has no modifiers or key",
                s.id
            );
        }
    }

    #[test]
    fn mac_labels_use_mac_modifiers_and_never_ctrl() {
        for s in table() {
            let label = format_label(s, true);
            let first = label.chars().next().expect("a label");
            assert!(
                matches!(first, '⌃' | '⌥' | '⇧' | '⌘'),
                "{}: {label} must start with a macOS modifier",
                s.id
            );
            assert!(!label.contains("Ctrl"), "{}: {label}", s.id);
            assert!(!label.contains("Super"), "{}: {label}", s.id);
        }
        assert_eq!(format_label(&table()[2], true), "⇧⌘Z");
        let save = table()
            .iter()
            .find(|s| s.id == "save-description")
            .expect("save hint");
        assert_eq!(format_label(save, true), "⌘Return");
    }

    #[test]
    fn other_labels_never_use_a_mac_symbol() {
        for s in table() {
            let label = format_label(s, false);
            for symbol in ['⌘', '⌥', '⇧', '⌃'] {
                assert!(!label.contains(symbol), "{}: {label}", s.id);
            }
            assert!(label.starts_with("Ctrl+"), "{}: {label}", s.id);
        }
        assert_eq!(format_label(&table()[2], false), "Ctrl+Y");
        assert_eq!(format_label(&table()[2], true), "⇧⌘Z");
    }

    #[test]
    fn the_menu_wiring_matches_the_table() {
        for id in [
            "palette",
            "undo",
            "redo",
            "begin-edit",
            "end-edit",
            "new-element",
            "save-description",
        ] {
            assert!(
                table().iter().any(|s| s.id == id),
                "{id} is in the menu wiring but not in the table"
            );
        }
    }

    /// The menus and key handlers read the table instead of spelling a
    /// shortcut themselves (UI-07.6). `shortcuts.rs` is the one place that
    /// may write a key part.
    #[test]
    fn no_slint_file_or_rust_string_spells_a_shortcut() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut found = Vec::new();
        for dir in ["ui", "src"] {
            scan(&root.join(dir), &mut found);
        }
        assert!(found.is_empty(), "hard-coded shortcut labels: {found:?}");
    }

    fn scan(dir: &Path, found: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).expect("source directory") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                scan(&path, found);
                continue;
            }
            let extension = path.extension().and_then(|e| e.to_str());
            if !matches!(extension, Some("slint" | "rs")) {
                continue;
            }
            if path.file_name().and_then(|n| n.to_str()) == Some("shortcuts.rs") {
                continue;
            }
            let text = std::fs::read_to_string(&path).expect("source file");
            if text.contains("Ctrl+") || text.contains('⌘') || text.contains("@keys(") {
                found.push(path.display().to_string());
            }
        }
    }
}
