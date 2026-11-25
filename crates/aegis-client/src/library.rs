//! Putting a library item into the project (job 057, LIB-03 and LIB-04).
//!
//! An item is copied: as a new tree, under a gate of a tree, or in place of a
//! step. The whole insert is one batch, so one undo takes it back. Ratings
//! come along only when the user asks, and each one is marked for review. A
//! suggested attack type is filled in on the copy and counted in the origin note.
//! A name that the tree already has is never merged silently: the copy gets
//! its own text ("name (2)") unless the user ticked the name to be shared.

use std::collections::BTreeSet;

use aegis_model::catalog_pack::CatalogPack;
use aegis_model::library::{ItemNode, LibraryItem, LibraryPack};
use aegis_model::{
    ElementRef, MAX_ATTACK_TREE_NODES, MAX_LABEL_BYTES, Node, NodeId, Op, Project, Provenance,
    StepTarget, ThreatId, TreeId,
};

use crate::commands::EditError;

/// Where the copy goes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InsertMode {
    /// A tree of its own, optionally linked to a threat.
    NewTree {
        /// New ID.
        id: TreeId,
        /// The threat to link it to.
        threat: Option<ThreatId>,
    },
    /// A child of a gate, after its other children.
    UnderGate {
        /// The tree.
        tree: TreeId,
        /// The gate.
        gate: NodeId,
    },
    /// Instead of one step, at its place.
    InPlaceOfLeaf {
        /// The tree.
        tree: TreeId,
        /// The step.
        leaf: NodeId,
    },
}

/// What the user chose besides the place.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InsertOptions {
    /// Bring the suggested ratings along (each is marked for review).
    pub with_ratings: bool,
    /// Texts of steps to share with the step the tree already has, instead
    /// of copying them.
    pub share: BTreeSet<String>,
    /// Share every step the tree already has (the panel's one tick).
    pub share_all: bool,
}

fn err(text: &str) -> EditError {
    EditError::Library(text.to_owned())
}

/// One built-in catalog pack, flattened for the library panel (job 112,
/// DOM-04). Catalog packs are metadata only: they are searchable and shown,
/// never inserted into a project.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CatalogView {
    /// The catalog key (for example `capec`).
    pub key: String,
    /// The original catalog name.
    pub name: String,
    /// The original short description.
    pub description: String,
    /// Official homepage.
    pub source_url: String,
    /// Official license-terms page.
    pub terms_url: String,
    /// The required attribution text.
    pub attribution: String,
    /// The license grant, kept for review.
    pub license_terms: String,
}

impl CatalogView {
    fn from_pack(pack: &CatalogPack) -> Self {
        let c = pack.catalog();
        CatalogView {
            key: pack.id().to_owned(),
            name: c.name().to_owned(),
            description: c.description().to_owned(),
            source_url: c.source_url().to_owned(),
            terms_url: c.terms_url().to_owned(),
            attribution: c.attribution().to_owned(),
            license_terms: c.license_terms().to_owned(),
        }
    }

    /// The text shown in the preview, with links and attribution.
    pub fn preview(&self) -> String {
        let mut out = String::new();
        out.push_str(&self.name);
        out.push('\n');
        out.push_str(&self.description);
        out.push('\n');
        out.push_str(&format!("Source: {}", self.source_url));
        out.push('\n');
        out.push_str(&format!("Terms: {}", self.terms_url));
        out.push('\n');
        out.push_str(&format!("Attribution: {}", self.attribution));
        out.push('\n');
        out.push_str(&format!("License: {}", self.license_terms));
        out
    }
}

/// The three built-in catalog packs, in a stable order (job 112).
pub fn builtin_catalogs() -> Vec<CatalogView> {
    [
        CatalogPack::builtin_capec(),
        CatalogPack::builtin_mitre_attack(),
        CatalogPack::builtin_emb3d(),
    ]
    .iter()
    .map(CatalogView::from_pack)
    .collect()
}

