//! Systems on the item definition page (job 119): the rows of the Systems
//! table, the picker data and the names a component row shows.

use std::collections::BTreeMap;

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
