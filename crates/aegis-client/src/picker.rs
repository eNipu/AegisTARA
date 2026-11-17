//! Client-level multi-link picker state (UI-15): the toolkit-independent state
//! of the picker opened from a relation registry row. One option per candidate
//! of the row's target kind, a chip list of linked options, an IME-capable
//! filter and a bounded logical window. The GPUI workbench renders it later,
//! so this module touches no UI file. Data level only.

use std::collections::HashSet;

use aegis_model::Project;

use crate::commands::Edit;
use crate::relations::{self, ElementKind, RelationKey};
use crate::view::Selection;

/// Longest filter text, in chars (not bytes).
pub const MAX_PICKER_FILTER_CHARS: usize = 256;

/// Keep at most `cap` chars of `text`, truncating on a char boundary.
fn truncate_chars(text: &mut String, cap: usize) {
    let keep = text
        .char_indices()
        .nth(cap)
        .map_or(text.len(), |(byte, _)| byte);
    text.truncate(keep);
}

/// IME-capable filter: committed text plus an in-progress composition
/// (preedit). The composition never affects a match until [`commit`](Self::commit)
/// moves it into the committed text.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PickerFilter {
    committed: String,
    composition: String,
}

impl PickerFilter {
    /// The committed text as typed.
    #[must_use]
    pub fn committed(&self) -> &str {
        &self.committed
    }

    /// The current preedit text.
    #[must_use]
    pub fn composition(&self) -> &str {
        &self.composition
    }

    /// Replaces the committed text, capped to [`MAX_PICKER_FILTER_CHARS`]
    /// chars. The lowercased form is stored lazily by [`matches`](Self::matches).
    /// Returns `true` when the stored text changed.
    pub fn set_committed(&mut self, text: impl Into<String>) -> bool {
        let mut text = text.into();
        truncate_chars(&mut text, MAX_PICKER_FILTER_CHARS);
        if text == self.committed {
            false
        } else {
            self.committed = text;
            true
        }
    }

    /// Replaces the preedit text, capped to [`MAX_PICKER_FILTER_CHARS`] chars.
    pub fn set_composition(&mut self, text: impl Into<String>) {
        let mut text = text.into();
        truncate_chars(&mut text, MAX_PICKER_FILTER_CHARS);
        self.composition = text;
    }

    /// Appends the preedit to the committed text, clears the preedit and
    /// applies the char cap. Returns `true` when the committed text changed.
    pub fn commit(&mut self) -> bool {
        let mut next = self.committed.clone();
        next.push_str(&self.composition);
        truncate_chars(&mut next, MAX_PICKER_FILTER_CHARS);
        self.composition.clear();
        if next == self.committed {
            false
        } else {
            self.committed = next;
            true
        }
    }

    /// Whether an option with this visual ID and label matches the committed
    /// text. Lowercases the committed needle once; an empty needle matches all.
    /// The preedit is ignored until it is committed. Plain-text matching is
    /// case-insensitive and works for any script, including CJK.
    #[must_use]
    pub fn matches(&self, visual_id: &str, label: &str) -> bool {
        let needle = self.committed.to_lowercase();
        if needle.is_empty() {
            return true;
        }
        visual_id.to_lowercase().contains(&needle) || label.to_lowercase().contains(&needle)
    }
}

/// One candidate of a relation row's target kind.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PickerOption {
    /// Identity of the candidate element.
    pub selection: Selection,
    /// Human-readable element ID (visual ID).
    pub visual_id: String,
    /// Element name, used as the display label.
    pub label: String,
    /// Whether the source currently links to this candidate.
    pub linked: bool,
}

/// The multi-link picker for one source element on one relation row.
pub struct MultiLinkPicker {
    key: RelationKey,
    from: Selection,
    options: Vec<PickerOption>,
    filter: PickerFilter,
    visible: Vec<usize>,
    cursor: usize,
    open: bool,
}

