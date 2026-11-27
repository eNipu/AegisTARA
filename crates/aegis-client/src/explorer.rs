//! The project explorer: the whole project as one tree (project, shared
//! analysis, every system with the same folders), with counts, problem
//! marks, keyboard moves, a filter and the drop rule. No UI toolkit; the
//! GPUI crate only renders [`ExplorerRow`]s.

use std::collections::{HashMap, HashSet};

use aegis_model::{ElementId, ElementNameDef, ElementNamesDef, Project, SystemSet, SystemUse};

use crate::commands::Edit;
use crate::i18n::Catalog;
use crate::pages::problems;
use crate::systems::system_rows;
use crate::view::Selection;
use crate::workspace::Workspace;

use Folder as F;

/// Whether an element with these systems belongs to an owner.
type Member<'a> = dyn Fn(SystemSet) -> bool + 'a;

/// Whose folder: the project, one system, or the elements in no system.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Owner {
    /// The project level (definition and shared analysis).
    Project,
    /// One system.
    System(ElementId),
    /// The elements in no system.
    Unassigned,
}

/// One folder of the explorer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub enum Folder {
    Overview,
    ProjectDefinition,
    Parties,
    SharedAnalysis,
    Systems,
    ItemDefinition,
    Components,
    Interfaces,
    DataFlows,
    TrustBoundaries,
    Assets,
    Damage,
    Threats,
    Trees,
    Controls,
    Risks,
    GoalsAndClaims,
    Goals,
    Claims,
    SharedSteps,
    Fragments,
    Problems,
}

/// One node of the explorer tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ExplorerKey {
    /// The project.
    Root,
    /// A folder of an owner.
    Folder(Owner, Folder),
    /// A system.
    System(ElementId),
    /// The elements in no system.
    Unassigned,
    /// An element in a folder of an owner.
    Element(Owner, Folder, Selection),
}

/// The worst lint finding at or below a node.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Mark {
    /// No finding.
    #[default]
    None,
    /// A warning.
    Warning,
    /// An error.
    Error,
}

/// One visible line of the explorer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExplorerRow {
    /// The node.
    pub key: ExplorerKey,
    /// Its parent, `None` for the root.
    pub parent: Option<ExplorerKey>,
    /// Indent level, 0 for the root.
    pub depth: u8,
    /// Display text.
    pub label: String,
    /// Lucide icon name.
    pub icon: &'static str,
    /// Number shown after the label, if any.
    pub count: Option<usize>,
    /// Worst finding at or below.
    pub mark: Mark,
    /// The folder shows shared elements linked to this system.
    pub linked: bool,
    /// The node has children.
    pub has_children: bool,
    /// The children are shown.
    pub expanded: bool,
    /// Colour slot 1-6 of a system row.
    pub system_slot: Option<u8>,
}

#[derive(Clone, Debug, Default)]
struct Node {
    parent: Option<ExplorerKey>,
    depth: u8,
    label: String,
    /// Byte length of the visual ID at the start of an element label.
    split: usize,
    count: Option<usize>,
    mark: Mark,
    linked: bool,
    children: Vec<ExplorerKey>,
    system_slot: Option<u8>,
}

/// One element with its display text and systems.
struct Item {
    sel: Selection,
    visual_id: String,
    name: String,
    set: SystemSet,
}

/// Every element, by the folder of its kind, in project order.
type Items = HashMap<Folder, Vec<Item>>;

/// Fills `out` with one list per kind: `Folder: iterator, visual ID
/// getter, selection variant, systems getter`.
macro_rules! collect_items {
    ($p:ident, $u:ident, $out:ident; $($f:ident: $iter:ident, $vid:ident, $sel:ident, $set:ident;)*) => {$(
        let list = $p.$iter().map(|(i, id, x)| Item {
            sel: Selection::$sel(id),
            visual_id: $p.$vid(i).unwrap_or_default(),
            name: x.name().to_owned(),
            set: $u.$set(i),
        });
        $out.insert(Folder::$f, list.collect());
    )*};
}

