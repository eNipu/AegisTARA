//! Library packs: reusable attack tree items (job 057, LIB-01 and LIB-02).
//!
//! A pack is one JSON file: a title, a version, a license, the public sources
//! it was written from, a SHA-256 hash of its items, and the items. An item is
//! a leaf step, a fragment (a gate with children), a complete tree or a
//! template, with names, tags, facets (STRIDE category, interface, asset type,
//! attack type, catalog IDs) and the tree itself. Items carry no ratings of
//! their own authority: a leaf may suggest a feasibility level by key, and
//! an insert marks it "needs review" (see `Op::SetReviewMark`).
//!
//! # Caps (R3)
//!
//! A file is read up to [`MAX_LIBRARY_PACK_BYTES`]; a pack has at most
//! [`MAX_LIBRARY_ITEMS`] items; an item has at most
//! [`MAX_LIBRARY_ITEM_NODES`] nodes, [`MAX_LIBRARY_ITEM_DEPTH`] levels deep,
//! and strings and lists of bounded size. Unknown fields are refused.
//! The hash covers the items, so a pack that was edited by hand no longer
//! matches; it detects accident and tampering, it is not a signature.
//!
//! # Search
//!
//! [`LibraryIndex`] matches character bigrams and trigrams of the query with
//! those of the item's name, aliases, tags, facet values, notes and node
//! labels, so typos and partial words match and Japanese works without a word
//! splitter. Text is lower-cased, full-width letters become half-width and
//! katakana becomes hiragana before it is cut into grams. Results are ordered
//! by score, then by position in the pack, so a search is deterministic (P4).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::Read;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::tree::GateKind;

/// Largest pack file read, in bytes.
pub const MAX_LIBRARY_PACK_BYTES: usize = 4 * 1024 * 1024;
/// Most items in one pack.
pub const MAX_LIBRARY_ITEMS: usize = 5_000;
/// Most nodes in one item, root included. An insert is one batch of at most
/// 1,024 ops, so an item must fit in one.
pub const MAX_LIBRARY_ITEM_NODES: usize = 256;
/// Deepest item tree, the root counting as level 1.
pub const MAX_LIBRARY_ITEM_DEPTH: usize = 32;
const MAX_NAME_CHARS: usize = 256;
const MAX_ID_CHARS: usize = 64;
const MAX_TAGS: usize = 32;
const MAX_ALIASES: usize = 16;
const MAX_FACET_VALUES: usize = 32;
const MAX_SHORT_CHARS: usize = 64;
const MAX_NOTES_CHARS: usize = 4_000;
const MAX_SOURCES_CHARS: usize = 2_000;

/// Why a pack was not accepted. Messages carry no pack text.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum LibraryError {
    /// The file is longer than [`MAX_LIBRARY_PACK_BYTES`].
    #[error("the library pack is larger than {MAX_LIBRARY_PACK_BYTES} bytes")]
    TooLarge,
    /// The file could not be read.
    #[error("the library pack could not be read")]
    Io,
    /// The file is not valid JSON of this shape.
    #[error("the library pack is not valid: {0}")]
    Decode(String),
    /// Not an `aegis-library` file, or a version this build does not read.
    #[error("this is not an aegis-library pack of a version this app reads")]
    Unsupported,
    /// A limit was exceeded (named, never quoted).
    #[error("the library pack is over a limit: {0}")]
    Limit(&'static str),
    /// A rule about the content was broken (named, never quoted).
    #[error("the library pack breaks a rule: {0}")]
    Invalid(&'static str),
    /// The items do not match the hash in the file.
    #[error("the items of the library pack do not match its hash")]
    HashMismatch,
}

/// Which kind of thing an item is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
    /// One attack step with defaults.
    Leaf,
    /// A gate with children, to graft under any gate.
    Fragment,
    /// A complete tree.
    Tree,
    /// A tree to be filled in. Phase 0 inserts its text as it is.
    Template,
}

/// A facet an item can be filtered by.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Facet {
    /// STRIDE category.
    Stride,
    /// Interface type (CAN, Ethernet, OBD, debug port, ...).
    Interface,
    /// Asset type.
    Asset,
    /// Attack type.
    Attack,
    /// Catalog IDs (R155 Annex 5, CAPEC, ATT&CK, EMB3D).
    Catalog,
}

/// The facets chosen to narrow a list: an item must have every one.
pub type FacetFilter = BTreeSet<(Facet, String)>;