impl MultiLinkPicker {
    /// Opens a picker for `from` on the row `key`. Every candidate of the row's
    /// target kind becomes an option; those the source already links to are
    /// marked linked. An unknown key or a wrong-kind `from` yields no options
    /// and never panics. Time O(elements + options), space O(options).
    #[must_use]
    pub fn open(project: &Project, key: RelationKey, from: Selection) -> Self {
        let mut picker = Self {
            key,
            from,
            options: Vec::new(),
            filter: PickerFilter::default(),
            visible: Vec::new(),
            cursor: 0,
            open: true,
        };
        picker.refresh(project);
        picker
    }

    /// Re-reads the options and their `linked` flags after an edit, keeping
    /// the filter and the cursor (clamped), so the picker stays open across
    /// several picks. Time O(candidates).
    pub fn refresh(&mut self, project: &Project) {
        let (key, from) = (self.key, self.from);
        self.options = match relations::relation(key) {
            Some(row) if ElementKind::kind_of(from) == Some(row.from) => {
                let linked: HashSet<Selection> =
                    relations::related(project, key, from).into_iter().collect();
                relations::candidates(project, key)
                    .into_iter()
                    .map(|selection| {
                        let (visual_id, label) = Self::option_text(project, selection);
                        PickerOption {
                            selection,
                            visual_id,
                            label,
                            linked: linked.contains(&selection),
                        }
                    })
                    .collect()
            }
            _ => Vec::new(),
        };
        let cursor = self.cursor;
        self.refilter();
        self.set_cursor(cursor);
    }

    /// Whether the picker is open.
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Closes the picker, clearing the preedit but keeping the committed filter
    /// and the options.
    pub fn close(&mut self) {
        self.open = false;
        self.filter.set_composition("");
    }

    /// The current filter.
    #[must_use]
    pub fn filter(&self) -> &PickerFilter {
        &self.filter
    }

    /// Replaces the committed filter and refilters the options.
    pub fn set_filter(&mut self, text: impl Into<String>) {
        self.filter.set_committed(text);
        self.refilter();
    }

    /// Replaces the preedit text. This does not refilter until it is committed.
    pub fn set_composition(&mut self, text: impl Into<String>) {
        self.filter.set_composition(text);
    }

    /// Commits the preedit and refilters the options.
    pub fn commit_filter(&mut self) {
        self.filter.commit();
        self.refilter();
    }

    /// Rebuilds the visible option indices, in option order, from those that
    /// match the filter, and resets the cursor. O(options).
    fn refilter(&mut self) {
        let visible: Vec<usize> = self
            .options
            .iter()
            .enumerate()
            .filter(|(_, option)| self.filter.matches(&option.visual_id, &option.label))
            .map(|(index, _)| index)
            .collect();
        self.visible = visible;
        self.cursor = 0;
    }

    /// All options, in candidate order.
    #[must_use]
    pub fn options(&self) -> &[PickerOption] {
        &self.options
    }

    /// Indices (into [`options`](Self::options)) currently visible, in order.
    #[must_use]
    pub fn visible(&self) -> &[usize] {
        &self.visible
    }

    /// Position of the cursor within [`visible`](Self::visible).
    #[must_use]
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Places the cursor at a visible position, clamped to the last entry.
    pub fn set_cursor(&mut self, index: usize) {
        self.cursor = index.min(self.visible.len().saturating_sub(1));
    }

    /// Moves the cursor by `delta`, clamped into `0..visible.len()`.
    pub fn move_cursor(&mut self, delta: isize) {
        let max = self.visible.len().saturating_sub(1) as isize;
        let next = (self.cursor as isize).saturating_add(delta);
        self.cursor = next.clamp(0, max) as usize;
    }

    /// The active (cursor) option, or `None` when nothing is visible.
    #[must_use]
    pub fn active(&self) -> Option<&PickerOption> {
        let index = *self.visible.get(self.cursor)?;
        self.options.get(index)
    }