fn collect(p: &Project, u: &SystemUse) -> Items {
    let mut out = Items::new();
    collect_items!(p, u, out;
        Components: components, component_visual_id, Component, component;
        Interfaces: interfaces, interface_visual_id, Interface, interface;
        DataFlows: data_flows, data_flow_visual_id, DataFlow, data_flow;
        TrustBoundaries: trust_boundaries, trust_boundary_visual_id, TrustBoundary, trust_boundary;
        Assets: assets, asset_visual_id, Asset, asset;
        Damage: damage_scenarios, damage_scenario_visual_id, Damage, damage;
        Threats: threat_scenarios, threat_scenario_visual_id, Threat, threat;
        Controls: security_controls, security_control_visual_id, Control, control;
        Goals: cybersecurity_goals, cybersecurity_goal_visual_id, Goal, goal;
        Claims: cybersecurity_claims, cybersecurity_claim_visual_id, Claim, claim;
        SharedSteps: shared_steps, shared_step_visual_id, SharedStep, shared_step;
    );
    let (trees, fragments): (Vec<_>, Vec<_>) = p
        .attack_trees()
        .partition(|(i, _, _)| p.attack_tree_role(*i) != Some(aegis_model::TreeRole::Fragment));
    let tree_item = |(i, id, t): (
        aegis_model::TreeIdx,
        aegis_model::TreeId,
        &aegis_model::AttackTree,
    )| Item {
        sel: Selection::Tree(id),
        visual_id: p.attack_tree_visual_id(i).unwrap_or_default(),
        name: t
            .get(t.root())
            .map(|n| n.label().to_owned())
            .unwrap_or_default(),
        set: u.tree(i),
    };
    out.insert(Folder::Trees, trees.into_iter().map(tree_item).collect());
    out.insert(
        Folder::Fragments,
        fragments.into_iter().map(tree_item).collect(),
    );
    let parties = p.parties().map(|(i, id, x)| Item {
        sel: Selection::Party(id),
        visual_id: p.party_visual_id(i).unwrap_or_default(),
        name: x.name().to_owned(),
        set: SystemSet::EMPTY,
    });
    out.insert(Folder::Parties, parties.collect());
    out
}

/// The items of kind `f` for which `member` holds, in project order.
fn pick<'a>(
    items: &'a Items,
    f: Folder,
    member: &'a Member<'a>,
) -> impl Iterator<Item = &'a Item> + 'a {
    items
        .get(&f)
        .into_iter()
        .flatten()
        .filter(move |i| member(i.set))
}

fn none(_: SystemSet) -> bool {
    false
}

/// The project-level folders and their parent folder (`None`: the root).
const PROJECT_TREE: [(Folder, Option<Folder>); 11] = [
    (F::Overview, None),
    (F::ProjectDefinition, None),
    (F::Parties, Some(F::ProjectDefinition)),
    (F::SharedAnalysis, None),
    (F::Damage, Some(F::SharedAnalysis)),
    (F::Controls, Some(F::SharedAnalysis)),
    (F::Goals, Some(F::SharedAnalysis)),
    (F::Claims, Some(F::SharedAnalysis)),
    (F::SharedSteps, Some(F::SharedAnalysis)),
    (F::Fragments, Some(F::SharedAnalysis)),
    (F::Systems, None),
];

/// The folders of a system and of "Not in a system", and their parent
/// folder (`None`: the owner's node).
const OWNER_TREE: [(Folder, Option<Folder>); 14] = [
    (F::ItemDefinition, None),
    (F::Components, Some(F::ItemDefinition)),
    (F::Interfaces, Some(F::ItemDefinition)),
    (F::DataFlows, Some(F::ItemDefinition)),
    (F::TrustBoundaries, Some(F::ItemDefinition)),
    (F::Assets, None),
    (F::Damage, None),
    (F::Threats, None),
    (F::Trees, None),
    (F::SharedSteps, None),
    (F::Controls, None),
    (F::Risks, None),
    (F::GoalsAndClaims, None),
    (F::Problems, None),
];