/// A node of an item's tree.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemNode {
    /// Text of the node.
    pub label: String,
    /// `None` for a step.
    #[serde(default, skip_serializing_if = "Option::is_none", with = "gate_serde")]
    pub gate: Option<GateKind>,
    /// A suggested feasibility level, as the methodology's key (steps only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feasibility: Option<String>,
    /// Children in order (gates only).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<ItemNode>,
}

/// One reusable item.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LibraryItem {
    /// Stable ID within the pack: letters, digits, `-`, `_`, `.`.
    pub id: String,
    /// Leaf, fragment, tree or template.
    pub kind: ItemKind,
    /// Display name.
    pub name: String,
    /// Other names, in any language; searched.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    /// Free tags; searched.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Facet values by facet.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub facets: BTreeMap<Facet, Vec<String>>,
    /// Notes, plain text; searched.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub notes: String,
    /// The tree.
    pub tree: ItemNode,
}

/// What a pack says about itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackMeta {
    /// Pack ID.
    pub id: String,
    /// Title.
    pub title: String,
    /// Version of the pack's content, for the origin note.
    pub pack_version: String,
    /// License of the content (an SPDX ID or a name).
    pub license: String,
    /// The public sources the items were written from.
    pub sources: String,
}

/// A pack: its description, its hash and its items.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibraryPack {
    meta: PackMeta,
    hash: String,
    items: Vec<LibraryItem>,
}

/// The file as written.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PackFile {
    format: String,
    version: u32,
    id: String,
    title: String,
    pack_version: String,
    license: String,
    sources: String,
    hash: String,
    items: Vec<LibraryItem>,
}

const FORMAT: &str = "aegis-library";
const VERSION: u32 = 1;
const BUILTIN: &str = include_str!("../packs/library-demo.json");

mod gate_serde {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    use crate::tree::GateKind;

    #[derive(Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    enum Wire {
        And,
        Or,
        SeqAnd,
    }

    // serde's `with` fixes this signature.
    #[allow(clippy::ref_option, clippy::trivially_copy_pass_by_ref)]
    pub(super) fn serialize<S: Serializer>(
        gate: &Option<GateKind>,
        s: S,
    ) -> Result<S::Ok, S::Error> {
        gate.map(|k| match k {
            GateKind::And => Wire::And,
            GateKind::Or => Wire::Or,
            GateKind::SeqAnd => Wire::SeqAnd,
        })
        .serialize(s)
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(
        d: D,
    ) -> Result<Option<GateKind>, D::Error> {
        Ok(Option::<Wire>::deserialize(d)?.map(|w| match w {
            Wire::And => GateKind::And,
            Wire::Or => GateKind::Or,
            Wire::SeqAnd => GateKind::SeqAnd,
        }))
    }
}

fn is_plain(text: &str) -> bool {
    !text.chars().any(|c| c.is_control() && c != '\n')
}

fn check_text(text: &str, max: usize, what: &'static str) -> Result<(), LibraryError> {
    if !is_plain(text) {
        return Err(LibraryError::Invalid("text has a control character"));
    }
    if text.chars().count() > max {
        return Err(LibraryError::Limit(what));
    }
    Ok(())
}

