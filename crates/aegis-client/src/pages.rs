//! The pages of an open project, as plain data (job 040,
//! docs/adr/0038-tara-pages.md).
//!
//! Every page of the TARA flow (direction section 11) has a builder here:
//! a pure function from the [`Workspace`] and the user's selection to rows
//! of text and the IDs behind them. The session builds only the page on
//! screen, so a publish costs O(entities of that page), never the whole
//! project (budget: interaction within 50 ms on the `large` project).
//!
//! Like the rest of the view model, nothing here is a source of truth (D10):
//! drop it and rebuild it at any time. Names appear in these rows because
//! they are for the user's own screen; the model's diagnostics never carry
//! names (R6), so [`problems`] adds them here.

use aegis_canvas::Tone;
use aegis_model::{
    AssetId, ClaimId, ComponentId, ControlId, DamageId, DataFlowId, ElementId, ElementKind, GoalId,
    InterfaceId, LintCode, LintSeverity, Methodology, NodeId, Project, Provenance, ThreatId,
    TreeId, TrustBoundaryId,
};

use crate::commands::TREATMENTS;
use crate::time::format_ms;
use crate::view::{RiskView, Selection, UiState};
use crate::workspace::Workspace;

/// The labels of the project's methodology, in the order edits refer to
/// them by position.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Vocabulary {
    /// Security properties of assets.
    pub properties: Vec<String>,
    /// Impact categories of damage scenarios, with their levels.
    pub impact_categories: Vec<ImpactCategoryView>,
    /// Threat categories (STRIDE in the built-in pack).
    pub threat_categories: Vec<String>,
    /// Attack feasibility levels.
    pub feasibility: Vec<String>,
    /// Risk levels.
    pub risk_levels: Vec<String>,
    /// Catalog schemes (UN R155 Annex 5 in the built-in pack).
    pub catalog_schemes: Vec<String>,
    /// Interface kinds for item definition (job 074).
    pub interface_kinds: Vec<String>,
    /// Attack types for steps and security control rules (CFG-02, CFG-03).
    pub attack_types: Vec<String>,
    /// Treatment decisions, as [`TREATMENTS`] orders them.
    pub treatments: Vec<String>,
    /// Methodology-defined singular/plural labels for the core element types
    /// and the feasibility/likelihood scale (`109`).
    pub element_names: aegis_model::ElementNamesDef,
    /// Whether this methodology uses multi-factor attack potential (`true`
    /// for ISO/SAE 21434) or a plain likelihood scale (`false`, e.g. EU CRA).
    pub has_attack_potential: bool,
}

/// One impact category and its levels.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImpactCategoryView {
    /// Label.
    pub label: String,
    /// Level labels, lowest first.
    pub levels: Vec<String>,
}

/// The vocabulary of a methodology. Time O(vocabulary size).
#[must_use]
pub fn vocabulary(m: &Methodology) -> Vocabulary {
    vocabulary_for_lang(m, "en")
}

/// The vocabulary of a methodology localized for `lang` (`"en"`, `"ja"`, etc.;
/// L10N-07). Falls back to the pack's default labels when `lang` has no pack
/// translation.
#[must_use]
pub fn vocabulary_for_lang(m: &Methodology, lang: &str) -> Vocabulary {
    Vocabulary {
        properties: m
            .properties()
            .map(|p| m.property_label_for_lang(p, lang).unwrap_or("?").to_owned())
            .collect(),
        impact_categories: m
            .impact_categories()
            .map(|c| ImpactCategoryView {
                label: m
                    .impact_category_label_for_lang(c, lang)
                    .unwrap_or("?")
                    .to_owned(),
                levels: m
                    .levels(c)
                    .map(|l| m.level_label_for_lang(c, l, lang).unwrap_or("?").to_owned())
                    .collect(),
            })
            .collect(),
        threat_categories: m
            .threat_categories()
            .map(|t| {
                m.threat_category_label_for_lang(t, lang)
                    .unwrap_or("?")
                    .to_owned()
            })
            .collect(),
        feasibility: m
            .feasibility_levels()
            .map(|(l, _, label)| {
                m.feasibility_level_label_for_lang(l, lang)
                    .unwrap_or(label)
                    .to_owned()
            })
            .collect(),
        risk_levels: m
            .risk_levels()
            .map(|(l, _, label)| {
                m.risk_level_label_for_lang(l, lang)
                    .unwrap_or(label)
                    .to_owned()
            })
            .collect(),
        catalog_schemes: m
            .catalog_schemes()
            .map(|s| {
                m.catalog_scheme_label_for_lang(s, lang)
                    .unwrap_or("?")
                    .to_owned()
            })
            .collect(),
        interface_kinds: m
            .interface_kinds()
            .map(|k| {
                m.interface_kind_label_for_lang(k, lang)
                    .unwrap_or("?")
                    .to_owned()
            })
            .collect(),
        attack_types: m
            .attack_types()
            .map(|a| {
                m.attack_type_label_for_lang(a, lang)
                    .unwrap_or("?")
                    .to_owned()
            })
            .collect(),
        treatments: TREATMENTS
            .iter()
            .map(|t| treatment_label(*t).to_owned())
            .collect(),
        element_names: m.element_names_for_lang(lang).clone(),
        has_attack_potential: m.has_attack_potential(),
    }
}

/// The word for a treatment decision.
#[must_use]
pub fn treatment_label(t: aegis_model::RiskTreatment) -> &'static str {
    match t {
        aegis_model::RiskTreatment::Avoid => "Avoid",
        aegis_model::RiskTreatment::Reduce => "Reduce",
        aegis_model::RiskTreatment::Share => "Share",
        aegis_model::RiskTreatment::Retain => "Retain",
    }
}

/// "Changed by alice on 25 Oct 2026 19:30 UTC", or empty for no provenance.
#[must_use]
pub fn changed_text(p: &Provenance) -> String {
    if p.actor().is_empty() {
        return String::new();
    }
    let source = match p.source() {
        aegis_model::ProvenanceSource::Human => "",
        aegis_model::ProvenanceSource::Import => " (imported)",
        aegis_model::ProvenanceSource::Catalog => " (from a catalog)",
        aegis_model::ProvenanceSource::Ai => " (AI suggestion)",
    };
    format!(
        "Last changed by {} on {}{source}",
        p.actor(),
        format_ms(p.timestamp_ms())
    )
}

/// A row that names a related entity and leads to it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkRow {
    /// Short human-readable visual ID (`A00`, `T01`, ...).
    pub visual_id: String,
    /// Main text.
    pub label: String,
    /// Second text (a rating, a kind).
    pub detail: String,
    /// Where a click goes.
    pub target: Selection,
}

fn risk_view(m: &Methodology, level: aegis_model::RiskLevel) -> Option<RiskView> {
    let label = m.risk_level_label(level)?.to_owned();
    let index = m.risk_levels().position(|(l, _, _)| l == level)?;
    Some(RiskView {
        label,
        tone: Tone::from_level(index, m.risk_level_count()),
    })
}

fn feasibility_position(m: &Methodology, level: aegis_model::FeasibilityLevel) -> Option<usize> {
    m.feasibility_levels().position(|(l, _, _)| l == level)
}

fn feasibility_label(m: &Methodology, level: aegis_model::FeasibilityLevel) -> String {
    m.feasibility_level_label(level).unwrap_or("?").to_owned()
}

/// Worst inherent and residual risk of a threat, from the cache.
fn threat_risks(ws: &Workspace, id: ThreatId) -> (Option<RiskView>, Option<RiskView>) {
    let project = ws.project();
    let m = project.methodology();
    let view = |r: Result<&aegis_model::ThreatRisk, aegis_model::ModelError>| {
        r.ok()
            .and_then(aegis_model::ThreatRisk::max_risk)
            .and_then(|l| risk_view(m, l))
    };
    (
        view(ws.cache().evaluate_risk(project, id)),
        view(ws.cache().evaluate_residual_risk(project, id)),
    )
}

fn worse(a: Option<RiskView>, b: Option<RiskView>) -> Option<RiskView> {
    match (a, b) {
        (Some(a), Some(b)) => Some(if b.tone > a.tone { b } else { a }),
        (a, b) => a.or(b),
    }
}

// ---------------------------------------------------------------------------
// Assets.
// ---------------------------------------------------------------------------

/// One asset in the table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssetRow {
    /// ID.
    pub id: AssetId,
    /// Short visual ID (`A00`, `A01`, ...).
    pub visual_id: String,
    /// Name.
    pub name: String,
    /// Name of the component it is bound to, or empty if unbound.
    pub component: String,
    /// One flag per methodology property.
    pub properties: Vec<bool>,
    /// Threats that target it.
    pub threats: usize,
    /// The worst inherent risk among them.
    pub worst: Option<RiskView>,
}

/// The selected asset.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssetDetail {
    /// ID.
    pub id: AssetId,
    /// Short visual ID (`A00`, `A01`, ...).
    pub visual_id: String,
    /// Name.
    pub name: String,
    /// Bound component, if any.
    pub component: Option<ComponentId>,
    /// Bound component's name, or empty if unbound.
    pub component_name: String,
    /// All components in the project that can be bound.
    pub components: Vec<Choice<ComponentId>>,
    /// One flag per methodology property.
    pub properties: Vec<bool>,
    /// The threats that target it.
    pub threats: Vec<LinkRow>,
    /// The damage scenarios those threats lead to, each once, in damage order.
    pub damages: Vec<LinkRow>,
    /// Provenance text.
    pub changed: String,
}

/// Every asset in arena order. Time O(assets x properties + threats).
#[must_use]
pub fn asset_rows(ws: &Workspace) -> Vec<AssetRow> {
    let project = ws.project();
    let m = project.methodology();
    let mut counts = vec![(0usize, None::<RiskView>); project.asset_count().max(1)];
    let mut positions = std::collections::HashMap::new();
    for (pos, (idx, _, _)) in project.assets().enumerate() {
        positions.insert(idx, pos);
    }
    for (_, id, t) in project.threat_scenarios() {
        if let Some(&pos) = positions.get(&t.asset())
            && let Some(slot) = counts.get_mut(pos)
        {
            slot.0 += 1;
            slot.1 = worse(slot.1.take(), threat_risks(ws, id).0);
        }
    }
    project
        .assets()
        .enumerate()
        .map(|(pos, (idx, id, a))| {
            let (threats, worst) = counts.get(pos).cloned().unwrap_or_default();
            AssetRow {
                id,
                visual_id: project.asset_visual_id(idx).unwrap_or_default(),
                name: a.name().to_owned(),
                component: a
                    .component()
                    .and_then(|c| project.component(c))
                    .map_or_else(String::new, |c| c.name().to_owned()),
                properties: m.properties().map(|p| a.properties().contains(p)).collect(),
                threats,
                worst,
            }
        })
        .collect()
}

/// The selected asset's details. Time O(threats + damage x threats).
#[must_use]
pub fn asset_detail(ws: &Workspace, id: AssetId) -> Option<AssetDetail> {
    let project = ws.project();
    let m = project.methodology();
    let idx = project.asset_idx(&id)?;
    let a = project.asset(idx)?;
    Some(AssetDetail {
        id,
        visual_id: project.asset_visual_id(idx).unwrap_or_default(),
        name: a.name().to_owned(),
        component: a.component().and_then(|c| project.component_id(c)),
        component_name: a
            .component()
            .and_then(|c| project.component(c))
            .map_or_else(String::new, |c| c.name().to_owned()),
        components: project
            .components()
            .map(|(c_idx, cid, c)| Choice {
                id: cid,
                visual_id: project.component_visual_id(c_idx).unwrap_or_default(),
                name: c.name().to_owned(),
            })
            .collect(),
        properties: m.properties().map(|p| a.properties().contains(p)).collect(),
        threats: project
            .threat_scenarios()
            .filter(|(_, _, t)| t.asset() == idx)
            .map(|(t_idx, tid, t)| LinkRow {
                visual_id: project.threat_scenario_visual_id(t_idx).unwrap_or_default(),
                label: t.name().to_owned(),
                detail: threat_risks(ws, tid)
                    .0
                    .map_or_else(|| "not rated".to_owned(), |r| r.label),
                target: Selection::Threat(tid),
            })
            .collect(),
        damages: project
            .damage_scenarios()
            .filter(|(d_idx, _, _)| {
                project
                    .threat_scenarios()
                    .any(|(_, _, t)| t.asset() == idx && t.damages().contains(d_idx))
            })
            .map(|(d_idx, did, d)| LinkRow {
                visual_id: project.damage_scenario_visual_id(d_idx).unwrap_or_default(),
                label: d.name().to_owned(),
                detail: String::new(),
                target: Selection::Damage(did),
            })
            .collect(),
        changed: changed_text(a.provenance()),
    })
}

// ---------------------------------------------------------------------------
// Item definition (job 074).
// ---------------------------------------------------------------------------