/// The display label of a folder.
fn label(f: Folder, n: &ElementNamesDef, c: &Catalog) -> String {
    let plural = |d: &ElementNameDef| d.plural.to_string();
    match f {
        F::Overview => c.tr("explorer-overview"),
        F::ProjectDefinition => c.tr("explorer-project-definition"),
        F::Parties => c.tr("explorer-parties"),
        F::SharedAnalysis => c.tr("explorer-shared-analysis"),
        F::Systems => c.tr("explorer-systems"),
        F::ItemDefinition => c.tr("explorer-item-definition"),
        F::Risks => c.tr("explorer-risk-treatment"),
        F::GoalsAndClaims => c.tr("explorer-goals-and-claims"),
        F::Problems => c.tr("explorer-problems"),
        F::Components => plural(&n.component),
        F::Interfaces => plural(&n.interface),
        F::DataFlows => plural(&n.data_flow),
        F::TrustBoundaries => plural(&n.trust_boundary),
        F::Assets => plural(&n.asset),
        F::Damage => plural(&n.damage_scenario),
        F::Threats => plural(&n.threat_scenario),
        F::Trees => plural(&n.attack_tree),
        F::Controls => plural(&n.security_control),
        F::Goals => plural(&n.cybersecurity_goal),
        F::Claims => plural(&n.cybersecurity_claim),
        F::SharedSteps => c.tr("folder-shared-steps"),
        F::Fragments => c.tr("folder-fragments"),
    }
}

/// The explorer tree of one project state: every node with its label,
/// count and mark. Rebuild it after every change.
pub struct ExplorerIndex {
    nodes: HashMap<ExplorerKey, Node>,
}

