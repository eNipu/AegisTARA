//! The append-only op log: `aegis-oplog` v1, JSON Lines.

use std::io::{BufRead, ErrorKind, Write};
use std::sync::Arc;

use serde::Serialize;

use super::wire::{
    self, AttackFilterView, CatalogRefView, CatalogRefsInOrder, FORMAT_VERSION, GateKindWire,
    HeaderView, HeaderWire, Ids, ImpactsInOrder, ItemView, KeyOrder, MethodologyView, NodesView,
    OpWire, PropertyKeys, ProvenanceView, RecordWire, Required, RiskTreatmentWire,
    StepFeasibilityInOrder, TargetView, Text, WireId,
};
use super::{
    Limit, MAX_ACTOR_BYTES, MAX_DESCRIPTION_BYTES, MAX_KEY_BYTES, MAX_NAME_BYTES,
    MAX_OP_RECORD_BYTES,
};
use crate::applicability::{MAX_ASSUMPTION_EXCLUDES, StepRef};
use crate::control::MAX_GOAL_CONTROLS;
use crate::error::StoreError;
use crate::id::{
    AssetId, AssumptionId, ClaimId, ComponentId, ConditionId, ControlId, DamageId, DataFlowId,
    ElementId, GoalId, InterfaceId, NodeId, PartyId, SharedStepId, ThreatId, TreeId,
    TrustBoundaryId,
};
use crate::item::MAX_TRUST_BOUNDARY_MEMBERS;
use crate::methodology::Methodology;
use crate::op::{Op, StepTarget};
use crate::threat::{MAX_CATALOG_REFS, MAX_DAMAGE_LINKS};
use crate::tree::MAX_TREE_OP_NODES;

/// One record of an op log: its sequence number and the op.
///
/// ```
/// use aegis_model::{AssetId, LoggedOp, Op};
///
/// let record = LoggedOp { seq: 1, op: Op::RemoveAsset { id: AssetId::new_v7() } };
/// assert_eq!(record.seq, 1);
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoggedOp {
    /// Position in the log: 1 for the first record, then up by exactly 1.
    pub seq: u64,
    /// The op, with indices of the reader's methodology.
    pub op: Op,
}

// ---------------------------------------------------------------------------
// Encoding.
// ---------------------------------------------------------------------------

/// One record line, borrowed from the op.
#[derive(Serialize)]
struct RecordView<'a> {
    seq: u64,
    op: OpView<'a>,
}