/// One component in the item definition table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComponentRow {
    /// ID.
    pub id: ComponentId,
    /// Short visual ID (`C00`, `C01`, ...).
    pub visual_id: String,
    /// Name.
    pub name: String,
    /// Parent component name, or empty if top-level.
    pub parent: String,
    /// Number of interfaces on this component.
    pub interfaces: usize,
    /// Number of assets bound to this component.
    pub assets: usize,
    /// Name of the system the component is in (through its top-level
    /// ancestor), or empty (job 119).
    pub system: String,
    /// The project has a system and this component is in none.
    pub no_system: bool,
}

/// One security setting applied to a component or an interface (CFG-03).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettingRow {
    /// The security control.
    pub id: ControlId,
    /// Short visual ID (`SC00`, `SC01`, ...).
    pub visual_id: String,
    /// Control name.
    pub name: String,
    /// One-line summary of its rules, e.g. `Debug access → Very low`.
    pub rules: String,
    /// On an interface: `true` when this setting comes from the owning
    /// component and is shown dimmed (CFG-03, D26).
    pub inherited: bool,
    /// Owning component name when `inherited` is true.
    pub from_component: String,
}

/// One security setting applied directly to a component or an interface.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppliedSettingRow {
    /// The component or interface.
    pub target: aegis_model::ItemRef,
    /// Its short visual ID (`C00`, `IF00`).
    pub target_visual_id: String,
    /// Its name; an interface reads `"ADAS ECU / JTAG"`.
    pub target_label: String,
    /// The security control.
    pub control: ControlId,
    /// The control's short visual ID (`SC00`).
    pub control_visual_id: String,
    /// The control's name.
    pub name: String,
    /// Its rules in the asked language, e.g. `Debug access → Low`; empty
    /// when it has none.
    pub rules: String,
}

/// Every setting applied directly to a component or an interface: components
/// in project order, each followed by its interfaces; inherited settings are
/// not repeated on interfaces. Labels use `lang`. Time O(components + interfaces × controls).
#[must_use]
pub fn applied_settings(ws: &Workspace, lang: &str) -> Vec<AppliedSettingRow> {
    let project = ws.project();
    let m = project.methodology();
    let mut out = Vec::new();
    let mut push = |target: aegis_model::ItemRef, visual_id: String| {
        for (c_idx, id, c) in project.controls_applied_to(target) {
            let rules = c
                .rules()
                .iter()
                .map(|r| {
                    let attack = m
                        .attack_type_label_for_lang(r.attack_type(), lang)
                        .or_else(|| m.attack_type_label(r.attack_type()))
                        .unwrap_or("?");
                    let ceiling = m
                        .feasibility_level_label_for_lang(r.ceiling(), lang)
                        .map_or_else(|| feasibility_label(m, r.ceiling()), str::to_owned);
                    format!("{attack} \u{2192} {ceiling}")
                })
                .collect::<Vec<_>>()
                .join(", ");
            out.push(AppliedSettingRow {
                target,
                target_visual_id: visual_id.clone(),
                target_label: crate::view::through_label(project, target),
                control: id,
                control_visual_id: project
                    .security_control_visual_id(c_idx)
                    .unwrap_or_default(),
                name: c.name().to_owned(),
                rules,
            });
        }
    };
    for (c_idx, c_id, _) in project.components() {
        push(
            aegis_model::ItemRef::Component(c_id),
            project.component_visual_id(c_idx).unwrap_or_default(),
        );
        for (i_idx, i_id, i) in project.interfaces() {
            if i.component() == c_idx {
                push(
                    aegis_model::ItemRef::Interface(i_id),
                    project.interface_visual_id(i_idx).unwrap_or_default(),
                );
            }
        }
    }
    out
}

/// One rule of a security control: attack type -> feasibility ceiling (CFG-03).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ControlRuleView {
    /// Position in the methodology's attack types.
    pub attack_type: usize,
    /// Attack type label.
    pub label: String,
    /// Position in the methodology's feasibility levels, or `None` when not set.
    pub level: Option<usize>,
}

/// The selected component.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComponentDetail {
    /// ID.
    pub id: ComponentId,
    /// Short visual ID (`C00`, `C01`, ...).
    pub visual_id: String,
    /// Name.
    pub name: String,
    /// Parent component, if any.
    pub parent: Option<ComponentId>,
    /// Candidate parent components (excludes self and descendants).
    pub parents: Vec<Choice<ComponentId>>,
    /// Interfaces on this component.
    pub interfaces: Vec<LinkRow>,
    /// Assets bound to this component.
    pub assets: Vec<LinkRow>,
    /// Security settings applied to this component (CFG-03).
    pub settings: Vec<SettingRow>,
    /// Controls in the project that can be applied to this component.
    pub available_controls: Vec<Choice<ControlId>>,
    /// Steps that go through this component or its interfaces (CFG-01).
    pub steps: Vec<crate::view::StepThrough>,
    /// Provenance text.
    pub changed: String,
    /// The system it is in, through its top-level ancestor (job 119).
    pub system: Option<ElementId>,
    /// The systems to choose from; empty when the project has none.
    pub systems: Vec<Choice<ElementId>>,
    /// For a child: which system it is in and why, read-only; empty for a
    /// top-level component.
    pub system_from_parent: String,
}

/// One interface in the item definition table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InterfaceRow {
    /// ID.
    pub id: InterfaceId,
    /// Short visual ID (`IF00`, `IF01`, ...).
    pub visual_id: String,
    /// Name.
    pub name: String,
    /// Owning component name.
    pub component: String,
    /// Interface kind label from the methodology.
    pub kind: String,
    /// Number of data flows using this interface.
    pub flows: usize,
}

/// The selected interface.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InterfaceDetail {
    /// ID.
    pub id: InterfaceId,
    /// Short visual ID (`IF00`, `IF01`, ...).
    pub visual_id: String,
    /// Name.
    pub name: String,
    /// Owning component ID.
    pub component: ComponentId,
    /// Owning component name.
    pub component_name: String,
    /// Position in the methodology's interface kinds.
    pub kind: usize,
    /// Data flows using this interface.
    pub flows: Vec<LinkRow>,
    /// Security settings on this interface (own first, then inherited from
    /// the owning component with `inherited = true`; CFG-03, D26).
    pub settings: Vec<SettingRow>,
    /// Controls in the project that can be applied directly to this interface.
    pub available_controls: Vec<Choice<ControlId>>,
    /// Steps that go through this interface (CFG-01).
    pub steps: Vec<crate::view::StepThrough>,
    /// Provenance text.
    pub changed: String,
}

/// One data flow in the item definition table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DataFlowRow {
    /// ID.
    pub id: DataFlowId,
    /// Short visual ID (`DF00`, `DF01`, ...).
    pub visual_id: String,
    /// Name.
    pub name: String,
    /// Source component name.
    pub from: String,
    /// Destination component name.
    pub to: String,
    /// Interface over which this flow travels, or empty.
    pub interface: String,
    /// Whether this flow crosses at least one trust boundary.
    pub crosses_boundary: bool,
}

/// The selected data flow.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DataFlowDetail {
    /// ID.
    pub id: DataFlowId,
    /// Short visual ID (`DF00`, `DF01`, ...).
    pub visual_id: String,
    /// Name.
    pub name: String,
    /// Source component ID.
    pub from: ComponentId,
    /// Source component label.
    pub from_label: String,
    /// Destination component ID.
    pub to: ComponentId,
    /// Destination component label.
    pub to_label: String,
    /// Interface ID over which the flow travels, if any.
    pub interface: Option<InterfaceId>,
    /// Candidate interfaces on `from` or `to`.
    pub interfaces: Vec<Choice<InterfaceId>>,
    /// Whether this flow crosses at least one trust boundary.
    pub crosses_boundary: bool,
    /// Provenance text.
    pub changed: String,
}

/// One trust boundary in the item definition table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrustBoundaryRow {
    /// ID.
    pub id: TrustBoundaryId,
    /// Short visual ID (`TB00`, `TB01`, ...).
    pub visual_id: String,
    /// Name.
    pub name: String,
    /// Number of member components.
    pub members: usize,
}

/// A component as seen from a trust boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrustBoundaryMember {
    /// Component ID.
    pub id: ComponentId,
    /// Short visual ID (`C00`, `C01`, ...).
    pub visual_id: String,
    /// Component name.
    pub name: String,
    /// Whether the component is inside this trust boundary.
    pub member: bool,
}

/// The selected trust boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrustBoundaryDetail {
    /// ID.
    pub id: TrustBoundaryId,
    /// Short visual ID (`TB00`, `TB01`, ...).
    pub visual_id: String,
    /// Name.
    pub name: String,
    /// Every component and whether it is a member.
    pub members: Vec<TrustBoundaryMember>,
    /// Provenance text.
    pub changed: String,
}

fn flow_crosses_boundary(project: &Project, df: &aegis_model::DataFlow) -> bool {
    let (from_c, to_c) = (df.from(), df.to());
    project
        .trust_boundaries()
        .any(|(_, _, tb)| tb.contains(from_c) != tb.contains(to_c))
}

/// Every component in arena order.
#[must_use]
pub fn component_rows(ws: &Workspace) -> Vec<ComponentRow> {
    let project = ws.project();
    let any_system = project.element_count(ElementKind::System) > 0;
    project
        .components()
        .map(|(idx, id, c)| {
            let sys = project.component_system(idx);
            ComponentRow {
                id,
                visual_id: project.component_visual_id(idx).unwrap_or_default(),
                name: c.name().to_owned(),
                parent: c
                    .parent()
                    .and_then(|p| project.component(p))
                    .map_or_else(String::new, |p| p.name().to_owned()),
                interfaces: project
                    .interfaces()
                    .filter(|(_, _, i)| i.component() == idx)
                    .count(),
                assets: project
                    .assets()
                    .filter(|(_, _, a)| a.component() == Some(idx))
                    .count(),
                system: sys
                    .and_then(|s| crate::systems::system_name(project, s))
                    .unwrap_or_default(),
                no_system: any_system && sys.is_none(),
            }
        })
        .collect()
}

