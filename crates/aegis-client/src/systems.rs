//! Systems on the item definition page (job 119): the rows of the Systems
//! table, the picker data and the names a component row shows.

use std::collections::{BTreeMap, BTreeSet};

use aegis_model::{ElementId, ElementKind, Project};

use crate::pages::{Choice, changed_text};
use crate::state::PageData;
use crate::view::{Page, Selection, ThreatRow};
use crate::workspace::Workspace;

/// One system in the Systems table of the item definition page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SystemRow {
    /// The system.
    pub id: ElementId,
    /// Short visual ID (`SYS00`, ...).
    pub visual_id: String,
    /// Name.
    pub name: String,
    /// Names of its top-level components, in arena order.
    pub components: Vec<String>,
    /// All its components, children included.
    pub component_count: usize,
    /// Provenance text (`pages::changed_text`).
    pub changed: String,
    /// Boundary flows with one end in this system.
    pub boundary_flows: usize,
    /// Boundary flows and the other systems (`combined::boundary_text`).
    pub boundary_text: String,
}

/// Every system in ID order; empty when the project has none. One pass over
/// the components: time O(components x depth + systems).
#[must_use]
pub fn system_rows(ws: &Workspace) -> Vec<SystemRow> {
    let project = ws.project();
    let mut rows: Vec<SystemRow> = project
        .elements(ElementKind::System)
        .map(|(id, e)| SystemRow {
            id,
            visual_id: project.element_visual_id(id).unwrap_or_default(),
            name: e.name().to_owned(),
            components: Vec::new(),
            component_count: 0,
            changed: changed_text(e.provenance()),
            boundary_flows: 0,
            boundary_text: String::new(),
        })
        .collect();
    if rows.is_empty() {
        return rows;
    }
    let at: BTreeMap<ElementId, usize> = rows.iter().enumerate().map(|(i, r)| (r.id, i)).collect();
    for (idx, _, c) in project.components() {
        let Some(row) = project
            .component_system(idx)
            .and_then(|s| at.get(&s))
            .and_then(|i| rows.get_mut(*i))
        else {
            continue;
        };
        row.component_count += 1;
        if c.parent().is_none() {
            row.components.push(c.name().to_owned());
        }
    }
    let mut others: Vec<BTreeSet<ElementId>> = vec![BTreeSet::new(); rows.len()];
    for f in crate::combined::boundary_flows(project) {
        for (this, other) in [(f.from_system, f.to_system), (f.to_system, f.from_system)] {
            if let Some(i) = at.get(&this).copied() {
                if let Some(r) = rows.get_mut(i) {
                    r.boundary_flows += 1;
                }
                if let Some(o) = others.get_mut(i) {
                    o.insert(other);
                }
            }
        }
    }
    for (row, set) in rows.iter_mut().zip(&others) {
        let names: Vec<String> = set
            .iter()
            .filter_map(|s| system_name(project, *s))
            .collect();
        row.boundary_text = crate::combined::boundary_text(&names, row.boundary_flows);
    }
    rows
}

/// The systems for a picker, in ID order; empty when there are none.
#[must_use]
pub fn system_choices(project: &Project) -> Vec<Choice<ElementId>> {
    project
        .elements(ElementKind::System)
        .map(|(id, e)| Choice {
            id,
            visual_id: project.element_visual_id(id).unwrap_or_default(),
            name: e.name().to_owned(),
        })
        .collect()
}

/// The name of a system, or `None` if there is no such system.
#[must_use]
pub fn system_name(project: &Project, id: ElementId) -> Option<String> {
    project.element(id).map(|e| e.name().to_owned())
}

/// Which systems the user looks at. Empty means all systems. A display
/// choice: never an op, never stored in the project (SYS-03).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SystemView(BTreeSet<ElementId>);

impl SystemView {
    /// All systems.
    #[must_use]
    pub fn all() -> Self {
        Self::default()
    }

    /// One system.
    #[must_use]
    pub fn only(id: ElementId) -> Self {
        Self(BTreeSet::from([id]))
    }

    /// The given systems; none gives all systems.
    #[must_use]
    pub fn of(ids: impl IntoIterator<Item = ElementId>) -> Self {
        Self(ids.into_iter().collect())
    }