fn check_items(meta: &PackMeta, items: &[LibraryItem]) -> Result<(), LibraryError> {
    for (text, what) in [
        (&meta.id, "pack id"),
        (&meta.title, "pack title"),
        (&meta.pack_version, "pack version"),
        (&meta.license, "pack license"),
    ] {
        if text.trim().is_empty() {
            return Err(LibraryError::Invalid(match what {
                "pack license" => "the pack names no license",
                _ => "the pack has no id, title or version",
            }));
        }
        check_text(text, MAX_NAME_CHARS, "a pack field is too long")?;
    }
    check_text(
        &meta.sources,
        MAX_SOURCES_CHARS,
        "the sources text is too long",
    )?;
    if items.len() > MAX_LIBRARY_ITEMS {
        return Err(LibraryError::Limit("too many items"));
    }
    let mut ids = BTreeSet::new();
    for it in items {
        if it.id.is_empty()
            || !it
                .id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        {
            return Err(LibraryError::Invalid(
                "an item id is empty or has odd characters",
            ));
        }
        if it.id.len() > MAX_ID_CHARS {
            return Err(LibraryError::Limit("an item id is too long"));
        }
        if !ids.insert(&it.id) {
            return Err(LibraryError::Invalid("two items have the same id"));
        }
        if it.name.trim().is_empty() {
            return Err(LibraryError::Invalid("an item has no name"));
        }
        check_text(&it.name, MAX_NAME_CHARS, "an item name is too long")?;
        if it.aliases.len() > MAX_ALIASES || it.tags.len() > MAX_TAGS {
            return Err(LibraryError::Limit("too many aliases or tags"));
        }
        for a in &it.aliases {
            check_text(a, MAX_NAME_CHARS, "an alias is too long")?;
        }
        for t in &it.tags {
            check_text(t, MAX_SHORT_CHARS, "a tag is too long")?;
        }
        for values in it.facets.values() {
            if values.len() > MAX_FACET_VALUES {
                return Err(LibraryError::Limit("too many facet values"));
            }
            for v in values {
                check_text(v, MAX_SHORT_CHARS, "a facet value is too long")?;
            }
        }
        check_text(&it.notes, MAX_NOTES_CHARS, "an item's notes are too long")?;
        check_tree(it)?;
    }
    Ok(())
}

/// Checks the tree of one item without recursion. Time O(nodes).
fn check_tree(it: &LibraryItem) -> Result<(), LibraryError> {
    let mut stack = vec![(&it.tree, 1usize)];
    let mut count = 0usize;
    while let Some((node, depth)) = stack.pop() {
        count += 1;
        if count > MAX_LIBRARY_ITEM_NODES {
            return Err(LibraryError::Limit("too many nodes in an item"));
        }
        if depth > MAX_LIBRARY_ITEM_DEPTH {
            return Err(LibraryError::Limit("an item's tree is too deep"));
        }
        if node.label.trim().is_empty() {
            return Err(LibraryError::Invalid("a node has no text"));
        }
        check_text(&node.label, MAX_NAME_CHARS, "a node text is too long")?;
        if node.gate.is_none() && !node.children.is_empty() {
            return Err(LibraryError::Invalid("a step has children"));
        }
        if node.gate.is_some() && node.feasibility.is_some() {
            return Err(LibraryError::Invalid("a gate has a rating"));
        }
        if let Some(key) = &node.feasibility {
            check_text(key, MAX_SHORT_CHARS, "a rating key is too long")?;
        }
        stack.extend(node.children.iter().map(|c| (c, depth + 1)));
    }
    if it.kind == ItemKind::Leaf && (it.tree.gate.is_some() || !it.tree.children.is_empty()) {
        return Err(LibraryError::Invalid("a leaf item is more than one step"));
    }
    if matches!(it.kind, ItemKind::Fragment | ItemKind::Tree) && it.tree.gate.is_none() {
        return Err(LibraryError::Invalid(
            "a fragment or tree starts with a gate",
        ));
    }
    Ok(())
}

fn items_hash(items: &[LibraryItem]) -> String {
    use std::fmt::Write as _;
    // Serializing plain structs and maps cannot fail.
    let bytes = serde_json::to_vec(items).unwrap_or_default();
    Sha256::digest(bytes)
        .iter()
        .fold(String::with_capacity(64), |mut out, b| {
            let _ = write!(out, "{b:02x}");
            out
        })
}

impl LibraryPack {
    /// Builds a pack from items, checking them and computing the hash.
    ///
    /// # Errors
    ///
    /// [`LibraryError::Limit`] or [`LibraryError::Invalid`].
    pub fn new(meta: PackMeta, items: Vec<LibraryItem>) -> Result<Self, LibraryError> {
        check_items(&meta, &items)?;
        Ok(Self {
            hash: items_hash(&items),
            meta,
            items,
        })
    }

    /// Reads a pack from at most [`MAX_LIBRARY_PACK_BYTES`] bytes.
    ///
    /// # Errors
    ///
    /// Any [`LibraryError`].
    pub fn from_reader<R: Read>(reader: &mut R) -> Result<Self, LibraryError> {
        let mut bytes = Vec::new();
        reader
            .take(MAX_LIBRARY_PACK_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| LibraryError::Io)?;
        Self::from_bytes(&bytes)
    }