/// The texts of the item that the tree it goes into already has, in item
/// order. The UI offers these as "share instead of copy" ticks.
#[must_use]
pub fn matching_labels(project: &Project, mode: &InsertMode, item: &LibraryItem) -> Vec<String> {
    let (tree, skip) = match mode {
        InsertMode::NewTree { .. } => return Vec::new(),
        InsertMode::UnderGate { tree, .. } => (tree, None),
        InsertMode::InPlaceOfLeaf { tree, leaf } => (tree, Some(*leaf)),
    };
    let Some(t) = project
        .attack_tree_idx(tree)
        .and_then(|i| project.attack_tree(i))
    else {
        return Vec::new();
    };
    let have: BTreeSet<&str> = t
        .iter()
        .filter(|(_, id, _)| Some(*id) != skip)
        .map(|(_, _, n)| n.label())
        .collect();
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    let mut stack = vec![&item.tree];
    while let Some(n) = stack.pop() {
        if have.contains(n.label.as_str()) && seen.insert(n.label.as_str()) {
            out.push(n.label.clone());
        }
        stack.extend(n.children.iter().rev());
    }
    out
}

struct Builder<'a> {
    project: &'a Project,
    tree: TreeId,
    options: &'a InsertOptions,
    prov: Provenance,
    ops: Vec<Op>,
    /// Texts in use, in the tree and among the copies.
    used: BTreeSet<String>,
    /// Existing steps by text that may be shared, with the number of extra
    /// parents each has, counting the links made so far.
    shareable: std::collections::BTreeMap<String, (NodeId, u32)>,
    /// Arena size as the ops build it up.
    count: u32,
    /// Steps whose attack type was filled in.
    typed: usize,
}

impl Builder<'_> {
    fn unique(&mut self, label: &str) -> Result<String, EditError> {
        let mut text = label.to_owned();
        let mut n = 2;
        while self.used.contains(&text) {
            text = format!("{label} ({n})");
            n += 1;
        }
        if text.len() > MAX_LABEL_BYTES {
            return Err(err("a name of the item is too long once it is made unique"));
        }
        self.used.insert(text.clone());
        Ok(text)
    }

    fn rating(&mut self, node: &ItemNode, id: NodeId) -> Result<(), EditError> {
        let (true, Some(key)) = (self.options.with_ratings, &node.feasibility) else {
            return Ok(());
        };
        let level = self
            .project
            .methodology()
            .feasibility_level(key)
            .ok_or_else(|| {
                err("the item rates a step with a level this methodology does not have")
            })?;
        self.ops.push(Op::SetStepFeasibility {
            target: StepTarget::Step(self.tree, id),
            level: Some(level),
            provenance: self.prov.clone(),
        });
        self.ops.push(Op::SetReviewMark {
            tree: self.tree,
            node: id,
            marked: true,
            provenance: self.prov.clone(),
        });
        Ok(())
    }

    /// Fills in the suggested attack type when the methodology has the key.
    fn attack_type(&mut self, node: &ItemNode, id: NodeId) {
        let Some(key) = &node.attack_type else {
            return;
        };
        if let Some(t) = self.project.methodology().attack_type(key) {
            self.ops.push(Op::SetStepAttackType {
                target: StepTarget::Step(self.tree, id),
                attack_type: Some(t),
                provenance: self.prov.clone(),
            });
            self.typed += 1;
        }
    }

    /// Adds `node` and everything below it, parents before children.
    fn add(&mut self, node: &ItemNode, parent: NodeId, position: u32) -> Result<(), EditError> {
        let mut stack = vec![(node, parent, position)];
        while let Some((n, parent, position)) = stack.pop() {
            if n.children.is_empty()
                && n.gate.is_none()
                && let Some((existing, extra)) = self
                    .shareable
                    .get_mut(&n.label)
                    .filter(|_| self.options.share_all || self.options.share.contains(&n.label))
            {
                self.ops.push(Op::LinkTreeNode {
                    tree: self.tree,
                    parent,
                    child: *existing,
                    position,
                    slot: *extra,
                    provenance: self.prov.clone(),
                });
                *extra += 1;
                continue;
            }
            let label = self.unique(&n.label)?;
            let id = NodeId::new_v7();
            let node_value = match n.gate {
                Some(kind) => Node::gate(kind, label.as_str()),
                None => Node::step(label.as_str()),
            };
            self.ops.push(Op::AddTreeNode {
                tree: self.tree,
                parent,
                id,
                node: node_value,
                index: self.count,
                position,
                provenance: self.prov.clone(),
            });
            self.count += 1;
            self.rating(n, id)?;
            self.attack_type(n, id);
            // Children are pushed backwards so they pop in order.
            for (i, c) in n.children.iter().enumerate().rev() {
                let p = u32::try_from(i).map_err(|_| err("too many children"))?;
                stack.push((c, id, p));
            }
        }
        Ok(())
    }
}