impl ExplorerIndex {
    /// Builds the tree: the systems of every element, the members of every
    /// folder, the counts and the marks from the linter. Time O(project x
    /// systems), space the same (an element is listed once per system).
    #[must_use]
    pub fn build(
        ws: &Workspace,
        names: &ElementNamesDef,
        catalog: &Catalog,
        project_name: &str,
    ) -> Self {
        use ExplorerKey as K;
        let usage = ws.project().system_use();
        let items = collect(ws.project(), &usage);
        let root = Node {
            label: project_name.to_owned(),
            ..Node::default()
        };
        let mut ix = ExplorerIndex {
            nodes: HashMap::from([(K::Root, root)]),
        };
        let fill = |ix: &mut Self, o, at, tree: &[_], member: &Member<'_>| {
            ix.folders(o, at, tree, &items, member, &|f| label(f, names, catalog));
        };
        fill(&mut ix, Owner::Project, K::Root, &PROJECT_TREE, &|_| true);
        let rows = system_rows(ws);
        let systems = K::Folder(Owner::Project, F::Systems);
        if let Some(node) = ix.nodes.get_mut(&systems) {
            node.count = Some(rows.len());
        }
        for (pos, row) in rows.iter().enumerate() {
            let node = ix.add(K::System(row.id), systems, row.name.clone());
            node.system_slot = u8::try_from(pos % 6 + 1).ok();
            let set = usage.set_of([row.id]);
            fill(
                &mut ix,
                Owner::System(row.id),
                K::System(row.id),
                &OWNER_TREE,
                &|s| s.intersects(set),
            );
        }
        let shared = [
            F::Damage,
            F::Controls,
            F::Goals,
            F::Claims,
            F::SharedSteps,
            F::Fragments,
            F::Parties,
        ];
        let unassigned = items.iter().filter(|(f, _)| !shared.contains(f));
        if unassigned
            .flat_map(|(_, list)| list)
            .any(|i| i.set.is_empty())
        {
            ix.add(K::Unassigned, systems, catalog.tr("explorer-not-in-system"));
            fill(
                &mut ix,
                Owner::Unassigned,
                K::Unassigned,
                &OWNER_TREE,
                &SystemSet::is_empty,
            );
        }
        ix.marks(ws, &usage, &items);
        ix
    }

    /// Adds `key` as the last child of `parent`.
    fn add(&mut self, key: ExplorerKey, parent: ExplorerKey, label: String) -> &mut Node {
        let mut depth = 0;
        if let Some(p) = self.nodes.get_mut(&parent) {
            p.children.push(key);
            depth = p.depth.saturating_add(1);
        }
        let node = self.nodes.entry(key).or_default();
        *node = Node {
            parent: Some(parent),
            depth,
            label,
            ..Node::default()
        };
        node
    }

    /// Adds the folders of `tree` for owner `o` under `at`, each with one
    /// element child per member of its kinds. A folder with an element
    /// kind counts its children (0 included); the item definition counts
    /// its four folders; shared kinds are linked and never unassigned.
    fn folders(
        &mut self,
        o: Owner,
        at: ExplorerKey,
        tree: &[(Folder, Option<Folder>)],
        items: &Items,
        member: &Member<'_>,
        label: &dyn Fn(Folder) -> String,
    ) {
        for &(f, up) in tree {
            let key = ExplorerKey::Folder(o, f);
            self.add(key, up.map_or(at, |u| ExplorerKey::Folder(o, u)), label(f));
            let linked = o != Owner::Project
                && matches!(
                    f,
                    F::Damage | F::Controls | F::GoalsAndClaims | F::SharedSteps
                );
            let member = if linked && o == Owner::Unassigned {
                &none
            } else {
                member
            };
            let kinds: &[Folder] = match f {
                F::Risks => &[F::Threats],
                F::GoalsAndClaims => &[F::Goals, F::Claims],
                _ => std::slice::from_ref(&f),
            };
            let mut n = 0;
            for i in kinds.iter().flat_map(|k| pick(items, *k, member)) {
                let label = format!("{} {}", i.visual_id, i.name);
                self.add(ExplorerKey::Element(o, f, i.sel), key, label)
                    .split = i.visual_id.len();
                n += 1;
            }
            let has_kind = kinds.iter().any(|k| items.contains_key(k));
            let sum = match f {
                F::ItemDefinition | F::Problems => Some(0),
                _ => has_kind.then_some(n),
            };
            if let Some(node) = self.nodes.get_mut(&key) {
                node.count = sum;
                node.linked = linked;
            }
            if let (Some(n), Some(F::ItemDefinition)) = (sum, up)
                && let Some(def) = self
                    .nodes
                    .get_mut(&ExplorerKey::Folder(o, F::ItemDefinition))
            {
                def.count = def.count.map(|c| c + n);
            }
        }
    }

    /// Marks the folders each finding lands in and their ancestors, and
    /// counts the findings per owner.
    fn marks(&mut self, ws: &Workspace, usage: &SystemUse, items: &Items) {
        use ExplorerKey as K;
        let kinds: HashMap<Selection, (Folder, SystemSet)> = items
            .iter()
            .flat_map(|(f, list)| list.iter().map(|i| (i.sel, (*f, i.set))))
            .collect();
        let mut counts: HashMap<Owner, usize> = HashMap::new();
        for row in problems(ws) {
            let m = if row.error {
                Mark::Error
            } else {
                Mark::Warning
            };
            let found = row.target.and_then(|s| match s {
                Selection::TreeNode(t, _) => kinds.get(&Selection::Tree(t)).copied(),
                s => kinds.get(&s).copied(),
            });
            let (mut keys, mut touched) = (Vec::new(), Vec::new());
            match found {
                None => keys.push(K::Root),
                Some((F::Parties, _)) => keys.push(K::Folder(Owner::Project, F::Parties)),
                Some((F::Fragments, _)) => keys.push(K::Folder(Owner::Project, F::Fragments)),
                Some((f, set)) => {
                    touched = usage.ids(set).map(Owner::System).collect();
                    let linked = match f {
                        F::Goals | F::Claims => Some(F::GoalsAndClaims),
                        F::Damage | F::Controls | F::SharedSteps => Some(f),
                        _ => None,
                    };
                    if let Some(sf) = linked {
                        keys.push(K::Folder(Owner::Project, f));
                        keys.extend(touched.iter().map(|o| K::Folder(*o, sf)));
                    } else {
                        if set.is_empty() {
                            touched.push(Owner::Unassigned);
                        }
                        keys.extend(touched.iter().map(|o| K::Folder(*o, f)));
                    }
                }
            }
            for o in touched {
                keys.push(K::Folder(o, Folder::Problems));
                *counts.entry(o).or_default() += 1;
            }
            for k in keys {
                self.mark(k, m);
            }
        }
        for (o, n) in counts {
            if let Some(node) = self.nodes.get_mut(&K::Folder(o, Folder::Problems)) {
                node.count = Some(n);
            }
        }
    }

    /// Raises the mark of `key` and of every ancestor to at least `m`.
    fn mark(&mut self, key: ExplorerKey, m: Mark) {
        let mut at = Some(key);
        while let Some(k) = at {
            let Some(node) = self.nodes.get_mut(&k) else {
                return;
            };
            node.mark = node.mark.max(m);
            at = node.parent;
        }
    }

    /// The visible rows, depth first, children only under expanded nodes;
    /// time O(visible rows). With a filter: the element rows whose visual
    /// ID or name holds it (any case), their ancestors shown expanded, and
    /// the root always; time O(project).
    #[must_use]
    pub fn rows(&self, state: &ExplorerState) -> Vec<ExplorerRow> {
        let keep = (!state.filter.is_empty()).then(|| self.matching(&state.filter.to_lowercase()));
        let mut out = Vec::new();
        let mut stack = vec![ExplorerKey::Root];
        while let Some(key) = stack.pop() {
            let Some(node) = self.nodes.get(&key) else {
                continue;
            };
            let shown = |c: &ExplorerKey| keep.as_ref().is_none_or(|k| k.contains(c));
            let expanded = match &keep {
                Some(k) => node.children.iter().any(|c| k.contains(c)),
                None => !node.children.is_empty() && state.expanded.contains(&key),
            };
            out.push(ExplorerRow {
                key,
                parent: node.parent,
                depth: node.depth,
                label: node.label.clone(),
                icon: icon(key, expanded),
                count: node.count,
                mark: node.mark,
                linked: node.linked,
                has_children: !node.children.is_empty(),
                expanded,
                system_slot: node.system_slot,
            });
            if expanded {
                stack.extend(node.children.iter().rev().filter(|c| shown(c)));
            }
        }
        out
    }

    /// The element nodes matching `needle` (lower case) and their ancestors.
    fn matching(&self, needle: &str) -> HashSet<ExplorerKey> {
        let mut keep = HashSet::new();
        for (key, node) in &self.nodes {
            if !matches!(key, ExplorerKey::Element(..)) {
                continue;
            }
            let (vid, name) = node.label.split_at(node.split.min(node.label.len()));
            if !(vid.to_lowercase().contains(needle)
                || name.trim_start().to_lowercase().contains(needle))
            {
                continue;
            }
            let mut at = Some(*key);
            while let Some(k) = at {
                if !keep.insert(k) {
                    break;
                }
                at = self.nodes.get(&k).and_then(|n| n.parent);
            }
        }
        keep
    }

    /// The first element node holding `sel`, depth first.
    fn find(&self, sel: Selection) -> Option<ExplorerKey> {
        let mut stack = vec![ExplorerKey::Root];
        while let Some(key) = stack.pop() {
            if let ExplorerKey::Element(_, _, s) = key
                && s == sel
            {
                return Some(key);
            }
            if let Some(node) = self.nodes.get(&key) {
                stack.extend(node.children.iter().rev());
            }
        }
        None
    }
}

