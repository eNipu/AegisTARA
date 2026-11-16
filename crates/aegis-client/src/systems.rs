//! Systems on the item definition page (job 119): the rows of the Systems
//! table, the picker data and the names a component row shows.

use std::collections::{BTreeMap, BTreeSet};

use aegis_model::{ElementId, ElementKind, Project};

use crate::pages::{Choice, changed_text};
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