fn format_rules(m: &Methodology, rules: &[aegis_model::ControlRule]) -> String {
    if rules.is_empty() {
        return "No rules yet".to_owned();
    }
    rules
        .iter()
        .map(|rule| {
            format!(
                "{} \u{2192} {}",
                m.attack_type_label(rule.attack_type()).unwrap_or("?"),
                feasibility_label(m, rule.ceiling())
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn format_control_rules(project: &Project, c: &aegis_model::SecurityControl) -> String {
    format_rules(project.methodology(), c.rules())
}

fn setting_row(
    project: &Project,
    c_idx: aegis_model::ControlIdx,
    inherited: bool,
    from_component: &str,
) -> Option<SettingRow> {
    let id = project.security_control_id(c_idx)?;
    let c = project.security_control(c_idx)?;
    Some(SettingRow {
        id,
        visual_id: project
            .security_control_visual_id(c_idx)
            .unwrap_or_default(),
        name: c.name().to_owned(),
        rules: format_control_rules(project, c),
        inherited,
        from_component: from_component.to_owned(),
    })
}

fn available_controls_for(
    project: &Project,
    target: aegis_model::ItemRef,
) -> Vec<Choice<ControlId>> {
    project
        .security_controls()
        .filter(|(c_idx, _, _)| !project.control_targets(*c_idx).any(|t| t == target))
        .map(|(c_idx, cid, c)| Choice {
            id: cid,
            visual_id: project
                .security_control_visual_id(c_idx)
                .unwrap_or_default(),
            name: c.name().to_owned(),
        })
        .collect()
}

/// One existing setting offered by "Add setting".
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AddSettingExisting {
    /// The security control.
    pub id: ControlId,
    /// Short visual ID (`SC00`, `SC01`, ...).
    pub visual_id: String,
    /// Control name.
    pub name: String,
    /// How many components and interfaces it is applied to already.
    pub applied_to: usize,
}

/// One pack preset offered by "Add setting".
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AddSettingPreset {
    /// Position in the methodology's control kinds, for `Edit::AddPresetSetting`.
    pub kind: usize,
    /// Label in the asked language; also the name the new setting gets.
    pub name: String,
    /// One-line summary of its rules, e.g. `Debug access \u{2192} Low, Flash read-out \u{2192} Low`.
    pub rules: String,
}

/// What "Add setting" lists for one component or interface (CFG-06).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AddSettingList {
    /// The pack's label, for the heading "From the pack (...)".
    pub pack: String,
    /// Existing settings not applied to the target yet.
    pub existing: Vec<AddSettingExisting>,
    /// Pack presets that fit the target and are not in the project yet.
    pub presets: Vec<AddSettingPreset>,
    /// The trimmed filter text: the name a custom setting would get. May be empty.
    pub custom: String,
}

/// What "Add setting" lists for `target` and a typed `filter` (CFG-06): the
/// existing settings that fit, the pack presets that fit and are not yet in
/// the project, and the custom entry. `None` when `target` is not in the
/// project. Time O(controls * targets + presets), space O(result).
#[must_use]
pub fn add_setting_list(
    ws: &Workspace,
    target: aegis_model::ItemRef,
    filter: &str,
    lang: &str,
) -> Option<AddSettingList> {
    let project = ws.project();
    let m = project.methodology();
    let exists = match target {
        aegis_model::ItemRef::Component(id) => project.component_idx(&id).is_some(),
        aegis_model::ItemRef::Interface(id) => project.interface_idx(&id).is_some(),
    };
    if !exists {
        return None;
    }
    let custom = filter.trim().to_owned();
    let needle = custom.to_lowercase();
    let matches = |name: &str| needle.is_empty() || name.to_lowercase().contains(&needle);
    let existing = project
        .security_controls()
        .filter(|(c_idx, _, c)| {
            matches(c.name())
                && c.kind()
                    .is_none_or(|k| project.control_kind_fits(k, target))
                && !project.control_targets(*c_idx).any(|t| t == target)
        })
        .map(|(c_idx, id, c)| AddSettingExisting {
            id,
            visual_id: project
                .security_control_visual_id(c_idx)
                .unwrap_or_default(),
            name: c.name().to_owned(),
            applied_to: project.control_targets(c_idx).count(),
        })
        .collect();
    let presets = m
        .control_kinds()
        .enumerate()
        .filter(|(_, k)| project.control_kind_fits(*k, target))
        .filter(|(_, k)| {
            !project
                .security_controls()
                .any(|(_, _, c)| c.kind() == Some(*k))
        })
        .filter_map(|(pos, k)| {
            let name = m.control_kind_label_for_lang(k, lang)?;
            matches(name).then(|| AddSettingPreset {
                kind: pos,
                name: name.to_owned(),
                rules: format_rules(m, m.control_kind_rules(k)),
            })
        })
        .collect();
    Some(AddSettingList {
        pack: m.label().to_owned(),
        existing,
        presets,
        custom,
    })
}

/// The selected component's details.
#[must_use]
pub fn component_detail(ws: &Workspace, id: ComponentId) -> Option<ComponentDetail> {
    let project = ws.project();
    let m = project.methodology();
    let idx = project.component_idx(&id)?;
    let c = project.component(idx)?;
    let is_descendant_or_self = |mut cand: aegis_model::ComponentIdx| -> bool {
        for _ in 0..=aegis_model::MAX_COMPONENT_DEPTH {
            if cand == idx {
                return true;
            }
            let Some(p) = project.component(cand).and_then(|x| x.parent()) else {
                return false;
            };
            cand = p;
        }
        true
    };
    let sys = project.component_system(idx);
    let system_from_parent = match c.parent() {
        None => String::new(),
        Some(_) => {
            let mut top = c;
            for _ in 0..=aegis_model::MAX_COMPONENT_DEPTH {
                match top.parent().and_then(|p| project.component(p)) {
                    Some(up) => top = up,
                    None => break,
                }
            }
            let named = sys.and_then(|s| crate::systems::system_name(project, s));
            match named {
                Some(n) => format!("In {n}, with its parent {}", top.name()),
                None => format!("In no system, with its parent {}", top.name()),
            }
        }
    };
    let target = aegis_model::ItemRef::Component(id);
    let settings = project
        .controls_applied_to(target)
        .filter_map(|(c_idx, _, _)| setting_row(project, c_idx, false, ""))
        .collect();
    Some(ComponentDetail {
        steps: crate::view::steps_through(project, target),
        id,
        visual_id: project.component_visual_id(idx).unwrap_or_default(),
        name: c.name().to_owned(),
        parent: c.parent().and_then(|p| project.component_id(p)),
        parents: project
            .components()
            .filter(|(c_idx, _, _)| !is_descendant_or_self(*c_idx))
            .map(|(c_idx, cid, comp)| Choice {
                id: cid,
                visual_id: project.component_visual_id(c_idx).unwrap_or_default(),
                name: comp.name().to_owned(),
            })
            .collect(),
        interfaces: project
            .interfaces()
            .filter(|(_, _, i)| i.component() == idx)
            .map(|(i_idx, iid, i)| LinkRow {
                visual_id: project.interface_visual_id(i_idx).unwrap_or_default(),
                label: i.name().to_owned(),
                detail: m.interface_kind_label(i.kind()).unwrap_or("?").to_owned(),
                target: Selection::Interface(iid),
            })
            .collect(),
        assets: project
            .assets()
            .filter(|(_, _, a)| a.component() == Some(idx))
            .map(|(a_idx, aid, a)| LinkRow {
                visual_id: project.asset_visual_id(a_idx).unwrap_or_default(),
                label: a.name().to_owned(),
                detail: String::new(),
                target: Selection::Asset(aid),
            })
            .collect(),
        settings,
        available_controls: available_controls_for(project, target),
        changed: changed_text(c.provenance()),
        system: sys,
        systems: crate::systems::system_choices(project),
        system_from_parent,
    })
}

/// Every interface in arena order.
#[must_use]
pub fn interface_rows(ws: &Workspace) -> Vec<InterfaceRow> {
    let project = ws.project();
    let m = project.methodology();
    project
        .interfaces()
        .map(|(idx, id, i)| InterfaceRow {
            id,
            visual_id: project.interface_visual_id(idx).unwrap_or_default(),
            name: i.name().to_owned(),
            component: project
                .component(i.component())
                .map_or_else(String::new, |c| c.name().to_owned()),
            kind: m.interface_kind_label(i.kind()).unwrap_or("?").to_owned(),
            flows: project
                .data_flows()
                .filter(|(_, _, df)| df.interface() == Some(idx))
                .count(),
        })
        .collect()
}

/// The selected interface's details.
#[must_use]
pub fn interface_detail(ws: &Workspace, id: InterfaceId) -> Option<InterfaceDetail> {
    let project = ws.project();
    let m = project.methodology();
    let idx = project.interface_idx(&id)?;
    let i = project.interface(idx)?;
    let component = project.component_id(i.component())?;
    let component_name = project
        .component(i.component())
        .map_or_else(String::new, |c| c.name().to_owned());
    let target = aegis_model::ItemRef::Interface(id);
    let direct_indices: Vec<aegis_model::ControlIdx> = project
        .controls_applied_to(target)
        .map(|(c_idx, _, _)| c_idx)
        .collect();
    let mut settings: Vec<SettingRow> = direct_indices
        .iter()
        .filter_map(|&c_idx| setting_row(project, c_idx, false, ""))
        .collect();
    for (c_idx, _, _) in project.controls_applied_to(aegis_model::ItemRef::Component(component)) {
        if !direct_indices.contains(&c_idx)
            && let Some(row) = setting_row(project, c_idx, true, &component_name)
        {
            settings.push(row);
        }
    }
    Some(InterfaceDetail {
        steps: crate::view::steps_through(project, target),
        id,
        visual_id: project.interface_visual_id(idx).unwrap_or_default(),
        name: i.name().to_owned(),
        component,
        component_name,
        kind: m.interface_kinds().position(|k| k == i.kind()).unwrap_or(0),
        flows: project
            .data_flows()
            .filter(|(_, _, df)| df.interface() == Some(idx))
            .map(|(df_idx, df_id, df)| LinkRow {
                visual_id: project.data_flow_visual_id(df_idx).unwrap_or_default(),
                label: df.name().to_owned(),
                detail: format!(
                    "{} -> {}",
                    project.component(df.from()).map_or("?", |c| c.name()),
                    project.component(df.to()).map_or("?", |c| c.name())
                ),
                target: Selection::DataFlow(df_id),
            })
            .collect(),
        settings,
        available_controls: available_controls_for(project, target),
        changed: changed_text(i.provenance()),
    })
}

/// Every data flow in arena order.
#[must_use]
pub fn data_flow_rows(ws: &Workspace) -> Vec<DataFlowRow> {
    let project = ws.project();
    project
        .data_flows()
        .map(|(idx, id, df)| DataFlowRow {
            id,
            visual_id: project.data_flow_visual_id(idx).unwrap_or_default(),
            name: df.name().to_owned(),
            from: project
                .component(df.from())
                .map_or_else(|| "?".to_owned(), |c| c.name().to_owned()),
            to: project
                .component(df.to())
                .map_or_else(|| "?".to_owned(), |c| c.name().to_owned()),
            interface: df
                .interface()
                .and_then(|i| project.interface(i))
                .map_or_else(String::new, |i| i.name().to_owned()),
            crosses_boundary: flow_crosses_boundary(project, df),
        })
        .collect()
}

/// The selected data flow's details.
#[must_use]
pub fn data_flow_detail(ws: &Workspace, id: DataFlowId) -> Option<DataFlowDetail> {
    let project = ws.project();
    let idx = project.data_flow_idx(&id)?;
    let df = project.data_flow(idx)?;
    Some(DataFlowDetail {
        id,
        visual_id: project.data_flow_visual_id(idx).unwrap_or_default(),
        name: df.name().to_owned(),
        from: project.component_id(df.from())?,
        from_label: project
            .component(df.from())
            .map_or_else(|| "?".to_owned(), |c| c.name().to_owned()),
        to: project.component_id(df.to())?,
        to_label: project
            .component(df.to())
            .map_or_else(|| "?".to_owned(), |c| c.name().to_owned()),
        interface: df.interface().and_then(|i| project.interface_id(i)),
        interfaces: project
            .interfaces()
            .filter(|(_, _, i)| i.component() == df.from() || i.component() == df.to())
            .map(|(i_idx, iid, i)| Choice {
                id: iid,
                visual_id: project.interface_visual_id(i_idx).unwrap_or_default(),
                name: i.name().to_owned(),
            })
            .collect(),
        crosses_boundary: flow_crosses_boundary(project, df),
        changed: changed_text(df.provenance()),
    })
}

/// Every trust boundary in arena order.
#[must_use]
pub fn trust_boundary_rows(ws: &Workspace) -> Vec<TrustBoundaryRow> {
    let project = ws.project();
    project
        .trust_boundaries()
        .map(|(idx, id, tb)| TrustBoundaryRow {
            id,
            visual_id: project.trust_boundary_visual_id(idx).unwrap_or_default(),
            name: tb.name().to_owned(),
            members: tb.components().len(),
        })
        .collect()
}

/// The selected trust boundary's details.
#[must_use]
pub fn trust_boundary_detail(ws: &Workspace, id: TrustBoundaryId) -> Option<TrustBoundaryDetail> {
    let project = ws.project();
    let idx = project.trust_boundary_idx(&id)?;
    let tb = project.trust_boundary(idx)?;
    Some(TrustBoundaryDetail {
        id,
        visual_id: project.trust_boundary_visual_id(idx).unwrap_or_default(),
        name: tb.name().to_owned(),
        members: project
            .components()
            .map(|(c_idx, cid, c)| TrustBoundaryMember {
                id: cid,
                visual_id: project.component_visual_id(c_idx).unwrap_or_default(),
                name: c.name().to_owned(),
                member: tb.contains(c_idx),
            })
            .collect(),
        changed: changed_text(tb.provenance()),
    })
}

// ---------------------------------------------------------------------------
// Damage scenarios.
// ---------------------------------------------------------------------------

/// One damage scenario in the table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DamageRow {
    /// ID.
    pub id: DamageId,
    /// Short visual ID (`D00`, `D01`, ...).
    pub visual_id: String,
    /// Name.
    pub name: String,
    /// Per impact category: the level's position, if rated.
    pub impacts: Vec<Option<usize>>,
    /// The ratings as text, for example `Safety: Severe`.
    pub impacts_text: String,
    /// Threats that link it.
    pub threats: usize,
}

/// The selected damage scenario.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DamageDetail {
    /// ID.
    pub id: DamageId,
    /// Short visual ID (`D00`, `D01`, ...).
    pub visual_id: String,
    /// Name.
    pub name: String,
    /// Per impact category: the level's position, if rated.
    pub impacts: Vec<Option<usize>>,
    /// The threats that link it.
    pub threats: Vec<LinkRow>,
    /// The assets those threats target, each once, in asset order.
    pub assets: Vec<LinkRow>,
    /// Provenance text.
    /// Why the impact is rated as it is (FND-09); empty when not given.
    pub impact_rationale: String,
    pub changed: String,
}

fn impacts(m: &Methodology, d: &aegis_model::DamageScenario) -> (Vec<Option<usize>>, String) {
    let mut positions = Vec::with_capacity(m.impact_category_count());
    let mut text = Vec::new();
    for cat in m.impact_categories() {
        let level = d.impact(cat);
        positions.push(level.and_then(|l| m.levels(cat).position(|x| x == l)));
        if let Some(l) = level {
            text.push(format!(
                "{}: {}",
                m.impact_category_label(cat).unwrap_or("?"),
                m.level_label(cat, l).unwrap_or("?")
            ));
        }
    }
    (positions, text.join(", "))
}

/// Every damage scenario. Time O(damage x categories + threats x links).
#[must_use]
pub fn damage_rows(ws: &Workspace) -> Vec<DamageRow> {
    let project = ws.project();
    let m = project.methodology();
    let mut links = std::collections::HashMap::new();
    for (_, _, t) in project.threat_scenarios() {
        for d in t.damages() {
            *links.entry(*d).or_insert(0usize) += 1;
        }
    }
    project
        .damage_scenarios()
        .map(|(idx, id, d)| {
            let (impacts, impacts_text) = impacts(m, d);
            DamageRow {
                id,
                visual_id: project.damage_scenario_visual_id(idx).unwrap_or_default(),
                name: d.name().to_owned(),
                impacts,
                impacts_text,
                threats: links.get(&idx).copied().unwrap_or(0),
            }
        })
        .collect()
}

/// The selected damage scenario's details. Time O(threats + assets x threats).
#[must_use]
pub fn damage_detail(ws: &Workspace, id: DamageId) -> Option<DamageDetail> {
    let project = ws.project();
    let m = project.methodology();
    let idx = project.damage_scenario_idx(&id)?;
    let d = project.damage_scenario(idx)?;
    Some(DamageDetail {
        id,
        visual_id: project.damage_scenario_visual_id(idx).unwrap_or_default(),
        name: d.name().to_owned(),
        impacts: impacts(m, d).0,
        threats: project
            .threat_scenarios()
            .filter(|(_, _, t)| t.damages().contains(&idx))
            .map(|(t_idx, tid, t)| LinkRow {
                visual_id: project.threat_scenario_visual_id(t_idx).unwrap_or_default(),
                label: t.name().to_owned(),
                detail: String::new(),
                target: Selection::Threat(tid),
            })
            .collect(),
        assets: project
            .assets()
            .filter(|(a_idx, _, _)| {
                project
                    .threat_scenarios()
                    .any(|(_, _, t)| t.asset() == *a_idx && t.damages().contains(&idx))
            })
            .map(|(a_idx, aid, a)| LinkRow {
                visual_id: project.asset_visual_id(a_idx).unwrap_or_default(),
                label: a.name().to_owned(),
                detail: String::new(),
                target: Selection::Asset(aid),
            })
            .collect(),
        impact_rationale: d.impact_rationale().to_owned(),
        changed: changed_text(d.provenance()),
    })
}

// ---------------------------------------------------------------------------
// Threat scenarios and the risk register.
// ---------------------------------------------------------------------------

/// A damage scenario as seen from a threat: linked or not.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DamageLink {
    /// The damage scenario.
    pub id: DamageId,
    /// Short visual ID (`D00`, `D01`, ...).
    pub visual_id: String,
    /// Its name.
    pub name: String,
    /// Whether the threat links it.
    pub linked: bool,
}