    /// Reads a pack from bytes.
    ///
    /// # Errors
    ///
    /// [`LibraryError::TooLarge`] before anything is decoded, then
    /// [`LibraryError::Decode`] (unknown fields are refused),
    /// [`LibraryError::Unsupported`], [`LibraryError::Limit`],
    /// [`LibraryError::Invalid`] and [`LibraryError::HashMismatch`].
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, LibraryError> {
        if bytes.len() > MAX_LIBRARY_PACK_BYTES {
            return Err(LibraryError::TooLarge);
        }
        let file: PackFile = serde_json::from_slice(bytes).map_err(|e| {
            LibraryError::Decode(format!(
                "{:?} at line {}, column {}",
                e.classify(),
                e.line(),
                e.column()
            ))
        })?;
        if file.format != FORMAT || file.version != VERSION {
            return Err(LibraryError::Unsupported);
        }
        let meta = PackMeta {
            id: file.id,
            title: file.title,
            pack_version: file.pack_version,
            license: file.license,
            sources: file.sources,
        };
        check_items(&meta, &file.items)?;
        if items_hash(&file.items) != file.hash {
            return Err(LibraryError::HashMismatch);
        }
        Ok(Self {
            meta,
            hash: file.hash,
            items: file.items,
        })
    }

    /// For pack authors: reads a pack file whose hash is stale (an item was
    /// edited by hand), checks everything else, and returns the canonical
    /// bytes with the hash recomputed. The hash in the file is ignored.
    ///
    /// # Errors
    ///
    /// As [`from_bytes`](Self::from_bytes), except [`LibraryError::HashMismatch`].
    pub fn rehash(bytes: &[u8]) -> Result<Vec<u8>, LibraryError> {
        match Self::from_bytes(bytes) {
            Ok(pack) => Ok(pack.to_bytes()),
            Err(LibraryError::HashMismatch) => {
                let file: PackFile = serde_json::from_slice(bytes)
                    .map_err(|_| LibraryError::Decode("not a pack".to_owned()))?;
                let meta = PackMeta {
                    id: file.id,
                    title: file.title,
                    pack_version: file.pack_version,
                    license: file.license,
                    sources: file.sources,
                };
                Ok(Self::new(meta, file.items)?.to_bytes())
            }
            Err(e) => Err(e),
        }
    }

    /// The pack as canonical JSON: the same pack always gives the same bytes.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let file = PackFile {
            format: FORMAT.to_owned(),
            version: VERSION,
            id: self.meta.id.clone(),
            title: self.meta.title.clone(),
            pack_version: self.meta.pack_version.clone(),
            license: self.meta.license.clone(),
            sources: self.meta.sources.clone(),
            hash: self.hash.clone(),
            items: self.items.clone(),
        };
        serde_json::to_vec(&file).unwrap_or_default()
    }

    /// The demo pack that ships with the app: a few items written from
    /// public sources (R155 Annex 5 and CAPEC titles), no ISO text.
    ///
    /// # Panics
    ///
    /// Never: a test checks that the file is a valid pack.
    #[must_use]
    pub fn builtin_demo() -> Self {
        Self::from_bytes(BUILTIN.as_bytes()).expect("the built-in library pack is valid")
    }

    /// The items, in pack order.
    #[must_use]
    pub fn items(&self) -> &[LibraryItem] {
        &self.items
    }

    /// SHA-256 of the items, as hex.
    #[must_use]
    pub fn hash(&self) -> &str {
        &self.hash
    }

    /// Pack ID.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.meta.id
    }

    /// Title.
    #[must_use]
    pub fn title(&self) -> &str {
        &self.meta.title
    }

    /// Version of the content.
    #[must_use]
    pub fn pack_version(&self) -> &str {
        &self.meta.pack_version
    }

    /// License of the content.
    #[must_use]
    pub fn license(&self) -> &str {
        &self.meta.license
    }

    /// The public sources the items were written from.
    #[must_use]
    pub fn sources(&self) -> &str {
        &self.meta.sources
    }
}

// ---------------------------------------------------------------------------
// Search.
// ---------------------------------------------------------------------------

/// A search result: the item's position in the pack and its score (0 to
/// 1,000; higher is closer).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hit {
    /// Position in [`LibraryPack::items`].
    pub index: usize,
    /// Closeness of the match.
    pub score: u32,
}

