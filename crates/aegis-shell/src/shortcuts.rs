//! The single source of keyboard shortcuts (UI-07.6, job 081).
//!
//! The command palette, the keyboard cheat sheet, and (from job 113) the
//! menus all read this one table, so a shortcut is written once and its label
//! follows the platform: `⌘`/`⌥`/`⇧` on macOS, `Ctrl`/`Alt`/`Shift`
//! elsewhere. Nothing in the `.slint` files hard-codes a `Ctrl+` string.

/// One keyboard shortcut.
pub struct Shortcut {
    /// Stable id, used by the palette to dispatch the action.
    pub id: &'static str,
    /// What the action is called, for the palette and the cheat sheet.
    pub label: &'static str,
    /// A coarse group for the cheat sheet (`Editing`, `View`, `Project`).
    pub group: &'static str,
    /// The macOS key text, for example `⌘Z`.
    pub mac: &'static str,
    /// The Windows/Linux key text, for example `Ctrl+Z`.
    pub other: &'static str,
}

impl Shortcut {
    const fn new(
        id: &'static str,
        label: &'static str,
        group: &'static str,
        mac: &'static str,
        other: &'static str,
    ) -> Self {
        Self {
            id,
            label,
            group,
            mac,
            other,
        }
    }
}

/// Whether the app runs on macOS (used to choose the key labels).
#[must_use]
pub const fn is_mac() -> bool {
    cfg!(target_os = "macos")
}

/// The key text for the current platform.
#[must_use]
pub fn keys(s: &Shortcut) -> &'static str {
    if is_mac() { s.mac } else { s.other }
}

/// The shortcuts, in cheat-sheet order.
#[must_use]
pub fn table() -> &'static [Shortcut] {
    const TABLE: [Shortcut; 16] = [
        Shortcut::new("palette", "Command palette", "General", "⌘K", "Ctrl+K"),
        Shortcut::new("undo", "Undo", "Editing", "⌘Z", "Ctrl+Z"),
        Shortcut::new("redo", "Redo", "Editing", "⇧⌘Z", "Ctrl+Shift+Z"),
        Shortcut::new(
            "begin-edit",
            "Take the edit lock",
            "Editing",
            "⌘E",
            "Ctrl+E",
        ),
        Shortcut::new(
            "end-edit",
            "Release the edit lock",
            "Editing",
            "⌘E",
            "Ctrl+E",
        ),
        Shortcut::new(
            "new-element",
            "New element on this page",
            "Project",
            "⇧⌘N",
            "Ctrl+Shift+N",
        ),
        Shortcut::new("overview", "Overview", "View", "⌘1", "Ctrl+1"),
        Shortcut::new("assets", "Assets", "View", "⌘2", "Ctrl+2"),
        Shortcut::new("damage", "Damage scenarios", "View", "⌘3", "Ctrl+3"),
        Shortcut::new("threats", "Threat scenarios", "View", "⌘4", "Ctrl+4"),
        Shortcut::new("trees", "Attack trees", "View", "⌘5", "Ctrl+5"),
        Shortcut::new("risks", "Risks and treatment", "View", "⌘6", "Ctrl+6"),
        Shortcut::new("controls", "Security controls", "View", "⌘7", "Ctrl+7"),
        Shortcut::new("goals", "Goals and claims", "View", "⌘8", "Ctrl+8"),
        Shortcut::new("problems", "Problems", "View", "⌘9", "Ctrl+9"),
        Shortcut::new("item", "Item definition", "View", "⌘0", "Ctrl+0"),
    ];
    &TABLE
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
/// page.
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
    for s in table() {
        out.push(PaletteEntry {
            id: format!("cmd:{}", s.id),
            label: s.label.to_owned(),
            group: s.group.to_owned(),
            keys: keys(s).to_owned(),
            name: String::new(),
            target: None,
        });
    }
    out
}