/// A goal or claim offered in a dropdown.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Choice<Id> {
    /// The entity.
    pub id: Id,
    /// Short visual ID (`C00`, `AT00`, `G00`, ...).
    pub visual_id: String,
    /// Its name.
    pub name: String,
}

/// The selected threat scenario, with everything that decides its risk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThreatDetail {
    /// ID.
    pub id: ThreatId,
    /// Short visual ID (`T00`, `T01`, ...).
    pub visual_id: String,
    /// Name.
    pub name: String,
    /// The targeted asset.
    pub asset: AssetId,
    /// Short visual ID of the targeted asset (`A00`, ...).
    pub asset_visual_id: String,
    /// Its name.
    pub asset_name: String,
    /// Position in the threat categories.
    pub category: usize,
    /// Catalog references as text.
    pub catalog_refs: Vec<String>,
    /// Every damage scenario of the project, linked or not.
    pub damages: Vec<DamageLink>,
    /// Direct feasibility rating, as a position in the levels.
    pub direct_feasibility: Option<usize>,
    /// The linked attack tree.
    pub tree: Option<TreeId>,
    /// The trees that can be linked.
    pub trees: Vec<Choice<TreeId>>,
    /// Treatment, as a position in [`TREATMENTS`].
    pub treatment: Option<usize>,
    /// The linked goal.
    pub goal: Option<GoalId>,
    /// Every goal.
    pub goals: Vec<Choice<GoalId>>,
    /// The linked claim.
    pub claim: Option<ClaimId>,
    /// Every claim.
    pub claims: Vec<Choice<ClaimId>>,
    /// Whether the attack scales to many vehicles (FND-10); `None` is not
    /// rated.
    pub scales: Option<bool>,
    /// Why it scales or not; empty when not given.
    pub scaling_reason: String,
    /// Why the direct feasibility is rated as it is (FND-09).
    pub feasibility_rationale: String,
    /// The feasibility the engine uses (from the tree, else direct).
    pub feasibility: Option<String>,
    /// Residual feasibility (after controls).
    pub residual_feasibility: Option<String>,
    /// Inherent risk.
    pub inherent: Option<RiskView>,
    /// Residual risk.
    pub residual: Option<RiskView>,
    /// How the risk was found, step by step.
    pub why: Vec<String>,
    /// Provenance text.
    pub changed: String,
}

fn explain_risk(
    project: &Project,
    risk: Result<&aegis_model::ThreatRisk, aegis_model::ModelError>,
    what: &str,
) -> Vec<String> {
    let m = project.methodology();
    let Ok(risk) = risk else {
        return vec![format!(
            "{what}: cannot be computed yet. It needs a feasibility (an attack tree or a direct rating) and at least one rated damage scenario."
        )];
    };
    let Some(entry) = risk.deciding_entry() else {
        return vec![format!("{what}: no rated damage scenario is linked.")];
    };
    let damage = project
        .damage_scenario_idx(&entry.damage())
        .and_then(|i| project.damage_scenario(i))
        .map_or("?", |d| d.name());
    let cat = entry.category();
    vec![format!(
        "{what}: {} = {} impact \"{}\" ({}) x feasibility {}. The worst of {} damage rating(s) decides.",
        m.risk_level_label(entry.risk()).unwrap_or("?"),
        m.impact_category_label(cat).unwrap_or("?"),
        damage,
        m.level_label(cat, entry.impact()).unwrap_or("?"),
        feasibility_label(m, entry.feasibility()),
        risk.entries().len(),
    )]
}

/// Settings that cap the step deciding a threat's residual feasibility, each
/// with its source, deciding one first (CFG-05). Empty when nothing caps it.
/// Time O(depth + rules on the step).
fn ceiling_lines(ws: &Workspace, id: ThreatId) -> Vec<String> {
    /// Ceilings written out; the step inspector lists all of them.
    const SHOWN: usize = 3;
    let project = ws.project();
    let m = project.methodology();
    let Some(tree_idx) = project
        .threat_scenario_idx(&id)
        .and_then(|i| project.threat_scenario(i))
        .and_then(aegis_model::ThreatScenario::attack_tree)
    else {
        return Vec::new();
    };
    let (Some(tree_id), Ok(eval)) = (
        project.attack_tree_id(tree_idx),
        ws.cache().evaluate_residual_threat(project, id),
    ) else {
        return Vec::new();
    };
    let Some(step) = project.deciding_step(tree_idx, eval) else {
        return Vec::new();
    };
    let node = step.node_id();
    let ceilings = step.ceilings();
    let mut lines: Vec<String> = ceilings
        .iter()
        .take(SHOWN)
        .filter_map(|c| {
            let name = project.security_control(c.control())?.name();
            let from = match c.source() {
                aegis_model::CeilingSource::ControlRule { target, .. } => {
                    format!(" (from {})", crate::view::through_label(project, target))
                }
                aegis_model::CeilingSource::StepMitigation => " (on this step)".to_owned(),
            };
            Some(format!(
                "Step \"{}\" is capped at {} by {name}{from}.",
                node_label(project, tree_id, node),
                feasibility_label(m, c.feasibility()),
            ))
        })
        .collect();
    if ceilings.len() > SHOWN {
        lines.push(format!("and {} more on that step.", ceilings.len() - SHOWN));
    }
    lines
}

/// The selected threat's details. Time O(damage + trees + goals + claims).
#[must_use]
pub fn threat_detail(ws: &Workspace, id: ThreatId) -> Option<ThreatDetail> {
    let project = ws.project();
    let m = project.methodology();
    let t_idx = project.threat_scenario_idx(&id)?;
    let t = project.threat_scenario(t_idx)?;
    let asset_idx = t.asset();
    let (inherent, residual) = threat_risks(ws, id);
    let mut why = explain_risk(
        project,
        ws.cache().evaluate_risk(project, id),
        "Risk before controls",
    );
    why.extend(explain_risk(
        project,
        ws.cache().evaluate_residual_risk(project, id),
        "Risk after controls",
    ));
    why.extend(ceiling_lines(ws, id));
    if t.attack_tree().is_some() && t.direct_feasibility().is_some() {
        why.push(
            "The attack tree decides the feasibility; the direct rating is kept but not used."
                .to_owned(),
        );
    }
    let feasibility = ws
        .cache()
        .evaluate_threat(project, id)
        .ok()
        .map(|e| feasibility_label(m, *e.root_value()));
    let residual_feasibility = ws
        .cache()
        .evaluate_residual_threat(project, id)
        .ok()
        .map(|e| feasibility_label(m, *e.root_value()));
    Some(ThreatDetail {
        id,
        visual_id: project.threat_scenario_visual_id(t_idx).unwrap_or_default(),
        name: t.name().to_owned(),
        asset: project.asset_id(asset_idx)?,
        asset_visual_id: project.asset_visual_id(asset_idx).unwrap_or_default(),
        asset_name: project
            .asset(asset_idx)
            .map(|a| a.name().to_owned())
            .unwrap_or_default(),
        category: m
            .threat_categories()
            .position(|c| c == t.category())
            .unwrap_or(0),
        catalog_refs: t
            .catalog_refs()
            .iter()
            .map(|r| {
                format!(
                    "{} {}",
                    m.catalog_scheme_label(r.scheme()).unwrap_or("?"),
                    m.display_catalog_ref(r)
                )
            })
            .collect(),
        damages: project
            .damage_scenarios()
            .map(|(idx, did, d)| DamageLink {
                id: did,
                visual_id: project.damage_scenario_visual_id(idx).unwrap_or_default(),
                name: d.name().to_owned(),
                linked: t.damages().contains(&idx),
            })
            .collect(),
        direct_feasibility: t
            .direct_feasibility()
            .and_then(|l| feasibility_position(m, l)),
        tree: t.attack_tree().and_then(|i| project.attack_tree_id(i)),
        trees: project
            .attack_trees()
            .map(|(tr_idx, tid, tree)| Choice {
                id: tid,
                visual_id: project.attack_tree_visual_id(tr_idx).unwrap_or_default(),
                name: tree
                    .get(tree.root())
                    .map(|n| n.label().to_owned())
                    .unwrap_or_default(),
            })
            .collect(),
        treatment: t
            .treatment()
            .and_then(|x| TREATMENTS.iter().position(|y| *y == x)),
        goal: t.goal().and_then(|i| project.cybersecurity_goal_id(i)),
        goals: project
            .cybersecurity_goals()
            .map(|(g_idx, gid, g)| Choice {
                id: gid,
                visual_id: project
                    .cybersecurity_goal_visual_id(g_idx)
                    .unwrap_or_default(),
                name: g.name().to_owned(),
            })
            .collect(),
        claim: t.claim().and_then(|i| project.cybersecurity_claim_id(i)),
        claims: project
            .cybersecurity_claims()
            .map(|(cl_idx, cid, c)| Choice {
                id: cid,
                visual_id: project
                    .cybersecurity_claim_visual_id(cl_idx)
                    .unwrap_or_default(),
                name: c.name().to_owned(),
            })
            .collect(),
        scales: t.scales(),
        scaling_reason: t.scaling_reason().to_owned(),
        feasibility_rationale: t.feasibility_rationale().to_owned(),
        feasibility,
        residual_feasibility,
        inherent,
        residual,
        why,
        changed: changed_text(t.provenance()),
    })
}

/// One line of the risk register.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RiskRow {
    /// The threat.
    pub id: ThreatId,
    /// Short visual ID (`T00`, `T01`, ...).
    pub visual_id: String,
    /// Its name.
    pub threat: String,
    /// The asset's name.
    pub asset: String,
    /// The damage scenario that decides the risk.
    pub damage: String,
    /// Its impact, for example `Safety: Severe`.
    pub impact: String,
    /// Attack feasibility used.
    pub feasibility: String,
    /// Risk before controls.
    pub inherent: Option<RiskView>,
    /// Risk after controls.
    pub residual: Option<RiskView>,
    /// Treatment decision.
    pub treatment: String,
    /// The goal or claim that backs the treatment.
    pub backing: String,
    /// Settings that cap the step deciding the risk after controls, deciding
    /// one first; each control once.
    pub reduced_by: Vec<ReducedByEntry>,
    /// The step that decides the risk after controls; `None` when the threat
    /// has no attack tree or it cannot be evaluated.
    pub deciding_step: Option<DecidingStepView>,
    /// The risk after controls is lower than the risk before controls.
    pub lowered: bool,
}

/// One setting that caps a risk's deciding step.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReducedByEntry {
    /// The setting.
    pub control: ControlId,
    /// Its short visual ID (`SC00`, `SC01`, ...).
    pub visual_id: String,
    /// The control's name.
    pub name: String,
    /// The component or interface it is applied to; empty when attached to
    /// the step by hand.
    pub on: String,
    /// Feasibility label of the ceiling, for example `Low`.
    pub ceiling: String,
}