fn origin_note(pack: &LibraryPack, item: &LibraryItem, typed: usize) -> String {
    let mut note = format!(
        "Copied from library \"{}\" ({} {}, item {}), license {}.",
        pack.title(),
        pack.id(),
        pack.pack_version(),
        item.id,
        pack.license()
    );
    if typed >= 1 {
        note.push_str(&format!(
            " Suggested attack types were filled in on {typed} step(s); check them in the step inspector."
        ));
    }
    note
}

/// Builds the ops of one insert. The author `actor` and time go into the
/// provenance of every op; the tree and its steps are marked as from a
/// catalog.
///
/// # Errors
///
/// [`EditError`] if the place is wrong (gone, a step instead of a gate, the
/// root or a shared step to replace), the tree would grow past the cap, or
/// the item does not fit this methodology.
pub fn insert_ops(
    project: &Project,
    pack: &LibraryPack,
    item: usize,
    mode: &InsertMode,
    options: &InsertOptions,
    actor: &str,
    now_ms: u64,
) -> Result<Vec<Op>, EditError> {
    let item = pack.items().get(item).ok_or(EditError::Missing)?;
    let prov = Provenance::catalog(actor, now_ms);
    let human = Provenance::human(actor, now_ms);
    let nodes = count_nodes(&item.tree);
    let mut b = Builder {
        project,
        tree: match mode {
            InsertMode::NewTree { id, .. } => *id,
            InsertMode::UnderGate { tree, .. } | InsertMode::InPlaceOfLeaf { tree, .. } => *tree,
        },
        options,
        prov: prov.clone(),
        ops: Vec::new(),
        used: BTreeSet::new(),
        shareable: std::collections::BTreeMap::new(),
        count: 0,
        typed: 0,
    };
    match mode {
        InsertMode::NewTree { id, threat } => {
            let label = b.unique(&item.tree.label)?;
            let root = NodeId::new_v7();
            let root_node = match item.tree.gate {
                Some(kind) => Node::gate(kind, label.as_str()),
                None => Node::step(label.as_str()),
            };
            let tree =
                aegis_model::AttackTree::new(root, root_node).map_err(|_| EditError::EmptyName)?;
            b.ops.push(Op::AddAttackTree {
                id: *id,
                tree,
                step_feasibility: Vec::new(),
                provenance: prov.clone(),
            });
            b.count = 1;
            b.rating(&item.tree, root)?;
            b.attack_type(&item.tree, root);
            for (i, c) in item.tree.children.iter().enumerate() {
                let p = u32::try_from(i).map_err(|_| err("too many children"))?;
                b.add(c, root, p)?;
            }
            if let Some(threat) = threat {
                b.ops.push(Op::SetThreatAttackTree {
                    threat: *threat,
                    tree: Some(*id),
                    provenance: human.clone(),
                });
            }
            let note = origin_note(pack, item, b.typed);
            b.ops.push(Op::SetDescription {
                target: ElementRef::Tree(*id),
                text: note.into(),
                provenance: human,
            });
        }
        InsertMode::UnderGate { tree, gate } | InsertMode::InPlaceOfLeaf { tree, leaf: gate } => {
            let t = project
                .attack_tree_idx(tree)
                .and_then(|i| project.attack_tree(i))
                .ok_or(EditError::Missing)?;
            let at = t.idx_of(gate).ok_or(EditError::Missing)?;
            let in_place = matches!(mode, InsertMode::InPlaceOfLeaf { .. });
            let (parent, position) = if in_place {
                if t.get(at).is_some_and(|n| n.gate_kind().is_some()) {
                    return Err(err("only a step can be replaced, not a gate"));
                }
                let mut parents = t.parents(at);
                let (Some(owner), None) = (parents.next(), parents.next()) else {
                    return Err(err(
                        "this step is shared or is the root; unlink it or pick another",
                    ));
                };
                let position = t.children(owner).iter().position(|c| *c == at).unwrap_or(0);
                (
                    t.id_of(owner).ok_or(EditError::Missing)?,
                    u32::try_from(position).map_err(|_| err("too many children"))?,
                )
            } else {
                if t.get(at).and_then(Node::gate_kind).is_none() {
                    return Err(err("a step cannot take children; pick a gate"));
                }
                (
                    *gate,
                    u32::try_from(t.children(at).len()).map_err(|_| err("too many children"))?,
                )
            };
            let removed = in_place.then_some(*gate);
            let total = t.len() + nodes - usize::from(in_place);
            if total > MAX_ATTACK_TREE_NODES {
                return Err(err("the tree would get too large"));
            }
            for (idx, id, n) in t.iter() {
                if Some(id) == removed {
                    continue;
                }
                b.used.insert(n.label().to_owned());
                if n.gate_kind().is_none() && n.children().is_empty() {
                    let extra =
                        u32::try_from(t.parents(idx).count().saturating_sub(1)).unwrap_or(0);
                    b.shareable.insert(n.label().to_owned(), (id, extra));
                }
            }
            b.count = u32::try_from(t.len()).map_err(|_| err("the tree is too large"))?;
            if let Some(leaf) = removed {
                b.ops.extend(crate::tree_diff::clear_step_ops(
                    project, *tree, leaf, &prov,
                ));
            }
            b.add(&item.tree, parent, position)?;
            if let Some(leaf) = removed {
                b.ops.push(Op::RemoveTreeNode {
                    tree: *tree,
                    id: leaf,
                    provenance: prov.clone(),
                });
            }
            let note = origin_note(pack, item, b.typed);
            let old = project.description(ElementRef::Tree(*tree)).unwrap_or("");
            let text = if old.is_empty() {
                note
            } else {
                format!("{old}\n{note}")
            };
            if text.chars().count() > ElementRef::Tree(*tree).max_chars() {
                return Err(err("the tree's description is full; shorten it first"));
            }
            b.ops.push(Op::SetDescription {
                target: ElementRef::Tree(*tree),
                text: text.into(),
                provenance: human,
            });
        }
    }
    Ok(b.ops)
}