    /// Linked options, in candidate order: the chip list.
    #[must_use]
    pub fn chips(&self) -> Vec<&PickerOption> {
        self.options.iter().filter(|option| option.linked).collect()
    }

    /// The Space toggle for the active option: a single link edit when it is
    /// unlinked, an unlink edit when it is linked. `None` when there is no
    /// active option or the row has no link function.
    #[must_use]
    pub fn toggle_active(&self) -> Option<Edit> {
        let option = self.active()?;
        let row = relations::relation(self.key)?;
        let edit = if option.linked { row.unlink } else { row.link }?;
        edit(self.from, option.selection)
    }

    /// Moves the cursor to a visible position and toggles that option.
    pub fn toggle_visible(&mut self, index: usize) -> Option<Edit> {
        self.set_cursor(index);
        self.toggle_active()
    }

    /// The unlink edit for a chip, or `None` when that chip is not linked or
    /// the row has no unlink function.
    #[must_use]
    pub fn remove_chip(&self, chip_index: usize) -> Option<Edit> {
        let option = *self.chips().get(chip_index)?;
        if !option.linked {
            return None;
        }
        let row = relations::relation(self.key)?;
        (row.unlink?)(self.from, option.selection)
    }

    /// A contiguous slice of [`visible`](Self::visible) of at most `capacity`
    /// entries containing the cursor, centred and clamped to the ends. A zero
    /// capacity gives an empty slice. This is only a bounded logical window;
    /// pixel size belongs to the GPUI widget. Never panics.
    #[must_use]
    pub fn window(&self, capacity: usize) -> &[usize] {
        let len = self.visible.len();
        if capacity == 0 || len == 0 {
            return &[];
        }
        let cap = capacity.min(len);
        let cursor = self.cursor.min(len - 1);
        let start = cursor.saturating_sub(cap / 2).min(len - cap);
        &self.visible[start..start + cap]
    }