/// The step that decides a risk after controls.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecidingStepView {
    /// The attack tree.
    pub tree: TreeId,
    /// The step in that tree.
    pub node: NodeId,
    /// The step's text.
    pub label: String,
}

/// Settings that cap the deciding step of a threat, the step itself, and
/// whether the risk after controls is lower than before. Time O(depth + controls).
fn reduction(
    ws: &Workspace,
    id: ThreatId,
) -> (Vec<ReducedByEntry>, Option<DecidingStepView>, bool) {
    let project = ws.project();
    let m = project.methodology();
    let Some(tree_idx) = project
        .threat_scenario_idx(&id)
        .and_then(|i| project.threat_scenario(i))
        .and_then(aegis_model::ThreatScenario::attack_tree)
    else {
        return (Vec::new(), None, false);
    };
    let (Some(tree_id), Ok(eval)) = (
        project.attack_tree_id(tree_idx),
        ws.cache().evaluate_residual_threat(project, id),
    ) else {
        return (Vec::new(), None, false);
    };
    let Some(step) = project.deciding_step(tree_idx, eval) else {
        return (Vec::new(), None, false);
    };
    let view = DecidingStepView {
        tree: tree_id,
        node: step.node_id(),
        label: node_label(project, tree_id, step.node_id()),
    };
    let mut entries: Vec<ReducedByEntry> = Vec::new();
    for c in step.ceilings() {
        if entries.iter().any(|e| e.control == c.control_id()) {
            continue;
        }
        entries.push(ReducedByEntry {
            control: c.control_id(),
            visual_id: project
                .security_control_visual_id(c.control())
                .unwrap_or_default(),
            name: project
                .security_control(c.control())
                .map_or_else(String::new, |k| k.name().to_owned()),
            on: match c.source() {
                aegis_model::CeilingSource::ControlRule { target, .. } => {
                    crate::view::through_label(project, target)
                }
                aegis_model::CeilingSource::StepMitigation => String::new(),
            },
            ceiling: feasibility_label(m, c.feasibility()),
        });
    }
    let level_pos = |r: Result<&aegis_model::ThreatRisk, aegis_model::ModelError>| {
        let level = r.ok().and_then(aegis_model::ThreatRisk::max_risk)?;
        m.risk_levels().position(|(l, _, _)| l == level)
    };
    let lowered = matches!(
        (
            level_pos(ws.cache().evaluate_risk(project, id)),
            level_pos(ws.cache().evaluate_residual_risk(project, id)),
        ),
        (Some(before), Some(after)) if after < before
    );
    (entries, Some(view), lowered)
}

/// Short text for the "Reduced by" table cell: the first setting and how many
/// more, the deciding step when nothing caps it, or a dash. Time O(1).
#[must_use]
pub fn reduced_by_cell(row: &RiskRow) -> String {
    match (row.reduced_by.first(), &row.deciding_step) {
        (Some(first), _) => match row.reduced_by.len() - 1 {
            0 => first.name.clone(),
            n => format!("{} +{n}", first.name),
        },
        (None, Some(step)) => format!("\u{2014} ({})", step.label),
        (None, None) => "\u{2014}".to_owned(),
    }
}

/// Full list of settings for reports and exports, `None` when nothing caps
/// the deciding step. Time O(settings).
#[must_use]
pub fn reduced_by_text(row: &RiskRow) -> Option<String> {
    if row.reduced_by.is_empty() {
        return None;
    }
    Some(
        row.reduced_by
            .iter()
            .map(|e| {
                if e.on.is_empty() {
                    e.name.clone()
                } else {
                    format!("{} ({})", e.name, e.on)
                }
            })
            .collect::<Vec<_>>()
            .join("; "),
    )
}

/// "Lowers no risk", "Lowers 1 risk", "Lowers 3 risks". Time O(1).
#[must_use]
pub fn lowers_label(n: usize) -> String {
    match n {
        0 => "Lowers no risk".to_owned(),
        1 => "Lowers 1 risk".to_owned(),
        n => format!("Lowers {n} risks"),
    }
}

/// The risk register: every threat, worst risk first (ties in project
/// order). Time O(threats log threats).
#[must_use]
pub fn risk_rows(ws: &Workspace) -> Vec<RiskRow> {
    let project = ws.project();
    let m = project.methodology();
    let mut rows: Vec<(usize, RiskRow)> = project
        .threat_scenarios()
        .enumerate()
        .map(|(pos, (t_idx, id, t))| {
            let (inherent, residual) = threat_risks(ws, id);
            let deciding = ws
                .cache()
                .evaluate_risk(project, id)
                .ok()
                .and_then(|r| r.deciding_entry().copied());
            let (damage, impact) = deciding
                .map(|e| {
                    let d = project
                        .damage_scenario_idx(&e.damage())
                        .and_then(|i| project.damage_scenario(i))
                        .map_or_else(String::new, |d| d.name().to_owned());
                    let c = e.category();
                    (
                        d,
                        format!(
                            "{}: {}",
                            m.impact_category_label(c).unwrap_or("?"),
                            m.level_label(c, e.impact()).unwrap_or("?")
                        ),
                    )
                })
                .unwrap_or_default();
            let (reduced_by, deciding_step, lowered) = reduction(ws, id);
            let backing = t
                .goal()
                .and_then(|g| project.cybersecurity_goal(g))
                .map(|g| format!("Goal: {}", g.name()))
                .or_else(|| {
                    t.claim()
                        .and_then(|c| project.cybersecurity_claim(c))
                        .map(|c| format!("Claim: {}", c.name()))
                })
                .unwrap_or_default();
            (
                pos,
                RiskRow {
                    id,
                    visual_id: project.threat_scenario_visual_id(t_idx).unwrap_or_default(),
                    threat: t.name().to_owned(),
                    asset: project
                        .asset(t.asset())
                        .map(|a| a.name().to_owned())
                        .unwrap_or_default(),
                    damage,
                    impact,
                    feasibility: ws
                        .cache()
                        .evaluate_threat(project, id)
                        .ok()
                        .map(|e| feasibility_label(m, *e.root_value()))
                        .unwrap_or_default(),
                    inherent,
                    residual,
                    treatment: t
                        .treatment()
                        .map(treatment_label)
                        .unwrap_or_default()
                        .to_owned(),
                    backing,
                    reduced_by,
                    deciding_step,
                    lowered,
                },
            )
        })
        .collect();
    let rank = |r: &RiskRow| r.inherent.as_ref().map_or(Tone::Unrated, |v| v.tone);
    rows.sort_by(|(pa, a), (pb, b)| rank(b).cmp(&rank(a)).then(pa.cmp(pb)));
    rows.into_iter().map(|(_, r)| r).collect()
}

// ---------------------------------------------------------------------------
// Attack trees.
// ---------------------------------------------------------------------------

/// One attack tree in the list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TreeRow {
    /// ID.
    pub id: TreeId,
    /// Short visual ID (`AT00`, `AT01`, ...).
    pub visual_id: String,
    /// The root's text.
    pub title: String,
    /// Number of nodes.
    pub nodes: usize,
    /// Names of the linked threats.
    pub threats: String,
    /// The evaluated feasibility at the root.
    pub feasibility: String,
}

/// Every attack tree. Time O(trees + threats).
#[must_use]
pub fn tree_rows(ws: &Workspace) -> Vec<TreeRow> {
    let project = ws.project();
    let m = project.methodology();
    project
        .attack_trees()
        .map(|(idx, id, tree)| {
            let threats: Vec<&str> = project
                .threat_scenarios()
                .filter(|(_, _, t)| t.attack_tree() == Some(idx))
                .map(|(_, _, t)| t.name())
                .collect();
            TreeRow {
                id,
                visual_id: project.attack_tree_visual_id(idx).unwrap_or_default(),
                title: tree
                    .get(tree.root())
                    .map(|n| n.label().to_owned())
                    .unwrap_or_default(),
                nodes: tree.len(),
                threats: threats.join(", "),
                feasibility: ws
                    .cache()
                    .evaluate_tree(project, id)
                    .ok()
                    .and_then(|e| e.get(tree.root()).copied())
                    .map(|l| feasibility_label(m, l))
                    .unwrap_or_else(|| "not rated".to_owned()),
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Controls, goals, claims.
// ---------------------------------------------------------------------------

/// One security control.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ControlRow {
    /// ID.
    pub id: ControlId,
    /// Short visual ID (`SC00`, `SC01`, ...).
    pub visual_id: String,
    /// Name.
    pub name: String,
    /// Steps it is attached to.
    pub steps: usize,
    /// Goals it belongs to.
    pub goals: usize,
    /// Risks whose risk after controls is lower and whose deciding step this
    /// control caps.
    pub lowers: usize,
}

/// The selected control.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ControlDetail {
    /// ID.
    pub id: ControlId,
    /// Short visual ID (`SC00`, `SC01`, ...).
    pub visual_id: String,
    /// Name.
    pub name: String,
    /// Components and interfaces this control is applied to (CFG-03).
    pub targets: Vec<LinkRow>,
    /// Attack-type feasibility ceiling rules (CFG-03).
    pub rules: Vec<ControlRuleView>,
    /// Where it is attached: step, tree and the cap.
    pub steps: Vec<LinkRow>,
    /// Goals that list it.
    pub goals: Vec<LinkRow>,
    /// Provenance text.
    pub changed: String,
    /// The pack preset's label, `None` for a custom setting; for the subtitle
    /// "Security setting · Secure boot (pack preset)".
    pub preset: Option<String>,
    /// The rules differ from the pack's current values; the screen offers
    /// `Edit::UsePackValues`.
    pub differs_from_preset: bool,
}

fn control_uses(project: &Project, c_idx: aegis_model::ControlIdx) -> Vec<LinkRow> {
    let m = project.methodology();
    let mut out = Vec::new();
    for (t_idx, tree_id, tree) in project.attack_trees() {
        let title = tree.get(tree.root()).map_or("", |n| n.label());
        let tree_vis = project.attack_tree_visual_id(t_idx).unwrap_or_default();
        for (n_idx, _, node) in tree.iter() {
            if let Some(s) = project
                .step_ceilings(t_idx, n_idx)
                .into_iter()
                .find(|s| s.control() == c_idx)
            {
                out.push(LinkRow {
                    visual_id: tree_vis.clone(),
                    label: node.label().to_owned(),
                    detail: format!(
                        "in \"{title}\", caps feasibility at {}",
                        feasibility_label(m, s.feasibility())
                    ),
                    target: Selection::Tree(tree_id),
                });
            }
        }
    }
    out
}

/// Every control. Time O(controls + tree nodes + goals + threats × depth).
#[must_use]
pub fn control_rows(ws: &Workspace) -> Vec<ControlRow> {
    let project = ws.project();
    let mut steps = std::collections::HashMap::new();
    for (t_idx, _, tree) in project.attack_trees() {
        for (n_idx, _, _) in tree.iter() {
            let mut seen = Vec::new();
            for s in project.step_ceilings(t_idx, n_idx) {
                if !seen.contains(&s.control()) {
                    seen.push(s.control());
                    *steps.entry(s.control()).or_insert(0usize) += 1;
                }
            }
        }
    }
    let mut goals = std::collections::HashMap::new();
    for (_, _, g) in project.cybersecurity_goals() {
        for c in g.controls() {
            *goals.entry(*c).or_insert(0usize) += 1;
        }
    }
    let mut lowers = std::collections::HashMap::new();
    for (_, id, _) in project.threat_scenarios() {
        let (entries, _, lowered) = reduction(ws, id);
        if lowered {
            for e in entries {
                *lowers.entry(e.control).or_insert(0usize) += 1;
            }
        }
    }
    project
        .security_controls()
        .map(|(idx, id, c)| ControlRow {
            id,
            visual_id: project.security_control_visual_id(idx).unwrap_or_default(),
            name: c.name().to_owned(),
            steps: steps.get(&idx).copied().unwrap_or(0),
            goals: goals.get(&idx).copied().unwrap_or(0),
            lowers: lowers.get(&id).copied().unwrap_or(0),
        })
        .collect()
}

/// The selected control's details.
#[must_use]
pub fn control_detail(ws: &Workspace, id: ControlId) -> Option<ControlDetail> {
    let project = ws.project();
    let m = project.methodology();
    let idx = project.security_control_idx(&id)?;
    let c = project.security_control(idx)?;
    let targets = project
        .control_targets(idx)
        .filter_map(|target| match target {
            aegis_model::ItemRef::Component(cid) => {
                let c_idx = project.component_idx(&cid)?;
                let comp = project.component(c_idx)?;
                Some(LinkRow {
                    visual_id: project.component_visual_id(c_idx).unwrap_or_default(),
                    label: comp.name().to_owned(),
                    detail: "Component".to_owned(),
                    target: Selection::Component(cid),
                })
            }
            aegis_model::ItemRef::Interface(iid) => {
                let i_idx = project.interface_idx(&iid)?;
                let iface = project.interface(i_idx)?;
                let owner = project
                    .component(iface.component())
                    .map_or("?", |comp| comp.name());
                Some(LinkRow {
                    visual_id: project.interface_visual_id(i_idx).unwrap_or_default(),
                    label: format!("{owner} / {}", iface.name()),
                    detail: "Interface".to_owned(),
                    target: Selection::Interface(iid),
                })
            }
        })
        .collect();
    let rules = m
        .attack_types()
        .enumerate()
        .map(|(pos, at)| ControlRuleView {
            attack_type: pos,
            label: m.attack_type_label(at).unwrap_or("?").to_owned(),
            level: c.rule_for(at).and_then(|l| feasibility_position(m, l)),
        })
        .collect();
    Some(ControlDetail {
        id,
        visual_id: project.security_control_visual_id(idx).unwrap_or_default(),
        name: c.name().to_owned(),
        targets,
        rules,
        steps: control_uses(project, idx),
        goals: project
            .cybersecurity_goals()
            .filter(|(_, _, g)| g.controls().contains(&idx))
            .map(|(g_idx, gid, g)| LinkRow {
                visual_id: project
                    .cybersecurity_goal_visual_id(g_idx)
                    .unwrap_or_default(),
                label: g.name().to_owned(),
                detail: String::new(),
                target: Selection::Goal(gid),
            })
            .collect(),
        changed: changed_text(c.provenance()),
        preset: c
            .kind()
            .and_then(|k| m.control_kind_label(k))
            .map(str::to_owned),
        differs_from_preset: project.control_differs_from_kind(idx),
    })
}

/// A goal row.
pub type GoalRow = GoalRowOf<GoalId>;
/// A claim row.
pub type ClaimRow = GoalRowOf<ClaimId>;

/// One goal or claim in a list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GoalRowOf<Id> {
    /// ID.
    pub id: Id,
    /// Short visual ID (`G00`, `CL00`, ...).
    pub visual_id: String,
    /// Name or statement.
    pub name: String,
    /// Linked controls (goals) or 0 (claims).
    pub controls: usize,
    /// Threats that reference it.
    pub threats: usize,
}

/// A control as seen from a goal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ControlLink {
    /// The control.
    pub id: ControlId,
    /// Short visual ID (`SC00`, `SC01`, ...).
    pub visual_id: String,
    /// Its name.
    pub name: String,
    /// Whether the goal lists it.
    pub linked: bool,
}