fn count_nodes(n: &ItemNode) -> usize {
    1 + n.children.iter().map(count_nodes).sum::<usize>()
}

/// The longest search text the panel takes.
pub const MAX_QUERY_CHARS: usize = 200;

/// What the Library panel shows and has chosen: the search text, the facet
/// narrowing and the picked item. Pure state; the window only draws it.
pub struct LibraryView {
    pack: std::sync::Arc<LibraryPack>,
    index: aegis_model::library::LibraryIndex,
    query: String,
    filter: aegis_model::library::FacetFilter,
    hits: Vec<aegis_model::library::Hit>,
    choices: Vec<(aegis_model::library::Facet, String, usize)>,
    picked: Option<usize>,
    /// The built-in catalog packs, searchable but not insertable (job 112).
    catalogs: Vec<CatalogView>,
    /// The picked catalog's position in `catalogs`.
    catalog_picked: Option<usize>,
}

fn facet_name(f: aegis_model::library::Facet) -> &'static str {
    use aegis_model::library::Facet;
    match f {
        Facet::Stride => "STRIDE",
        Facet::Interface => "Interface",
        Facet::Asset => "Asset",
        Facet::Attack => "Attack",
        Facet::Catalog => "Catalog",
    }
}

impl LibraryView {
    /// A panel over `pack`, listing everything.
    #[must_use]
    pub fn new(pack: std::sync::Arc<LibraryPack>) -> Self {
        let index = aegis_model::library::LibraryIndex::new(&pack);
        let mut v = Self {
            pack,
            index,
            query: String::new(),
            filter: aegis_model::library::FacetFilter::new(),
            hits: Vec::new(),
            choices: Vec::new(),
            picked: None,
            catalogs: builtin_catalogs(),
            catalog_picked: None,
        };
        v.refresh();
        v
    }

