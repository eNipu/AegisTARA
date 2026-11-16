//! The combined view across systems (job 120): boundary flows and the overview's per-system rows.

use aegis_model::{DataFlow, DataFlowId, ElementId, ElementKind, Project};

use crate::systems::system_name;

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