/// The selected goal or claim.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GoalDetail<Id> {
    /// ID.
    pub id: Id,
    /// Short visual ID (`G00`, `CL00`, ...).
    pub visual_id: String,
    /// Name or statement.
    pub name: String,
    /// Every control and whether it is linked (goals only).
    pub controls: Vec<ControlLink>,
    /// Threats that reference it.
    pub threats: Vec<LinkRow>,
    /// Provenance text.
    pub changed: String,
}

/// Every goal. Time O(goals + threats).
#[must_use]
pub fn goal_rows(ws: &Workspace) -> Vec<GoalRow> {
    let project = ws.project();
    project
        .cybersecurity_goals()
        .map(|(idx, id, g)| GoalRowOf {
            id,
            visual_id: project
                .cybersecurity_goal_visual_id(idx)
                .unwrap_or_default(),
            name: g.name().to_owned(),
            controls: g.controls().len(),
            threats: project
                .threat_scenarios()
                .filter(|(_, _, t)| t.goal() == Some(idx))
                .count(),
        })
        .collect()
}

/// Every claim. Time O(claims + threats).
#[must_use]
pub fn claim_rows(ws: &Workspace) -> Vec<ClaimRow> {
    let project = ws.project();
    project
        .cybersecurity_claims()
        .map(|(idx, id, c)| GoalRowOf {
            id,
            visual_id: project
                .cybersecurity_claim_visual_id(idx)
                .unwrap_or_default(),
            name: c.name().to_owned(),
            controls: 0,
            threats: project
                .threat_scenarios()
                .filter(|(_, _, t)| t.claim() == Some(idx))
                .count(),
        })
        .collect()
}

/// The selected goal's details.
#[must_use]
pub fn goal_detail(ws: &Workspace, id: GoalId) -> Option<GoalDetail<GoalId>> {
    let project = ws.project();
    let idx = project.cybersecurity_goal_idx(&id)?;
    let g = project.cybersecurity_goal(idx)?;
    Some(GoalDetail {
        id,
        visual_id: project
            .cybersecurity_goal_visual_id(idx)
            .unwrap_or_default(),
        name: g.name().to_owned(),
        controls: project
            .security_controls()
            .map(|(c_idx, cid, c)| ControlLink {
                id: cid,
                visual_id: project
                    .security_control_visual_id(c_idx)
                    .unwrap_or_default(),
                name: c.name().to_owned(),
                linked: g.controls().contains(&c_idx),
            })
            .collect(),
        threats: project
            .threat_scenarios()
            .filter(|(_, _, t)| t.goal() == Some(idx))
            .map(|(t_idx, tid, t)| LinkRow {
                visual_id: project.threat_scenario_visual_id(t_idx).unwrap_or_default(),
                label: t.name().to_owned(),
                detail: t
                    .treatment()
                    .map(treatment_label)
                    .unwrap_or_default()
                    .to_owned(),
                target: Selection::Threat(tid),
            })
            .collect(),
        changed: changed_text(g.provenance()),
    })
}

/// The selected claim's details.
#[must_use]
pub fn claim_detail(ws: &Workspace, id: ClaimId) -> Option<GoalDetail<ClaimId>> {
    let project = ws.project();
    let idx = project.cybersecurity_claim_idx(&id)?;
    let c = project.cybersecurity_claim(idx)?;
    Some(GoalDetail {
        id,
        visual_id: project
            .cybersecurity_claim_visual_id(idx)
            .unwrap_or_default(),
        name: c.name().to_owned(),
        controls: Vec::new(),
        threats: project
            .threat_scenarios()
            .filter(|(_, _, t)| t.claim() == Some(idx))
            .map(|(t_idx, tid, t)| LinkRow {
                visual_id: project.threat_scenario_visual_id(t_idx).unwrap_or_default(),
                label: t.name().to_owned(),
                detail: t
                    .treatment()
                    .map(treatment_label)
                    .unwrap_or_default()
                    .to_owned(),
                target: Selection::Threat(tid),
            })
            .collect(),
        changed: changed_text(c.provenance()),
    })
}

// ---------------------------------------------------------------------------
// Problems.
// ---------------------------------------------------------------------------

/// One lint finding, in words, with a way to get to it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProblemRow {
    /// Blocks evaluation or treatment (error) or not (warning).
    pub error: bool,
    /// Short visual ID of the target element (`A00`, `T01`, ...).
    pub visual_id: String,
    /// What is wrong, naming the entity.
    pub text: String,
    /// The entity to open.
    pub target: Option<Selection>,
}

/// Short human-readable visual ID for `s` (`A00`, `T01`, `AT00`, ...).
#[must_use]
pub fn selection_visual_id(project: &Project, s: Selection) -> String {
    match s {
        Selection::Component(id) => project
            .component_idx(&id)
            .and_then(|i| project.component_visual_id(i)),
        Selection::Interface(id) => project
            .interface_idx(&id)
            .and_then(|i| project.interface_visual_id(i)),
        Selection::DataFlow(id) => project
            .data_flow_idx(&id)
            .and_then(|i| project.data_flow_visual_id(i)),
        Selection::TrustBoundary(id) => project
            .trust_boundary_idx(&id)
            .and_then(|i| project.trust_boundary_visual_id(i)),
        Selection::Asset(id) => project
            .asset_idx(&id)
            .and_then(|i| project.asset_visual_id(i)),
        Selection::Damage(id) => project
            .damage_scenario_idx(&id)
            .and_then(|i| project.damage_scenario_visual_id(i)),
        Selection::Threat(id) => project
            .threat_scenario_idx(&id)
            .and_then(|i| project.threat_scenario_visual_id(i)),
        Selection::Tree(id) | Selection::TreeNode(id, _) => project
            .attack_tree_idx(&id)
            .and_then(|i| project.attack_tree_visual_id(i)),
        Selection::Control(id) => project
            .security_control_idx(&id)
            .and_then(|i| project.security_control_visual_id(i)),
        Selection::Goal(id) => project
            .cybersecurity_goal_idx(&id)
            .and_then(|i| project.cybersecurity_goal_visual_id(i)),
        Selection::Claim(id) => project
            .cybersecurity_claim_idx(&id)
            .and_then(|i| project.cybersecurity_claim_visual_id(i)),
    }
    .unwrap_or_default()
}

fn named(project: &Project, s: Selection) -> &str {
    match s {
        Selection::Component(id) => project
            .component_idx(&id)
            .and_then(|i| project.component(i))
            .map(aegis_model::Component::name),
        Selection::Interface(id) => project
            .interface_idx(&id)
            .and_then(|i| project.interface(i))
            .map(aegis_model::Interface::name),
        Selection::DataFlow(id) => project
            .data_flow_idx(&id)
            .and_then(|i| project.data_flow(i))
            .map(aegis_model::DataFlow::name),
        Selection::TrustBoundary(id) => project
            .trust_boundary_idx(&id)
            .and_then(|i| project.trust_boundary(i))
            .map(aegis_model::TrustBoundary::name),
        Selection::Asset(id) => project
            .asset_idx(&id)
            .and_then(|i| project.asset(i))
            .map(aegis_model::Asset::name),
        Selection::Damage(id) => project
            .damage_scenario_idx(&id)
            .and_then(|i| project.damage_scenario(i))
            .map(aegis_model::DamageScenario::name),
        Selection::Threat(id) => project
            .threat_scenario_idx(&id)
            .and_then(|i| project.threat_scenario(i))
            .map(aegis_model::ThreatScenario::name),
        Selection::Tree(id) => project
            .attack_tree_idx(&id)
            .and_then(|i| project.attack_tree(i))
            .and_then(|t| t.get(t.root()))
            .map(aegis_model::Node::label),
        Selection::TreeNode(tree, node) => project
            .attack_tree_idx(&tree)
            .and_then(|i| project.attack_tree(i))
            .and_then(|t| t.idx_of(&node).and_then(|n| t.get(n)))
            .map(aegis_model::Node::label),
        Selection::Control(id) => project
            .security_control_idx(&id)
            .and_then(|i| project.security_control(i))
            .map(aegis_model::SecurityControl::name),
        Selection::Goal(id) => project
            .cybersecurity_goal_idx(&id)
            .and_then(|i| project.cybersecurity_goal(i))
            .map(aegis_model::CybersecurityGoal::name),
        Selection::Claim(id) => project
            .cybersecurity_claim_idx(&id)
            .and_then(|i| project.cybersecurity_claim(i))
            .map(aegis_model::CybersecurityClaim::name),
    }
    .unwrap_or("?")
}

fn cited(project: &Project, s: Selection) -> String {
    let vis = selection_visual_id(project, s);
    let name = named(project, s);
    if vis.is_empty() {
        format!("\"{name}\"")
    } else {
        format!("{vis} \"{name}\"")
    }
}

fn node_label(project: &Project, tree: TreeId, node: aegis_model::NodeId) -> String {
    project
        .attack_tree_idx(&tree)
        .and_then(|i| project.attack_tree(i))
        .and_then(|t| t.idx_of(&node).and_then(|n| t.get(n)))
        .map_or_else(|| "?".to_owned(), |n| n.label().to_owned())
}