    /// True when the view is all systems.
    #[must_use]
    pub fn is_all(&self) -> bool {
        self.0.is_empty()
    }

    /// True when `id` is checked.
    #[must_use]
    pub fn contains(&self, id: ElementId) -> bool {
        self.0.contains(&id)
    }

    /// The checked systems, in ID order.
    pub fn systems(&self) -> impl Iterator<Item = ElementId> + '_ {
        self.0.iter().copied()
    }

    /// Checks or unchecks one system. Unchecking the last one gives all
    /// systems.
    #[must_use]
    pub fn toggled(&self, id: ElementId) -> Self {
        let mut set = self.0.clone();
        if !set.remove(&id) {
            set.insert(id);
        }
        Self(set)
    }

    /// Drops every ID that is not a live system of `project`.
    #[must_use]
    pub fn normalized(&self, project: &Project) -> Self {
        Self(
            self.0
                .iter()
                .copied()
                .filter(|id| {
                    project
                        .element(*id)
                        .is_some_and(|e| e.kind() == ElementKind::System)
                })
                .collect(),
        )
    }
}

/// The view of the systems whose name equals one of `names` exactly; no
/// match gives all systems. Time O(s * k) for s systems and k names.
#[must_use]
pub fn view_of_names(project: &Project, names: &[String]) -> SystemView {
    if names.is_empty() {
        return SystemView::all();
    }
    SystemView::of(
        project
            .elements(ElementKind::System)
            .filter(|(_, e)| names.iter().any(|n| n == e.name()))
            .map(|(id, _)| id),
    )
}

/// The names of the live systems of `view`, in ID order; empty for all
/// systems. Time O(k log n) for k systems in the view.
#[must_use]
pub fn view_names(project: &Project, view: &SystemView) -> Vec<String> {
    view.normalized(project)
        .systems()
        .filter_map(|id| system_name(project, id))
        .collect()
}

/// What a filtered output (the report) keeps: the project's system usage
/// and the view's set, or nothing when the view is all systems.
#[derive(Debug)]
pub struct Scope {
    usage: aegis_model::SystemUse,
    view: aegis_model::SystemSet,
    on: bool,
}

impl Scope {
    /// The scope of `view`. Time O(k log n) when the view is all systems,
    /// O(project) otherwise (it computes the systems of every element).
    #[must_use]
    pub fn new(project: &Project, view: &SystemView) -> Self {
        let view = view.normalized(project);
        if view.is_all() {
            return Self {
                usage: aegis_model::SystemUse::default(),
                view: aegis_model::SystemSet::EMPTY,
                on: false,
            };
        }
        let usage = project.system_use();
        let set = usage.set_of(view.systems());
        Self {
            usage,
            view: set,
            on: true,
        }
    }

    /// True when the scope drops what other systems hold. Time O(1).
    #[must_use]
    pub fn is_on(&self) -> bool {
        self.on
    }

    /// The systems of every element (empty when the scope is off). Time O(1).
    #[must_use]
    pub fn usage(&self) -> &aegis_model::SystemUse {
        &self.usage
    }

    /// Whether an element shows: always when the scope is off or `set` gives
    /// `None` (the element is gone), else when its systems show in the view.
    /// Time O(1) plus the closure.
    pub fn shows(
        &self,
        set: impl FnOnce(&aegis_model::SystemUse) -> Option<aegis_model::SystemSet>,
    ) -> bool {
        !self.on || set(&self.usage).is_none_or(|s| s.shows_in(self.view))
    }
}

/// One system in the view menu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ViewChoice {
    /// The system.
    pub id: ElementId,
    /// Name.
    pub name: String,
    /// All its components, children included.
    pub components: usize,
    /// True when the view includes it.
    pub checked: bool,
}

/// The data of the view menu and its chip in the top bar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ViewMenu {
    /// "All systems", "Brake", "Brake + ADAS", "Brake +2".
    pub chip: String,
    /// For a narrow window: "All systems", "Brake", "Brake +1".
    pub chip_short: String,
    /// True when the view is all systems (the radio at the top).
    pub all: bool,
    /// Every component of the project.
    pub total: usize,
    /// One per system, in ID order.
    pub choices: Vec<ViewChoice>,
    /// Components in no system.
    pub unassigned: usize,
}