/// An op with every index resolved to a key while it is written.
#[derive(Serialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum OpView<'a> {
    SetReviewMark {
        tree: Text<TreeId>,
        node: Text<NodeId>,
        marked: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    AddTreeNode {
        tree: Text<TreeId>,
        parent: Text<NodeId>,
        id: Text<NodeId>,
        label: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        gate: Option<GateKindWire>,
        index: u32,
        position: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    RemoveTreeNode {
        tree: Text<TreeId>,
        id: Text<NodeId>,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    LinkTreeNode {
        tree: Text<TreeId>,
        parent: Text<NodeId>,
        child: Text<NodeId>,
        position: u32,
        slot: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    UnlinkTreeNode {
        tree: Text<TreeId>,
        parent: Text<NodeId>,
        child: Text<NodeId>,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    RenameTreeNode {
        tree: Text<TreeId>,
        id: Text<NodeId>,
        label: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    SetTreeNodeGate {
        tree: Text<TreeId>,
        id: Text<NodeId>,
        gate: GateKindWire,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    SetDescription {
        target: TargetView,
        text: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    SetDescriptionTranslation {
        target: TargetView,
        lang: &'a str,
        text: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    SetProjectLanguages {
        langs: Vec<&'a str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    SetAttackFilter {
        filter: AttackFilterView<'a>,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    AddAsset {
        id: Text<AssetId>,
        name: &'a str,
        properties: PropertyKeys<'a>,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    RenameAsset {
        id: Text<AssetId>,
        name: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    SetAssetProperties {
        id: Text<AssetId>,
        properties: PropertyKeys<'a>,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    RemoveAsset {
        id: Text<AssetId>,
    },
    AddDamageScenario {
        id: Text<DamageId>,
        name: &'a str,
        impacts: ImpactsInOrder<'a>,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    RenameDamageScenario {
        id: Text<DamageId>,
        name: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    SetImpact {
        id: Text<DamageId>,
        category: Required<&'a str>,
        level: Option<Required<&'a str>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    RemoveDamageScenario {
        id: Text<DamageId>,
    },
    AddThreatScenario {
        id: Text<ThreatId>,
        name: &'a str,
        asset: Text<AssetId>,
        category: Required<&'a str>,
        catalog_refs: CatalogRefsInOrder<'a>,
        damages: Ids<'a, DamageId>,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    RenameThreatScenario {
        id: Text<ThreatId>,
        name: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    SetThreatCategory {
        id: Text<ThreatId>,
        category: Required<&'a str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    AddCatalogRef {
        threat: Text<ThreatId>,
        catalog_ref: CatalogRefView<'a>,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    RemoveCatalogRef {
        threat: Text<ThreatId>,
        catalog_ref: CatalogRefView<'a>,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    LinkDamage {
        threat: Text<ThreatId>,
        damage: Text<DamageId>,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    UnlinkDamage {
        threat: Text<ThreatId>,
        damage: Text<DamageId>,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    RemoveThreatScenario {
        id: Text<ThreatId>,
    },
    AddAttackTree {
        #[serde(skip_serializing_if = "crate::TreeRole::is_threat")]
        role: crate::TreeRole,
        id: Text<TreeId>,
        nodes: NodesView<'a>,
        step_feasibility: StepFeasibilityInOrder<'a>,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    SetStepFeasibility {
        #[serde(skip_serializing_if = "Option::is_none")]
        tree: Option<Text<TreeId>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        node: Option<Text<NodeId>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        shared_step: Option<Text<SharedStepId>>,
        level: Option<Required<&'a str>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    SetStepVia {
        #[serde(skip_serializing_if = "Option::is_none")]
        tree: Option<Text<TreeId>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        node: Option<Text<NodeId>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        shared_step: Option<Text<SharedStepId>>,
        via: Option<ItemView>,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    SetStepAttackType {
        #[serde(skip_serializing_if = "Option::is_none")]
        tree: Option<Text<TreeId>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        node: Option<Text<NodeId>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        shared_step: Option<Text<SharedStepId>>,
        attack_type: Option<Required<&'a str>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    SetStepFactor {
        #[serde(skip_serializing_if = "Option::is_none")]
        tree: Option<Text<TreeId>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        node: Option<Text<NodeId>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        shared_step: Option<Text<SharedStepId>>,
        factor: Required<&'a str>,
        level: Option<Required<&'a str>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    SetThreatFeasibility {
        threat: Text<ThreatId>,
        level: Option<Required<&'a str>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    SetThreatAttackTree {
        threat: Text<ThreatId>,
        tree: Option<Text<TreeId>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    RemoveAttackTree {
        id: Text<TreeId>,
    },
    SetRiskTreatment {
        threat: Text<ThreatId>,
        treatment: Option<RiskTreatmentWire>,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    AddSecurityControl {
        id: Text<ControlId>,
        name: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    RenameSecurityControl {
        id: Text<ControlId>,
        name: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    RemoveSecurityControl {
        id: Text<ControlId>,
    },
    ApplyControl {
        control: Text<ControlId>,
        target: ItemView,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    UnapplyControl {
        control: Text<ControlId>,
        target: ItemView,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    SetControlRule {
        control: Text<ControlId>,
        attack_type: Required<&'a str>,
        ceiling: Option<Required<&'a str>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    SetControlKind {
        control: Text<ControlId>,
        kind: Option<Required<&'a str>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    SetControlStatus {
        control: Text<ControlId>,
        status: &'static str,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    SetStepMitigation {
        tree: Text<TreeId>,
        node: Text<NodeId>,
        control: Text<ControlId>,
        feasibility: Option<Required<&'a str>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    AddCybersecurityGoal {
        id: Text<GoalId>,
        name: &'a str,
        controls: Ids<'a, ControlId>,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    RenameCybersecurityGoal {
        id: Text<GoalId>,
        name: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    LinkGoalControl {
        goal: Text<GoalId>,
        control: Text<ControlId>,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    UnlinkGoalControl {
        goal: Text<GoalId>,
        control: Text<ControlId>,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    LinkGoalParty {
        goal: Text<GoalId>,
        party: Text<PartyId>,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    UnlinkGoalParty {
        goal: Text<GoalId>,
        party: Text<PartyId>,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    RemoveCybersecurityGoal {
        id: Text<GoalId>,
    },
    AddCybersecurityClaim {
        id: Text<ClaimId>,
        name: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    RenameCybersecurityClaim {
        id: Text<ClaimId>,
        name: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    LinkClaimParty {
        claim: Text<ClaimId>,
        party: Text<PartyId>,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    UnlinkClaimParty {
        claim: Text<ClaimId>,
        party: Text<PartyId>,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    LinkClaimAssumption {
        claim: Text<ClaimId>,
        assumption: Text<AssumptionId>,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    UnlinkClaimAssumption {
        claim: Text<ClaimId>,
        assumption: Text<AssumptionId>,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    RemoveCybersecurityClaim {
        id: Text<ClaimId>,
    },
    AddParty {
        id: Text<PartyId>,
        name: &'a str,
        kind: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    RenameParty {
        id: Text<PartyId>,
        name: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    SetPartyKind {
        id: Text<PartyId>,
        kind: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    RemoveParty {
        id: Text<PartyId>,
    },
    AddCondition {
        id: Text<ConditionId>,
        name: &'a str,
        description: &'a str,
        present: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    RenameCondition {
        id: Text<ConditionId>,
        name: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    SetConditionPresent {
        id: Text<ConditionId>,
        present: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    RemoveCondition {
        id: Text<ConditionId>,
    },
    AddAssumption {
        id: Text<AssumptionId>,
        name: &'a str,
        holds: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    RenameAssumption {
        id: Text<AssumptionId>,
        name: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    SetAssumptionHolds {
        id: Text<AssumptionId>,
        holds: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    SetAssumptionExcludes {
        id: Text<AssumptionId>,
        trees: Vec<Text<TreeId>>,
        steps: Vec<StepRefView>,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    RemoveAssumption {
        id: Text<AssumptionId>,
    },
    AddSharedStep {
        id: Text<SharedStepId>,
        name: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    RenameSharedStep {
        id: Text<SharedStepId>,
        name: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    RemoveSharedStep {
        id: Text<SharedStepId>,
    },
    UseSharedStep {
        tree: Text<TreeId>,
        node: Text<NodeId>,
        shared_step: Text<SharedStepId>,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    UnuseSharedStep {
        tree: Text<TreeId>,
        node: Text<NodeId>,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    NeedCondition {
        element: TargetView,
        condition: Text<ConditionId>,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    UnneedCondition {
        element: TargetView,
        condition: Text<ConditionId>,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    NeedStepCondition {
        tree: Text<TreeId>,
        node: Text<NodeId>,
        condition: Text<ConditionId>,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    UnneedStepCondition {
        tree: Text<TreeId>,
        node: Text<NodeId>,
        condition: Text<ConditionId>,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    SetThreatGoal {
        threat: Text<ThreatId>,
        goal: Option<Text<GoalId>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    LinkThreatGoal {
        threat: Text<ThreatId>,
        goal: Text<GoalId>,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    UnlinkThreatGoal {
        threat: Text<ThreatId>,
        goal: Text<GoalId>,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    SetThreatClaim {
        threat: Text<ThreatId>,
        claim: Option<Text<ClaimId>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    SetThreatScaling {
        threat: Text<ThreatId>,
        scales: Option<bool>,
        reason: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    SetImpactRationale {
        damage: Text<DamageId>,
        text: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    SetFeasibilityRationale {
        threat: Text<ThreatId>,
        text: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    SetDamageLinkRationale {
        threat: Text<ThreatId>,
        damage: Text<DamageId>,
        text: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    AddComponent {
        id: Text<ComponentId>,
        name: &'a str,
        parent: Option<Text<ComponentId>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    RenameComponent {
        id: Text<ComponentId>,
        name: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    SetComponentParent {
        id: Text<ComponentId>,
        parent: Option<Text<ComponentId>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    SetComponentSystem {
        id: Text<ComponentId>,
        system: Option<Text<ElementId>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    RemoveComponent {
        id: Text<ComponentId>,
    },
    AddInterface {
        id: Text<InterfaceId>,
        name: &'a str,
        component: Text<ComponentId>,
        kind: Required<&'a str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    RenameInterface {
        id: Text<InterfaceId>,
        name: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    SetInterfaceKind {
        id: Text<InterfaceId>,
        kind: Required<&'a str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    RemoveInterface {
        id: Text<InterfaceId>,
    },
    AddDataFlow {
        id: Text<DataFlowId>,
        name: &'a str,
        from: Text<ComponentId>,
        to: Text<ComponentId>,
        interface: Option<Text<InterfaceId>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    RenameDataFlow {
        id: Text<DataFlowId>,
        name: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    SetDataFlowInterface {
        id: Text<DataFlowId>,
        interface: Option<Text<InterfaceId>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    RemoveDataFlow {
        id: Text<DataFlowId>,
    },
    AddTrustBoundary {
        id: Text<TrustBoundaryId>,
        name: &'a str,
        components: Ids<'a, ComponentId>,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    RenameTrustBoundary {
        id: Text<TrustBoundaryId>,
        name: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    AddBoundaryComponent {
        boundary: Text<TrustBoundaryId>,
        component: Text<ComponentId>,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    RemoveBoundaryComponent {
        boundary: Text<TrustBoundaryId>,
        component: Text<ComponentId>,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    RemoveTrustBoundary {
        id: Text<TrustBoundaryId>,
    },
    SetAssetComponent {
        asset: Text<AssetId>,
        component: Option<Text<ComponentId>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    AddElement {
        id: Text<ElementId>,
        kind: &'static str,
        name: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    RenameElement {
        id: Text<ElementId>,
        name: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    RemoveElement {
        id: Text<ElementId>,
    },
    SetElementAttribute {
        id: Text<ElementId>,
        key: &'a str,
        value: Option<AttrView<'a>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    LinkElement {
        id: Text<ElementId>,
        target: LinkView,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
    UnlinkElement {
        id: Text<ElementId>,
        target: LinkView,
        #[serde(skip_serializing_if = "Option::is_none")]
        provenance: Option<ProvenanceView<'a>>,
    },
}

/// One element link target as written to the op log.
#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum LinkView {
    Threat(Text<ThreatId>),
    Component(Text<ComponentId>),
}

impl LinkView {
    pub(super) fn of(t: crate::element::LinkTarget) -> LinkView {
        match t {
            crate::element::LinkTarget::Threat(id) => LinkView::Threat(Text(id)),
            crate::element::LinkTarget::Component(id) => LinkView::Component(Text(id)),
        }
    }
}

/// One element attribute value as written to the op log.
#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum AttrView<'a> {
    Level(&'a str),
    Choice(&'a str),
    Text(&'a str),
}

/// One excluded attack-tree step as written to the op log.
#[derive(Serialize)]
struct StepRefView {
    tree: Text<TreeId>,
    node: Text<NodeId>,
}

impl<'a> AttrView<'a> {
    pub(super) fn of(v: &'a crate::element::AttrValue) -> AttrView<'a> {
        use crate::element::AttrValue as V;
        match v {
            V::Level(k) => AttrView::Level(k),
            V::Choice(k) => AttrView::Choice(k),
            V::Text(t) => AttrView::Text(t),
        }
    }
}

/// Refuses a name, provenance actor, or list the reader would refuse, so
/// the writer never writes a record it cannot read back. Time O(V) for an
/// `AddAttackTree` op with V nodes, O(1) for every other op.
fn within_caps(op: &Op) -> bool {
    if op
        .provenance()
        .is_some_and(|p| p.actor().len() > MAX_ACTOR_BYTES)
    {
        return false;
    }
    let name_ok = |name: &str| name.len() <= MAX_NAME_BYTES;
    match op {
        Op::SetDescription { text, .. }
        | Op::SetThreatScaling { reason: text, .. }
        | Op::SetImpactRationale { text, .. }
        | Op::SetFeasibilityRationale { text, .. }
        | Op::SetDamageLinkRationale { text, .. } => text.len() <= MAX_DESCRIPTION_BYTES,
        Op::SetDescriptionTranslation { lang, text, .. } => {
            lang.len() <= crate::description::MAX_LANG_TAG_BYTES
                && text.len() <= MAX_DESCRIPTION_BYTES
        }
        Op::SetProjectLanguages { langs, .. } => langs
            .iter()
            .all(|l| l.len() <= crate::description::MAX_LANG_TAG_BYTES),
        Op::SetElementAttribute { key, value, .. } => {
            use crate::element::AttrValue as V;
            key.len() <= MAX_KEY_BYTES
                && match value {
                    None => true,
                    Some(V::Level(k) | V::Choice(k)) => k.len() <= MAX_KEY_BYTES,
                    Some(V::Text(t)) => t.len() <= MAX_NAME_BYTES,
                }
        }
        Op::AddTreeNode { node, .. } => name_ok(node.label()),
        Op::RenameTreeNode { label, .. } => name_ok(label),
        Op::AddCondition {
            name, description, ..
        } => name_ok(name) && description.len() <= MAX_DESCRIPTION_BYTES,
        Op::AddAsset { name, .. }
        | Op::RenameAsset { name, .. }
        | Op::AddDamageScenario { name, .. }
        | Op::RenameDamageScenario { name, .. }
        | Op::RenameThreatScenario { name, .. }
        | Op::AddSecurityControl { name, .. }
        | Op::RenameSecurityControl { name, .. }
        | Op::RenameCybersecurityGoal { name, .. }
        | Op::AddCybersecurityClaim { name, .. }
        | Op::RenameCybersecurityClaim { name, .. }
        | Op::AddParty { name, .. }
        | Op::RenameParty { name, .. }
        | Op::RenameCondition { name, .. }
        | Op::AddAssumption { name, .. }
        | Op::RenameAssumption { name, .. }
        | Op::AddSharedStep { name, .. }
        | Op::RenameSharedStep { name, .. }
        | Op::AddElement { name, .. }
        | Op::RenameElement { name, .. }
        | Op::AddComponent { name, .. }
        | Op::RenameComponent { name, .. }
        | Op::AddInterface { name, .. }
        | Op::RenameInterface { name, .. }
        | Op::AddDataFlow { name, .. }
        | Op::RenameDataFlow { name, .. }
        | Op::RenameTrustBoundary { name, .. } => name_ok(name),
        Op::AddCybersecurityGoal { name, controls, .. } => {
            name_ok(name) && controls.len() <= MAX_GOAL_CONTROLS
        }
        Op::AddTrustBoundary {
            name, components, ..
        } => name_ok(name) && components.len() <= MAX_TRUST_BOUNDARY_MEMBERS,
        Op::AddThreatScenario {
            name,
            catalog_refs,
            damages,
            ..
        } => {
            name_ok(name)
                && catalog_refs.len() <= MAX_CATALOG_REFS
                && damages.len() <= MAX_DAMAGE_LINKS
        }
        Op::AddAttackTree {
            tree,
            step_feasibility,
            ..
        } => {
            tree.len() <= MAX_TREE_OP_NODES
                && step_feasibility.len() <= MAX_TREE_OP_NODES
                && tree.iter().all(|(_, _, n)| name_ok(n.label()))
        }
        Op::SetAssumptionExcludes { trees, steps, .. } => {
            trees.len().saturating_add(steps.len()) <= MAX_ASSUMPTION_EXCLUDES
        }
        Op::SetAttackFilter { provenance, .. } => provenance.actor().len() <= MAX_ACTOR_BYTES,
        _ => true,
    }
}

type StepTargetView = (
    Option<Text<TreeId>>,
    Option<Text<NodeId>>,
    Option<Text<SharedStepId>>,
);

fn step_target_view(target: StepTarget) -> StepTargetView {
    match target {
        StepTarget::Step(tree, node) => (Some(Text(tree)), Some(Text(node)), None),
        StepTarget::Shared(id) => (None, None, Some(Text(id))),
    }
}

/// Builds the borrowed view of `op`. Index-to-key lookups happen while it
/// is serialized. Time O(1).
#[allow(clippy::too_many_lines)]
fn op_view<'a>(m: &'a Methodology, order: &'a KeyOrder, op: &'a Op) -> OpView<'a> {
    let props = |set| PropertyKeys { m, order, set };
    match op {
        Op::AddTreeNode {
            tree,
            parent,
            id,
            node,
            index,
            position,
            provenance,
        } => OpView::AddTreeNode {
            tree: Text(*tree),
            parent: Text(*parent),
            id: Text(*id),
            label: node.label(),
            gate: node.gate_kind().map(GateKindWire::from),
            index: *index,
            position: *position,
            provenance: ProvenanceView::of(provenance),
        },
        Op::RemoveTreeNode {
            tree,
            id,
            provenance,
        } => OpView::RemoveTreeNode {
            tree: Text(*tree),
            id: Text(*id),
            provenance: ProvenanceView::of(provenance),
        },
        Op::LinkTreeNode {
            tree,
            parent,
            child,
            position,
            slot,
            provenance,
        } => OpView::LinkTreeNode {
            tree: Text(*tree),
            parent: Text(*parent),
            child: Text(*child),
            position: *position,
            slot: *slot,
            provenance: ProvenanceView::of(provenance),
        },
        Op::UnlinkTreeNode {
            tree,
            parent,
            child,
            provenance,
        } => OpView::UnlinkTreeNode {
            tree: Text(*tree),
            parent: Text(*parent),
            child: Text(*child),
            provenance: ProvenanceView::of(provenance),
        },
        Op::RenameTreeNode {
            tree,
            id,
            label,
            provenance,
        } => OpView::RenameTreeNode {
            tree: Text(*tree),
            id: Text(*id),
            label,
            provenance: ProvenanceView::of(provenance),
        },
        Op::SetTreeNodeGate {
            tree,
            id,
            gate,
            provenance,
        } => OpView::SetTreeNodeGate {
            tree: Text(*tree),
            id: Text(*id),
            gate: GateKindWire::from(*gate),
            provenance: ProvenanceView::of(provenance),
        },
        Op::SetReviewMark {
            tree,
            node,
            marked,
            provenance,
        } => OpView::SetReviewMark {
            tree: Text(*tree),
            node: Text(*node),
            marked: *marked,
            provenance: ProvenanceView::of(provenance),
        },
        Op::SetDescription {
            target,
            text,
            provenance,
        } => OpView::SetDescription {
            target: (*target).into(),
            text,
            provenance: ProvenanceView::of(provenance),
        },
        Op::SetDescriptionTranslation {
            target,
            lang,
            text,
            provenance,
        } => OpView::SetDescriptionTranslation {
            target: (*target).into(),
            lang,
            text,
            provenance: ProvenanceView::of(provenance),
        },
        Op::SetProjectLanguages { langs, provenance } => OpView::SetProjectLanguages {
            langs: langs.iter().map(|l| &**l).collect(),
            provenance: ProvenanceView::of(provenance),
        },
        Op::SetAttackFilter { filter, provenance } => OpView::SetAttackFilter {
            filter: AttackFilterView {
                m,
                order,
                filter: *filter,
            },
            provenance: ProvenanceView::of(provenance),
        },
        Op::AddAsset {
            id,
            name,
            properties,
            provenance,
        } => OpView::AddAsset {
            id: Text(*id),
            name,
            properties: props(*properties),
            provenance: ProvenanceView::of(provenance),
        },
        Op::RenameAsset {
            id,
            name,
            provenance,
        } => OpView::RenameAsset {
            id: Text(*id),
            name,
            provenance: ProvenanceView::of(provenance),
        },
        Op::SetAssetProperties {
            id,
            properties,
            provenance,
        } => OpView::SetAssetProperties {
            id: Text(*id),
            properties: props(*properties),
            provenance: ProvenanceView::of(provenance),
        },
        Op::RemoveAsset { id } => OpView::RemoveAsset { id: Text(*id) },
        Op::AddDamageScenario {
            id,
            name,
            impacts,
            provenance,
        } => OpView::AddDamageScenario {
            id: Text(*id),
            name,
            impacts: ImpactsInOrder { m, impacts },
            provenance: ProvenanceView::of(provenance),
        },
        Op::RenameDamageScenario {
            id,
            name,
            provenance,
        } => OpView::RenameDamageScenario {
            id: Text(*id),
            name,
            provenance: ProvenanceView::of(provenance),
        },
        Op::SetImpact {
            id,
            category,
            level,
            provenance,
        } => OpView::SetImpact {
            id: Text(*id),
            category: Required(m.impact_category_key(*category)),
            level: level.map(|l| Required(m.level_key(*category, l))),
            provenance: ProvenanceView::of(provenance),
        },
        Op::RemoveDamageScenario { id } => OpView::RemoveDamageScenario { id: Text(*id) },
        Op::AddThreatScenario {
            id,
            name,
            asset,
            category,
            catalog_refs,
            damages,
            provenance,
        } => OpView::AddThreatScenario {
            id: Text(*id),
            name,
            asset: Text(*asset),
            category: Required(m.threat_category_key(*category)),
            catalog_refs: CatalogRefsInOrder {
                m,
                refs: catalog_refs,
            },
            damages: Ids(damages),
            provenance: ProvenanceView::of(provenance),
        },
        Op::RenameThreatScenario {
            id,
            name,
            provenance,
        } => OpView::RenameThreatScenario {
            id: Text(*id),
            name,
            provenance: ProvenanceView::of(provenance),
        },
        Op::SetThreatCategory {
            id,
            category,
            provenance,
        } => OpView::SetThreatCategory {
            id: Text(*id),
            category: Required(m.threat_category_key(*category)),
            provenance: ProvenanceView::of(provenance),
        },
        Op::AddCatalogRef {
            threat,
            catalog_ref,
            provenance,
        } => OpView::AddCatalogRef {
            threat: Text(*threat),
            catalog_ref: CatalogRefView { m, r: *catalog_ref },
            provenance: ProvenanceView::of(provenance),
        },
        Op::RemoveCatalogRef {
            threat,
            catalog_ref,
            provenance,
        } => OpView::RemoveCatalogRef {
            threat: Text(*threat),
            catalog_ref: CatalogRefView { m, r: *catalog_ref },
            provenance: ProvenanceView::of(provenance),
        },
        Op::LinkDamage {
            threat,
            damage,
            provenance,
        } => OpView::LinkDamage {
            threat: Text(*threat),
            damage: Text(*damage),
            provenance: ProvenanceView::of(provenance),
        },
        Op::UnlinkDamage {
            threat,
            damage,
            provenance,
        } => OpView::UnlinkDamage {
            threat: Text(*threat),
            damage: Text(*damage),
            provenance: ProvenanceView::of(provenance),
        },
        Op::RemoveThreatScenario { id } => OpView::RemoveThreatScenario { id: Text(*id) },
        Op::AddAttackTree {
            role,
            id,
            tree,
            step_feasibility,
            provenance,
        } => OpView::AddAttackTree {
            role: *role,
            id: Text(*id),
            nodes: NodesView(tree),
            step_feasibility: StepFeasibilityInOrder {
                m,
                ratings: step_feasibility,
            },
            provenance: ProvenanceView::of(provenance),
        },
        Op::SetStepFeasibility {
            target,
            level,
            provenance,
        } => {
            let (tree, node, shared_step) = step_target_view(*target);
            OpView::SetStepFeasibility {
                tree,
                node,
                shared_step,
                level: level.map(|l| Required(m.feasibility_level_key(l))),
                provenance: ProvenanceView::of(provenance),
            }
        }
        Op::SetStepVia {
            target,
            via,
            provenance,
        } => {
            let (tree, node, shared_step) = step_target_view(*target);
            OpView::SetStepVia {
                tree,
                node,
                shared_step,
                via: via.map(ItemView::of),
                provenance: ProvenanceView::of(provenance),
            }
        }
        Op::SetStepAttackType {
            target,
            attack_type,
            provenance,
        } => {
            let (tree, node, shared_step) = step_target_view(*target);
            OpView::SetStepAttackType {
                tree,
                node,
                shared_step,
                attack_type: attack_type.map(|t| Required(m.attack_type_key(t))),
                provenance: ProvenanceView::of(provenance),
            }
        }
        Op::SetStepFactor {
            target,
            factor,
            level,
            provenance,
        } => {
            let (tree, node, shared_step) = step_target_view(*target);
            OpView::SetStepFactor {
                tree,
                node,
                shared_step,
                factor: Required(m.feasibility_factor_key(*factor)),
                level: level.map(|l| Required(m.factor_level_key(*factor, l))),
                provenance: ProvenanceView::of(provenance),
            }
        }
        Op::SetThreatFeasibility {
            threat,
            level,
            provenance,
        } => OpView::SetThreatFeasibility {
            threat: Text(*threat),
            level: level.map(|l| Required(m.feasibility_level_key(l))),
            provenance: ProvenanceView::of(provenance),
        },
        Op::SetThreatAttackTree {
            threat,
            tree,
            provenance,
        } => OpView::SetThreatAttackTree {
            threat: Text(*threat),
            tree: tree.map(Text),
            provenance: ProvenanceView::of(provenance),
        },
        Op::RemoveAttackTree { id } => OpView::RemoveAttackTree { id: Text(*id) },
        Op::SetRiskTreatment {
            threat,
            treatment,
            provenance,
        } => OpView::SetRiskTreatment {
            threat: Text(*threat),
            treatment: (*treatment).map(RiskTreatmentWire::from),
            provenance: ProvenanceView::of(provenance),
        },
        Op::AddSecurityControl {
            id,
            name,
            provenance,
        } => OpView::AddSecurityControl {
            id: Text(*id),
            name,
            provenance: ProvenanceView::of(provenance),
        },
        Op::RenameSecurityControl {
            id,
            name,
            provenance,
        } => OpView::RenameSecurityControl {
            id: Text(*id),
            name,
            provenance: ProvenanceView::of(provenance),
        },
        Op::RemoveSecurityControl { id } => OpView::RemoveSecurityControl { id: Text(*id) },
        Op::ApplyControl {
            control,
            target,
            provenance,
        } => OpView::ApplyControl {
            control: Text(*control),
            target: ItemView::of(*target),
            provenance: ProvenanceView::of(provenance),
        },
        Op::UnapplyControl {
            control,
            target,
            provenance,
        } => OpView::UnapplyControl {
            control: Text(*control),
            target: ItemView::of(*target),
            provenance: ProvenanceView::of(provenance),
        },
        Op::SetControlRule {
            control,
            attack_type,
            ceiling,
            provenance,
        } => OpView::SetControlRule {
            control: Text(*control),
            attack_type: Required(m.attack_type_key(*attack_type)),
            ceiling: ceiling.map(|l| Required(m.feasibility_level_key(l))),
            provenance: ProvenanceView::of(provenance),
        },
        Op::SetControlKind {
            control,
            kind,
            provenance,
        } => OpView::SetControlKind {
            control: Text(*control),
            kind: kind.map(|k| Required(m.control_kind_key(k))),
            provenance: ProvenanceView::of(provenance),
        },
        Op::SetControlStatus {
            control,
            status,
            provenance,
        } => OpView::SetControlStatus {
            control: Text(*control),
            status: status.as_str(),
            provenance: ProvenanceView::of(provenance),
        },
        Op::SetStepMitigation {
            tree,
            node,
            control,
            feasibility,
            provenance,
        } => OpView::SetStepMitigation {
            tree: Text(*tree),
            node: Text(*node),
            control: Text(*control),
            feasibility: feasibility.map(|l| Required(m.feasibility_level_key(l))),
            provenance: ProvenanceView::of(provenance),
        },
        Op::AddCybersecurityGoal {
            id,
            name,
            controls,
            provenance,
        } => OpView::AddCybersecurityGoal {
            id: Text(*id),
            name,
            controls: Ids(controls),
            provenance: ProvenanceView::of(provenance),
        },
        Op::RenameCybersecurityGoal {
            id,
            name,
            provenance,
        } => OpView::RenameCybersecurityGoal {
            id: Text(*id),
            name,
            provenance: ProvenanceView::of(provenance),
        },
        Op::LinkGoalControl {
            goal,
            control,
            provenance,
        } => OpView::LinkGoalControl {
            goal: Text(*goal),
            control: Text(*control),
            provenance: ProvenanceView::of(provenance),
        },
        Op::UnlinkGoalControl {
            goal,
            control,
            provenance,
        } => OpView::UnlinkGoalControl {
            goal: Text(*goal),
            control: Text(*control),
            provenance: ProvenanceView::of(provenance),
        },
        Op::LinkGoalParty {
            goal,
            party,
            provenance,
        } => OpView::LinkGoalParty {
            goal: Text(*goal),
            party: Text(*party),
            provenance: ProvenanceView::of(provenance),
        },
        Op::UnlinkGoalParty {
            goal,
            party,
            provenance,
        } => OpView::UnlinkGoalParty {
            goal: Text(*goal),
            party: Text(*party),
            provenance: ProvenanceView::of(provenance),
        },
        Op::RemoveCybersecurityGoal { id } => OpView::RemoveCybersecurityGoal { id: Text(*id) },
        Op::AddCybersecurityClaim {
            id,
            name,
            provenance,
        } => OpView::AddCybersecurityClaim {
            id: Text(*id),
            name,
            provenance: ProvenanceView::of(provenance),
        },
        Op::RenameCybersecurityClaim {
            id,
            name,
            provenance,
        } => OpView::RenameCybersecurityClaim {
            id: Text(*id),
            name,
            provenance: ProvenanceView::of(provenance),
        },
        Op::LinkClaimParty {
            claim,
            party,
            provenance,
        } => OpView::LinkClaimParty {
            claim: Text(*claim),
            party: Text(*party),
            provenance: ProvenanceView::of(provenance),
        },
        Op::UnlinkClaimParty {
            claim,
            party,
            provenance,
        } => OpView::UnlinkClaimParty {
            claim: Text(*claim),
            party: Text(*party),
            provenance: ProvenanceView::of(provenance),
        },
        Op::LinkClaimAssumption {
            claim,
            assumption,
            provenance,
        } => OpView::LinkClaimAssumption {
            claim: Text(*claim),
            assumption: Text(*assumption),
            provenance: ProvenanceView::of(provenance),
        },
        Op::UnlinkClaimAssumption {
            claim,
            assumption,
            provenance,
        } => OpView::UnlinkClaimAssumption {
            claim: Text(*claim),
            assumption: Text(*assumption),
            provenance: ProvenanceView::of(provenance),
        },
        Op::RemoveCybersecurityClaim { id } => OpView::RemoveCybersecurityClaim { id: Text(*id) },
        Op::AddParty {
            id,
            name,
            kind,
            provenance,
        } => OpView::AddParty {
            id: Text(*id),
            name,
            kind: kind.as_str(),
            provenance: ProvenanceView::of(provenance),
        },
        Op::RenameParty {
            id,
            name,
            provenance,
        } => OpView::RenameParty {
            id: Text(*id),
            name,
            provenance: ProvenanceView::of(provenance),
        },
        Op::SetPartyKind {
            id,
            kind,
            provenance,
        } => OpView::SetPartyKind {
            id: Text(*id),
            kind: kind.as_str(),
            provenance: ProvenanceView::of(provenance),
        },
        Op::RemoveParty { id } => OpView::RemoveParty { id: Text(*id) },
        Op::AddCondition {
            id,
            name,
            description,
            present,
            provenance,
        } => OpView::AddCondition {
            id: Text(*id),
            name,
            description,
            present: *present,
            provenance: ProvenanceView::of(provenance),
        },
        Op::RenameCondition {
            id,
            name,
            provenance,
        } => OpView::RenameCondition {
            id: Text(*id),
            name,
            provenance: ProvenanceView::of(provenance),
        },
        Op::SetConditionPresent {
            id,
            present,
            provenance,
        } => OpView::SetConditionPresent {
            id: Text(*id),
            present: *present,
            provenance: ProvenanceView::of(provenance),
        },
        Op::RemoveCondition { id } => OpView::RemoveCondition { id: Text(*id) },
        Op::AddAssumption {
            id,
            name,
            holds,
            provenance,
        } => OpView::AddAssumption {
            id: Text(*id),
            name,
            holds: *holds,
            provenance: ProvenanceView::of(provenance),
        },
        Op::RenameAssumption {
            id,
            name,
            provenance,
        } => OpView::RenameAssumption {
            id: Text(*id),
            name,
            provenance: ProvenanceView::of(provenance),
        },
        Op::SetAssumptionHolds {
            id,
            holds,
            provenance,
        } => OpView::SetAssumptionHolds {
            id: Text(*id),
            holds: *holds,
            provenance: ProvenanceView::of(provenance),
        },
        Op::SetAssumptionExcludes {
            id,
            trees,
            steps,
            provenance,
        } => OpView::SetAssumptionExcludes {
            id: Text(*id),
            trees: trees.iter().copied().map(Text).collect(),
            steps: steps
                .iter()
                .map(|s| StepRefView {
                    tree: Text(s.tree),
                    node: Text(s.node),
                })
                .collect(),
            provenance: ProvenanceView::of(provenance),
        },
        Op::RemoveAssumption { id } => OpView::RemoveAssumption { id: Text(*id) },
        Op::AddSharedStep {
            id,
            name,
            provenance,
        } => OpView::AddSharedStep {
            id: Text(*id),
            name,
            provenance: ProvenanceView::of(provenance),
        },
        Op::RenameSharedStep {
            id,
            name,
            provenance,
        } => OpView::RenameSharedStep {
            id: Text(*id),
            name,
            provenance: ProvenanceView::of(provenance),
        },
        Op::RemoveSharedStep { id } => OpView::RemoveSharedStep { id: Text(*id) },
        Op::UseSharedStep {
            tree,
            node,
            step,
            provenance,
        } => OpView::UseSharedStep {
            tree: Text(*tree),
            node: Text(*node),
            shared_step: Text(*step),
            provenance: ProvenanceView::of(provenance),
        },
        Op::UnuseSharedStep {
            tree,
            node,
            provenance,
        } => OpView::UnuseSharedStep {
            tree: Text(*tree),
            node: Text(*node),
            provenance: ProvenanceView::of(provenance),
        },
        Op::NeedCondition {
            element,
            condition,
            provenance,
        } => OpView::NeedCondition {
            element: (*element).into(),
            condition: Text(*condition),
            provenance: ProvenanceView::of(provenance),
        },
        Op::UnneedCondition {
            element,
            condition,
            provenance,
        } => OpView::UnneedCondition {
            element: (*element).into(),
            condition: Text(*condition),
            provenance: ProvenanceView::of(provenance),
        },
        Op::NeedStepCondition {
            tree,
            node,
            condition,
            provenance,
        } => OpView::NeedStepCondition {
            tree: Text(*tree),
            node: Text(*node),
            condition: Text(*condition),
            provenance: ProvenanceView::of(provenance),
        },
        Op::UnneedStepCondition {
            tree,
            node,
            condition,
            provenance,
        } => OpView::UnneedStepCondition {
            tree: Text(*tree),
            node: Text(*node),
            condition: Text(*condition),
            provenance: ProvenanceView::of(provenance),
        },
        Op::SetThreatGoal {
            threat,
            goal,
            provenance,
        } => OpView::SetThreatGoal {
            threat: Text(*threat),
            goal: goal.map(Text),
            provenance: ProvenanceView::of(provenance),
        },
        Op::LinkThreatGoal {
            threat,
            goal,
            provenance,
        } => OpView::LinkThreatGoal {
            threat: Text(*threat),
            goal: Text(*goal),
            provenance: ProvenanceView::of(provenance),
        },
        Op::UnlinkThreatGoal {
            threat,
            goal,
            provenance,
        } => OpView::UnlinkThreatGoal {
            threat: Text(*threat),
            goal: Text(*goal),
            provenance: ProvenanceView::of(provenance),
        },
        Op::SetThreatClaim {
            threat,
            claim,
            provenance,
        } => OpView::SetThreatClaim {
            threat: Text(*threat),
            claim: claim.map(Text),
            provenance: ProvenanceView::of(provenance),
        },
        Op::SetThreatScaling {
            threat,
            scales,
            reason,
            provenance,
        } => OpView::SetThreatScaling {
            threat: Text(*threat),
            scales: *scales,
            reason,
            provenance: ProvenanceView::of(provenance),
        },
        Op::SetImpactRationale {
            damage,
            text,
            provenance,
        } => OpView::SetImpactRationale {
            damage: Text(*damage),
            text,
            provenance: ProvenanceView::of(provenance),
        },
        Op::SetFeasibilityRationale {
            threat,
            text,
            provenance,
        } => OpView::SetFeasibilityRationale {
            threat: Text(*threat),
            text,
            provenance: ProvenanceView::of(provenance),
        },
        Op::SetDamageLinkRationale {
            threat,
            damage,
            text,
            provenance,
        } => OpView::SetDamageLinkRationale {
            threat: Text(*threat),
            damage: Text(*damage),
            text,
            provenance: ProvenanceView::of(provenance),
        },
        Op::AddComponent {
            id,
            name,
            parent,
            provenance,
        } => OpView::AddComponent {
            id: Text(*id),
            name,
            parent: parent.map(Text),
            provenance: ProvenanceView::of(provenance),
        },
        Op::RenameComponent {
            id,
            name,
            provenance,
        } => OpView::RenameComponent {
            id: Text(*id),
            name,
            provenance: ProvenanceView::of(provenance),
        },
        Op::SetComponentParent {
            id,
            parent,
            provenance,
        } => OpView::SetComponentParent {
            id: Text(*id),
            parent: parent.map(Text),
            provenance: ProvenanceView::of(provenance),
        },
        Op::SetComponentSystem {
            id,
            system,
            provenance,
        } => OpView::SetComponentSystem {
            id: Text(*id),
            system: system.map(Text),
            provenance: ProvenanceView::of(provenance),
        },
        Op::RemoveComponent { id } => OpView::RemoveComponent { id: Text(*id) },
        Op::AddInterface {
            id,
            name,
            component,
            kind,
            provenance,
        } => OpView::AddInterface {
            id: Text(*id),
            name,
            component: Text(*component),
            kind: Required(m.interface_kind_key(*kind)),
            provenance: ProvenanceView::of(provenance),
        },
        Op::RenameInterface {
            id,
            name,
            provenance,
        } => OpView::RenameInterface {
            id: Text(*id),
            name,
            provenance: ProvenanceView::of(provenance),
        },
        Op::SetInterfaceKind {
            id,
            kind,
            provenance,
        } => OpView::SetInterfaceKind {
            id: Text(*id),
            kind: Required(m.interface_kind_key(*kind)),
            provenance: ProvenanceView::of(provenance),
        },
        Op::RemoveInterface { id } => OpView::RemoveInterface { id: Text(*id) },
        Op::AddDataFlow {
            id,
            name,
            from,
            to,
            interface,
            provenance,
        } => OpView::AddDataFlow {
            id: Text(*id),
            name,
            from: Text(*from),
            to: Text(*to),
            interface: interface.map(Text),
            provenance: ProvenanceView::of(provenance),
        },
        Op::RenameDataFlow {
            id,
            name,
            provenance,
        } => OpView::RenameDataFlow {
            id: Text(*id),
            name,
            provenance: ProvenanceView::of(provenance),
        },
        Op::SetDataFlowInterface {
            id,
            interface,
            provenance,
        } => OpView::SetDataFlowInterface {
            id: Text(*id),
            interface: interface.map(Text),
            provenance: ProvenanceView::of(provenance),
        },
        Op::RemoveDataFlow { id } => OpView::RemoveDataFlow { id: Text(*id) },
        Op::AddTrustBoundary {
            id,
            name,
            components,
            provenance,
        } => OpView::AddTrustBoundary {
            id: Text(*id),
            name,
            components: Ids(components),
            provenance: ProvenanceView::of(provenance),
        },
        Op::RenameTrustBoundary {
            id,
            name,
            provenance,
        } => OpView::RenameTrustBoundary {
            id: Text(*id),
            name,
            provenance: ProvenanceView::of(provenance),
        },
        Op::AddBoundaryComponent {
            boundary,
            component,
            provenance,
        } => OpView::AddBoundaryComponent {
            boundary: Text(*boundary),
            component: Text(*component),
            provenance: ProvenanceView::of(provenance),
        },
        Op::RemoveBoundaryComponent {
            boundary,
            component,
            provenance,
        } => OpView::RemoveBoundaryComponent {
            boundary: Text(*boundary),
            component: Text(*component),
            provenance: ProvenanceView::of(provenance),
        },
        Op::RemoveTrustBoundary { id } => OpView::RemoveTrustBoundary { id: Text(*id) },
        Op::SetAssetComponent {
            asset,
            component,
            provenance,
        } => OpView::SetAssetComponent {
            asset: Text(*asset),
            component: component.map(Text),
            provenance: ProvenanceView::of(provenance),
        },
        Op::AddElement {
            id,
            kind,
            name,
            provenance,
        } => OpView::AddElement {
            id: Text(*id),
            kind: kind.key(),
            name,
            provenance: ProvenanceView::of(provenance),
        },
        Op::RenameElement {
            id,
            name,
            provenance,
        } => OpView::RenameElement {
            id: Text(*id),
            name,
            provenance: ProvenanceView::of(provenance),
        },
        Op::RemoveElement { id } => OpView::RemoveElement { id: Text(*id) },
        Op::SetElementAttribute {
            id,
            key,
            value,
            provenance,
        } => OpView::SetElementAttribute {
            id: Text(*id),
            key,
            value: value.as_ref().map(AttrView::of),
            provenance: ProvenanceView::of(provenance),
        },
        Op::LinkElement {
            id,
            target,
            provenance,
        } => OpView::LinkElement {
            id: Text(*id),
            target: LinkView::of(*target),
            provenance: ProvenanceView::of(provenance),
        },
        Op::UnlinkElement {
            id,
            target,
            provenance,
        } => OpView::UnlinkElement {
            id: Text(*id),
            target: LinkView::of(*target),
            provenance: ProvenanceView::of(provenance),
        },
    }
}

/// Encodes one record line, `\n` included, into `buf` (cleared first). On
/// error `buf` holds garbage and must not be written. Time O(k log k).
fn encode_record(
    buf: &mut Vec<u8>,
    m: &Methodology,
    order: &KeyOrder,
    seq: u64,
    op: &Op,
) -> Result<(), StoreError> {
    buf.clear();
    if !within_caps(op) {
        return Err(StoreError::UnencodableOp);
    }
    let record = RecordView {
        seq,
        op: op_view(m, order, op),
    };
    serde_json::to_writer(&mut *buf, &record).map_err(wire::encode_error)?;
    if buf.len() > MAX_OP_RECORD_BYTES {
        return Err(StoreError::UnencodableOp);
    }
    buf.push(b'\n');
    Ok(())
}

// ---------------------------------------------------------------------------
// Decoding.
// ---------------------------------------------------------------------------

fn step_target_from_wire(
    tree: Option<WireId<TreeId>>,
    node: Option<WireId<NodeId>>,
    shared_step: Option<WireId<SharedStepId>>,
    line: u64,
) -> Result<StepTarget, StoreError> {
    match (tree, node, shared_step) {
        (Some(tree), Some(node), None) => Ok(StepTarget::Step(tree.0, node.0)),
        (None, None, Some(shared_step)) => Ok(StepTarget::Shared(shared_step.0)),
        _ => Err(StoreError::Decode { line, column: 0 }),
    }
}

/// Turns a decoded op into an `Op` of `m`. Vocabulary was resolved while
/// decoding; only a `SetImpact` level is resolved here, against its
/// category. Lists keep their order. Time O(k).
#[allow(clippy::too_many_lines)]
fn op_from_wire(m: &Methodology, w: OpWire, line: u64) -> Result<Op, StoreError> {
    Ok(match w {
        OpWire::AddTreeNode {
            tree,
            parent,
            id,
            label,
            gate,
            index,
            position,
            provenance,
        } => Op::AddTreeNode {
            tree: tree.0,
            parent: parent.0,
            id: id.0,
            node: match gate {
                Some(kind) => crate::tree::Node::gate(kind.into(), label.0),
                None => crate::tree::Node::step(label.0),
            },
            index,
            position,
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::RemoveTreeNode {
            tree,
            id,
            provenance,
        } => Op::RemoveTreeNode {
            tree: tree.0,
            id: id.0,
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::LinkTreeNode {
            tree,
            parent,
            child,
            position,
            slot,
            provenance,
        } => Op::LinkTreeNode {
            tree: tree.0,
            parent: parent.0,
            child: child.0,
            position,
            slot,
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::UnlinkTreeNode {
            tree,
            parent,
            child,
            provenance,
        } => Op::UnlinkTreeNode {
            tree: tree.0,
            parent: parent.0,
            child: child.0,
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::RenameTreeNode {
            tree,
            id,
            label,
            provenance,
        } => Op::RenameTreeNode {
            tree: tree.0,
            id: id.0,
            label: label.0,
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::SetTreeNodeGate {
            tree,
            id,
            gate,
            provenance,
        } => Op::SetTreeNodeGate {
            tree: tree.0,
            id: id.0,
            gate: gate.into(),
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::SetReviewMark {
            tree,
            node,
            marked,
            provenance,
        } => Op::SetReviewMark {
            tree: tree.0,
            node: node.0,
            marked,
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::SetDescription {
            target,
            text,
            provenance,
        } => Op::SetDescription {
            target: target.into_ref(),
            text: text.0,
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::SetDescriptionTranslation {
            target,
            lang,
            text,
            provenance,
        } => Op::SetDescriptionTranslation {
            target: target.into_ref(),
            lang: lang.0,
            text: text.0,
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::SetProjectLanguages { langs, provenance } => Op::SetProjectLanguages {
            langs: langs.into_iter().map(|l| l.0).collect(),
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::SetAttackFilter { filter, provenance } => Op::SetAttackFilter {
            filter: filter.0,
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::AddAsset {
            id,
            name,
            properties,
            provenance,
        } => Op::AddAsset {
            id: id.0,
            name: name.0,
            properties: properties.0,
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::RenameAsset {
            id,
            name,
            provenance,
        } => Op::RenameAsset {
            id: id.0,
            name: name.0,
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::SetAssetProperties {
            id,
            properties,
            provenance,
        } => Op::SetAssetProperties {
            id: id.0,
            properties: properties.0,
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::RemoveAsset { id } => Op::RemoveAsset { id: id.0 },
        OpWire::AddDamageScenario {
            id,
            name,
            impacts,
            provenance,
        } => Op::AddDamageScenario {
            id: id.0,
            name: name.0,
            impacts: impacts.0,
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::RenameDamageScenario {
            id,
            name,
            provenance,
        } => Op::RenameDamageScenario {
            id: id.0,
            name: name.0,
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::SetImpact {
            id,
            category,
            level,
            provenance,
        } => Op::SetImpact {
            id: id.0,
            category: category.0,
            level: level
                .as_ref()
                .map(|l| wire::resolve_level(m, category.0, l))
                .transpose()?,
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::RemoveDamageScenario { id } => Op::RemoveDamageScenario { id: id.0 },
        OpWire::AddThreatScenario {
            id,
            name,
            asset,
            category,
            catalog_refs,
            damages,
            provenance,
        } => Op::AddThreatScenario {
            id: id.0,
            name: name.0,
            asset: asset.0,
            category: category.0,
            catalog_refs: catalog_refs.0.into_iter().map(|r| r.0).collect(),
            damages: damages.0.into_iter().map(|d| d.0).collect(),
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::RenameThreatScenario {
            id,
            name,
            provenance,
        } => Op::RenameThreatScenario {
            id: id.0,
            name: name.0,
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::SetThreatCategory {
            id,
            category,
            provenance,
        } => Op::SetThreatCategory {
            id: id.0,
            category: category.0,
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::AddCatalogRef {
            threat,
            catalog_ref,
            provenance,
        } => Op::AddCatalogRef {
            threat: threat.0,
            catalog_ref: catalog_ref.0,
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::RemoveCatalogRef {
            threat,
            catalog_ref,
            provenance,
        } => Op::RemoveCatalogRef {
            threat: threat.0,
            catalog_ref: catalog_ref.0,
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::LinkDamage {
            threat,
            damage,
            provenance,
        } => Op::LinkDamage {
            threat: threat.0,
            damage: damage.0,
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::UnlinkDamage {
            threat,
            damage,
            provenance,
        } => Op::UnlinkDamage {
            threat: threat.0,
            damage: damage.0,
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::RemoveThreatScenario { id } => Op::RemoveThreatScenario { id: id.0 },
        OpWire::AddAttackTree {
            role,
            id,
            nodes,
            step_feasibility,
            provenance,
        } => Op::AddAttackTree {
            role,
            id: id.0,
            tree: nodes.0,
            step_feasibility: step_feasibility.0,
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::SetStepFeasibility {
            tree,
            node,
            shared_step,
            level,
            provenance,
        } => Op::SetStepFeasibility {
            target: step_target_from_wire(tree, node, shared_step, line)?,
            level: level.map(|l| l.0),
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::SetStepVia {
            tree,
            node,
            shared_step,
            via,
            provenance,
        } => Op::SetStepVia {
            target: step_target_from_wire(tree, node, shared_step, line)?,
            via: via.map(wire::ItemWire::unwrap_item),
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::SetStepAttackType {
            tree,
            node,
            shared_step,
            attack_type,
            provenance,
        } => Op::SetStepAttackType {
            target: step_target_from_wire(tree, node, shared_step, line)?,
            attack_type: attack_type.map(|t| t.0),
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::SetStepFactor {
            tree,
            node,
            shared_step,
            factor,
            level,
            provenance,
        } => Op::SetStepFactor {
            target: step_target_from_wire(tree, node, shared_step, line)?,
            factor: factor.0,
            level: level
                .as_ref()
                .map(|l| wire::resolve_factor_level(m, factor.0, l))
                .transpose()?,
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::SetThreatFeasibility {
            threat,
            level,
            provenance,
        } => Op::SetThreatFeasibility {
            threat: threat.0,
            level: level.map(|l| l.0),
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::SetThreatAttackTree {
            threat,
            tree,
            provenance,
        } => Op::SetThreatAttackTree {
            threat: threat.0,
            tree: tree.map(|t| t.0),
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::RemoveAttackTree { id } => Op::RemoveAttackTree { id: id.0 },
        OpWire::SetRiskTreatment {
            threat,
            treatment,
            provenance,
        } => Op::SetRiskTreatment {
            threat: threat.0,
            treatment: treatment.map(Into::into),
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::AddSecurityControl {
            id,
            name,
            provenance,
        } => Op::AddSecurityControl {
            id: id.0,
            name: name.0,
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::RenameSecurityControl {
            id,
            name,
            provenance,
        } => Op::RenameSecurityControl {
            id: id.0,
            name: name.0,
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::RemoveSecurityControl { id } => Op::RemoveSecurityControl { id: id.0 },
        OpWire::ApplyControl {
            control,
            target,
            provenance,
        } => Op::ApplyControl {
            control: control.0,
            target: target.unwrap_item(),
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::UnapplyControl {
            control,
            target,
            provenance,
        } => Op::UnapplyControl {
            control: control.0,
            target: target.unwrap_item(),
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::SetControlRule {
            control,
            attack_type,
            ceiling,
            provenance,
        } => Op::SetControlRule {
            control: control.0,
            attack_type: attack_type.0,
            ceiling: ceiling.map(|l| l.0),
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::SetControlKind {
            control,
            kind,
            provenance,
        } => Op::SetControlKind {
            control: control.0,
            kind: kind.map(|k| k.0),
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::SetControlStatus {
            control,
            status,
            provenance,
        } => Op::SetControlStatus {
            control: control.0,
            status: status.0,
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::SetStepMitigation {
            tree,
            node,
            control,
            feasibility,
            provenance,
        } => Op::SetStepMitigation {
            tree: tree.0,
            node: node.0,
            control: control.0,
            feasibility: feasibility.map(|l| l.0),
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::AddCybersecurityGoal {
            id,
            name,
            controls,
            provenance,
        } => Op::AddCybersecurityGoal {
            id: id.0,
            name: name.0,
            controls: controls.0.into_iter().map(|c| c.0).collect(),
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::RenameCybersecurityGoal {
            id,
            name,
            provenance,
        } => Op::RenameCybersecurityGoal {
            id: id.0,
            name: name.0,
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::LinkGoalControl {
            goal,
            control,
            provenance,
        } => Op::LinkGoalControl {
            goal: goal.0,
            control: control.0,
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::UnlinkGoalControl {
            goal,
            control,
            provenance,
        } => Op::UnlinkGoalControl {
            goal: goal.0,
            control: control.0,
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::LinkGoalParty {
            goal,
            party,
            provenance,
        } => Op::LinkGoalParty {
            goal: goal.0,
            party: party.0,
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::UnlinkGoalParty {
            goal,
            party,
            provenance,
        } => Op::UnlinkGoalParty {
            goal: goal.0,
            party: party.0,
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::RemoveCybersecurityGoal { id } => Op::RemoveCybersecurityGoal { id: id.0 },
        OpWire::AddCybersecurityClaim {
            id,
            name,
            provenance,
        } => Op::AddCybersecurityClaim {
            id: id.0,
            name: name.0,
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::RenameCybersecurityClaim {
            id,
            name,
            provenance,
        } => Op::RenameCybersecurityClaim {
            id: id.0,
            name: name.0,
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::LinkClaimParty {
            claim,
            party,
            provenance,
        } => Op::LinkClaimParty {
            claim: claim.0,
            party: party.0,
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::UnlinkClaimParty {
            claim,
            party,
            provenance,
        } => Op::UnlinkClaimParty {
            claim: claim.0,
            party: party.0,
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::LinkClaimAssumption {
            claim,
            assumption,
            provenance,
        } => Op::LinkClaimAssumption {
            claim: claim.0,
            assumption: assumption.0,
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::UnlinkClaimAssumption {
            claim,
            assumption,
            provenance,
        } => Op::UnlinkClaimAssumption {
            claim: claim.0,
            assumption: assumption.0,
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::RemoveCybersecurityClaim { id } => Op::RemoveCybersecurityClaim { id: id.0 },
        OpWire::AddParty {
            id,
            name,
            kind,
            provenance,
        } => Op::AddParty {
            id: id.0,
            name: name.0,
            kind,
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::RenameParty {
            id,
            name,
            provenance,
        } => Op::RenameParty {
            id: id.0,
            name: name.0,
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::SetPartyKind {
            id,
            kind,
            provenance,
        } => Op::SetPartyKind {
            id: id.0,
            kind,
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::RemoveParty { id } => Op::RemoveParty { id: id.0 },
        OpWire::AddCondition {
            id,
            name,
            description,
            present,
            provenance,
        } => Op::AddCondition {
            id: id.0,
            name: name.0,
            description: description.0,
            present,
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::RenameCondition {
            id,
            name,
            provenance,
        } => Op::RenameCondition {
            id: id.0,
            name: name.0,
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::SetConditionPresent {
            id,
            present,
            provenance,
        } => Op::SetConditionPresent {
            id: id.0,
            present,
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::RemoveCondition { id } => Op::RemoveCondition { id: id.0 },
        OpWire::AddAssumption {
            id,
            name,
            holds,
            provenance,
        } => Op::AddAssumption {
            id: id.0,
            name: name.0,
            holds,
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::RenameAssumption {
            id,
            name,
            provenance,
        } => Op::RenameAssumption {
            id: id.0,
            name: name.0,
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::SetAssumptionHolds {
            id,
            holds,
            provenance,
        } => Op::SetAssumptionHolds {
            id: id.0,
            holds,
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::SetAssumptionExcludes {
            id,
            trees,
            steps,
            provenance,
        } => {
            if trees.0.len().saturating_add(steps.0.len()) > MAX_ASSUMPTION_EXCLUDES {
                return Err(StoreError::LimitExceeded(Limit::ASSUMPTION_EXCLUDES));
            }
            Op::SetAssumptionExcludes {
                id: id.0,
                trees: trees.0.into_iter().map(|t| t.0).collect(),
                steps: steps
                    .0
                    .into_iter()
                    .map(|s| StepRef {
                        tree: s.tree.0,
                        node: s.node.0,
                    })
                    .collect(),
                provenance: wire::unwrap_provenance(provenance),
            }
        }
        OpWire::RemoveAssumption { id } => Op::RemoveAssumption { id: id.0 },
        OpWire::AddSharedStep {
            id,
            name,
            provenance,
        } => Op::AddSharedStep {
            id: id.0,
            name: name.0,
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::RenameSharedStep {
            id,
            name,
            provenance,
        } => Op::RenameSharedStep {
            id: id.0,
            name: name.0,
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::RemoveSharedStep { id } => Op::RemoveSharedStep { id: id.0 },
        OpWire::UseSharedStep {
            tree,
            node,
            shared_step,
            provenance,
        } => Op::UseSharedStep {
            tree: tree.0,
            node: node.0,
            step: shared_step.0,
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::UnuseSharedStep {
            tree,
            node,
            provenance,
        } => Op::UnuseSharedStep {
            tree: tree.0,
            node: node.0,
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::NeedCondition {
            element,
            condition,
            provenance,
        } => Op::NeedCondition {
            element: element.into_ref(),
            condition: condition.0,
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::UnneedCondition {
            element,
            condition,
            provenance,
        } => Op::UnneedCondition {
            element: element.into_ref(),
            condition: condition.0,
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::NeedStepCondition {
            tree,
            node,
            condition,
            provenance,
        } => Op::NeedStepCondition {
            tree: tree.0,
            node: node.0,
            condition: condition.0,
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::UnneedStepCondition {
            tree,
            node,
            condition,
            provenance,
        } => Op::UnneedStepCondition {
            tree: tree.0,
            node: node.0,
            condition: condition.0,
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::SetThreatGoal {
            threat,
            goal,
            provenance,
        } => Op::SetThreatGoal {
            threat: threat.0,
            goal: goal.map(|g| g.0),
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::LinkThreatGoal {
            threat,
            goal,
            provenance,
        } => Op::LinkThreatGoal {
            threat: threat.0,
            goal: goal.0,
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::UnlinkThreatGoal {
            threat,
            goal,
            provenance,
        } => Op::UnlinkThreatGoal {
            threat: threat.0,
            goal: goal.0,
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::SetThreatClaim {
            threat,
            claim,
            provenance,
        } => Op::SetThreatClaim {
            threat: threat.0,
            claim: claim.map(|cl| cl.0),
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::SetThreatScaling {
            threat,
            scales,
            reason,
            provenance,
        } => Op::SetThreatScaling {
            threat: threat.0,
            scales,
            reason: reason.0,
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::SetImpactRationale {
            damage,
            text,
            provenance,
        } => Op::SetImpactRationale {
            damage: damage.0,
            text: text.0,
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::SetDamageLinkRationale {
            threat,
            damage,
            text,
            provenance,
        } => Op::SetDamageLinkRationale {
            threat: threat.0,
            damage: damage.0,
            text: text.0,
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::SetFeasibilityRationale {
            threat,
            text,
            provenance,
        } => Op::SetFeasibilityRationale {
            threat: threat.0,
            text: text.0,
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::AddComponent {
            id,
            name,
            parent,
            provenance,
        } => Op::AddComponent {
            id: id.0,
            name: name.0,
            parent: parent.map(|p| p.0),
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::RenameComponent {
            id,
            name,
            provenance,
        } => Op::RenameComponent {
            id: id.0,
            name: name.0,
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::SetComponentParent {
            id,
            parent,
            provenance,
        } => Op::SetComponentParent {
            id: id.0,
            parent: parent.map(|p| p.0),
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::SetComponentSystem {
            id,
            system,
            provenance,
        } => Op::SetComponentSystem {
            id: id.0,
            system: system.map(|s| s.0),
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::RemoveComponent { id } => Op::RemoveComponent { id: id.0 },
        OpWire::AddInterface {
            id,
            name,
            component,
            kind,
            provenance,
        } => Op::AddInterface {
            id: id.0,
            name: name.0,
            component: component.0,
            kind: kind.0,
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::RenameInterface {
            id,
            name,
            provenance,
        } => Op::RenameInterface {
            id: id.0,
            name: name.0,
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::SetInterfaceKind {
            id,
            kind,
            provenance,
        } => Op::SetInterfaceKind {
            id: id.0,
            kind: kind.0,
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::RemoveInterface { id } => Op::RemoveInterface { id: id.0 },
        OpWire::AddDataFlow {
            id,
            name,
            from,
            to,
            interface,
            provenance,
        } => Op::AddDataFlow {
            id: id.0,
            name: name.0,
            from: from.0,
            to: to.0,
            interface: interface.map(|i| i.0),
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::RenameDataFlow {
            id,
            name,
            provenance,
        } => Op::RenameDataFlow {
            id: id.0,
            name: name.0,
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::SetDataFlowInterface {
            id,
            interface,
            provenance,
        } => Op::SetDataFlowInterface {
            id: id.0,
            interface: interface.map(|i| i.0),
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::RemoveDataFlow { id } => Op::RemoveDataFlow { id: id.0 },
        OpWire::AddTrustBoundary {
            id,
            name,
            components,
            provenance,
        } => Op::AddTrustBoundary {
            id: id.0,
            name: name.0,
            components: components.0.into_iter().map(|c| c.0).collect(),
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::RenameTrustBoundary {
            id,
            name,
            provenance,
        } => Op::RenameTrustBoundary {
            id: id.0,
            name: name.0,
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::AddBoundaryComponent {
            boundary,
            component,
            provenance,
        } => Op::AddBoundaryComponent {
            boundary: boundary.0,
            component: component.0,
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::RemoveBoundaryComponent {
            boundary,
            component,
            provenance,
        } => Op::RemoveBoundaryComponent {
            boundary: boundary.0,
            component: component.0,
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::RemoveTrustBoundary { id } => Op::RemoveTrustBoundary { id: id.0 },
        OpWire::SetAssetComponent {
            asset,
            component,
            provenance,
        } => Op::SetAssetComponent {
            asset: asset.0,
            component: component.map(|c| c.0),
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::AddElement {
            id,
            kind,
            name,
            provenance,
        } => Op::AddElement {
            id: id.0,
            kind: kind.0,
            name: name.0,
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::RenameElement {
            id,
            name,
            provenance,
        } => Op::RenameElement {
            id: id.0,
            name: name.0,
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::RemoveElement { id } => Op::RemoveElement { id: id.0 },
        OpWire::SetElementAttribute {
            id,
            key,
            value,
            provenance,
        } => Op::SetElementAttribute {
            id: id.0,
            key: key.0,
            value: value.map(wire::AttrWire::into_value),
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::LinkElement {
            id,
            target,
            provenance,
        } => Op::LinkElement {
            id: id.0,
            target: target.into_target(),
            provenance: wire::unwrap_provenance(provenance),
        },
        OpWire::UnlinkElement {
            id,
            target,
            provenance,
        } => Op::UnlinkElement {
            id: id.0,
            target: target.into_target(),
            provenance: wire::unwrap_provenance(provenance),
        },
    })
}

/// Reads one line, without its `\n`, into `buf` (cleared first). Returns
/// `Ok(false)` at a clean end of input. Never holds more than
/// [`MAX_OP_RECORD_BYTES`] bytes. Time O(line bytes).
///
/// # Errors
///
/// [`StoreError::TruncatedRecord`] if the input ends inside the line,
/// [`StoreError::LimitExceeded`] if the line is too long, or I/O errors.
fn read_line<R: BufRead>(r: &mut R, buf: &mut Vec<u8>, line: u64) -> Result<bool, StoreError> {
    buf.clear();
    loop {
        let available = match r.fill_buf() {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == ErrorKind::Interrupted => continue,
            Err(e) => return Err(e.into()),
        };
        if available.is_empty() {
            return if buf.is_empty() {
                Ok(false)
            } else {
                Err(StoreError::TruncatedRecord { line })
            };
        }
        let newline = available.iter().position(|&b| b == b'\n');
        let chunk = match newline {
            Some(i) => available.get(..i).unwrap_or(available),
            None => available,
        };
        if buf.len().saturating_add(chunk.len()) > MAX_OP_RECORD_BYTES {
            return Err(StoreError::LimitExceeded(Limit::OP_RECORD));
        }
        buf.extend_from_slice(chunk);
        let used = chunk.len() + usize::from(newline.is_some());
        r.consume(used);
        if newline.is_some() {
            return Ok(true);
        }
    }
}

/// Rejects a `\r` anywhere in a line: lines end in `\n` only, and a raw
/// `\r` is never valid inside a JSON string. Time O(line bytes).
fn reject_cr(buf: &[u8], line: u64) -> Result<(), StoreError> {
    match buf.iter().position(|&b| b == b'\r') {
        Some(pos) => Err(StoreError::Decode {
            line,
            column: wire::to_u64(pos).saturating_add(1),
        }),
        None => Ok(()),
    }
}

// ---------------------------------------------------------------------------
// Writer.
// ---------------------------------------------------------------------------

/// Appends ops to an `aegis-oplog` v1 log.
///
/// The writer only appends: it has no API that rewrites or truncates.
/// Each record is encoded into a reused internal buffer first and written
/// with one `write_all` only when encoding succeeded, so a refused op
/// writes nothing.
///
/// Keeping a project and its log in step is the caller's job: apply the op
/// to the project, then [`append`](Self::append) it; if the append fails,
/// apply the inverse to undo it.
///
/// ```
/// use std::sync::Arc;
/// use aegis_model::{AssetId, Methodology, Op, OpLogReader, OpLogWriter, Project, Provenance};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let m = Arc::new(Methodology::iso21434_r155()?);
/// let mut project = Project::new(Arc::clone(&m));
/// let mut log = OpLogWriter::create(Vec::new(), Arc::clone(&m))?;
///
/// let op = Op::AddAsset {
///     id: AssetId::new_v7(),
///     name: "ECU".into(),
///     properties: m.all_properties(),
///     provenance: Provenance::default(),
/// };
/// let undo = project.apply(op.clone())?;
/// if let Err(e) = log.append(&op) {
///     project.apply(undo)?;
///     return Err(e.into());
/// }
/// let bytes = log.into_inner()?;
///
/// // Replaying the log onto an empty project rebuilds it.
/// let mut replayed = Project::new(Arc::clone(&m));
/// for record in OpLogReader::open(bytes.as_slice(), m)? {
///     replayed.apply(record?.op)?;
/// }
/// assert_eq!(replayed, project);
/// # Ok(())
/// # }
/// ```
pub struct OpLogWriter<W: Write> {
    w: W,
    m: Arc<Methodology>,
    order: KeyOrder,
    last_seq: u64,
    buf: Vec<u8>,
}

impl<W: Write> OpLogWriter<W> {
    /// Starts a new log on `w`: writes the header line with the
    /// methodology's key and version. The first [`append`](Self::append)
    /// gets `seq` 1.
    ///
    /// Time O(m log m) for the key order, m = methodology size.
    ///
    /// # Errors
    ///
    /// [`StoreError::Io`] if writing the header fails.
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{Methodology, OpLogWriter};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let bytes = OpLogWriter::create(Vec::new(), m)?.into_inner()?;
    /// assert_eq!(
    ///     String::from_utf8(bytes)?,
    ///     "{\"format\":\"aegis-oplog\",\"version\":1,\
    ///      \"methodology\":{\"key\":\"iso21434-r155\",\"version\":\"0.1.0\"}}\n",
    /// );
    /// # Ok(())
    /// # }
    /// ```
    pub fn create(w: W, m: Arc<Methodology>) -> Result<Self, StoreError> {
        let mut writer = Self::resume(w, m, 0);
        let header = HeaderView {
            format: "aegis-oplog",
            version: FORMAT_VERSION,
            methodology: MethodologyView::of(&writer.m),
        };
        serde_json::to_writer(&mut writer.buf, &header).map_err(wire::encode_error)?;
        writer.buf.push(b'\n');
        writer.w.write_all(&writer.buf)?;
        Ok(writer)
    }

    /// Continues an existing log on `w`, positioned after its last record
    /// (for example a file opened for appending). `last_seq` is the
    /// sequence number of that record, as reported by
    /// [`OpLogReader::last_seq`]; the next [`append`](Self::append) gets
    /// `last_seq + 1`. Writes nothing.
    ///
    /// Time O(m log m) for the key order.
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{AssetId, Methodology, OpLogWriter, Op};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let mut log = OpLogWriter::resume(Vec::new(), m, 41);
    /// assert_eq!(log.append(&Op::RemoveAsset { id: AssetId::new_v7() })?, 42);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn resume(w: W, m: Arc<Methodology>, last_seq: u64) -> Self {
        let order = KeyOrder::new(&m);
        OpLogWriter {
            w,
            m,
            order,
            last_seq,
            buf: Vec::new(),
        }
    }

    /// Appends `op` as the next record and returns its `seq`.
    ///
    /// The op is not checked against a project; log ops after
    /// [`Project::apply`](crate::Project::apply) accepted them. The record
    /// is encoded first and written with one `write_all`; nothing is
    /// written if encoding fails. Writes are not flushed; call
    /// [`into_inner`](Self::into_inner) (or flush the underlying writer)
    /// for durability. After an I/O error the log may end in a partial
    /// line, which a reader reports as [`StoreError::TruncatedRecord`];
    /// stop appending to it.
    ///
    /// Time O(k log k), extra space O(k) in a reused buffer, k = size of
    /// the op.
    ///
    /// # Errors
    ///
    /// [`StoreError::UnencodableOp`] if an index in the op is not in the
    /// writer's methodology, an impact category is repeated, a name or list
    /// is over its decode cap, the record would exceed
    /// [`MAX_OP_RECORD_BYTES`](crate::MAX_OP_RECORD_BYTES), or `seq` would
    /// overflow; [`StoreError::Io`] if writing fails.
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{AssetId, Methodology, Op, OpLogWriter};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let mut log = OpLogWriter::create(Vec::new(), m)?;
    /// let id = AssetId::new_v7();
    /// assert_eq!(log.append(&Op::RemoveAsset { id })?, 1);
    /// assert_eq!(log.append(&Op::RemoveAsset { id })?, 2);
    /// assert_eq!(log.last_seq(), 2);
    /// # Ok(())
    /// # }
    /// ```
    pub fn append(&mut self, op: &Op) -> Result<u64, StoreError> {
        let seq = self
            .last_seq
            .checked_add(1)
            .ok_or(StoreError::UnencodableOp)?;
        encode_record(&mut self.buf, &self.m, &self.order, seq, op)?;
        self.w.write_all(&self.buf)?;
        self.last_seq = seq;
        Ok(seq)
    }

    /// Returns the `seq` of the last record written, or the `last_seq`
    /// given to [`resume`](Self::resume) (0 for a new log). Time O(1).
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{Methodology, OpLogWriter};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let log = OpLogWriter::create(Vec::new(), Arc::new(Methodology::iso21434_r155()?))?;
    /// assert_eq!(log.last_seq(), 0);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn last_seq(&self) -> u64 {
        self.last_seq
    }

    /// Flushes and returns the underlying writer. Time O(1) besides the
    /// flush.
    ///
    /// # Errors
    ///
    /// [`StoreError::Io`] if the flush fails.
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{Methodology, OpLogWriter};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let log = OpLogWriter::create(Vec::new(), Arc::new(Methodology::iso21434_r155()?))?;
    /// let bytes = log.into_inner()?;
    /// assert_eq!(bytes.iter().filter(|&&b| b == b'\n').count(), 1);
    /// # Ok(())
    /// # }
    /// ```
    pub fn into_inner(mut self) -> Result<W, StoreError> {
        self.w.flush()?;
        Ok(self.w)
    }
}

// ---------------------------------------------------------------------------
// Single-op codec.
// ---------------------------------------------------------------------------

/// Encodes and decodes one [`Op`] at a time, in the same JSON shape as the
/// `op` member of an op log record: `{"op":"add_asset","id":...}`.
///
/// This is the wire form for ops outside a log file: a client submitting an
/// op to the server, or a database row holding one. It shares the encoder
/// and the strict, capped decoder of [`OpLogWriter`] and [`OpLogReader`], so
/// there is one definition of what a valid op looks like (R3: caps before
/// allocation, vocabulary keys resolved while decoding). The bytes carry no
/// `seq` and no trailing newline.
///
/// Build one per methodology and reuse it: [`new`](Self::new) sorts the
/// methodology's keys once.
///
/// ```
/// use std::sync::Arc;
/// use aegis_model::{AssetId, Methodology, Op, OpCodec};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let codec = OpCodec::new(Arc::new(Methodology::iso21434_r155()?));
/// let op = Op::RemoveAsset { id: AssetId::new_v7() };
/// let bytes = codec.encode(&op)?;
/// assert_eq!(codec.decode(&bytes)?, op);
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct OpCodec {
    m: Arc<Methodology>,
    order: KeyOrder,
}

impl OpCodec {
    /// Builds a codec for `m`. Time O(m log m), m = methodology size.
    #[must_use]
    pub fn new(m: Arc<Methodology>) -> Self {
        let order = KeyOrder::new(&m);
        OpCodec { m, order }
    }

    /// The methodology this codec resolves keys against. Time O(1).
    #[must_use]
    pub fn methodology(&self) -> &Arc<Methodology> {
        &self.m
    }

    /// Encodes `op` as canonical JSON: the same op always gives the same
    /// bytes (P4). Time O(k log k), space O(k), k = size of the op.
    ///
    /// # Errors
    ///
    /// [`StoreError::UnencodableOp`] if an index in the op is not in this
    /// codec's methodology, an impact category is repeated, a name or list
    /// is over its decode cap, or the encoding would exceed
    /// [`MAX_OP_RECORD_BYTES`](crate::MAX_OP_RECORD_BYTES).
    pub fn encode(&self, op: &Op) -> Result<Vec<u8>, StoreError> {
        if !within_caps(op) {
            return Err(StoreError::UnencodableOp);
        }
        let mut buf = Vec::new();
        serde_json::to_writer(&mut buf, &op_view(&self.m, &self.order, op))
            .map_err(wire::encode_error)?;
        if buf.len() > MAX_OP_RECORD_BYTES {
            return Err(StoreError::UnencodableOp);
        }
        Ok(buf)
    }

    /// Decodes one op. Decoding is strict (unknown, missing, and repeated
    /// fields, and trailing data, are errors) and capped while it runs.
    /// The op is not checked against a project; pass it to
    /// [`Project::validate`](crate::Project::validate) or
    /// [`Project::apply`](crate::Project::apply). Time O(bytes + k log k).
    ///
    /// # Errors
    ///
    /// [`StoreError::LimitExceeded`] if `bytes` is longer than
    /// [`MAX_OP_RECORD_BYTES`](crate::MAX_OP_RECORD_BYTES) or a field is
    /// over its cap; [`StoreError::Decode`] (position only, never input
    /// text) for malformed JSON or unknown fields; the vocabulary errors of
    /// [`StoreError`] for unknown keys.
    pub fn decode(&self, bytes: &[u8]) -> Result<Op, StoreError> {
        if bytes.len() > MAX_OP_RECORD_BYTES {
            return Err(StoreError::LimitExceeded(Limit::OP_RECORD));
        }
        let wire: OpWire = wire::from_slice(&self.m, bytes, Some(1))?;
        op_from_wire(&self.m, wire, 1)
    }
}

// ---------------------------------------------------------------------------
// Reader.
// ---------------------------------------------------------------------------

/// Reads an `aegis-oplog` v1 log: the header on [`open`](Self::open), then
/// one [`LoggedOp`] per line as an iterator.
///
/// Each line is read through a reused buffer of at most
/// [`MAX_OP_RECORD_BYTES`](crate::MAX_OP_RECORD_BYTES), so memory stays
/// bounded whatever the input. Records must count 1, 2, 3, ... The
/// iterator is fused after the first error: it yields that error once and
/// then `None`.
///
/// ```
/// use std::sync::Arc;
/// use aegis_model::{DamageId, Methodology, Op, OpLogReader, OpLogWriter, Provenance};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let m = Arc::new(Methodology::iso21434_r155()?);
/// let mut log = OpLogWriter::create(Vec::new(), Arc::clone(&m))?;
/// let op = Op::AddDamageScenario {
///     id: DamageId::new_v7(),
///     name: "Leak".into(),
///     impacts: vec![],
///     provenance: Provenance::default(),
/// };
/// log.append(&op)?;
/// let bytes = log.into_inner()?;
///
/// let mut reader = OpLogReader::open(bytes.as_slice(), m)?;
/// let record = reader.next().ok_or("empty")??;
/// assert_eq!((record.seq, record.op), (1, op));
/// assert!(reader.next().is_none());
/// # Ok(())
/// # }
/// ```
pub struct OpLogReader<R: BufRead> {
    r: R,
    m: Arc<Methodology>,
    version: Arc<str>,
    last_seq: u64,
    line: u64,
    buf: Vec<u8>,
    done: bool,
}

impl<R: BufRead> OpLogReader<R> {
    /// Reads and checks the header line: the format must be `aegis-oplog`
    /// version 1 and the methodology key must equal `m.key()`. A different
    /// methodology version is accepted; see
    /// [`methodology_version`](Self::methodology_version).
    ///
    /// Time O(header bytes + m) at most; space O(64 KiB) for the line
    /// buffer.
    ///
    /// # Errors
    ///
    /// [`StoreError::MissingHeader`] for empty input;
    /// [`StoreError::TruncatedRecord`] if the header has no `\n`;
    /// [`StoreError::LimitExceeded`], [`StoreError::Decode`],
    /// [`StoreError::UnsupportedFormat`],
    /// [`StoreError::MethodologyMismatch`], or [`StoreError::Io`].
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{Methodology, OpLogReader, StoreError};
    ///
    /// # fn main() -> Result<(), aegis_model::PackError> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let err = OpLogReader::open(&b""[..], m).err();
    /// assert!(matches!(err, Some(StoreError::MissingHeader)));
    /// # Ok(())
    /// # }
    /// ```
    pub fn open(mut r: R, m: Arc<Methodology>) -> Result<Self, StoreError> {
        let mut buf = Vec::new();
        if !read_line(&mut r, &mut buf, 1)? {
            return Err(StoreError::MissingHeader);
        }
        reject_cr(&buf, 1)?;
        let header: HeaderWire = wire::from_slice(&m, &buf, Some(1))?;
        Ok(OpLogReader {
            r,
            m,
            version: header.methodology.version,
            last_seq: 0,
            line: 1,
            buf,
            done: false,
        })
    }

    /// Returns the methodology version in the header, which may differ
    /// from the supplied methodology's version. Time O(1).
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{Methodology, OpLogReader, OpLogWriter};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let bytes = OpLogWriter::create(Vec::new(), Arc::clone(&m))?.into_inner()?;
    /// let reader = OpLogReader::open(bytes.as_slice(), m)?;
    /// assert_eq!(reader.methodology_version(), "0.1.0");
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn methodology_version(&self) -> &str {
        &self.version
    }

    /// Returns the `seq` of the last record read successfully, 0 before
    /// the first. Pass it to [`OpLogWriter::resume`] to continue the log.
    /// Time O(1).
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{AssetId, Methodology, Op, OpLogReader, OpLogWriter};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let mut log = OpLogWriter::create(Vec::new(), Arc::clone(&m))?;
    /// log.append(&Op::RemoveAsset { id: AssetId::new_v7() })?;
    /// let bytes = log.into_inner()?;
    /// let mut reader = OpLogReader::open(bytes.as_slice(), m)?;
    /// assert_eq!(reader.last_seq(), 0);
    /// reader.by_ref().count();
    /// assert_eq!(reader.last_seq(), 1);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn last_seq(&self) -> u64 {
        self.last_seq
    }

    /// Returns the 1-based line of the last line read: 1 after
    /// [`open`](Self::open) (the header), 2 after the first record. Time
    /// O(1).
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{Methodology, OpLogReader, OpLogWriter};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let bytes = OpLogWriter::create(Vec::new(), Arc::clone(&m))?.into_inner()?;
    /// assert_eq!(OpLogReader::open(bytes.as_slice(), m)?.line(), 1);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn line(&self) -> u64 {
        self.line
    }

    /// Reads, decodes, and checks the next record. Time O(record bytes +
    /// k log k).
    fn read_record(&mut self) -> Result<Option<LoggedOp>, StoreError> {
        let line = self.line.saturating_add(1);
        if !read_line(&mut self.r, &mut self.buf, line)? {
            return Ok(None);
        }
        self.line = line;
        reject_cr(&self.buf, line)?;
        let record: RecordWire = wire::from_slice(&self.m, &self.buf, Some(line))?;
        let expected = self.last_seq.saturating_add(1);
        if record.seq != expected || self.last_seq == u64::MAX {
            return Err(StoreError::Sequence {
                expected,
                found: record.seq,
            });
        }
        let op = op_from_wire(&self.m, record.op, line)?;
        self.last_seq = record.seq;
        Ok(Some(LoggedOp {
            seq: record.seq,
            op,
        }))
    }
}

impl<R: BufRead> Iterator for OpLogReader<R> {
    type Item = Result<LoggedOp, StoreError>;

    /// Yields the next record, `None` at the end of the log, or one error
    /// and then `None` forever. Time O(record bytes + k log k) per record.
    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        match self.read_record() {
            Ok(Some(record)) => Some(Ok(record)),
            Ok(None) => {
                self.done = true;
                None
            }
            Err(e) => {
                self.done = true;
                Some(Err(e))
            }
        }
    }
}

impl<R: BufRead> std::iter::FusedIterator for OpLogReader<R> {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provenance::Provenance;
    use crate::testkit;

    #[test]
    fn read_line_frames_lines() {
        let mut r: &[u8] = b"ab\n\ncd";
        let mut buf = Vec::new();
        assert!(matches!(read_line(&mut r, &mut buf, 1), Ok(true)));
        assert_eq!(buf, b"ab");
        assert!(matches!(read_line(&mut r, &mut buf, 2), Ok(true)));
        assert_eq!(buf, b"");
        assert!(matches!(
            read_line(&mut r, &mut buf, 3),
            Err(StoreError::TruncatedRecord { line: 3 })
        ));
        let mut empty: &[u8] = b"";
        assert!(matches!(read_line(&mut empty, &mut buf, 1), Ok(false)));
    }

    #[test]
    fn read_line_caps_across_buffer_refills() {
        let long = vec![b'x'; MAX_OP_RECORD_BYTES + 1];
        // A 7-byte BufReader forces many partial chunks.
        let mut r = std::io::BufReader::with_capacity(7, long.as_slice());
        let mut buf = Vec::new();
        assert!(matches!(
            read_line(&mut r, &mut buf, 1),
            Err(StoreError::LimitExceeded(Limit::OpRecordBytes { .. }))
        ));
        assert!(buf.len() <= MAX_OP_RECORD_BYTES);
        let mut exact = vec![b'x'; MAX_OP_RECORD_BYTES];
        exact.push(b'\n');
        let mut r = std::io::BufReader::with_capacity(7, exact.as_slice());
        assert!(matches!(read_line(&mut r, &mut buf, 1), Ok(true)));
        assert_eq!(buf.len(), MAX_OP_RECORD_BYTES);
    }

    #[test]
    fn reject_cr_reports_its_column() {
        assert!(reject_cr(b"abc", 4).is_ok());
        assert!(matches!(
            reject_cr(b"ab\rc", 4),
            Err(StoreError::Decode { line: 4, column: 3 })
        ));
    }

    #[test]
    fn within_caps_matches_the_decoder() {
        let id = testkit::asset_id(1);
        let ok = Op::RenameAsset {
            id,
            name: "x".repeat(MAX_NAME_BYTES).into(),
            provenance: Provenance::default(),
        };
        let long = Op::RenameAsset {
            id,
            name: "x".repeat(MAX_NAME_BYTES + 1).into(),
            provenance: Provenance::default(),
        };
        assert!(within_caps(&ok));
        assert!(!within_caps(&long));
        assert!(within_caps(&Op::RemoveAsset { id }));
        let tree = TreeId::new_v7();
        let excl = Op::SetAssumptionExcludes {
            id: AssumptionId::new_v7(),
            trees: vec![tree; 130],
            steps: vec![
                StepRef {
                    tree,
                    node: NodeId::new_v7(),
                };
                127
            ],
            provenance: Provenance::default(),
        };
        assert!(!within_caps(&excl));
    }

    #[test]
    fn encode_refuses_foreign_indices_and_leaves_no_output() {
        let m = testkit::second();
        let wide = testkit::wide();
        let order = KeyOrder::new(&m);
        let mut buf = Vec::new();
        let foreign = Op::SetAssetProperties {
            id: testkit::asset_id(1),
            properties: wide.all_properties(),
            provenance: Provenance::default(),
        };
        assert!(matches!(
            encode_record(&mut buf, &m, &order, 1, &foreign),
            Err(StoreError::UnencodableOp)
        ));
        let mut log = OpLogWriter::resume(Vec::new(), m, 0);
        assert!(log.append(&foreign).is_err());
        assert_eq!(log.last_seq(), 0);
        assert!(log.into_inner().is_ok_and(|w| w.is_empty()));
    }

    #[test]
    fn append_refuses_seq_overflow() {
        let mut log = OpLogWriter::resume(Vec::new(), testkit::second(), u64::MAX);
        let op = Op::RemoveAsset {
            id: testkit::asset_id(1),
        };
        assert!(matches!(log.append(&op), Err(StoreError::UnencodableOp)));
    }
    #[test]
    fn codec_round_trips_and_matches_the_record_op_member() {
        let m = testkit::builtin();
        let codec = OpCodec::new(Arc::clone(&m));
        let op = Op::AddAsset {
            id: testkit::asset_id(1),
            name: "Brake ECU".into(),
            properties: m.all_properties(),
            provenance: Provenance::default(),
        };
        let bytes = codec.encode(&op).expect("encodes");
        assert_eq!(codec.decode(&bytes).expect("decodes"), op);
        assert_eq!(codec.encode(&op).expect("encodes"), bytes);

        // The bare op is exactly the `op` member of the log record.
        let mut record = Vec::new();
        encode_record(&mut record, &m, &KeyOrder::new(&m), 7, &op).expect("encodes");
        let expected = format!("{{\"seq\":7,\"op\":{}}}\n", String::from_utf8_lossy(&bytes));
        assert_eq!(String::from_utf8(record).expect("utf8"), expected);
    }

    #[test]
    fn codec_decode_is_strict_and_capped() {
        let codec = OpCodec::new(testkit::builtin());
        let id = testkit::asset_id(1);
        let good = format!("{{\"op\":\"remove_asset\",\"id\":\"{id}\"}}");
        assert!(codec.decode(good.as_bytes()).is_ok());
        for bad in [
            format!("{good} "),
            format!("{good}x"),
            format!("{{\"op\":\"remove_asset\",\"id\":\"{id}\",\"extra\":1}}"),
            "{\"op\":\"no_such_op\"}".to_string(),
            String::new(),
        ] {
            let result = codec.decode(bad.as_bytes());
            // Trailing whitespace is JSON-legal; everything else is an error.
            if bad.ends_with(' ') {
                assert!(result.is_ok(), "{bad}");
            } else {
                assert!(matches!(result, Err(StoreError::Decode { .. })), "{bad}");
            }
        }
        let long = vec![b' '; MAX_OP_RECORD_BYTES + 1];
        assert!(matches!(
            codec.decode(&long),
            Err(StoreError::LimitExceeded(Limit::OpRecordBytes { .. }))
        ));
    }

    #[test]
    fn codec_refuses_foreign_indices() {
        let codec = OpCodec::new(testkit::second());
        let wide = testkit::wide();
        let foreign = Op::SetAssetProperties {
            id: testkit::asset_id(1),
            properties: wide.all_properties(),
            provenance: Provenance::default(),
        };
        assert!(matches!(
            codec.encode(&foreign),
            Err(StoreError::UnencodableOp)
        ));
    }
}