/// Turns one lint code into a sentence and a target.
#[must_use]
pub fn problem(project: &Project, code: LintCode) -> ProblemRow {
    use LintCode as L;
    let m = project.methodology();
    let q = |s: Selection| cited(project, s);
    let (text, target) = match code {
        L::AssetWithoutThreats { asset } => {
            let s = Selection::Asset(asset);
            (format!("Asset {} has no threat scenarios.", q(s)), s)
        }
        L::AssetPropertyUntested { asset, property } => {
            let s = Selection::Asset(asset);
            (
                format!(
                    "No threat scenario on asset {} violates its property \"{}\".",
                    q(s),
                    m.property_label(property).unwrap_or("?")
                ),
                s,
            )
        }
        L::DamageScenarioWithoutImpacts { damage } => {
            let s = Selection::Damage(damage);
            (
                format!(
                    "{} {} has no impact rating.",
                    m.element_names().damage_scenario.singular,
                    q(s)
                ),
                s,
            )
        }
        L::ImpactWithoutRationale { damage } => {
            let s = Selection::Damage(damage);
            (
                format!(
                    "{} {} has an impact rating without a rationale.",
                    m.element_names().damage_scenario.singular,
                    q(s)
                ),
                s,
            )
        }
        L::FeasibilityWithoutRationale { threat } => {
            let s = Selection::Threat(threat);
            (
                format!(
                    "{} {} has a feasibility rating without a rationale.",
                    m.element_names().threat_scenario.singular,
                    q(s)
                ),
                s,
            )
        }
        L::DamageScenarioUnlinked { damage } => {
            let s = Selection::Damage(damage);
            (
                format!(
                    "No {} leads to {} {}.",
                    m.element_names().threat_scenario.singular.to_lowercase(),
                    m.element_names().damage_scenario.singular.to_lowercase(),
                    q(s)
                ),
                s,
            )
        }
        L::ThreatWithoutDamages { threat } => {
            let s = Selection::Threat(threat);
            (
                format!(
                    "{} {} links no {}, so it has no risk.",
                    m.element_names().threat_scenario.singular,
                    q(s),
                    m.element_names().damage_scenario.singular.to_lowercase()
                ),
                s,
            )
        }
        L::ThreatWithoutFeasibility { threat } => {
            let s = Selection::Threat(threat);
            (
                format!(
                    "{} {} has no {} and no direct feasibility rating.",
                    m.element_names().threat_scenario.singular,
                    q(s),
                    m.element_names().attack_tree.singular.to_lowercase()
                ),
                s,
            )
        }
        L::RedundantDirectFeasibility { threat } => {
            let s = Selection::Threat(threat);
            (
                format!(
                    "{} {} has a direct rating that its {} overrides.",
                    m.element_names().threat_scenario.singular,
                    q(s),
                    m.element_names().attack_tree.singular.to_lowercase()
                ),
                s,
            )
        }
        L::ThreatWithoutTreatment { threat } => {
            let s = Selection::Threat(threat);
            (
                format!(
                    "{} {} has no treatment decision.",
                    m.element_names().threat_scenario.singular,
                    q(s)
                ),
                s,
            )
        }
        L::ReduceTreatmentWithoutControls { threat } => {
            let s = Selection::Threat(threat);
            (
                format!(
                    "{} {} is reduced, but no control mitigates any of its attack steps.",
                    m.element_names().threat_scenario.singular,
                    q(s)
                ),
                s,
            )
        }
        L::ReduceTreatmentWithoutGoal { threat } => {
            let s = Selection::Threat(threat);
            (
                format!(
                    "{} {} is reduced but has no {}.",
                    m.element_names().threat_scenario.singular,
                    q(s),
                    m.element_names().cybersecurity_goal.singular.to_lowercase()
                ),
                s,
            )
        }
        L::RetainOrShareTreatmentWithoutClaim { threat } => {
            let s = Selection::Threat(threat);
            (
                format!(
                    "{} {} is retained or shared but has no {}.",
                    m.element_names().threat_scenario.singular,
                    q(s),
                    m.element_names()
                        .cybersecurity_claim
                        .singular
                        .to_lowercase()
                ),
                s,
            )
        }
        L::GoalOnNonReduceThreat { threat, goal } => {
            let s = Selection::Threat(threat);
            (
                format!(
                    "{} {} has goal {} but its treatment is not Reduce.",
                    m.element_names().threat_scenario.singular,
                    q(s),
                    q(Selection::Goal(goal))
                ),
                s,
            )
        }
        L::ClaimOnNonRetainOrShareThreat { threat, claim } => {
            let s = Selection::Threat(threat);
            (
                format!(
                    "{} {} has claim {} but its treatment is not Retain or Share.",
                    m.element_names().threat_scenario.singular,
                    q(s),
                    q(Selection::Claim(claim))
                ),
                s,
            )
        }
        L::AttackTreeUnlinked { tree } => {
            let s = Selection::Tree(tree);
            (
                format!(
                    "{} {} is not linked to a {}.",
                    m.element_names().attack_tree.singular,
                    q(s),
                    m.element_names().threat_scenario.singular.to_lowercase()
                ),
                s,
            )
        }
        L::AttackTreeGateEmpty { tree, node } => {
            let s = Selection::Tree(tree);
            (
                format!(
                    "Gate \"{}\" in {} {} has no children.",
                    node_label(project, tree, node),
                    m.element_names().attack_tree.singular.to_lowercase(),
                    q(s)
                ),
                s,
            )
        }
        L::LibraryRatingNotReviewed { tree, node } => {
            let s = Selection::Tree(tree);
            (
                format!(
                    "Step \"{}\" in {} {} has a rating copied from the library that nobody has reviewed.",
                    node_label(project, tree, node),
                    m.element_names().attack_tree.singular.to_lowercase(),
                    q(s)
                ),
                s,
            )
        }
        L::AttackTreeStepUnrated { tree, node } => {
            let s = Selection::Tree(tree);
            (
                format!(
                    "Step \"{}\" in {} {} is not rated.",
                    node_label(project, tree, node),
                    m.element_names().attack_tree.singular.to_lowercase(),
                    q(s)
                ),
                s,
            )
        }
        L::AttackTreeStepIncompleteFactors { tree, node } => {
            let s = Selection::Tree(tree);
            (
                format!(
                    "Step \"{}\" in {} {} has some attack potential factors but not all.",
                    node_label(project, tree, node),
                    m.element_names().attack_tree.singular.to_lowercase(),
                    q(s)
                ),
                s,
            )
        }
        L::RedundantStepFeasibility { tree, node } => {
            let s = Selection::Tree(tree);
            (
                format!(
                    "Step \"{}\" in {} {} has a direct rating that its attack potential overrides.",
                    node_label(project, tree, node),
                    m.element_names().attack_tree.singular.to_lowercase(),
                    q(s)
                ),
                s,
            )
        }
        L::StepSettingsIncomplete { tree, node } => {
            let s = Selection::Tree(tree);
            (
                format!(
                    "Step \"{}\" in {} {} needs both a Through item and an Attack type, or no security setting can apply to it.",
                    node_label(project, tree, node),
                    m.element_names().attack_tree.singular.to_lowercase(),
                    q(s)
                ),
                s,
            )
        }
        L::SecurityControlUnused { control } => {
            let s = Selection::Control(control);
            (
                format!("Control {} is not attached to any attack step.", q(s)),
                s,
            )
        }
        L::ControlRulesDifferFromKind { control } => {
            let s = Selection::Control(control);
            let label = project
                .security_control_idx(&control)
                .and_then(|idx| project.security_control(idx))
                .and_then(|c| c.kind())
                .and_then(|k| m.control_kind_label(k))
                .unwrap_or("?");
            (
                format!(
                    "Setting {} has rules that differ from its pack preset \"{label}\".",
                    q(s)
                ),
                s,
            )
        }
        L::CybersecurityGoalWithoutControls { goal } => {
            let s = Selection::Goal(goal);
            (
                format!(
                    "{} {} has no controls.",
                    m.element_names().cybersecurity_goal.singular,
                    q(s)
                ),
                s,
            )
        }
        L::CybersecurityGoalUnlinked { goal } => {
            let s = Selection::Goal(goal);
            (
                format!(
                    "No {} uses {} {}.",
                    m.element_names().threat_scenario.singular.to_lowercase(),
                    m.element_names().cybersecurity_goal.singular.to_lowercase(),
                    q(s)
                ),
                s,
            )
        }
        L::CybersecurityClaimUnlinked { claim } => {
            let s = Selection::Claim(claim);
            (
                format!(
                    "No {} uses {} {}.",
                    m.element_names().threat_scenario.singular.to_lowercase(),
                    m.element_names()
                        .cybersecurity_claim
                        .singular
                        .to_lowercase(),
                    q(s)
                ),
                s,
            )
        }
        L::AssetUnbound { asset } => {
            let s = Selection::Asset(asset);
            (
                format!(
                    "{} {} is not bound to any {}.",
                    m.element_names().asset.singular,
                    q(s),
                    m.element_names().component.singular.to_lowercase()
                ),
                s,
            )
        }
        L::ComponentWithoutSystem { component } => {
            let s = Selection::Component(component);
            (
                format!(
                    "{} {} is in no system.",
                    m.element_names().component.singular,
                    q(s)
                ),
                s,
            )
        }
        L::SystemBoundaryFlowWithoutThreat { flow } => {
            let s = Selection::DataFlow(flow);
            (
                format!(
                    "{} {} links two systems but has no {}.",
                    m.element_names().data_flow.singular,
                    q(s),
                    m.element_names().threat_scenario.singular.to_lowercase()
                ),
                s,
            )
        }
        L::BoundaryCrossingDataFlowWithoutThreat { flow } => {
            let s = Selection::DataFlow(flow);
            (
                format!(
                    "{} {} crosses a {} but has no {}.",
                    m.element_names().data_flow.singular,
                    q(s),
                    m.element_names().trust_boundary.singular.to_lowercase(),
                    m.element_names().threat_scenario.singular.to_lowercase()
                ),
                s,
            )
        }
    };
    ProblemRow {
        error: code.severity() == LintSeverity::Error,
        visual_id: selection_visual_id(project, target),
        text,
        target: Some(target),
    }
}

/// Every problem, errors first. Time O(lint).
#[must_use]
pub fn problems(ws: &Workspace) -> Vec<ProblemRow> {
    let project = ws.project();
    let mut rows: Vec<ProblemRow> = project
        .lint()
        .iter()
        .map(|f| problem(project, f.code()))
        .collect();
    rows.sort_by_key(|r| !r.error);
    rows
}

// ---------------------------------------------------------------------------
// Overview.
// ---------------------------------------------------------------------------

/// A count with a label.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Count {
    /// What is counted.
    pub label: String,
    /// How many.
    pub count: usize,
    /// Color bucket (risk distribution only).
    pub tone: Tone,
}

/// The project at a glance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Overview {
    /// Entities by kind.
    pub counts: Vec<Count>,
    /// Threats per inherent risk level, highest first, then "not rated".
    pub inherent: Vec<Count>,
    /// Threats per residual risk level.
    pub residual: Vec<Count>,
    /// Problems that block (errors).
    pub errors: usize,
    /// Problems that do not block.
    pub warnings: usize,
    /// The five highest risks.
    pub top: Vec<RiskRow>,
}

fn distribution(ws: &Workspace, residual: bool) -> Vec<Count> {
    let project = ws.project();
    let m = project.methodology();
    let levels: Vec<(aegis_model::RiskLevel, String)> = m
        .risk_levels()
        .map(|(l, _, label)| (l, label.to_owned()))
        .collect();
    let mut counts = vec![0usize; levels.len() + 1];
    for (_, id, _) in project.threat_scenarios() {
        let r = if residual {
            ws.cache().evaluate_residual_risk(project, id)
        } else {
            ws.cache().evaluate_risk(project, id)
        };
        let slot = r
            .ok()
            .and_then(aegis_model::ThreatRisk::max_risk)
            .and_then(|l| levels.iter().position(|(x, _)| *x == l))
            .unwrap_or(levels.len());
        counts[slot] += 1;
    }
    let n = levels.len();
    let mut out: Vec<Count> = levels
        .into_iter()
        .enumerate()
        .rev()
        .map(|(i, (_, label))| Count {
            label,
            count: counts[i],
            tone: Tone::from_level(i, n),
        })
        .collect();
    out.push(Count {
        label: "Not rated".to_owned(),
        count: counts[n],
        tone: Tone::Unrated,
    });
    out
}

/// The overview. Time O(threats log threats + lint).
#[must_use]
pub fn overview(ws: &Workspace) -> Overview {
    let p = ws.project();
    let names = p.methodology().element_names();
    let c = |label: &str, count| Count {
        label: label.to_owned(),
        count,
        tone: Tone::Unrated,
    };
    let lint = p.lint();
    let errors = lint
        .iter()
        .filter(|f| f.severity() == LintSeverity::Error)
        .count();
    let mut top = risk_rows(ws);
    top.truncate(5);
    let mut counts = vec![
        c(&names.asset.plural, p.asset_count()),
        c(&names.damage_scenario.plural, p.damage_scenario_count()),
        c(&names.threat_scenario.plural, p.threat_scenario_count()),
        c(&names.attack_tree.plural, p.attack_tree_count()),
        c(&names.security_control.plural, p.security_control_count()),
        c(
            &names.cybersecurity_goal.plural,
            p.cybersecurity_goal_count(),
        ),
        c(
            &names.cybersecurity_claim.plural,
            p.cybersecurity_claim_count(),
        ),
    ];
    if p.component_count() > 0
        || p.interface_count() > 0
        || p.data_flow_count() > 0
        || p.trust_boundary_count() > 0
    {
        counts.push(c(&names.component.plural, p.component_count()));
        counts.push(c(&names.interface.plural, p.interface_count()));
        counts.push(c(&names.data_flow.plural, p.data_flow_count()));
        counts.push(c(&names.trust_boundary.plural, p.trust_boundary_count()));
    }
    Overview {
        counts,
        inherent: distribution(ws, false),
        residual: distribution(ws, true),
        errors,
        warnings: lint.len() - errors,
        top,
    }
}