/// The view menu, or `None` when the project has no systems. The view is
/// normalized first. Time O(components x depth + systems).
#[must_use]
pub fn view_menu(project: &Project, view: &SystemView) -> Option<ViewMenu> {
    let view = view.normalized(project);
    let mut choices: Vec<ViewChoice> = project
        .elements(ElementKind::System)
        .map(|(id, e)| ViewChoice {
            id,
            name: e.name().to_owned(),
            components: 0,
            checked: view.contains(id),
        })
        .collect();
    if choices.is_empty() {
        return None;
    }
    let at: BTreeMap<ElementId, usize> =
        choices.iter().enumerate().map(|(i, c)| (c.id, i)).collect();
    let mut total = 0;
    let mut unassigned = 0;
    for (idx, _, _) in project.components() {
        total += 1;
        match project
            .component_system(idx)
            .and_then(|s| at.get(&s))
            .and_then(|i| choices.get_mut(*i))
        {
            Some(c) => c.components += 1,
            None => unassigned += 1,
        }
    }
    let checked: Vec<&str> = choices
        .iter()
        .filter(|c| c.checked)
        .map(|c| c.name.as_str())
        .collect();
    let (chip, chip_short) = match checked.as_slice() {
        [] => ("All systems".to_owned(), "All systems".to_owned()),
        [one] => ((*one).to_owned(), (*one).to_owned()),
        [a, b] => (format!("{a} + {b}"), format!("{a} +1")),
        [a, rest @ ..] => {
            let text = format!("{a} +{}", rest.len());
            (text.clone(), text)
        }
    };
    Some(ViewMenu {
        chip,
        chip_short,
        all: view.is_all(),
        total,
        choices,
        unassigned,
    })
}

/// One command palette entry that sets the view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ViewCommand {
    /// The text shown.
    pub label: String,
    /// The view it sets.
    pub view: SystemView,
}

/// The palette entries: all, each system alone, then (when the view is not
/// all) each system not yet in the view added to it. Empty when the project
/// has no systems.
#[must_use]
pub fn view_commands(project: &Project, view: &SystemView) -> Vec<ViewCommand> {
    let view = view.normalized(project);
    let systems: Vec<(ElementId, &str)> = project
        .elements(ElementKind::System)
        .map(|(id, e)| (id, e.name()))
        .collect();
    if systems.is_empty() {
        return Vec::new();
    }
    let mut out = vec![ViewCommand {
        label: "View: All systems".to_owned(),
        view: SystemView::all(),
    }];
    for (id, name) in &systems {
        out.push(ViewCommand {
            label: format!("View: {name}"),
            view: SystemView::only(*id),
        });
    }
    if !view.is_all() {
        for (id, name) in &systems {
            if !view.contains(*id) {
                out.push(ViewCommand {
                    label: format!("View: add {name}"),
                    view: view.toggled(*id),
                });
            }
        }
    }
    out
}

/// Keeps the rows whose systems show in `view`; a row `set_of` cannot place
/// (its ID is gone) is kept.
fn retain_shown<T>(
    rows: &mut Vec<T>,
    view: aegis_model::SystemSet,
    set_of: impl Fn(&T) -> Option<aegis_model::SystemSet>,
) {
    rows.retain(|r| set_of(r).is_none_or(|s| s.shows_in(view)));
}

/// The systems of the element a selection points at; `None` when the
/// element is gone. Time O(log n).
#[must_use]
pub fn selection_systems(
    project: &Project,
    usage: &aegis_model::SystemUse,
    s: Selection,
) -> Option<aegis_model::SystemSet> {
    match s {
        Selection::Component(id) => Some(usage.component(project.component_idx(&id)?)),
        Selection::Interface(id) => Some(usage.interface(project.interface_idx(&id)?)),
        Selection::DataFlow(id) => Some(usage.data_flow(project.data_flow_idx(&id)?)),
        Selection::TrustBoundary(id) => {
            Some(usage.trust_boundary(project.trust_boundary_idx(&id)?))
        }
        Selection::Asset(id) => Some(usage.asset(project.asset_idx(&id)?)),
        Selection::Damage(id) => Some(usage.damage(project.damage_scenario_idx(&id)?)),
        Selection::Threat(id) => Some(usage.threat(project.threat_scenario_idx(&id)?)),
        Selection::Tree(id) | Selection::TreeNode(id, _) => {
            Some(usage.tree(project.attack_tree_idx(&id)?))
        }
        Selection::Control(id) => Some(usage.control(project.security_control_idx(&id)?)),
        Selection::Goal(id) => Some(usage.goal(project.cybersecurity_goal_idx(&id)?)),
        Selection::Claim(id) => Some(usage.claim(project.cybersecurity_claim_idx(&id)?)),
    }
}

