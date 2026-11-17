//! `Project::save` and `Project::load`: the `aegis-project` v1 snapshot.

use std::io::{BufReader, BufWriter, Read, Write};
use std::sync::Arc;

use serde::{Deserialize, Serialize, Serializer};

use super::MAX_PROJECT_FILE_BYTES;
use super::op_log::{AttrView, LinkView};
use super::wire::{
    self, CappedReader, CatalogRefsByKey, ControlRulesByKey, ControlTargetsByItem, FORMAT_VERSION,
    ImpactsByKey, KeyOrder, MethodologyView, NodesView, PropertyKeys, ProvenanceView, Required,
    RiskTreatmentWire, SnapshotWire, StepAttackTypeByNode, StepFactorsByNode,
    StepFeasibilityByNode, StepMitigationsByNode, StepViaByNode, TargetView, Text, VisualDisplay,
};
use crate::element::{AttrValue, ElementKind};
use crate::error::StoreError;
use crate::id::{
    AssetId, ClaimId, ComponentId, ControlId, DamageId, DataFlowId, ElementId, GoalId, InterfaceId,
    PartyId, PartyIdx, ThreatId, TreeId, TrustBoundaryId, VisualPrefix,
};
use crate::methodology::Methodology;
use crate::op::Op;
use crate::project::Project;
use crate::store::Limit;

/// The whole snapshot, borrowed from the project.
#[derive(Serialize)]
struct SnapshotView<'a> {
    format: &'static str,
    version: u32,
    methodology: MethodologyView<'a>,
    assets: Assets<'a>,
    damage_scenarios: Damages<'a>,
    threat_scenarios: Threats<'a>,
    #[serde(skip_serializing_if = "Trees::is_empty")]
    attack_trees: Trees<'a>,
    #[serde(skip_serializing_if = "Controls::is_empty")]
    security_controls: Controls<'a>,
    #[serde(skip_serializing_if = "Goals::is_empty")]
    cybersecurity_goals: Goals<'a>,
    #[serde(skip_serializing_if = "Claims::is_empty")]
    cybersecurity_claims: Claims<'a>,
    #[serde(skip_serializing_if = "Components::is_empty")]
    components: Components<'a>,
    #[serde(skip_serializing_if = "Interfaces::is_empty")]
    interfaces: Interfaces<'a>,
    #[serde(skip_serializing_if = "DataFlows::is_empty")]
    data_flows: DataFlows<'a>,
    #[serde(skip_serializing_if = "TrustBoundaries::is_empty")]
    trust_boundaries: TrustBoundaries<'a>,
    #[serde(skip_serializing_if = "Descriptions::is_empty")]
    descriptions: Descriptions<'a>,
    #[serde(skip_serializing_if = "ReviewMarks::is_empty")]
    review_marks: ReviewMarks<'a>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    languages: Vec<&'a str>,
    #[serde(skip_serializing_if = "Elements::is_empty")]
    elements: Elements<'a>,
    #[serde(skip_serializing_if = "Parties::is_empty")]
    parties: Parties<'a>,
}

/// What every entity view needs.
#[derive(Clone, Copy)]
struct Ctx<'a> {
    p: &'a Project,
    m: &'a Methodology,
    order: &'a KeyOrder,
}

struct Assets<'a>(Ctx<'a>);
struct Damages<'a>(Ctx<'a>);
struct Threats<'a>(Ctx<'a>);
struct Trees<'a>(Ctx<'a>);
struct Controls<'a>(Ctx<'a>);
struct Goals<'a>(&'a Project);
struct Claims<'a>(&'a Project);
struct Parties<'a>(&'a Project);
struct Components<'a>(&'a Project);
struct Interfaces<'a>(Ctx<'a>);
struct DataFlows<'a>(&'a Project);
struct TrustBoundaries<'a>(&'a Project);
struct Descriptions<'a>(&'a Project);
struct ReviewMarks<'a>(&'a Project);
struct Elements<'a>(&'a Project);

impl Elements<'_> {
    fn is_empty(&self) -> bool {
        ElementKind::ALL
            .iter()
            .all(|k| self.0.element_count(*k) == 0)
    }
}

impl Parties<'_> {
    fn is_empty(&self) -> bool {
        self.0.parties().next().is_none()
    }
}

/// The elements of one kind, in ID order.
struct ElementList<'a>(&'a Project, ElementKind);

#[derive(Serialize)]
struct ElementView<'a> {
    id: Text<ElementId>,
    visual_id: String,
    name: &'a str,
    #[serde(skip_serializing_if = "ElementAttrs::is_empty")]
    attributes: ElementAttrs<'a>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    links: Vec<LinkView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    provenance: Option<ProvenanceView<'a>>,
}

/// Attributes by key, in byte order (the store keeps them sorted).
struct ElementAttrs<'a>(&'a [(Arc<str>, AttrValue)]);

impl ElementAttrs<'_> {
    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl Serialize for ElementAttrs<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_map(self.0.iter().map(|(k, v)| (&**k, AttrView::of(v))))
    }
}

impl Serialize for Elements<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_map(
            ElementKind::ALL
                .iter()
                .filter(|k| self.0.element_count(**k) > 0)
                .map(|k| (k.key(), ElementList(self.0, *k))),
        )
    }
}

impl Serialize for ElementList<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let p = self.0;
        let mut views = Vec::with_capacity(p.element_count(self.1));
        for (id, e) in p.elements(self.1) {
            let visual_id = p
                .element_visual_id(id)
                .ok_or_else(|| serde::ser::Error::custom("element has no visual ID"))?;
            views.push(ElementView {
                id: Text(id),
                visual_id,
                name: e.name(),
                attributes: ElementAttrs(e.attributes()),
                links: e.links().iter().map(|t| LinkView::of(*t)).collect(),
                provenance: ProvenanceView::of(e.provenance()),
            });
        }
        s.collect_seq(views)
    }
}

impl ReviewMarks<'_> {
    fn is_empty(&self) -> bool {
        self.0.review_marks().next().is_none()
    }
}

#[derive(Serialize)]
struct ReviewMarkView {
    tree: Text<TreeId>,
    node: Text<crate::id::NodeId>,
}

impl Serialize for ReviewMarks<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_seq(self.0.review_marks().map(|(tree, node)| ReviewMarkView {
            tree: Text(tree),
            node: Text(node),
        }))
    }
}

impl Descriptions<'_> {
    fn is_empty(&self) -> bool {
        self.0.description_count() == 0
    }
}

#[derive(Serialize)]
struct DescriptionView<'a> {
    target: TargetView,
    text: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    provenance: Option<ProvenanceView<'a>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    translations: Vec<TranslationView<'a>>,
}

