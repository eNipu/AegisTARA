//! The combined view across systems (job 120): boundary flows and the overview's per-system rows.

use std::collections::{BTreeMap, BTreeSet};

use aegis_model::{DataFlow, DataFlowId, ElementId, ElementKind, Project};

use crate::pages::risk_view;
use crate::systems::{SystemView, system_name};
use crate::view::RiskView;
use crate::workspace::Workspace;

/// One data flow whose ends are in different systems.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoundaryFlow {
    /// The data flow.
    pub id: DataFlowId,
    /// Short visual ID (`DF00`, ...).
    pub visual_id: String,
    /// Name.
    pub name: String,
    /// Source component name.
    pub from: String,
    /// Destination component name.
    pub to: String,
    /// System of the source component.
    pub from_system: ElementId,
    /// System of the destination component.
    pub to_system: ElementId,
    /// "ADAS → Gateway".
    pub systems: String,
    /// Threat scenarios on assets bound to either end component.
    pub threats: usize,
    /// True when `threats` is 0.
    pub no_threat: bool,
}

/// Every boundary flow in arena order; empty when the project has no
/// systems. Time O(threats + flows x depth).
#[must_use]
pub fn boundary_flows(project: &Project) -> Vec<BoundaryFlow> {
    if project.element_count(ElementKind::System) == 0 {
        return Vec::new();
    }
    let counts = threats_per_component(project);
    let name_of = |idx| {
        project
            .component(idx)
            .map_or_else(|| "?".to_owned(), |c| c.name().to_owned())
    };
    project
        .data_flows()
        .filter_map(|(idx, id, df)| {
            let (from_system, to_system) = project.boundary_flow(idx)?;
            let threats = flow_threats(&counts, df);
            let systems = format!(
                "{} → {}",
                system_name(project, from_system).unwrap_or_else(|| "?".to_owned()),
                system_name(project, to_system).unwrap_or_else(|| "?".to_owned()),
            );
            Some(BoundaryFlow {
                id,
                visual_id: project.data_flow_visual_id(idx).unwrap_or_default(),
                name: df.name().to_owned(),
                from: name_of(df.from()),
                to: name_of(df.to()),
                from_system,
                to_system,
                systems,
                threats,
                no_threat: threats == 0,
            })
        })
        .collect()
}

/// Threat scenarios per component slot (index `ComponentIdx::index()`):
/// those whose asset is bound to that component. Time O(threats).
pub(crate) fn threats_per_component(project: &Project) -> Vec<usize> {
    let mut counts: Vec<usize> = Vec::new();
    for (_, _, t) in project.threat_scenarios() {
        let Some(c) = project.asset(t.asset()).and_then(|a| a.component()) else {
            continue;
        };
        if counts.len() <= c.index() {
            counts.resize(c.index() + 1, 0);
        }
        if let Some(n) = counts.get_mut(c.index()) {
            *n += 1;
        }
    }
    counts
}

/// Threats at either end of a flow; an end counted once when both ends
/// are the same component.
pub(crate) fn flow_threats(counts: &[usize], flow: &DataFlow) -> usize {
    let at = |i: usize| counts.get(i).copied().unwrap_or(0);
    let (a, b) = (flow.from().index(), flow.to().index());
    if a == b { at(a) } else { at(a) + at(b) }
}

/// "" for no flow, "Gateway 2" for flows with one other system,
/// "3 systems 6" for several.
pub(crate) fn boundary_text(others: &[String], flows: usize) -> String {
    match others {
        [] => String::new(),
        [one] => format!("{one} {flows}"),
        many => format!("{} systems {flows}", many.len()),
    }
}

/// One row of the overview's systems table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SystemTotals {
    /// The system; `None` for the "No system" row and the total row.
    pub system: Option<ElementId>,
    /// The system's name, "No system", "All" or "This view".
    pub name: String,
    /// Components, children included.
    pub components: usize,
    /// Assets.
    pub assets: usize,
    /// Threat scenarios.
    pub threats: usize,
    /// The highest risk after controls among its threats.
    pub highest_residual: Option<RiskView>,
    /// Boundary flows with an end in this system; for the total row,
    /// the number of different flows.
    pub boundary_flows: usize,
    /// "Gateway 2", "3 systems 6"; empty for none and for the total row.
    pub boundary_text: String,
}

/// The systems table of the overview and the boundary flows under it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SystemsOverview {
    /// One row per system in the view, in ID order, then "No system"
    /// when a component, asset or threat is in no system.
    pub rows: Vec<SystemTotals>,
    /// The sum of the rows: "All", or "This view" for a subset.
    pub total: SystemTotals,
    /// The boundary flows with an end in the view, in arena order.
    pub flows: Vec<BoundaryFlow>,
}

