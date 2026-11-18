//! Command palette entries and search (WBX-04).
//!
//! The palette lists the workbench commands and every project element, and
//! finds any of them by visual ID, name or catalog ID. No UI toolkit here.

use std::collections::HashMap;

use aegis_model::ThreatId;

use crate::command_table::{commands, shortcut_label};
use crate::i18n::Catalog;
use crate::pages::palette_elements;
use crate::view::Selection;
use crate::workspace::Workspace;

/// What picking a palette entry does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PaletteTarget {
    /// Runs the command with this id (see [`crate::command_table`]).
    Command(&'static str),
    /// Selects this element.
    Element(Selection),
}

/// One row of the palette.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaletteEntry {
    /// Command label or element name.
    pub title: String,
    /// Command group or element kind.
    pub detail: String,
    /// Element visual ID (`A00`, `T01`); empty for a command.
    pub visual_id: String,
    /// Catalog IDs of a threat scenario as `"<scheme label> <ref>"`.
    pub catalog: Vec<String>,
    /// Shortcut label of a command; empty if none.
    pub keys: String,
    /// What picking the entry does.
    pub target: PaletteTarget,
}

/// The palette entries: the palette commands in table order, then every
/// element of the project (if one is open). Time O(commands + project size).
#[must_use]
pub fn entries(ws: Option<&Workspace>, catalog: &Catalog, mac: bool) -> Vec<PaletteEntry> {
    let mut out: Vec<PaletteEntry> = commands()
        .iter()
        .filter(|c| c.in_palette)
        .map(|c| PaletteEntry {
            title: catalog.tr(&c.label_id()),
            detail: c.group.name().to_owned(),
            visual_id: String::new(),
            catalog: Vec::new(),
            keys: c
                .keys_for(mac)
                .map(|k| shortcut_label(&k, mac))
                .unwrap_or_default(),
            target: PaletteTarget::Command(c.id),
        })
        .collect();
    let Some(ws) = ws else {
        return out;
    };
    let p = ws.project();
    let m = p.methodology();
    let refs: HashMap<ThreatId, Vec<String>> = p
        .threat_scenarios()
        .map(|(_, id, t)| {
            let texts = t
                .catalog_refs()
                .iter()
                .map(|r| {
                    format!(
                        "{} {}",
                        m.catalog_scheme_label(r.scheme()).unwrap_or("?"),
                        m.display_catalog_ref(r)
                    )
                })
                .collect();
            (id, texts)
        })
        .collect();
    out.extend(palette_elements(ws).into_iter().map(|e| {
        let catalog = match e.target {
            Selection::Threat(id) => refs.get(&id).cloned().unwrap_or_default(),
            _ => Vec::new(),
        };
        PaletteEntry {
            title: e.name,
            detail: e.kind,
            visual_id: e.visual_id,
            catalog,
            keys: String::new(),
            target: PaletteTarget::Element(e.target),
        }
    }));
    out
}

/// The best (lowest) match tier of `entry` for a trimmed, lower-cased
/// query, or `None` when it does not match at all.
fn tier(entry: &PaletteEntry, q: &str) -> Option<u8> {
    let vid = entry.visual_id.to_lowercase();
    if vid == q {
        return Some(0);
    }
    if vid.starts_with(q) {
        return Some(1);
    }
    let catalog_hit = entry.catalog.iter().any(|c| {
        let c = c.to_lowercase();
        let ref_part = c.rsplit(' ').next().unwrap_or("");
        ref_part.starts_with(q) || c.starts_with(q)
    });
    if catalog_hit {
        return Some(2);
    }
    let title = entry.title.to_lowercase();
    if title.starts_with(q) {
        return Some(3);
    }
    if title
        .split(|ch: char| ch.is_whitespace() || ch == '-')
        .any(|w| w.starts_with(q))
    {
        return Some(4);
    }
    if title.contains(q) || entry.detail.to_lowercase().contains(q) || vid.contains(q) {
        return Some(5);
    }
    None
}

/// Finds entries for `query`, best match first, at most `limit`.
///
/// Tiers (lowest wins): 0 visual ID equals the query; 1 visual ID starts
/// with it; 2 a catalog ID's ref or whole text starts with it; 3 title starts
/// with it; 4 a title word starts with it; 5 title, detail or visual ID
/// contains it. Ties keep input order. An empty query returns the first
/// `limit` entries. Time O(entries x query length) plus the sort.
#[must_use]
pub fn search<'a>(entries: &'a [PaletteEntry], query: &str, limit: usize) -> Vec<&'a PaletteEntry> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return entries.iter().take(limit).collect();
    }
    let mut hits: Vec<(u8, &PaletteEntry)> = entries
        .iter()
        .filter_map(|e| tier(e, &q).map(|t| (t, e)))
        .collect();
    hits.sort_by_key(|(t, _)| *t);
    hits.into_iter().take(limit).map(|(_, e)| e).collect()
}