#[derive(serde::Serialize)]
struct TranslationView<'a> {
    lang: &'a str,
    text: &'a str,
}

impl Serialize for Descriptions<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_seq(self.0.descriptions().map(|(target, text, provenance)| {
            DescriptionView {
                target: target.into(),
                text,
                provenance: ProvenanceView::of(provenance),
                translations: self
                    .0
                    .description_translations(target)
                    .map(|(lang, text)| TranslationView { lang, text })
                    .collect(),
            }
        }))
    }
}

impl Trees<'_> {
    fn is_empty(&self) -> bool {
        self.0.p.attack_tree_count() == 0
    }
}

impl Controls<'_> {
    fn is_empty(&self) -> bool {
        self.0.p.security_control_count() == 0
    }
}

impl Goals<'_> {
    fn is_empty(&self) -> bool {
        self.0.cybersecurity_goal_count() == 0
    }
}

impl Claims<'_> {
    fn is_empty(&self) -> bool {
        self.0.cybersecurity_claim_count() == 0
    }
}

impl Components<'_> {
    fn is_empty(&self) -> bool {
        self.0.component_count() == 0
    }
}

impl Interfaces<'_> {
    fn is_empty(&self) -> bool {
        self.0.p.interface_count() == 0
    }
}

impl DataFlows<'_> {
    fn is_empty(&self) -> bool {
        self.0.data_flow_count() == 0
    }
}

impl TrustBoundaries<'_> {
    fn is_empty(&self) -> bool {
        self.0.trust_boundary_count() == 0
    }
}

#[derive(Serialize)]
struct AssetView<'a> {
    id: Text<AssetId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    visual_id: Option<Text<VisualDisplay>>,
    name: &'a str,
    properties: PropertyKeys<'a>,
    #[serde(skip_serializing_if = "Option::is_none")]
    component: Option<Required<Text<ComponentId>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    provenance: Option<ProvenanceView<'a>>,
}

#[derive(Serialize)]
struct DamageView<'a> {
    id: Text<DamageId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    visual_id: Option<Text<VisualDisplay>>,
    name: &'a str,
    impacts: ImpactsByKey<'a>,
    #[serde(skip_serializing_if = "str::is_empty")]
    impact_rationale: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    provenance: Option<ProvenanceView<'a>>,
}

#[derive(Serialize)]
struct ThreatView<'a> {
    id: Text<ThreatId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    visual_id: Option<Text<VisualDisplay>>,
    name: &'a str,
    asset: Required<Text<AssetId>>,
    category: Required<&'a str>,
    catalog_refs: CatalogRefsByKey<'a>,
    damages: LinkedDamages<'a>,
    #[serde(skip_serializing_if = "Option::is_none")]
    direct_feasibility: Option<Required<&'a str>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    attack_tree: Option<Required<Text<TreeId>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    treatment: Option<RiskTreatmentWire>,
    #[serde(skip_serializing_if = "Option::is_none")]
    goal: Option<Required<Text<GoalId>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    claim: Option<Required<Text<ClaimId>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    scales: Option<bool>,
    #[serde(skip_serializing_if = "str::is_empty")]
    scaling_reason: &'a str,
    #[serde(skip_serializing_if = "str::is_empty")]
    feasibility_rationale: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    provenance: Option<ProvenanceView<'a>>,
}

#[derive(Serialize)]
struct TreeView<'a> {
    id: Text<TreeId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    visual_id: Option<Text<VisualDisplay>>,
    nodes: NodesView<'a>,
    step_feasibility: StepFeasibilityByNode<'a>,
    #[serde(skip_serializing_if = "StepFactorsByNode::is_empty")]
    step_factors: StepFactorsByNode<'a>,
    #[serde(skip_serializing_if = "StepMitigationsByNode::is_empty")]
    step_mitigations: StepMitigationsByNode<'a>,
    #[serde(skip_serializing_if = "StepViaByNode::is_empty")]
    step_via: StepViaByNode<'a>,
    #[serde(skip_serializing_if = "StepAttackTypeByNode::is_empty")]
    step_attack_type: StepAttackTypeByNode<'a>,
    #[serde(skip_serializing_if = "Option::is_none")]
    provenance: Option<ProvenanceView<'a>>,
}

#[derive(Serialize)]
struct ControlView<'a> {
    id: Text<ControlId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    visual_id: Option<Text<VisualDisplay>>,
    name: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    kind: Option<Required<&'a str>>,
    #[serde(skip_serializing_if = "ControlTargetsByItem::is_empty")]
    targets: ControlTargetsByItem<'a>,
    #[serde(skip_serializing_if = "ControlRulesByKey::is_empty")]
    rules: ControlRulesByKey<'a>,
    #[serde(skip_serializing_if = "Option::is_none")]
    provenance: Option<ProvenanceView<'a>>,
}

#[derive(Serialize)]
struct GoalView<'a> {
    id: Text<GoalId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    visual_id: Option<Text<VisualDisplay>>,
    name: &'a str,
    controls: LinkedControls<'a>,
    #[serde(skip_serializing_if = "LinkedParties::is_empty")]
    responsible: LinkedParties<'a>,
    #[serde(skip_serializing_if = "Option::is_none")]
    provenance: Option<ProvenanceView<'a>>,
}

#[derive(Serialize)]
struct ClaimView<'a> {
    id: Text<ClaimId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    visual_id: Option<Text<VisualDisplay>>,
    name: &'a str,
    #[serde(skip_serializing_if = "LinkedParties::is_empty")]
    responsible: LinkedParties<'a>,
    #[serde(skip_serializing_if = "Option::is_none")]
    provenance: Option<ProvenanceView<'a>>,
}

#[derive(Serialize)]
struct PartyView<'a> {
    id: Text<PartyId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    visual_id: Option<Text<VisualDisplay>>,
    name: &'a str,
    kind: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    provenance: Option<ProvenanceView<'a>>,
}

#[derive(Serialize)]
struct ComponentView<'a> {
    id: Text<ComponentId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    visual_id: Option<Text<VisualDisplay>>,
    name: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    parent: Option<Required<Text<ComponentId>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<Text<ElementId>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    provenance: Option<ProvenanceView<'a>>,
}

#[derive(Serialize)]
struct InterfaceView<'a> {
    id: Text<InterfaceId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    visual_id: Option<Text<VisualDisplay>>,
    name: &'a str,
    component: Required<Text<ComponentId>>,
    kind: Required<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    provenance: Option<ProvenanceView<'a>>,
}