    /// The `(visual_id, name)` of a selection. Stale indices yield empty strings
    /// via `unwrap_or_default()`; this never panics.
    fn option_text(project: &Project, selection: Selection) -> (String, String) {
        macro_rules! named {
            ($idx:expr, $visual:expr, $get:expr, $name:expr) => {{
                let idx = $idx;
                let visual_id = idx.and_then($visual).unwrap_or_default();
                let label = idx.and_then($get).map($name).unwrap_or_default();
                (visual_id, label)
            }};
        }
        match selection {
            Selection::Asset(id) => named!(
                project.asset_idx(&id),
                |i| project.asset_visual_id(i),
                |i| project.asset(i),
                |e| e.name().to_owned()
            ),
            Selection::Damage(id) => named!(
                project.damage_scenario_idx(&id),
                |i| project.damage_scenario_visual_id(i),
                |i| project.damage_scenario(i),
                |e| e.name().to_owned()
            ),
            Selection::Threat(id) => named!(
                project.threat_scenario_idx(&id),
                |i| project.threat_scenario_visual_id(i),
                |i| project.threat_scenario(i),
                |e| e.name().to_owned()
            ),
            Selection::Tree(id) => named!(
                project.attack_tree_idx(&id),
                |i| project.attack_tree_visual_id(i),
                |i| project.attack_tree(i),
                |t| t
                    .get(t.root())
                    .map_or_else(String::new, |n| n.label().to_owned())
            ),
            Selection::Control(id) => named!(
                project.security_control_idx(&id),
                |i| project.security_control_visual_id(i),
                |i| project.security_control(i),
                |e| e.name().to_owned()
            ),
            Selection::Goal(id) => named!(
                project.cybersecurity_goal_idx(&id),
                |i| project.cybersecurity_goal_visual_id(i),
                |i| project.cybersecurity_goal(i),
                |e| e.name().to_owned()
            ),
            Selection::Claim(id) => named!(
                project.cybersecurity_claim_idx(&id),
                |i| project.cybersecurity_claim_visual_id(i),
                |i| project.cybersecurity_claim(i),
                |e| e.name().to_owned()
            ),
            Selection::Party(id) => named!(
                project.party_idx(&id),
                |i| project.party_visual_id(i),
                |i| project.party(i),
                |e| e.name().to_owned()
            ),
            Selection::Condition(id) => named!(
                project.condition_idx(&id),
                |i| project.condition_visual_id(i),
                |i| project.condition(i),
                |e| e.name().to_owned()
            ),
            Selection::Component(id) => named!(
                project.component_idx(&id),
                |i| project.component_visual_id(i),
                |i| project.component(i),
                |e| e.name().to_owned()
            ),
            Selection::Interface(id) => named!(
                project.interface_idx(&id),
                |i| project.interface_visual_id(i),
                |i| project.interface(i),
                |e| e.name().to_owned()
            ),
            Selection::DataFlow(id) => named!(
                project.data_flow_idx(&id),
                |i| project.data_flow_visual_id(i),
                |i| project.data_flow(i),
                |e| e.name().to_owned()
            ),
            Selection::TrustBoundary(id) => named!(
                project.trust_boundary_idx(&id),
                |i| project.trust_boundary_visual_id(i),
                |i| project.trust_boundary(i),
                |e| e.name().to_owned()
            ),
            Selection::TreeNode(tree_id, node_id) => {
                let Some(tree_idx) = project.attack_tree_idx(&tree_id) else {
                    return (String::new(), String::new());
                };
                let visual_id = project.attack_tree_visual_id(tree_idx).unwrap_or_default();
                let label = project
                    .attack_tree(tree_idx)
                    .and_then(|tree| {
                        tree.idx_of(&node_id)
                            .and_then(|node_idx| tree.get(node_idx))
                            .map(|node| node.label().to_owned())
                    })
                    .unwrap_or_default();
                (visual_id, label)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn committed_is_capped_in_chars() {
        let mut f = PickerFilter::default();
        let text = "a".repeat(300);
        assert!(f.set_committed(text.clone()));
        assert_eq!(f.committed().chars().count(), MAX_PICKER_FILTER_CHARS);
        assert!(!f.set_committed(text));
    }

    #[test]
    fn composition_is_capped_in_chars() {
        let mut f = PickerFilter::default();
        f.set_composition("界".repeat(300));
        assert_eq!(f.composition().chars().count(), MAX_PICKER_FILTER_CHARS);
    }

    #[test]
    fn empty_filter_matches_all() {
        let f = PickerFilter::default();
        assert!(f.matches("A00", "Brake"));
        assert!(f.matches("", ""));
    }

    #[test]
    fn ascii_match_is_case_insensitive() {
        let mut f = PickerFilter::default();
        f.set_committed("BRAKE");
        assert!(f.matches("C01", "Anti-lock Brake"));
        assert!(f.matches("brake", "something"));
        assert!(!f.matches("C01", "Airbag"));
    }

    #[test]
    fn cjk_needle_matches_cjk_label() {
        let mut f = PickerFilter::default();
        f.set_committed("刹车");
        assert!(f.matches("C01", "防抱死刹车系统"));
        assert!(!f.matches("C01", "安全气囊"));
    }

    #[test]
    fn preedit_does_not_match_until_commit() {
        let mut f = PickerFilter::default();
        f.set_composition("airbag");
        // No committed text: everything matches, the preedit is ignored.
        assert!(f.matches("C01", "Brake"), "preedit must not filter");
        assert!(f.commit());
        assert_eq!(f.committed(), "airbag");
        assert_eq!(f.composition(), "");
        // After commit the now-committed text actually filters.
        assert!(!f.matches("C01", "Brake"));
        assert!(f.matches("C01", "Airbag"));
    }

    #[test]
    fn commit_appends_preedit_to_committed() {
        let mut f = PickerFilter::default();
        f.set_committed("brake");
        f.set_composition(" pad");
        assert!(f.commit());
        assert_eq!(f.committed(), "brake pad");
        assert_eq!(f.composition(), "");
    }
}