fn icon(key: ExplorerKey, expanded: bool) -> &'static str {
    match key {
        ExplorerKey::Root => "book-open",
        ExplorerKey::System(_) | ExplorerKey::Unassigned => "square",
        ExplorerKey::Element(..) => "file",
        ExplorerKey::Folder(_, Folder::Overview) => "chart-pie",
        ExplorerKey::Folder(_, Folder::Systems) => "cpu",
        ExplorerKey::Folder(_, Folder::Problems) => "triangle-alert",
        ExplorerKey::Folder(..) if expanded => "folder-open",
        ExplorerKey::Folder(..) => "folder",
    }
}

/// What the user has opened, selected and typed in the explorer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExplorerState {
    /// The expanded nodes.
    pub expanded: HashSet<ExplorerKey>,
    /// The selected node.
    pub selected: Option<ExplorerKey>,
    /// The filter text; empty shows everything.
    pub filter: String,
}

impl Default for ExplorerState {
    fn default() -> Self {
        let pj = |f| ExplorerKey::Folder(Owner::Project, f);
        ExplorerState {
            expanded: [
                ExplorerKey::Root,
                pj(Folder::SharedAnalysis),
                pj(Folder::Systems),
            ]
            .into_iter()
            .collect(),
            selected: Some(ExplorerKey::Root),
            filter: String::new(),
        }
    }
}