/// Lower-cases, makes full-width ASCII half-width, turns katakana into
/// hiragana, and cuts at everything that is not a letter or digit.
fn tokens(text: &str) -> Vec<Vec<char>> {
    let mut out = vec![Vec::new()];
    for c in text.chars() {
        let c = match c {
            '\u{FF01}'..='\u{FF5E}' => char::from_u32(c as u32 - 0xFEE0).unwrap_or(c),
            '\u{30A1}'..='\u{30F6}' => char::from_u32(c as u32 - 0x60).unwrap_or(c),
            _ => c,
        };
        if c.is_alphanumeric() {
            if let Some(t) = out.last_mut() {
                t.extend(c.to_lowercase());
            }
        } else if out.last().is_some_and(|t| !t.is_empty()) {
            out.push(Vec::new());
        }
    }
    out.retain(|t| !t.is_empty());
    out
}

/// Bigrams and trigrams of every token (a one-character token is its own
/// gram).
fn grams(text: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for t in tokens(text) {
        if t.len() == 1 {
            out.insert(t.iter().collect());
            continue;
        }
        for n in [2usize, 3] {
            for w in t.windows(n) {
                out.insert(w.iter().collect());
            }
        }
    }
    out
}

/// A search index over one pack. Built once, O(pack text).
#[derive(Clone, Debug)]
pub struct LibraryIndex {
    /// Per item: gram to the weight of the best field it appears in.
    weights: Vec<HashMap<String, u32>>,
    facets: Vec<BTreeSet<(Facet, String)>>,
}

impl LibraryIndex {
    /// Indexes the items of `pack`.
    #[must_use]
    pub fn new(pack: &LibraryPack) -> Self {
        let mut weights = Vec::with_capacity(pack.items.len());
        let mut facets = Vec::with_capacity(pack.items.len());
        for it in &pack.items {
            let mut w: HashMap<String, u32> = HashMap::new();
            let mut add = |text: &str, weight: u32| {
                for g in grams(text) {
                    let slot = w.entry(g).or_insert(0);
                    *slot = (*slot).max(weight);
                }
            };
            add(&it.name, 4);
            it.aliases.iter().for_each(|a| add(a, 3));
            it.tags.iter().for_each(|t| add(t, 3));
            it.facets.values().flatten().for_each(|v| add(v, 2));
            add(&it.notes, 1);
            let mut stack = vec![&it.tree];
            while let Some(n) = stack.pop() {
                add(&n.label, 1);
                stack.extend(n.children.iter());
            }
            weights.push(w);
            facets.push(
                it.facets
                    .iter()
                    .flat_map(|(f, vs)| vs.iter().map(move |v| (*f, v.clone())))
                    .collect(),
            );
        }
        Self { weights, facets }
    }

    fn passes(&self, i: usize, filter: &FacetFilter) -> bool {
        filter.iter().all(|f| self.facets[i].contains(f))
    }

    /// Items that match `query` and have every facet in `filter`, closest
    /// first. At least half of the query's grams must be found. An empty
    /// query lists the items that pass the filter in pack order.
    #[must_use]
    pub fn search(&self, query: &str, filter: &FacetFilter) -> Vec<Hit> {
        let q = grams(query);
        let mut hits = Vec::new();
        for i in 0..self.weights.len() {
            if !self.passes(i, filter) {
                continue;
            }
            if q.is_empty() {
                hits.push(Hit { index: i, score: 0 });
                continue;
            }
            let (mut found, mut total) = (0usize, 0u32);
            for g in &q {
                if let Some(w) = self.weights[i].get(g) {
                    found += 1;
                    total += w;
                }
            }
            if found * 2 < q.len() {
                continue;
            }
            let score = total * 1000 / (4 * u32::try_from(q.len()).unwrap_or(u32::MAX));
            hits.push(Hit { index: i, score });
        }
        // Stable: equal scores keep pack order.
        hits.sort_by_key(|h| std::cmp::Reverse(h.score));
        hits
    }

    /// The facet values of the items that pass `filter`, with how many items
    /// have each, in facet and value order.
    #[must_use]
    pub fn facet_counts(&self, filter: &FacetFilter) -> Vec<(Facet, String, usize)> {
        let mut counts: BTreeMap<(Facet, String), usize> = BTreeMap::new();
        for i in 0..self.facets.len() {
            if self.passes(i, filter) {
                for f in &self.facets[i] {
                    *counts.entry(f.clone()).or_default() += 1;
                }
            }
        }
        counts.into_iter().map(|((f, v), n)| (f, v, n)).collect()
    }
}