#[derive(Serialize)]
struct DataFlowView<'a> {
    id: Text<DataFlowId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    visual_id: Option<Text<VisualDisplay>>,
    name: &'a str,
    from: Required<Text<ComponentId>>,
    to: Required<Text<ComponentId>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    interface: Option<Required<Text<InterfaceId>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    provenance: Option<ProvenanceView<'a>>,
}

#[derive(Serialize)]
struct TrustBoundaryView<'a> {
    id: Text<TrustBoundaryId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    visual_id: Option<Text<VisualDisplay>>,
    name: &'a str,
    components: LinkedBoundaryComponents<'a>,
    #[serde(skip_serializing_if = "Option::is_none")]
    provenance: Option<ProvenanceView<'a>>,
}

/// A threat's damage links as public IDs, in stored (ID) order, together
/// with its sparse link rationales (only non-empty texts, by public ID).
struct LinkedDamages<'a> {
    p: &'a Project,
    links: &'a [crate::id::DamageIdx],
    rationales: &'a [(crate::id::DamageIdx, Arc<str>)],
}

/// One damage link in the object form: `damage` then `rationale`. The
/// rationale is always written, even when empty, so the bytes are
/// deterministic.
#[derive(Serialize)]
struct DamageLinkView<'a> {
    damage: Required<Text<DamageId>>,
    rationale: &'a str,
}

/// A goal's control links as public IDs, in stored (ID) order.
struct LinkedControls<'a> {
    p: &'a Project,
    links: &'a [crate::id::ControlIdx],
}

/// A goal's or claim's responsible parties as public IDs, in stored (ID) order.
struct LinkedParties<'a> {
    p: &'a Project,
    links: &'a [PartyIdx],
}

impl LinkedParties<'_> {
    fn is_empty(&self) -> bool {
        self.links.is_empty()
    }
}

/// A trust boundary's member components as public IDs, in stored (ID) order.
struct LinkedBoundaryComponents<'a> {
    p: &'a Project,
    links: &'a [crate::id::ComponentIdx],
}

impl Serialize for Assets<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Ctx { p, m, order } = self.0;
        s.collect_seq(p.assets().enumerate().map(|(pos, (idx, id, a))| AssetView {
            id: Text(id),
            visual_id: VisualDisplay::non_default(
                VisualPrefix::Asset,
                p.asset_visual_num(idx),
                pos,
            ),
            name: a.name(),
            properties: PropertyKeys {
                m,
                order,
                set: a.properties(),
            },
            component: a.component().map(|c| Required(p.component_id(c).map(Text))),
            provenance: ProvenanceView::of(a.provenance()),
        }))
    }
}

impl Serialize for Damages<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Ctx { p, m, order } = self.0;
        s.collect_seq(
            p.damage_scenarios()
                .enumerate()
                .map(|(pos, (idx, id, d))| DamageView {
                    id: Text(id),
                    visual_id: VisualDisplay::non_default(
                        VisualPrefix::Damage,
                        p.damage_scenario_visual_num(idx),
                        pos,
                    ),
                    name: d.name(),
                    impacts: ImpactsByKey {
                        m,
                        order,
                        damage: d,
                    },
                    impact_rationale: d.impact_rationale(),
                    provenance: ProvenanceView::of(d.provenance()),
                }),
        )
    }
}

impl Serialize for Threats<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Ctx { p, m, order } = self.0;
        s.collect_seq(p.threat_scenarios().enumerate().map(|(pos, (idx, id, t))| {
            ThreatView {
                id: Text(id),
                visual_id: VisualDisplay::non_default(
                    VisualPrefix::Threat,
                    p.threat_scenario_visual_num(idx),
                    pos,
                ),
                name: t.name(),
                asset: Required(p.asset_id(t.asset()).map(Text)),
                category: Required(m.threat_category_key(t.category())),
                catalog_refs: CatalogRefsByKey {
                    m,
                    order,
                    refs: t.catalog_refs(),
                },
                damages: LinkedDamages {
                    p,
                    links: t.damages(),
                    rationales: &t.damage_rationales,
                },
                direct_feasibility: t
                    .direct_feasibility()
                    .map(|l| Required(m.feasibility_level_key(l))),
                attack_tree: t
                    .attack_tree()
                    .map(|idx| Required(p.attack_tree_id(idx).map(Text))),
                treatment: t.treatment().map(RiskTreatmentWire::from),
                goal: t
                    .goal()
                    .map(|idx| Required(p.cybersecurity_goal_id(idx).map(Text))),
                claim: t
                    .claim()
                    .map(|idx| Required(p.cybersecurity_claim_id(idx).map(Text))),
                scales: t.scales(),
                scaling_reason: t.scaling_reason(),
                feasibility_rationale: t.feasibility_rationale(),
                provenance: ProvenanceView::of(t.provenance()),
            }
        }))
    }
}

impl Serialize for Trees<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Ctx { p, order, .. } = self.0;
        s.collect_seq(
            p.attack_trees()
                .enumerate()
                .map(|(pos, (tree_idx, id, tree))| TreeView {
                    id: Text(id),
                    visual_id: VisualDisplay::non_default(
                        VisualPrefix::Tree,
                        p.attack_tree_visual_num(tree_idx),
                        pos,
                    ),
                    nodes: NodesView(tree),
                    step_feasibility: StepFeasibilityByNode {
                        project: p,
                        tree_idx,
                        tree,
                    },
                    step_factors: StepFactorsByNode {
                        project: p,
                        order,
                        tree_idx,
                        tree,
                    },
                    step_mitigations: StepMitigationsByNode {
                        project: p,
                        tree_idx,
                        tree,
                    },
                    step_via: StepViaByNode {
                        project: p,
                        tree_idx,
                        tree,
                    },
                    step_attack_type: StepAttackTypeByNode {
                        project: p,
                        tree_idx,
                        tree,
                    },
                    provenance: p
                        .attack_tree_provenance(tree_idx)
                        .and_then(ProvenanceView::of),
                }),
        )
    }
}

impl Serialize for Controls<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Ctx { p, m, order } = self.0;
        s.collect_seq(
            p.security_controls()
                .enumerate()
                .map(|(pos, (idx, id, c))| ControlView {
                    id: Text(id),
                    visual_id: VisualDisplay::non_default(
                        VisualPrefix::Control,
                        p.security_control_visual_num(idx),
                        pos,
                    ),
                    name: c.name(),
                    kind: c.kind().map(|k| Required(m.control_kind_key(k))),
                    targets: ControlTargetsByItem {
                        project: p,
                        control: c,
                    },
                    rules: ControlRulesByKey {
                        m,
                        order,
                        control: c,
                    },
                    provenance: ProvenanceView::of(c.provenance()),
                }),
        )
    }
}