fn empty_totals(system: Option<ElementId>, name: String) -> SystemTotals {
    SystemTotals {
        system,
        name,
        components: 0,
        assets: 0,
        threats: 0,
        highest_residual: None,
        boundary_flows: 0,
        boundary_text: String::new(),
    }
}

/// The systems table, or `None` when the project has no systems. The
/// view is normalized first. Time O(components x depth + assets +
/// threats + flows x depth).
#[must_use]
pub fn systems_overview(ws: &Workspace, view: &SystemView) -> Option<SystemsOverview> {
    let project = ws.project();
    let m = project.methodology();
    let view = view.normalized(project);
    let mut rows: Vec<SystemTotals> = Vec::new();
    let mut at: BTreeMap<ElementId, usize> = BTreeMap::new();
    for (id, e) in project.elements(ElementKind::System) {
        if view.is_all() || view.contains(id) {
            at.insert(id, rows.len());
            rows.push(empty_totals(Some(id), e.name().to_owned()));
        }
    }
    if project.element_count(ElementKind::System) == 0 {
        return None;
    }
    // The last slot is "No system"; elements of a system outside the view
    // have no slot.
    let none_slot = rows.len();
    rows.push(empty_totals(None, "No system".to_owned()));
    let outside = |s: Option<ElementId>| s.is_some_and(|s| !at.contains_key(&s));
    let slot = |s: Option<ElementId>| s.map_or(Some(none_slot), |s| at.get(&s).copied());
    // Position of the highest residual risk per row, in `m.risk_levels()`.
    let mut best: Vec<Option<usize>> = vec![None; rows.len()];
    for (idx, _, _) in project.components() {
        let s = project.component_system(idx);
        if let Some(row) = slot(s).and_then(|i| rows.get_mut(i)) {
            row.components += 1;
        }
    }
    let uses = project.system_use();
    for (idx, _, _) in project.assets() {
        let s = uses.ids(uses.asset(idx)).next();
        if let Some(row) = slot(s).and_then(|i| rows.get_mut(i)) {
            row.assets += 1;
        }
    }
    for (idx, id, _) in project.threat_scenarios() {
        let s = uses.ids(uses.threat(idx)).next();
        let Some(i) = slot(s) else { continue };
        if let Some(row) = rows.get_mut(i) {
            row.threats += 1;
        }
        let level = ws
            .cache()
            .evaluate_residual_risk(project, id)
            .ok()
            .and_then(aegis_model::ThreatRisk::max_risk);
        let pos = level.and_then(|l| m.risk_levels().position(|(x, _, _)| x == l));
        if let Some(b) = best.get_mut(i) {
            *b = (*b).max(pos);
        }
    }
    let flows: Vec<BoundaryFlow> = boundary_flows(project)
        .into_iter()
        .filter(|f| !outside(Some(f.from_system)) || !outside(Some(f.to_system)))
        .collect();
    let mut others: Vec<BTreeSet<ElementId>> = vec![BTreeSet::new(); rows.len()];
    for f in &flows {
        for (this, other) in [(f.from_system, f.to_system), (f.to_system, f.from_system)] {
            let Some(i) = at.get(&this).copied() else {
                continue;
            };
            if let Some(r) = rows.get_mut(i) {
                r.boundary_flows += 1;
            }
            if let Some(o) = others.get_mut(i) {
                o.insert(other);
            }
        }
    }
    for (row, set) in rows.iter_mut().zip(&others) {
        let names: Vec<String> = set
            .iter()
            .filter_map(|s| system_name(project, *s))
            .collect();
        row.boundary_text = boundary_text(&names, row.boundary_flows);
    }
    let levels: Vec<_> = m.risk_levels().map(|(l, _, _)| l).collect();
    for (row, b) in rows.iter_mut().zip(&best) {
        row.highest_residual = b
            .and_then(|p| levels.get(p).copied())
            .and_then(|l| risk_view(m, l));
    }
    let named = rows
        .pop()
        .unwrap_or_else(|| empty_totals(None, String::new()));
    if named.components + named.assets + named.threats > 0 {
        rows.push(named);
    }
    let mut total = empty_totals(
        None,
        if view.is_all() { "All" } else { "This view" }.to_owned(),
    );
    for r in &rows {
        total.components += r.components;
        total.assets += r.assets;
        total.threats += r.threats;
    }
    let top = best.iter().copied().max().flatten();
    total.highest_residual = top
        .and_then(|p| levels.get(p).copied())
        .and_then(|l| risk_view(m, l));
    total.boundary_flows = flows.len();
    Some(SystemsOverview { rows, total, flows })
}
