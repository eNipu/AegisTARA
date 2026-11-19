//! Per-column filters for table tabs. No UI toolkit.

use std::collections::{BTreeMap, BTreeSet};

use crate::table::{Cell, TableRow};
use crate::view::ColumnSpec;

/// Longest filter query, in Unicode scalar values (chars).
pub const MAX_FILTER_CHARS: usize = 256;

/// Filter criteria for a single table column.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ColumnFilter {
    query: String,
    allowed: BTreeSet<String>,
}

impl ColumnFilter {
    /// Substring query for this column.
    #[must_use]
    pub fn query(&self) -> &str {
        &self.query
    }

    /// Exact cell text values allowed when non-empty.
    #[must_use]
    pub fn allowed(&self) -> &BTreeSet<String> {
        &self.allowed
    }

    /// Whether neither a query nor an allowed value set is active.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.query.is_empty() && self.allowed.is_empty()
    }

    /// Replaces the substring query, capped at [`MAX_FILTER_CHARS`] chars.
    pub fn set_query(&mut self, text: impl Into<String>) {
        self.query = text.into().chars().take(MAX_FILTER_CHARS).collect();
    }

    /// Toggles `value` in the allowed set.
    pub fn toggle_value(&mut self, value: String) {
        if !self.allowed.remove(&value) {
            self.allowed.insert(value);
        }
    }

    /// Whether `cell` matches the allowed set and case-insensitive substring
    /// query against `cell.text` or any chip's `visual_id`/`label` (ASCII & CJK).
    #[must_use]
    pub fn matches(&self, cell: &Cell) -> bool {
        if !self.allowed.is_empty() && !self.allowed.contains(&cell.text) {
            return false;
        }
        if self.query.is_empty() {
            return true;
        }
        let needle = self.query.to_lowercase();
        cell.text.to_lowercase().contains(&needle)
            || cell.chips.iter().any(|c| {
                c.visual_id.to_lowercase().contains(&needle)
                    || c.label.to_lowercase().contains(&needle)
            })
    }
}

/// Active per-column filters for one table, keyed by [`ColumnSpec::key`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TableFilters {
    columns: BTreeMap<String, ColumnFilter>,
}

impl TableFilters {
    /// Sets the query for `col_key`, removing the entry when empty.
    pub fn set_query(&mut self, col_key: &str, query: impl Into<String>) {
        let f = self.columns.entry(col_key.to_owned()).or_default();
        f.set_query(query);
        if f.is_empty() {
            self.columns.remove(col_key);
        }
    }

    /// Toggles `value` in `col_key`'s allowed set, removing the entry when empty.
    pub fn toggle_value(&mut self, col_key: &str, value: String) {
        let f = self.columns.entry(col_key.to_owned()).or_default();
        f.toggle_value(value);
        if f.is_empty() {
            self.columns.remove(col_key);
        }
    }

    /// Removes any filter on `col_key`.
    pub fn clear_column(&mut self, col_key: &str) {
        self.columns.remove(col_key);
    }

    /// Removes all column filters.
    pub fn clear_all(&mut self) {
        self.columns.clear();
    }

    /// The active filter for `col_key`, if any.
    #[must_use]
    pub fn column(&self, col_key: &str) -> Option<&ColumnFilter> {
        self.columns.get(col_key)
    }

    /// Whether at least one column filter is active.
    #[must_use]
    pub fn is_active(&self) -> bool {
        !self.columns.is_empty()
    }

    /// Sorted keys of all columns with an active filter.
    #[must_use]
    pub fn active_keys(&self) -> Vec<&str> {
        self.columns.keys().map(String::as_str).collect()
    }

    /// Sorted, deduplicated cell text values at `col_ix` across `rows`.
    /// Time O(rows log rows).
    #[must_use]
    pub fn distinct_values(rows: &[TableRow], col_ix: usize) -> Vec<String> {
        rows.iter()
            .filter_map(|r| r.cells.get(col_ix).map(|c| c.text.as_str()))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .map(str::to_owned)
            .collect()
    }

    /// Row indices matching every active column filter (AND across columns).
    /// Time O(rows x active_filters).
    #[must_use]
    pub fn matching_rows(&self, rows: &[TableRow], columns: &[ColumnSpec]) -> Vec<usize> {
        let active: Vec<_> = columns
            .iter()
            .enumerate()
            .filter_map(|(i, c)| self.columns.get(c.key).map(|f| (i, f)))
            .collect();
        rows.iter()
            .enumerate()
            .filter(|(_, r)| {
                active
                    .iter()
                    .all(|&(i, f)| r.cells.get(i).is_some_and(|c| f.matches(c)))
            })
            .map(|(i, _)| i)
            .collect()
    }
}