impl Serialize for Goals<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let p = self.0;
        s.collect_seq(
            p.cybersecurity_goals()
                .enumerate()
                .map(|(pos, (idx, id, g))| GoalView {
                    id: Text(id),
                    visual_id: VisualDisplay::non_default(
                        VisualPrefix::Goal,
                        p.cybersecurity_goal_visual_num(idx),
                        pos,
                    ),
                    name: g.name(),
                    controls: LinkedControls {
                        p,
                        links: g.controls(),
                    },
                    responsible: LinkedParties {
                        p,
                        links: g.responsible(),
                    },
                    provenance: ProvenanceView::of(g.provenance()),
                }),
        )
    }
}

impl Serialize for Claims<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let p = self.0;
        s.collect_seq(
            p.cybersecurity_claims()
                .enumerate()
                .map(|(pos, (idx, id, cl))| ClaimView {
                    id: Text(id),
                    visual_id: VisualDisplay::non_default(
                        VisualPrefix::Claim,
                        p.cybersecurity_claim_visual_num(idx),
                        pos,
                    ),
                    name: cl.name(),
                    responsible: LinkedParties {
                        p,
                        links: cl.responsible(),
                    },
                    provenance: ProvenanceView::of(cl.provenance()),
                }),
        )
    }
}

impl Serialize for Parties<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let p = self.0;
        s.collect_seq(
            p.parties()
                .enumerate()
                .map(|(pos, (idx, id, party))| PartyView {
                    id: Text(id),
                    visual_id: VisualDisplay::non_default(
                        VisualPrefix::Party,
                        p.party_visual_num(idx),
                        pos,
                    ),
                    name: party.name(),
                    kind: party.kind().as_str(),
                    provenance: ProvenanceView::of(party.provenance()),
                }),
        )
    }
}

impl Serialize for Components<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let p = self.0;
        s.collect_seq(p.components().enumerate().map(|(pos, (idx, id, c))| {
            ComponentView {
                id: Text(id),
                visual_id: VisualDisplay::non_default(
                    VisualPrefix::Component,
                    p.component_visual_num(idx),
                    pos,
                ),
                name: c.name(),
                parent: c
                    .parent()
                    .map(|par| Required(p.component_id(par).map(Text))),
                system: c.system().map(Text),
                provenance: ProvenanceView::of(c.provenance()),
            }
        }))
    }
}

impl Serialize for Interfaces<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Ctx { p, m, .. } = self.0;
        s.collect_seq(
            p.interfaces()
                .enumerate()
                .map(|(pos, (idx, id, iface))| InterfaceView {
                    id: Text(id),
                    visual_id: VisualDisplay::non_default(
                        VisualPrefix::Interface,
                        p.interface_visual_num(idx),
                        pos,
                    ),
                    name: iface.name(),
                    component: Required(p.component_id(iface.component()).map(Text)),
                    kind: Required(m.interface_kind_key(iface.kind())),
                    provenance: ProvenanceView::of(iface.provenance()),
                }),
        )
    }
}

impl Serialize for DataFlows<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let p = self.0;
        s.collect_seq(p.data_flows().enumerate().map(|(pos, (idx, id, df))| {
            DataFlowView {
                id: Text(id),
                visual_id: VisualDisplay::non_default(
                    VisualPrefix::DataFlow,
                    p.data_flow_visual_num(idx),
                    pos,
                ),
                name: df.name(),
                from: Required(p.component_id(df.from()).map(Text)),
                to: Required(p.component_id(df.to()).map(Text)),
                interface: df
                    .interface()
                    .map(|i| Required(p.interface_id(i).map(Text))),
                provenance: ProvenanceView::of(df.provenance()),
            }
        }))
    }
}

impl Serialize for TrustBoundaries<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let p = self.0;
        s.collect_seq(
            p.trust_boundaries()
                .enumerate()
                .map(|(pos, (idx, id, tb))| TrustBoundaryView {
                    id: Text(id),
                    visual_id: VisualDisplay::non_default(
                        VisualPrefix::TrustBoundary,
                        p.trust_boundary_visual_num(idx),
                        pos,
                    ),
                    name: tb.name(),
                    components: LinkedBoundaryComponents {
                        p,
                        links: tb.components(),
                    },
                    provenance: ProvenanceView::of(tb.provenance()),
                }),
        )
    }
}

impl Serialize for LinkedDamages<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        // Keep the exact v1 bytes when no link has a rationale: a plain
        // sequence of ID strings. Only switch to the object form when at
        // least one rationale is non-empty.
        if !self.rationales.iter().any(|(_, text)| !text.is_empty()) {
            return s.collect_seq(
                self.links
                    .iter()
                    .map(|&d| Required(self.p.damage_scenario_id(d).map(Text))),
            );
        }
        s.collect_seq(self.links.iter().map(|&d| {
            DamageLinkView {
                damage: Required(self.p.damage_scenario_id(d).map(Text)),
                rationale: self
                    .rationales
                    .iter()
                    .find(|(idx, _)| *idx == d)
                    .map_or("", |(_, text)| text.as_ref()),
            }
        }))
    }
}

impl Serialize for LinkedControls<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_seq(
            self.links
                .iter()
                .map(|&c| Required(self.p.security_control_id(c).map(Text))),
        )
    }
}

impl Serialize for LinkedParties<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_seq(
            self.links
                .iter()
                .map(|&idx| Required(self.p.party_id(idx).map(Text))),
        )
    }
}

impl Serialize for LinkedBoundaryComponents<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_seq(
            self.links
                .iter()
                .map(|&c| Required(self.p.component_id(c).map(Text))),
        )
    }
}

/// Size of the read buffer between the input and the parser.
const READ_BUFFER_BYTES: usize = 64 * 1024;