    fn refresh(&mut self) {
        self.hits = self.index.search(&self.query, &self.filter);
        self.choices = self.index.facet_counts(&self.filter);
        // The chosen facets stay on the list, so they can be turned off.
        for (f, v) in &self.filter {
            if !self.choices.iter().any(|(cf, cv, _)| cf == f && cv == v) {
                self.choices.push((*f, v.clone(), 0));
            }
        }
        self.choices.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));
        if self
            .picked
            .is_some_and(|p| !self.hits.iter().any(|h| h.index == p))
        {
            self.picked = None;
        }
        if self
            .catalog_picked
            .is_some_and(|p| !self.catalog_hits().iter().any(|(i, _)| *i == p))
        {
            self.catalog_picked = None;
        }
    }

    /// The pack.
    #[must_use]
    pub fn pack(&self) -> &std::sync::Arc<LibraryPack> {
        &self.pack
    }

    /// Searches. Text beyond [`MAX_QUERY_CHARS`] is dropped.
    pub fn set_query(&mut self, text: &str) {
        self.query = text.chars().take(MAX_QUERY_CHARS).collect();
        self.refresh();
    }

    /// The search text.
    #[must_use]
    pub fn query(&self) -> &str {
        &self.query
    }

    /// The facet choices as dropdown lines, `Interface: CAN (4)`; a chosen
    /// one starts with a check mark.
    #[must_use]
    pub fn facet_choices(&self) -> Vec<String> {
        self.choices
            .iter()
            .map(|(f, v, n)| {
                let mark = if self.filter.contains(&(*f, v.clone())) {
                    "✓ "
                } else {
                    ""
                };
                format!("{mark}{}: {v} ({n})", facet_name(*f))
            })
            .collect()
    }

    /// Turns the facet at this position of [`Self::facet_choices`] on or off.
    pub fn toggle_facet(&mut self, position: usize) {
        let Some((f, v, _)) = self.choices.get(position) else {
            return;
        };
        let key = (*f, v.clone());
        if !self.filter.remove(&key) {
            self.filter.insert(key);
        }
        self.refresh();
    }

    /// Drops every facet choice.
    pub fn clear_filters(&mut self) {
        self.filter.clear();
        self.refresh();
    }

    /// The chosen facets as one line, empty when there are none.
    #[must_use]
    pub fn filter_text(&self) -> String {
        self.filter
            .iter()
            .map(|(f, v)| format!("{}: {v}", facet_name(*f)))
            .collect::<Vec<_>>()
            .join("; ")
    }

    /// The list rows: name and kind.
    #[must_use]
    pub fn rows(&self) -> Vec<Vec<String>> {
        use aegis_model::library::ItemKind;
        self.hits
            .iter()
            .map(|h| {
                let it = &self.pack.items()[h.index];
                let kind = match it.kind {
                    ItemKind::Leaf => "Step",
                    ItemKind::Fragment => "Fragment",
                    ItemKind::Tree => "Tree",
                    ItemKind::Template => "Template",
                };
                vec![it.name.clone(), kind.to_owned()]
            })
            .collect()
    }

    /// Picks the item at this row, or none.
    pub fn pick(&mut self, row: Option<usize>) {
        self.picked = row.and_then(|r| self.hits.get(r)).map(|h| h.index);
    }

    /// The picked item's position in the pack.
    #[must_use]
    pub fn picked_item(&self) -> Option<usize> {
        self.picked
    }

    /// The row of the picked item.
    #[must_use]
    pub fn picked_row(&self) -> Option<usize> {
        let p = self.picked?;
        self.hits.iter().position(|h| h.index == p)
    }

    /// The text of the preview: the outline, the notes, where it is from.
    #[must_use]
    pub fn preview(&self) -> String {
        let Some(item) = self.picked.and_then(|p| self.pack.items().get(p)) else {
            return String::new();
        };
        let mut out = String::new();
        let mut stack = vec![(&item.tree, 0usize)];
        while let Some((n, depth)) = stack.pop() {
            let gate = match n.gate {
                Some(aegis_model::GateKind::And) => "AND ",
                Some(aegis_model::GateKind::Or) => "OR ",
                Some(aegis_model::GateKind::SeqAnd) => "SEQ ",
                None => "- ",
            };
            out.push_str(&"  ".repeat(depth));
            out.push_str(gate);
            out.push_str(&n.label);
            if let Some(f) = &n.feasibility {
                out.push_str(&format!("  [{f}]"));
            }
            if let Some(k) = &n.attack_type {
                out.push_str(&format!("  {{type: {k}}}"));
            }
            out.push('\n');
            stack.extend(n.children.iter().rev().map(|c| (c, depth + 1)));
        }
        if !item.notes.is_empty() {
            out.push('\n');
            out.push_str(&item.notes);
            out.push('\n');
        }
        out.push_str(&format!(
            "\nFrom \"{}\" {}, license {}.\n",
            self.pack.title(),
            self.pack.pack_version(),
            self.pack.license()
        ));
        out
    }

    /// The catalog packs whose metadata matches the query. An empty query
    /// lists every catalog (job 112).
    fn catalog_hits(&self) -> Vec<(usize, &CatalogView)> {
        let q = self.query.to_lowercase();
        self.catalogs
            .iter()
            .enumerate()
            .filter(|(_, c)| {
                q.is_empty()
                    || c.name.to_lowercase().contains(&q)
                    || c.key.to_lowercase().contains(&q)
                    || c.description.to_lowercase().contains(&q)
                    || c.attribution.to_lowercase().contains(&q)
            })
            .collect()
    }

    /// The catalog list rows: name and key.
    #[must_use]
    pub fn catalog_rows(&self) -> Vec<Vec<String>> {
        self.catalog_hits()
            .into_iter()
            .map(|(_, c)| vec![c.name.clone(), c.key.clone()])
            .collect()
    }

    /// Picks the catalog at this row, or none.
    pub fn pick_catalog(&mut self, row: Option<usize>) {
        self.catalog_picked = row.and_then(|r| self.catalog_hits().get(r).map(|(i, _)| *i));
    }

    /// The row of the picked catalog.
    #[must_use]
    pub fn catalog_picked_row(&self) -> Option<usize> {
        let p = self.catalog_picked?;
        self.catalog_hits().iter().position(|(i, _)| *i == p)
    }

    /// The preview of the picked catalog, or a hint when none is picked.
    #[must_use]
    pub fn catalog_preview(&self) -> String {
        self.catalog_picked
            .and_then(|p| self.catalogs.get(p))
            .map_or_else(String::new, CatalogView::preview)
    }
}