impl ExplorerState {
    fn position(&self, rows: &[ExplorerRow]) -> Option<usize> {
        let sel = self.selected?;
        rows.iter().position(|r| r.key == sel)
    }

    fn select_at(&mut self, rows: &[ExplorerRow], i: usize) {
        if let Some(r) = rows.get(i) {
            self.selected = Some(r.key);
        }
    }

    /// Selects the next row; stops at the last.
    pub fn down(&mut self, rows: &[ExplorerRow]) {
        let last = rows.len().saturating_sub(1);
        let i = self.position(rows).map_or(0, |i| (i + 1).min(last));
        self.select_at(rows, i);
    }

    /// Selects the previous row; stops at the first.
    pub fn up(&mut self, rows: &[ExplorerRow]) {
        let i = self.position(rows).map_or(0, |i| i.saturating_sub(1));
        self.select_at(rows, i);
    }

    /// Expands a collapsed node, or moves into an expanded one.
    pub fn right(&mut self, rows: &[ExplorerRow]) {
        let Some(i) = self.position(rows) else {
            return;
        };
        let Some(row) = rows.get(i) else {
            return;
        };
        if row.has_children && !row.expanded {
            self.expanded.insert(row.key);
        } else if row.expanded
            && let Some(next) = rows.get(i + 1)
            && next.parent == Some(row.key)
        {
            self.selected = Some(next.key);
        }
    }

    /// Collapses an expanded node, or moves to the parent.
    pub fn left(&mut self, rows: &[ExplorerRow]) {
        let Some(row) = self.position(rows).and_then(|i| rows.get(i)) else {
            return;
        };
        if row.expanded && self.expanded.remove(&row.key) {
            return;
        }
        if let Some(parent) = row.parent {
            self.selected = Some(parent);
        }
    }

    /// Expands or collapses `key`.
    pub fn toggle(&mut self, key: ExplorerKey) {
        if !self.expanded.remove(&key) {
            self.expanded.insert(key);
        }
    }

    /// Selects `key`.
    pub fn select(&mut self, key: ExplorerKey) {
        self.selected = Some(key);
    }

    /// Selects the first element row holding `sel` and expands its
    /// ancestors; does nothing when no row holds it. Time O(tree).
    pub fn reveal(&mut self, index: &ExplorerIndex, sel: Selection) {
        let Some(key) = index.find(sel) else {
            return;
        };
        let mut at = index.nodes.get(&key).and_then(|n| n.parent);
        while let Some(k) = at {
            self.expanded.insert(k);
            at = index.nodes.get(&k).and_then(|n| n.parent);
        }
        self.selected = Some(key);
    }
}

/// The edit for dropping `dragged` on `target`: a top-level component onto
/// another system (or onto "Not in a system") moves it there. Every other
/// pair returns `None`.
#[must_use]
pub fn drop_edit(project: &Project, dragged: &ExplorerKey, target: &ExplorerKey) -> Option<Edit> {
    let ExplorerKey::Element(_, Folder::Components, Selection::Component(c)) = *dragged else {
        return None;
    };
    let component = project
        .component_idx(&c)
        .and_then(|i| project.component(i))?;
    if component.parent().is_some() {
        return None;
    }
    let system = match *target {
        ExplorerKey::System(s) | ExplorerKey::Folder(Owner::System(s), _) => Some(s),
        ExplorerKey::Unassigned | ExplorerKey::Folder(Owner::Unassigned, _) => None,
        _ => return None,
    };
    (component.system() != system).then(|| Edit::MoveToSystem {
        components: vec![c],
        system,
    })
}