impl Project {
    /// Writes the project as an `aegis-project` v1 snapshot: one line of
    /// compact JSON ending in `\n` (format in
    /// `docs/adr/0009-save-load-format.md`).
    ///
    /// The output holds only live entities, in public-ID order, with every
    /// vocabulary item written as its key and every list in canonical
    /// order, so two equal projects give identical bytes whatever their
    /// edit history or their methodology's declaration order (P4). Writes
    /// go through an internal `BufWriter`, which is flushed before return.
    ///
    /// Time O(n + L log `L_max` + m log m), with n live entities, L damage
    /// links plus catalog refs, `L_max` the longest list on one threat, and m
    /// the methodology size. Extra space O(m) for the key-sorted
    /// vocabulary order; the project is not copied.
    ///
    /// # Errors
    ///
    /// [`StoreError::Io`] if writing fails. [`StoreError::UnencodableOp`]
    /// if the project broke its own invariants (an index not in its
    /// methodology), which the model prevents.
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{AssetId, Methodology, Op, Project, Provenance};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let mut project = Project::new(Arc::clone(&m));
    /// let id = AssetId::new_v7();
    /// let integrity = m.property("integrity").ok_or("no property")?;
    /// project.apply(Op::AddAsset {
    ///     id,
    ///     name: "Brake ECU".into(),
    ///     properties: [integrity].into_iter().collect(),
    ///     provenance: Provenance::default(),
    /// })?;
    ///
    /// let mut bytes = Vec::new();
    /// project.save(&mut bytes)?;
    /// let text = String::from_utf8(bytes)?;
    /// assert!(text.starts_with(r#"{"format":"aegis-project","version":1,"#));
    /// assert!(text.contains(r#""properties":["integrity"]"#));
    /// assert!(text.ends_with("}\n"));
    /// # Ok(())
    /// # }
    /// ```
    pub fn save<W: Write>(&self, w: W) -> Result<(), StoreError> {
        self.with_snapshot_view(|view| {
            let mut out = BufWriter::new(w);
            serde_json::to_writer(&mut out, view).map_err(wire::encode_error)?;
            out.write_all(b"\n")?;
            out.flush()?;
            Ok(())
        })
    }

    /// The snapshot as a JSON value, plus the visual ID of each entry of
    /// its `attack_trees` list, in the same order. O(n) in project size.
    pub(super) fn snapshot_value(&self) -> Result<(serde_json::Value, Vec<String>), StoreError> {
        let value =
            self.with_snapshot_view(|view| serde_json::to_value(view).map_err(wire::encode_error))?;
        let ids = self
            .attack_trees()
            .map(|(idx, _, _)| self.attack_tree_visual_id(idx))
            .collect::<Option<Vec<_>>>()
            .ok_or(StoreError::UnencodableOp)?;
        Ok((value, ids))
    }