/// The view that shows the element `s` points at, with the name of the
/// system added to get there. `None` when the view already shows it
/// (all systems, an element in no system, or one of its systems is in
/// the view) or the element is gone. Adds the element's first system
/// in ID order. Time O(project) for `system_use`; nothing is computed
/// when the view is all.
#[must_use]
pub fn reveal(project: &Project, view: &SystemView, s: Selection) -> Option<(SystemView, String)> {
    if view.is_all() {
        return None;
    }
    let usage = project.system_use();
    let set = selection_systems(project, &usage, s)?;
    if set.shows_in(usage.set_of(view.systems())) {
        return None;
    }
    let id = usage.ids(set).next()?;
    let name = system_name(project, id)?;
    Some((view.toggled(id), name))
}

/// Keeps the table rows of `page` that show in `view`. Lists that only
/// feed choices are left alone. Does nothing when the view is all
/// systems or the project has none. Time O(project) for `system_use`
/// plus O(rows log n).
pub fn filter_page(project: &Project, view: &SystemView, page: Page, d: &mut PageData) {
    let view = view.normalized(project);
    if view.is_all() {
        return;
    }
    let usage = project.system_use();
    let v = usage.set_of(view.systems());
    match page {
        Page::Item => {
            retain_shown(&mut d.components, v, |r| {
                Some(usage.component(project.component_idx(&r.id)?))
            });
            retain_shown(&mut d.interfaces, v, |r| {
                Some(usage.interface(project.interface_idx(&r.id)?))
            });
            retain_shown(&mut d.data_flows, v, |r| {
                Some(usage.data_flow(project.data_flow_idx(&r.id)?))
            });
            retain_shown(&mut d.trust_boundaries, v, |r| {
                Some(usage.trust_boundary(project.trust_boundary_idx(&r.id)?))
            });
        }
        Page::Assets => retain_shown(&mut d.assets, v, |r| {
            Some(usage.asset(project.asset_idx(&r.id)?))
        }),
        Page::Damage => retain_shown(&mut d.damages, v, |r| {
            Some(usage.damage(project.damage_scenario_idx(&r.id)?))
        }),
        Page::Risks => retain_shown(&mut d.risks, v, |r| {
            Some(usage.threat(project.threat_scenario_idx(&r.id)?))
        }),
        Page::Trees => retain_shown(&mut d.trees, v, |r| {
            Some(usage.tree(project.attack_tree_idx(&r.id)?))
        }),
        Page::Controls => retain_shown(&mut d.controls, v, |r| {
            Some(usage.control(project.security_control_idx(&r.id)?))
        }),
        Page::Goals => {
            retain_shown(&mut d.goals, v, |r| {
                Some(usage.goal(project.cybersecurity_goal_idx(&r.id)?))
            });
            retain_shown(&mut d.claims, v, |r| {
                Some(usage.claim(project.cybersecurity_claim_idx(&r.id)?))
            });
        }
        Page::Problems => retain_shown(&mut d.problems, v, |r| {
            selection_systems(project, &usage, r.target?)
        }),
        _ => {}
    }
}

/// The same for the threat rows of the Threats and Risks pages.
pub fn filter_threats(project: &Project, view: &SystemView, rows: &mut Vec<ThreatRow>) {
    let view = view.normalized(project);
    if view.is_all() {
        return;
    }
    let usage = project.system_use();
    let v = usage.set_of(view.systems());
    retain_shown(rows, v, |r| {
        Some(usage.threat(project.threat_scenario_idx(&r.id)?))
    });
}