/// The page each selection belongs to.
#[must_use]
pub fn page_of(s: Selection) -> crate::view::Page {
    use crate::view::Page;
    match s {
        Selection::Component(_)
        | Selection::Interface(_)
        | Selection::DataFlow(_)
        | Selection::TrustBoundary(_) => Page::Item,
        Selection::Asset(_) => Page::Assets,
        Selection::Damage(_) => Page::Damage,
        Selection::Threat(_) => Page::Threats,
        Selection::Tree(_) | Selection::TreeNode(_, _) => Page::Trees,
        Selection::Control(_) => Page::Controls,
        Selection::Goal(_) | Selection::Claim(_) => Page::Goals,
    }
}

/// Applies a selection to the UI state.
pub fn select(ui: &mut UiState, s: Selection) {
    match s {
        Selection::Component(id) => ui.selected_component = Some(id),
        Selection::Interface(id) => ui.selected_interface = Some(id),
        Selection::DataFlow(id) => ui.selected_data_flow = Some(id),
        Selection::TrustBoundary(id) => ui.selected_trust_boundary = Some(id),
        Selection::Asset(id) => ui.selected_asset = Some(id),
        Selection::Damage(id) => ui.selected_damage = Some(id),
        Selection::Threat(id) => {
            ui.selected_threat = Some(id);
        }
        Selection::Tree(id) => {
            if ui.selected_tree != Some(id) {
                ui.selected_node = None;
            }
            ui.selected_tree = Some(id);
        }
        Selection::TreeNode(tree, node) => {
            ui.selected_tree = Some(tree);
            ui.selected_node = Some(node);
        }
        Selection::Control(id) => ui.selected_control = Some(id),
        Selection::Goal(id) => ui.selected_goal = Some(id),
        Selection::Claim(id) => ui.selected_claim = Some(id),
    }
}

// ---------------------------------------------------------------------------
// Command palette element index (FND-07).
// ---------------------------------------------------------------------------

/// One element entry for the command palette (FND-07).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaletteElement {
    /// Short human-readable visual ID (`A00`, `T01`, `AT00`, ...).
    pub visual_id: String,
    /// Element name or title.
    pub name: String,
    /// Element type singular label (`Asset`, `Threat scenario`, ...).
    pub kind: String,
    /// Selection target when picked.
    pub target: Selection,
}

/// Every project element indexed by visual ID and name for the command palette.
#[must_use]
pub fn palette_elements(ws: &Workspace) -> Vec<PaletteElement> {
    let p = ws.project();
    let names = p.methodology().element_names();
    let mut out = Vec::new();
    for (idx, id, a) in p.assets() {
        out.push(PaletteElement {
            visual_id: p.asset_visual_id(idx).unwrap_or_default(),
            name: a.name().to_owned(),
            kind: names.asset.singular.to_string(),
            target: Selection::Asset(id),
        });
    }
    for (idx, id, d) in p.damage_scenarios() {
        out.push(PaletteElement {
            visual_id: p.damage_scenario_visual_id(idx).unwrap_or_default(),
            name: d.name().to_owned(),
            kind: names.damage_scenario.singular.to_string(),
            target: Selection::Damage(id),
        });
    }
    for (idx, id, t) in p.threat_scenarios() {
        out.push(PaletteElement {
            visual_id: p.threat_scenario_visual_id(idx).unwrap_or_default(),
            name: t.name().to_owned(),
            kind: names.threat_scenario.singular.to_string(),
            target: Selection::Threat(id),
        });
    }
    for (idx, id, tree) in p.attack_trees() {
        out.push(PaletteElement {
            visual_id: p.attack_tree_visual_id(idx).unwrap_or_default(),
            name: tree
                .get(tree.root())
                .map(|n| n.label().to_owned())
                .unwrap_or_default(),
            kind: names.attack_tree.singular.to_string(),
            target: Selection::Tree(id),
        });
    }
    for (idx, id, c) in p.security_controls() {
        out.push(PaletteElement {
            visual_id: p.security_control_visual_id(idx).unwrap_or_default(),
            name: c.name().to_owned(),
            kind: names.security_control.singular.to_string(),
            target: Selection::Control(id),
        });
    }
    for (idx, id, g) in p.cybersecurity_goals() {
        out.push(PaletteElement {
            visual_id: p.cybersecurity_goal_visual_id(idx).unwrap_or_default(),
            name: g.name().to_owned(),
            kind: names.cybersecurity_goal.singular.to_string(),
            target: Selection::Goal(id),
        });
    }
    for (idx, id, c) in p.cybersecurity_claims() {
        out.push(PaletteElement {
            visual_id: p.cybersecurity_claim_visual_id(idx).unwrap_or_default(),
            name: c.name().to_owned(),
            kind: names.cybersecurity_claim.singular.to_string(),
            target: Selection::Claim(id),
        });
    }
    for (idx, id, c) in p.components() {
        out.push(PaletteElement {
            visual_id: p.component_visual_id(idx).unwrap_or_default(),
            name: c.name().to_owned(),
            kind: names.component.singular.to_string(),
            target: Selection::Component(id),
        });
    }
    for (idx, id, i) in p.interfaces() {
        out.push(PaletteElement {
            visual_id: p.interface_visual_id(idx).unwrap_or_default(),
            name: i.name().to_owned(),
            kind: names.interface.singular.to_string(),
            target: Selection::Interface(id),
        });
    }
    for (idx, id, df) in p.data_flows() {
        out.push(PaletteElement {
            visual_id: p.data_flow_visual_id(idx).unwrap_or_default(),
            name: df.name().to_owned(),
            kind: names.data_flow.singular.to_string(),
            target: Selection::DataFlow(id),
        });
    }
    for (idx, id, tb) in p.trust_boundaries() {
        out.push(PaletteElement {
            visual_id: p.trust_boundary_visual_id(idx).unwrap_or_default(),
            name: tb.name().to_owned(),
            kind: names.trust_boundary.singular.to_string(),
            target: Selection::TrustBoundary(id),
        });
    }
    out
}

// ---------------------------------------------------------------------------
// Versions: comparison with a baseline.
// ---------------------------------------------------------------------------

/// One line of a comparison.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffLine {
    /// `Added`, `Removed`, `Changed` or `Risk`.
    pub kind: String,
    /// What, in words.
    pub text: String,
}

/// The current state compared with a baseline.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Comparison {
    /// The baseline's name.
    pub baseline: String,
    /// One sentence.
    pub summary: String,
    /// The changes, entity kind by kind, then the risks that moved.
    pub lines: Vec<DiffLine>,
}

fn risk_snapshot_text(m: &Methodology, s: Option<aegis_model::RiskSnapshot>) -> String {
    match s {
        None => "not evaluated".to_owned(),
        Some(s) => match s.max_risk() {
            Some(r) => m.risk_level_label(r).unwrap_or("?").to_owned(),
            None => format!("feasibility {}", feasibility_label(m, s.feasibility())),
        },
    }
}

/// Compares `base` (a baseline) with `current`. Time proportional to the
/// change (rule A7, `Project::diff`) plus the lines written.
///
/// # Errors
///
/// The model's error if the two use different methodologies.
pub fn comparison(
    base: &Project,
    baseline: &str,
    current: &Project,
) -> Result<Comparison, aegis_model::ModelError> {
    let diff = base.diff(current)?;
    let m = current.methodology();
    let mut lines = Vec::new();
    let mut push = |kind: &str, text: String| {
        lines.push(DiffLine {
            kind: kind.to_owned(),
            text,
        });
    };
    macro_rules! entity {
        ($what:literal, $set:expr, $cite:expr) => {{
            let set = $set;
            for id in set.added() {
                push("Added", format!("{} {}", $what, $cite(current, *id)));
            }
            for id in set.removed() {
                push("Removed", format!("{} {}", $what, $cite(base, *id)));
            }
            for id in set.modified() {
                push("Changed", format!("{} {}", $what, $cite(current, *id)));
            }
        }};
    }
    entity!("Component", diff.components(), |p: &Project, id| cited(
        p,
        Selection::Component(id)
    ));
    {
        let sys_of =
            |p: &Project, id: ComponentId| p.component_idx(&id).and_then(|i| p.component_system(i));
        let sys_text = |p: &Project, s: Option<ElementId>| {
            s.and_then(|s| crate::systems::system_name(p, s))
                .map_or_else(|| "no system".to_owned(), |n| format!("system \"{n}\""))
        };
        for id in diff.components().modified() {
            let (old, new) = (sys_of(base, *id), sys_of(current, *id));
            if old != new {
                push(
                    "Changed",
                    format!(
                        "Component {}: moved from {} to {}",
                        cited(current, Selection::Component(*id)),
                        sys_text(base, old),
                        sys_text(current, new)
                    ),
                );
            }
        }
    }
    entity!("Interface", diff.interfaces(), |p: &Project, id| cited(
        p,
        Selection::Interface(id)
    ));
    entity!("Data flow", diff.data_flows(), |p: &Project, id| cited(
        p,
        Selection::DataFlow(id)
    ));
    entity!(
        "Trust boundary",
        diff.trust_boundaries(),
        |p: &Project, id| cited(p, Selection::TrustBoundary(id))
    );
    {
        let system = |p: &Project, id: ElementId| {
            let e = p.element(id).filter(|e| e.kind() == ElementKind::System)?;
            Some(format!(
                "System {} \"{}\"",
                p.element_visual_id(id).unwrap_or_default(),
                e.name()
            ))
        };
        let set = diff.elements();
        for id in set.added() {
            if let Some(t) = system(current, *id) {
                push("Added", t);
            }
        }
        for id in set.removed() {
            if let Some(t) = system(base, *id) {
                push("Removed", t);
            }
        }
        for id in set.modified() {
            if let Some(t) = system(current, *id) {
                push("Changed", t);
            }
        }
    }
    entity!("Asset", diff.assets(), |p: &Project, id| cited(
        p,
        Selection::Asset(id)
    ));
    entity!(
        "Damage scenario",
        diff.damage_scenarios(),
        |p: &Project, id| cited(p, Selection::Damage(id))
    );
    entity!(
        "Threat scenario",
        diff.threat_scenarios(),
        |p: &Project, id| cited(p, Selection::Threat(id))
    );
    entity!("Attack tree", diff.attack_trees(), |p: &Project, id| cited(
        p,
        Selection::Tree(id)
    ));
    {
        let set = diff.security_controls();
        for id in set.added() {
            push(
                "Added",
                format!(
                    "Security control {}",
                    cited(current, Selection::Control(*id))
                ),
            );
        }
        for id in set.removed() {
            push(
                "Removed",
                format!("Security control {}", cited(base, Selection::Control(*id))),
            );
        }
        for id in set.modified() {
            let what = cited(current, Selection::Control(*id));
            // Applied items and rules are the settings; a rename shows neither.
            let state = |p: &Project| {
                p.security_control_idx(id).and_then(|i| {
                    let c = p.security_control(i)?;
                    Some((p.control_targets(i).collect::<Vec<_>>(), c.rules().to_vec()))
                })
            };
            let detail = match (state(base), state(current)) {
                (Some((bt, br)), Some((ct, cr))) => {
                    let mut parts = Vec::new();
                    if bt != ct {
                        parts.push(if ct.len() == bt.len() {
                            "applied items changed".to_owned()
                        } else {
                            format!("applied to {} (was {})", ct.len(), bt.len())
                        });
                    }
                    if br != cr {
                        parts.push(if cr.len() == br.len() {
                            "rule ceilings changed".to_owned()
                        } else {
                            format!("{} rule(s) (was {})", cr.len(), br.len())
                        });
                    }
                    if parts.is_empty() {
                        String::new()
                    } else {
                        format!(": {}", parts.join(", "))
                    }
                }
                _ => String::new(),
            };
            push("Changed", format!("Security control {what}{detail}"));
        }
    }
    entity!("Goal", diff.cybersecurity_goals(), |p: &Project, id| cited(
        p,
        Selection::Goal(id)
    ));
    entity!("Claim", diff.cybersecurity_claims(), |p: &Project, id| {
        cited(p, Selection::Claim(id))
    });
    let mut moved = 0;
    for d in diff.changed_risks() {
        moved += 1;
        let threat_ref = if current.threat_scenario_idx(&d.threat()).is_some() {
            cited(current, Selection::Threat(d.threat()))
        } else {
            cited(base, Selection::Threat(d.threat()))
        };
        push(
            "Risk",
            format!(
                "{threat_ref}: before controls {} to {} ({}); after controls {} to {} ({})",
                risk_snapshot_text(m, d.inherent().before()),
                risk_snapshot_text(m, d.inherent().after()),
                d.inherent().change().as_str(),
                risk_snapshot_text(m, d.residual().before()),
                risk_snapshot_text(m, d.residual().after()),
                d.residual().change().as_str(),
            ),
        );
    }
    let count = |k: &str| lines.iter().filter(|l| l.kind == k).count();
    let summary = if lines.is_empty() {
        "Nothing changed since this baseline.".to_owned()
    } else {
        format!(
            "Since this baseline: {} added, {} removed, {} changed, {} risk(s) moved.",
            count("Added"),
            count("Removed"),
            count("Changed"),
            moved
        )
    };
    Ok(Comparison {
        baseline: baseline.to_owned(),
        summary,
        lines,
    })
}