    /// Builds the [`SnapshotView`] and hands it to `f`.
    fn with_snapshot_view<R>(&self, f: impl FnOnce(&SnapshotView<'_>) -> R) -> R {
        let m = &**self.methodology();
        let order = KeyOrder::new(m);
        let ctx = Ctx {
            p: self,
            m,
            order: &order,
        };
        let view = SnapshotView {
            format: "aegis-project",
            version: FORMAT_VERSION,
            methodology: MethodologyView::of(m),
            assets: Assets(ctx),
            damage_scenarios: Damages(ctx),
            threat_scenarios: Threats(ctx),
            attack_trees: Trees(ctx),
            security_controls: Controls(ctx),
            cybersecurity_goals: Goals(self),
            cybersecurity_claims: Claims(self),
            components: Components(self),
            interfaces: Interfaces(ctx),
            data_flows: DataFlows(self),
            trust_boundaries: TrustBoundaries(self),
            descriptions: Descriptions(self),
            review_marks: ReviewMarks(self),
            languages: self.languages().iter().map(|l| &**l).collect(),
            elements: Elements(self),
            parties: Parties(self),
        };
        f(&view)
    }

    /// Reads an `aegis-project` v1 snapshot written by
    /// [`save`](Self::save) and rebuilds the project over `methodology`.
    ///
    /// The input is streamed through a 64 KiB buffer, never read whole.
    /// Decoding is capped before any model check: at most
    /// [`MAX_PROJECT_FILE_BYTES`] of input, and every string and list
    /// within its cap ([`Limit`]). The file's methodology key must equal
    /// `methodology.key()`, checked as soon as it is read (before any
    /// vocabulary key when `methodology` comes before the entity lists, as
    /// `save` writes it; else a foreign file may fail with
    /// [`StoreError::UnknownKey`] instead); its methodology
    /// version may differ, since the file holds keys, not indices. Every
    /// vocabulary key is resolved to its index the moment it is read, so a
    /// key the methodology does not have fails with
    /// [`StoreError::UnknownKey`] inside the entity that holds it, and no
    /// key string is kept. The project is then built only through
    /// [`Project::apply`], with `AddAsset`, then `AddDamageScenario`, then
    /// `AddThreatScenario` ops, so every model rule is checked exactly as
    /// for an edit.
    ///
    /// The snapshot is lenient about JSON whitespace between tokens and
    /// about a missing final `\n` (the op log is not); a re-save is always
    /// canonical.
    ///
    /// Time O(B + n log n + L log n), with B input bytes, n entities, and L
    /// damage links plus catalog refs. Extra space O(n + L + the longest
    /// string) besides the project: the decoded entities hold names and
    /// 1-byte indices only. Measured heap, as a factor of the file size
    /// (`benches/store_memory.rs`, ADRs 0009 and 0010), as peak during the
    /// load and kept (the returned project): 1.03x peak and 0.83x kept for
    /// the typical project; 0.02x peak when keys are unknown, since the
    /// load fails inside the first entity. The worst case is a file of
    /// minimal entities of one kind, and it is mostly the loaded project,
    /// not the decoder (the decoder adds at most about 0.9x). With the
    /// built-in methodology it peaks at 2.73x (about 175 MiB at the 64 MiB
    /// cap) and keeps at most 1.85x. With a methodology at every vocabulary
    /// limit (64 properties, 32 impact categories) it peaks at 3.53x (about
    /// 226 MiB) and keeps at most 2.65x, both from damage scenarios' one
    /// rating slot per impact category. Kept stays within 3x for every
    /// shape measured.
    ///
    /// # Errors
    ///
    /// [`StoreError::Io`] if reading fails;
    /// [`StoreError::LimitExceeded`] if a cap breaks;
    /// [`StoreError::Decode`] for bad JSON or a wrong shape;
    /// [`StoreError::UnsupportedFormat`], [`StoreError::MethodologyMismatch`],
    /// [`StoreError::UnknownKey`], [`StoreError::InvalidId`],
    /// [`StoreError::InvalidCatalogRef`], [`StoreError::DuplicateEntry`];
    /// and [`StoreError::Model`] if the content breaks a model rule (for
    /// example a threat that names a missing asset).
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{DamageId, Methodology, Op, Project, Provenance};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let mut project = Project::new(Arc::clone(&m));
    /// project.apply(Op::AddDamageScenario {
    ///     id: DamageId::new_v7(),
    ///     name: "Data leak".into(),
    ///     impacts: vec![],
    ///     provenance: Provenance::default(),
    /// })?;
    /// let mut bytes = Vec::new();
    /// project.save(&mut bytes)?;
    /// let loaded = Project::load(bytes.as_slice(), m)?;
    /// assert_eq!(loaded, project);
    /// # Ok(())
    /// # }
    /// ```
    #[allow(clippy::too_many_lines)]
    pub fn load<R: Read>(r: R, methodology: Arc<Methodology>) -> Result<Project, StoreError> {
        let mut input = CappedReader::new(r, MAX_PROJECT_FILE_BYTES);
        let decoded = wire::decode(&methodology, None, || {
            let buffered = BufReader::with_capacity(READ_BUFFER_BYTES, &mut input);
            let mut de = serde_json::Deserializer::from_reader(buffered);
            let snapshot = SnapshotWire::deserialize(&mut de)?;
            de.end()?;
            Ok(snapshot)
        });
        if input.exceeded() {
            return Err(StoreError::LimitExceeded(Limit::PROJECT_FILE));
        }
        let snapshot = decoded?;
        let mut project = Project::new(methodology);
        // Add components in two passes (roots first, then parent links) so
        // parent-after-child ID ordering in snapshots never fails.
        let mut pending_parents = Vec::new();
        let mut pending_systems = Vec::new();
        let (mut used, mut next_vis) = (std::collections::BTreeSet::new(), 0u32);
        for (pos, c) in snapshot.components.into_iter().enumerate() {
            let vis = claim_visual(c.visual_id, pos, &mut used, &mut next_vis)?;
            let provenance = wire::unwrap_provenance(c.provenance);
            let id = c.id.0;
            project.apply(Op::AddComponent {
                id,
                name: c.name.0,
                parent: None,
                provenance: provenance.clone(),
            })?;
            project.set_component_loaded_visual(&id, vis, next_vis);
            if let Some(parent) = c.parent {
                pending_parents.push((id, parent.0, provenance.clone()));
            }
            if let Some(system) = c.system {
                pending_systems.push((id, system.0, provenance));
            }
        }
        for (id, parent, provenance) in pending_parents {
            project.apply(Op::SetComponentParent {
                id,
                parent: Some(parent),
                provenance,
            })?;
        }
        let (mut used, mut next_vis) = (std::collections::BTreeSet::new(), 0u32);
        for (pos, iface) in snapshot.interfaces.into_iter().enumerate() {
            let vis = claim_visual(iface.visual_id, pos, &mut used, &mut next_vis)?;
            let id = iface.id.0;
            project.apply(Op::AddInterface {
                id,
                name: iface.name.0,
                component: iface.component.0,
                kind: iface.kind.0,
                provenance: wire::unwrap_provenance(iface.provenance),
            })?;
            project.set_interface_loaded_visual(&id, vis, next_vis);
        }
        let (mut used, mut next_vis) = (std::collections::BTreeSet::new(), 0u32);
        for (pos, df) in snapshot.data_flows.into_iter().enumerate() {
            let vis = claim_visual(df.visual_id, pos, &mut used, &mut next_vis)?;
            let id = df.id.0;
            project.apply(Op::AddDataFlow {
                id,
                name: df.name.0,
                from: df.from.0,
                to: df.to.0,
                interface: df.interface.map(|i| i.0),
                provenance: wire::unwrap_provenance(df.provenance),
            })?;
            project.set_data_flow_loaded_visual(&id, vis, next_vis);
        }
        let (mut used, mut next_vis) = (std::collections::BTreeSet::new(), 0u32);
        for (pos, tb) in snapshot.trust_boundaries.into_iter().enumerate() {
            let vis = claim_visual(tb.visual_id, pos, &mut used, &mut next_vis)?;
            let id = tb.id.0;
            project.apply(Op::AddTrustBoundary {
                id,
                name: tb.name.0,
                components: tb.components.0.into_iter().map(|c| c.0).collect(),
                provenance: wire::unwrap_provenance(tb.provenance),
            })?;
            project.set_trust_boundary_loaded_visual(&id, vis, next_vis);
        }
        let (mut used, mut next_vis) = (std::collections::BTreeSet::new(), 0u32);
        for (pos, a) in snapshot.assets.into_iter().enumerate() {
            let vis = claim_visual(a.visual_id, pos, &mut used, &mut next_vis)?;
            let provenance = wire::unwrap_provenance(a.provenance);
            let id = a.id.0;
            let component = a.component.map(|c| c.0);
            project.apply(Op::AddAsset {
                id,
                name: a.name.0,
                properties: a.properties.0,
                provenance: provenance.clone(),
            })?;
            project.set_asset_loaded_visual(&id, vis, next_vis);
            if let Some(comp) = component {
                project.apply(Op::SetAssetComponent {
                    asset: id,
                    component: Some(comp),
                    provenance,
                })?;
            }
        }
        let (mut used, mut next_vis) = (std::collections::BTreeSet::new(), 0u32);
        for (pos, d) in snapshot.damage_scenarios.into_iter().enumerate() {
            let vis = claim_visual(d.visual_id, pos, &mut used, &mut next_vis)?;
            let id = d.id.0;
            let provenance = wire::unwrap_provenance(d.provenance);
            project.apply(Op::AddDamageScenario {
                id,
                name: d.name.0,
                impacts: d.impacts.0,
                provenance: provenance.clone(),
            })?;
            if let Some(text) = d.impact_rationale {
                project.apply(Op::SetImpactRationale {
                    damage: id,
                    text: text.0,
                    provenance,
                })?;
            }
            project.set_damage_loaded_visual(&id, vis, next_vis);
        }
        let (mut used, mut next_vis) = (std::collections::BTreeSet::new(), 0u32);
        for (pos, c) in snapshot.security_controls.into_iter().enumerate() {
            let vis = claim_visual(c.visual_id, pos, &mut used, &mut next_vis)?;
            let provenance = wire::unwrap_provenance(c.provenance);
            let id = c.id.0;
            project.apply(Op::AddSecurityControl {
                id,
                name: c.name.0,
                provenance: provenance.clone(),
            })?;
            project.set_control_loaded_visual(&id, vis, next_vis);
            if let Some(kind) = c.kind {
                project.apply(Op::SetControlKind {
                    control: id,
                    kind: Some(kind.0),
                    provenance: provenance.clone(),
                })?;
            }
            for target in c.targets.0 {
                project.apply(Op::ApplyControl {
                    control: id,
                    target,
                    provenance: provenance.clone(),
                })?;
            }
            for (attack_type, ceiling) in c.rules.0 {
                project.apply(Op::SetControlRule {
                    control: id,
                    attack_type,
                    ceiling: Some(ceiling),
                    provenance: provenance.clone(),
                })?;
            }
        }
        let (mut used, mut next_vis) = (std::collections::BTreeSet::new(), 0u32);
        for (pos, party) in snapshot.parties.0.into_iter().enumerate() {
            let vis = claim_visual(party.visual_id, pos, &mut used, &mut next_vis)?;
            let id = party.id.0;
            project.apply(Op::AddParty {
                id,
                name: party.name.0,
                kind: party.kind,
                provenance: wire::unwrap_provenance(party.provenance),
            })?;
            project.set_party_loaded_visual(&id, vis, next_vis);
        }
        let (mut used, mut next_vis) = (std::collections::BTreeSet::new(), 0u32);
        for (pos, g) in snapshot.cybersecurity_goals.into_iter().enumerate() {
            let vis = claim_visual(g.visual_id, pos, &mut used, &mut next_vis)?;
            let id = g.id.0;
            let provenance = wire::unwrap_provenance(g.provenance);
            project.apply(Op::AddCybersecurityGoal {
                id,
                name: g.name.0,
                controls: g.controls.0.into_iter().map(|c| c.0).collect(),
                provenance: provenance.clone(),
            })?;
            project.set_goal_loaded_visual(&id, vis, next_vis);
            for party in g.responsible.0 {
                project.apply(Op::LinkGoalParty {
                    goal: id,
                    party: party.0,
                    provenance: provenance.clone(),
                })?;
            }
        }
        let (mut used, mut next_vis) = (std::collections::BTreeSet::new(), 0u32);
        for (pos, cl) in snapshot.cybersecurity_claims.into_iter().enumerate() {
            let vis = claim_visual(cl.visual_id, pos, &mut used, &mut next_vis)?;
            let id = cl.id.0;
            let provenance = wire::unwrap_provenance(cl.provenance);
            project.apply(Op::AddCybersecurityClaim {
                id,
                name: cl.name.0,
                provenance: provenance.clone(),
            })?;
            project.set_claim_loaded_visual(&id, vis, next_vis);
            for party in cl.responsible.0 {
                project.apply(Op::LinkClaimParty {
                    claim: id,
                    party: party.0,
                    provenance: provenance.clone(),
                })?;
            }
        }
        // Marks go on with their tree, under the tree's own provenance.
        let mut marks: std::collections::BTreeSet<(crate::id::TreeId, crate::id::NodeId)> =
            snapshot
                .review_marks
                .iter()
                .map(|m| (m.tree.0, m.node.0))
                .collect();
        let (mut used, mut next_vis) = (std::collections::BTreeSet::new(), 0u32);
        for (pos, tr) in snapshot.attack_trees.into_iter().enumerate() {
            let vis = claim_visual(tr.visual_id, pos, &mut used, &mut next_vis)?;
            let provenance = wire::unwrap_provenance(tr.provenance);
            let id = tr.id.0;
            // A snapshot may hold a bigger tree than one op can carry.
            project.load_attack_tree(id, tr.nodes.0, tr.step_feasibility.0, provenance.clone())?;
            project.set_tree_loaded_visual(&id, vis, next_vis);
            for (node, factors) in tr.step_factors.0 {
                for (factor, level) in factors {
                    project.apply(Op::SetStepFactor {
                        tree: id,
                        node,
                        factor,
                        level: Some(level),
                        provenance: provenance.clone(),
                    })?;
                }
            }
            for (node, mitigations) in tr.step_mitigations.0 {
                for (control, feasibility) in mitigations {
                    project.apply(Op::SetStepMitigation {
                        tree: id,
                        node,
                        control,
                        feasibility: Some(feasibility),
                        provenance: provenance.clone(),
                    })?;
                }
            }
            for (node, via) in tr.step_via.into_iter().flat_map(|v| v.0) {
                project.apply(Op::SetStepVia {
                    tree: id,
                    node,
                    via: Some(via.unwrap_item()),
                    provenance: provenance.clone(),
                })?;
            }
            for (node, attack_type) in tr.step_attack_type.into_iter().flat_map(|v| v.0) {
                project.apply(Op::SetStepAttackType {
                    tree: id,
                    node,
                    attack_type: Some(attack_type.0),
                    provenance: provenance.clone(),
                })?;
            }
            let own: Vec<_> = marks.iter().filter(|(t, _)| *t == id).copied().collect();
            for (tree, node) in own {
                marks.remove(&(tree, node));
                project.apply(Op::SetReviewMark {
                    tree,
                    node,
                    marked: true,
                    provenance: provenance.clone(),
                })?;
            }
        }
        let (mut used, mut next_vis) = (std::collections::BTreeSet::new(), 0u32);
        for (pos, mut t) in snapshot.threat_scenarios.into_iter().enumerate() {
            let vis = claim_visual(t.visual_id.take(), pos, &mut used, &mut next_vis)?;
            let id = t.id.0;
            apply_snapshot_threat(&mut project, t)?;
            project.set_threat_loaded_visual(&id, vis, next_vis);
        }
        for (kind, list) in snapshot.elements.0 {
            let (mut used, mut next_vis) = (std::collections::BTreeSet::new(), 0u32);
            for e in list {
                let vis = kind
                    .parse_visual(&e.visual_id.0)
                    .ok_or(StoreError::InvalidId)?;
                if !used.insert(vis) {
                    return Err(StoreError::DuplicateEntry);
                }
                next_vis = next_vis.max(vis.saturating_add(1));
                let id = e.id.0;
                apply_snapshot_element(&mut project, kind, e)?;
                project.set_element_loaded_visual(id, vis, next_vis);
            }
        }
        // Systems are elements, so they are put on components once loaded.
        for (id, system, provenance) in pending_systems {
            project.apply(Op::SetComponentSystem {
                id,
                system: Some(system),
                provenance,
            })?;
        }
        // Marks that name no tree of this snapshot are refused by the model.
        for (tree, node) in marks {
            project.apply(Op::SetReviewMark {
                tree,
                node,
                marked: true,
                provenance: crate::provenance::Provenance::default(),
            })?;
        }
        project.apply(Op::SetProjectLanguages {
            langs: snapshot.languages.into_iter().map(|l| l.0).collect(),
            provenance: crate::provenance::Provenance::default(),
        })?;
        // Descriptions last: every element they name exists by now.
        for d in snapshot.descriptions {
            let target = d.target.into_ref();
            let provenance = wire::unwrap_provenance(d.provenance);
            let translations = d.translations;
            project.apply(Op::SetDescription {
                target,
                text: d.text.0,
                provenance,
            })?;
            for t in translations {
                project.apply(Op::SetDescriptionTranslation {
                    target,
                    lang: t.lang.0,
                    text: t.text.0,
                    provenance: crate::provenance::Provenance::default(),
                })?;
            }
        }
        Ok(project)
    }
}

fn claim_visual<K>(
    wire: Option<wire::WireVisual<K>>,
    pos: usize,
    used: &mut std::collections::BTreeSet<u32>,
    next: &mut u32,
) -> Result<u32, StoreError> {
    let vis = match wire {
        Some(v) => v.0,
        None => u32::try_from(pos).map_err(|_| StoreError::InvalidId)?,
    };
    if !used.insert(vis) {
        return Err(StoreError::DuplicateEntry);
    }
    *next = (*next).max(vis.saturating_add(1));
    Ok(vis)
}

/// Adds one snapshot element through `apply`: `AddElement`, then one
/// `SetElementAttribute` per attribute and one `LinkElement` per link, all
/// with the stored provenance. Replaying the links rebuilds the inbound
/// link counts.
fn apply_snapshot_element(
    project: &mut Project,
    kind: ElementKind,
    e: wire::ElementWire,
) -> Result<(), StoreError> {
    let provenance = wire::unwrap_provenance(e.provenance);
    let id = e.id.0;
    project.apply(Op::AddElement {
        id,
        kind,
        name: e.name.0,
        provenance: provenance.clone(),
    })?;
    for (key, value) in e.attributes.0 {
        project.apply(Op::SetElementAttribute {
            id,
            key,
            value: Some(value.into_value()),
            provenance: provenance.clone(),
        })?;
    }
    for link in e.links.0 {
        project.apply(Op::LinkElement {
            id,
            target: link.into_target(),
            provenance: provenance.clone(),
        })?;
    }
    Ok(())
}

fn apply_snapshot_threat(project: &mut Project, t: wire::ThreatWire) -> Result<(), StoreError> {
    let provenance = wire::unwrap_provenance(t.provenance);
    let id = t.id.0;
    let direct_feasibility = t.direct_feasibility.map(|w| w.0);
    let attack_tree = t.attack_tree.map(|w| w.0);
    let treatment = t.treatment.map(Into::into);
    let goal = t.goal.map(|w| w.0);
    let claim = t.claim.map(|w| w.0);
    let scales = t.scales;
    let scaling_reason = t.scaling_reason.map(|w| w.0);
    let feasibility_rationale = t.feasibility_rationale.map(|w| w.0);
    let (damage_ids, damage_rationales): (Vec<DamageId>, Vec<(DamageId, Arc<str>)>) = match t
        .damages
    {
        wire::DamageLinksWire::Ids(ids) => (ids.0.into_iter().map(|d| d.0).collect(), Vec::new()),
        wire::DamageLinksWire::Links(links) => {
            let mut ids = Vec::with_capacity(links.0.len());
            let mut rationales = Vec::new();
            for link in links.0 {
                ids.push(link.damage.0);
                if let Some(text) = link.rationale {
                    rationales.push((link.damage.0, text.0));
                }
            }
            (ids, rationales)
        }
    };
    project.apply(Op::AddThreatScenario {
        id,
        name: t.name.0,
        asset: t.asset.0,
        category: t.category.0,
        catalog_refs: t.catalog_refs.0.into_iter().map(|r| r.0).collect(),
        damages: damage_ids,
        provenance: provenance.clone(),
    })?;
    for (damage, text) in damage_rationales {
        project.apply(Op::SetDamageLinkRationale {
            threat: id,
            damage,
            text,
            provenance: provenance.clone(),
        })?;
    }
    if let Some(level) = direct_feasibility {
        project.apply(Op::SetThreatFeasibility {
            threat: id,
            level: Some(level),
            provenance: provenance.clone(),
        })?;
    }
    if let Some(tree) = attack_tree {
        project.apply(Op::SetThreatAttackTree {
            threat: id,
            tree: Some(tree),
            provenance: provenance.clone(),
        })?;
    }
    if let Some(treatment) = treatment {
        project.apply(Op::SetRiskTreatment {
            threat: id,
            treatment: Some(treatment),
            provenance: provenance.clone(),
        })?;
    }
    if let Some(goal) = goal {
        project.apply(Op::SetThreatGoal {
            threat: id,
            goal: Some(goal),
            provenance: provenance.clone(),
        })?;
    }
    if let Some(claim) = claim {
        project.apply(Op::SetThreatClaim {
            threat: id,
            claim: Some(claim),
            provenance: provenance.clone(),
        })?;
    }
    if scales.is_some() || scaling_reason.is_some() {
        project.apply(Op::SetThreatScaling {
            threat: id,
            scales,
            reason: scaling_reason.unwrap_or_else(|| Arc::from("")),
            provenance: provenance.clone(),
        })?;
    }
    if let Some(text) = feasibility_rationale {
        project.apply(Op::SetFeasibilityRationale {
            threat: id,
            text,
            provenance,
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit;

    #[test]
    fn capped_reader_allows_exactly_the_cap() {
        let mut sink = std::io::sink();
        let cap = MAX_PROJECT_FILE_BYTES;
        let mut at = CappedReader::new(std::io::repeat(b' ').take(cap), cap);
        assert_eq!(std::io::copy(&mut at, &mut sink).ok(), Some(cap));
        assert!(!at.exceeded());
        let mut over = CappedReader::new(std::io::repeat(b' '), cap);
        assert!(std::io::copy(&mut over, &mut sink).is_err());
        assert!(over.exceeded());
        assert_eq!(over.bytes_read(), cap + 1);
    }

    #[test]
    fn load_reports_io_errors_as_io() {
        struct Broken;
        impl Read for Broken {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("disk gone"))
            }
        }
        assert!(matches!(
            Project::load(Broken, testkit::second()),
            Err(StoreError::Io(_))
        ));
    }
    #[test]
    fn empty_project_round_trips() {
        let m = testkit::second();
        let p = Project::new(Arc::clone(&m));
        let mut bytes = Vec::new();
        assert!(p.save(&mut bytes).is_ok());
        // The test methodology is named only through the test kit, so no
        // `src` file names it (job 012, AC6).
        let (key, version) = (m.key(), m.version());
        assert_eq!(
            String::from_utf8_lossy(&bytes),
            format!(
                "{{\"format\":\"aegis-project\",\"version\":1,\"methodology\":{{\"key\":\"{key}\",\"version\":\"{version}\"}},\"assets\":[],\"damage_scenarios\":[],\"threat_scenarios\":[]}}\n"
            )
        );
        assert_eq!(Project::load(bytes.as_slice(), m).ok(), Some(p));
    }
}
