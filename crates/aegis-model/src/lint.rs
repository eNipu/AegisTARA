//! Completeness and consistency linting for a [`Project`](crate::Project).
//!
//! [`Project::lint`](crate::Project::lint) inspects a project's assets,
//! damage scenarios, threat scenarios, attack trees, and security controls
//! and returns a canonical, deterministic list of [`LintFinding`]s.
//!
//! Findings never contain entity names, labels, or actor strings (rule R6);
//! they reference project entities only by their stable public IDs
//! ([`AssetId`], [`DamageId`], [`ThreatId`], [`TreeId`], [`NodeId`],
//! [`ControlId`]) and vocabulary indices ([`Property`]).

use std::fmt;

use crate::id::{
    AssetId, ClaimId, ComponentId, ControlId, DamageId, DataFlowId, ElementId, GoalId, NodeId,
    ThreatId, TreeId,
};
use crate::item::ItemRef;
use crate::methodology::{ControlKind, Property};

/// Severity of a [`LintFinding`].
///
/// Ordered from lowest (`Warning`) to highest (`Error`).
///
/// ```
/// use aegis_model::LintSeverity;
///
/// assert!(LintSeverity::Warning < LintSeverity::Error);
/// assert_eq!(LintSeverity::Warning.as_str(), "warning");
/// assert_eq!(LintSeverity::Error.as_str(), "error");
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LintSeverity {
    /// An incomplete or redundant part of the analysis that does not block
    /// evaluation (for example an unlinked asset or unused security control).
    Warning,
    /// A structural gap or inconsistency that prevents evaluating a threat's
    /// feasibility or realizing its chosen treatment (for example a threat
    /// without feasibility, an empty gate, an unrated step, or a `Reduce`
    /// treatment with no control mitigations).
    Error,
}

impl LintSeverity {
    /// Returns the lowercase keyword for this severity (`"warning"` or
    /// `"error"`). Time O(1), space O(1).
    ///
    /// ```
    /// use aegis_model::LintSeverity;
    ///
    /// assert_eq!(LintSeverity::Warning.as_str(), "warning");
    /// assert_eq!(LintSeverity::Error.as_str(), "error");
    /// ```
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }
}

impl fmt::Display for LintSeverity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Machine-readable completeness or consistency rule violation in a
/// [`Project`](crate::Project).
///
/// Carries only stable IDs and vocabulary indices, never user-supplied names
/// or text (rule R6). 40 bytes at most, `Copy`.
///
/// ```
/// use aegis_model::{AssetId, LintCode, LintSeverity};
///
/// let id = AssetId::new_v7();
/// let code = LintCode::AssetWithoutThreats { asset: id };
/// assert_eq!(code.severity(), LintSeverity::Warning);
/// assert!(code.to_string().contains(&id.to_string()));
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LintCode {
    /// An asset is not targeted by any threat scenario in the project.
    AssetWithoutThreats {
        /// The asset with zero targeting threat scenarios.
        asset: AssetId,
    },
    /// An asset is targeted by at least one threat scenario, but one of its
    /// declared security properties is not violated by any threat scenario on
    /// that asset.
    AssetPropertyUntested {
        /// The asset whose property is untested.
        asset: AssetId,
        /// The declared property with zero violating threat scenarios on `asset`.
        property: Property,
    },
    /// A damage scenario has no impact ratings in any impact category.
    DamageScenarioWithoutImpacts {
        /// The unrated damage scenario.
        damage: DamageId,
    },
    /// A damage scenario has an impact rating but no rationale, and the pack
    /// asks for one (FND-09).
    ImpactWithoutRationale {
        /// The damage scenario.
        damage: DamageId,
    },
    /// A threat scenario has a direct feasibility rating but no rationale,
    /// and the pack asks for one (FND-09).
    FeasibilityWithoutRationale {
        /// The threat scenario.
        threat: ThreatId,
    },
    /// A damage scenario is not linked by any threat scenario.
    DamageScenarioUnlinked {
        /// The unlinked damage scenario.
        damage: DamageId,
    },
    /// A threat scenario does not link to any damage scenario, so its risk
    /// cannot be determined.
    ThreatWithoutDamages {
        /// The threat scenario with an empty damage list.
        threat: ThreatId,
    },
    /// A threat scenario has neither a linked attack tree nor a direct
    /// feasibility rating, so its feasibility cannot be evaluated.
    ThreatWithoutFeasibility {
        /// The threat scenario lacking feasibility.
        threat: ThreatId,
    },
    /// A threat scenario has both a linked attack tree and a direct
    /// feasibility rating; the linked attack tree overrides the direct rating.
    RedundantDirectFeasibility {
        /// The threat scenario with a shadowed direct feasibility rating.
        threat: ThreatId,
    },
    /// A threat scenario has no risk treatment decision recorded.
    ThreatWithoutTreatment {
        /// The untreated threat scenario.
        threat: ThreatId,
    },
    /// A threat scenario has a [`RiskTreatment::Reduce`](crate::RiskTreatment::Reduce)
    /// decision, but either has no linked attack tree or its linked attack
    /// tree has no security control mitigations attached to any step.
    ReduceTreatmentWithoutControls {
        /// The threat scenario whose `Reduce` treatment has no control mitigations.
        threat: ThreatId,
    },
    /// A threat scenario has a [`RiskTreatment::Reduce`](crate::RiskTreatment::Reduce)
    /// decision, but has no linked [`CybersecurityGoal`](crate::CybersecurityGoal).
    ReduceTreatmentWithoutGoal {
        /// The threat scenario lacking a linked cybersecurity goal.
        threat: ThreatId,
    },
    /// A threat scenario has a [`RiskTreatment::Retain`](crate::RiskTreatment::Retain)
    /// or [`RiskTreatment::Share`](crate::RiskTreatment::Share) decision, but
    /// has no linked [`CybersecurityClaim`](crate::CybersecurityClaim).
    RetainOrShareTreatmentWithoutClaim {
        /// The threat scenario lacking a linked cybersecurity claim.
        threat: ThreatId,
    },
    /// A threat scenario has a [`RiskTreatment::Share`](crate::RiskTreatment::Share)
    /// decision and a linked [`CybersecurityClaim`](crate::CybersecurityClaim),
    /// but that claim names no responsible party other than `Us`.
    ShareTreatmentWithoutOtherParty {
        /// The threat scenario whose shared claim has no other party.
        threat: ThreatId,
    },
    /// A threat scenario links to a [`CybersecurityGoal`](crate::CybersecurityGoal),
    /// but its risk treatment is not [`RiskTreatment::Reduce`](crate::RiskTreatment::Reduce).
    GoalOnNonReduceThreat {
        /// The threat scenario linking a goal without reduce treatment.
        threat: ThreatId,
        /// The linked cybersecurity goal.
        goal: GoalId,
    },
    /// A threat scenario links to a [`CybersecurityClaim`](crate::CybersecurityClaim),
    /// but its risk treatment is neither [`RiskTreatment::Retain`](crate::RiskTreatment::Retain)
    /// nor [`RiskTreatment::Share`](crate::RiskTreatment::Share).
    ClaimOnNonRetainOrShareThreat {
        /// The threat scenario linking a claim without retain or share treatment.
        threat: ThreatId,
        /// The linked cybersecurity claim.
        claim: ClaimId,
    },
    /// An attack tree in the project is not linked to any threat scenario.
    AttackTreeUnlinked {
        /// The unlinked attack tree.
        tree: TreeId,
    },
    /// A gate node in a project attack tree has no children, preventing tree
    /// evaluation.
    AttackTreeGateEmpty {
        /// The attack tree containing the empty gate.
        tree: TreeId,
        /// The empty gate node.
        node: NodeId,
    },
    /// The rating of a step came from a library item and nobody has marked it
    /// as reviewed. Feasibility depends on the product, so a reused rating
    /// must be looked at before it reaches a report.
    LibraryRatingNotReviewed {
        /// The attack tree.
        tree: TreeId,
        /// The step whose rating came from a library item.
        node: NodeId,
    },
    /// A leaf step node in a project attack tree has no inherent feasibility
    /// rating (neither complete attack potential factor ratings nor a direct
    /// step feasibility rating), preventing tree evaluation.
    AttackTreeStepUnrated {
        /// The attack tree containing the unrated step.
        tree: TreeId,
        /// The unrated step node.
        node: NodeId,
    },
    /// A leaf step node in a project attack tree has some, but not all, of the
    /// methodology's attack potential factors rated (`0 < rated < factor_count`).
    AttackTreeStepIncompleteFactors {
        /// The attack tree containing the partially factor-rated step.
        tree: TreeId,
        /// The step node with incomplete factor ratings.
        node: NodeId,
    },
    /// A leaf step node in a project attack tree has both a complete set of
    /// attack potential factor ratings and a direct step feasibility rating;
    /// the resolved attack potential overrides the direct rating.
    RedundantStepFeasibility {
        /// The attack tree containing the step with a shadowed direct rating.
        tree: TreeId,
        /// The step node whose direct feasibility rating is shadowed.
        node: NodeId,
    },
    /// A step has an attack type but no "through" item, or a "through" item
    /// but no attack type, while some security control carries rules. Such a
    /// step can never match a rule, so a setting cannot cap it (CFG-02).
    StepSettingsIncomplete {
        /// The attack tree containing the step.
        tree: TreeId,
        /// The step missing its "through" item or its attack type.
        node: NodeId,
    },
    /// A security control in the project is not attached to any attack step.
    SecurityControlUnused {
        /// The unused security control.
        control: ControlId,
    },
    /// A security control was created from a pack preset and its rules no
    /// longer equal the preset's rules in the project's methodology (CFG-06).
    ControlRulesDifferFromKind {
        /// The security control.
        control: ControlId,
    },
    /// Two settings whose pack presets say they conflict are applied to
    /// the same component or interface (CFG-09). A warning, never a
    /// block: the pair may record a real defect.
    ConflictingSettings {
        /// The component or interface both settings are applied to.
        item: ItemRef,
        /// The preset of one setting; the lower vocabulary index.
        first: ControlKind,
        /// The preset of the other setting; the higher vocabulary index.
        second: ControlKind,
    },
    /// A cybersecurity goal links to zero security controls.
    CybersecurityGoalWithoutControls {
        /// The cybersecurity goal with an empty control list.
        goal: GoalId,
    },
    /// A cybersecurity goal in the project is not linked by any threat scenario.
    CybersecurityGoalUnlinked {
        /// The unlinked cybersecurity goal.
        goal: GoalId,
    },
    /// A cybersecurity claim in the project is not linked by any threat scenario.
    CybersecurityClaimUnlinked {
        /// The unlinked cybersecurity claim.
        claim: ClaimId,
    },
    /// The project has components defined, but an asset is not bound to any
    /// component.
    AssetUnbound {
        /// The unbound asset.
        asset: AssetId,
    },
    /// A data flow crosses at least one trust boundary, but neither its source
    /// nor its destination component has any bound asset with a threat
    /// scenario.
    BoundaryCrossingDataFlowWithoutThreat {
        /// The boundary-crossing data flow without associated threat scenarios.
        flow: DataFlowId,
    },
    /// The project has systems, but a top-level component is in none.
    ComponentWithoutSystem {
        /// The component in no system.
        component: ComponentId,
    },
    /// A data flow links components of two systems, but neither its source
    /// nor its destination component has any bound asset with a threat
    /// scenario.
    SystemBoundaryFlowWithoutThreat {
        /// The boundary flow without associated threat scenarios.
        flow: DataFlowId,
    },
    /// Two systems have the same name, so a report profile that names it
    /// covers both.
    SystemNameShared {
        /// The later system.
        system: ElementId,
        /// The system with the lowest ID that has this name.
        first: ElementId,
    },
}

impl LintCode {
    /// Returns the [`LintSeverity`] of this rule violation. Time O(1),
    /// space O(1).
    ///
    /// ```
    /// use aegis_model::{AssetId, LintCode, LintSeverity, ThreatId};
    ///
    /// assert_eq!(
    ///     LintCode::AssetWithoutThreats { asset: AssetId::new_v7() }.severity(),
    ///     LintSeverity::Warning
    /// );
    /// assert_eq!(
    ///     LintCode::ThreatWithoutFeasibility { threat: ThreatId::new_v7() }.severity(),
    ///     LintSeverity::Error
    /// );
    /// ```
    #[must_use]
    pub const fn severity(&self) -> LintSeverity {
        match self {
            Self::AssetWithoutThreats { .. }
            | Self::AssetPropertyUntested { .. }
            | Self::DamageScenarioWithoutImpacts { .. }
            | Self::ImpactWithoutRationale { .. }
            | Self::FeasibilityWithoutRationale { .. }
            | Self::DamageScenarioUnlinked { .. }
            | Self::ThreatWithoutDamages { .. }
            | Self::RedundantDirectFeasibility { .. }
            | Self::ThreatWithoutTreatment { .. }
            | Self::AttackTreeUnlinked { .. }
            | Self::AttackTreeStepIncompleteFactors { .. }
            | Self::RedundantStepFeasibility { .. }
            | Self::SecurityControlUnused { .. }
            | Self::ControlRulesDifferFromKind { .. }
            | Self::ConflictingSettings { .. }
            | Self::StepSettingsIncomplete { .. }
            | Self::ReduceTreatmentWithoutGoal { .. }
            | Self::RetainOrShareTreatmentWithoutClaim { .. }
            | Self::ShareTreatmentWithoutOtherParty { .. }
            | Self::GoalOnNonReduceThreat { .. }
            | Self::ClaimOnNonRetainOrShareThreat { .. }
            | Self::CybersecurityGoalWithoutControls { .. }
            | Self::CybersecurityGoalUnlinked { .. }
            | Self::CybersecurityClaimUnlinked { .. }
            | Self::AssetUnbound { .. }
            | Self::BoundaryCrossingDataFlowWithoutThreat { .. }
            | Self::ComponentWithoutSystem { .. }
            | Self::SystemBoundaryFlowWithoutThreat { .. }
            | Self::SystemNameShared { .. }
            | Self::LibraryRatingNotReviewed { .. } => LintSeverity::Warning,
            Self::ThreatWithoutFeasibility { .. }
            | Self::ReduceTreatmentWithoutControls { .. }
            | Self::AttackTreeGateEmpty { .. }
            | Self::AttackTreeStepUnrated { .. } => LintSeverity::Error,
        }
    }
}

impl fmt::Display for LintCode {
    // One arm per lint code, so the length follows the number of codes.
    #[allow(clippy::too_many_lines)]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AssetWithoutThreats { asset } => {
                write!(f, "asset {asset} has no threat scenarios")
            }
            Self::AssetPropertyUntested { asset, property } => write!(
                f,
                "asset {asset} property index {} is not targeted by any threat scenario",
                property.index()
            ),
            Self::DamageScenarioWithoutImpacts { damage } => {
                write!(f, "damage scenario {damage} has no impact ratings")
            }
            Self::ImpactWithoutRationale { damage } => {
                write!(
                    f,
                    "damage scenario {damage} has an impact rating without a rationale"
                )
            }
            Self::FeasibilityWithoutRationale { threat } => write!(
                f,
                "threat scenario {threat} has a direct feasibility rating without a rationale"
            ),
            Self::DamageScenarioUnlinked { damage } => write!(
                f,
                "damage scenario {damage} is not linked to any threat scenario"
            ),
            Self::ThreatWithoutDamages { threat } => write!(
                f,
                "threat scenario {threat} is not linked to any damage scenario"
            ),
            Self::ThreatWithoutFeasibility { threat } => write!(
                f,
                "threat scenario {threat} has neither an attack tree nor a direct feasibility rating"
            ),
            Self::RedundantDirectFeasibility { threat } => write!(
                f,
                "threat scenario {threat} has a direct feasibility rating shadowed by its attack tree"
            ),
            Self::ThreatWithoutTreatment { threat } => {
                write!(f, "threat scenario {threat} has no risk treatment decision")
            }
            Self::ReduceTreatmentWithoutControls { threat } => write!(
                f,
                "threat scenario {threat} has reduce treatment without any step mitigations"
            ),
            Self::AttackTreeUnlinked { tree } => {
                write!(f, "attack tree {tree} is not linked to any threat scenario")
            }
            Self::AttackTreeGateEmpty { tree, node } => {
                write!(f, "attack tree {tree} gate {node} has no children")
            }
            Self::LibraryRatingNotReviewed { tree, node } => write!(
                f,
                "the rating of step {node} in attack tree {tree} came from a library item and has not been reviewed"
            ),
            Self::AttackTreeStepUnrated { tree, node } => write!(
                f,
                "attack tree {tree} step {node} has no feasibility rating"
            ),
            Self::AttackTreeStepIncompleteFactors { tree, node } => write!(
                f,
                "attack tree {tree} step {node} has incomplete attack potential factor ratings"
            ),
            Self::RedundantStepFeasibility { tree, node } => write!(
                f,
                "attack tree {tree} step {node} has a direct feasibility rating shadowed by its attack potential factors"
            ),
            Self::StepSettingsIncomplete { tree, node } => {
                write!(
                    f,
                    "attack tree {tree} step {node} lacks a through item or an attack type"
                )
            }
            Self::SecurityControlUnused { control } => write!(
                f,
                "security control {control} is not attached to any attack step"
            ),
            Self::ControlRulesDifferFromKind { control } => write!(
                f,
                "security control {control} has rules that differ from its pack preset"
            ),
            Self::ConflictingSettings {
                item,
                first,
                second,
            } => write!(
                f,
                "{item} has settings of kinds {} and {} that the pack says conflict",
                first.index(),
                second.index()
            ),
            Self::ReduceTreatmentWithoutGoal { threat } => write!(
                f,
                "threat scenario {threat} has reduce treatment without a linked cybersecurity goal"
            ),
            Self::RetainOrShareTreatmentWithoutClaim { threat } => write!(
                f,
                "threat scenario {threat} has retain or share treatment without a linked cybersecurity claim"
            ),
            Self::ShareTreatmentWithoutOtherParty { threat } => write!(
                f,
                "threat scenario {threat} shares risk without a responsible party other than us"
            ),
            Self::GoalOnNonReduceThreat { threat, goal } => write!(
                f,
                "threat scenario {threat} links cybersecurity goal {goal} without reduce treatment"
            ),
            Self::ClaimOnNonRetainOrShareThreat { threat, claim } => write!(
                f,
                "threat scenario {threat} links cybersecurity claim {claim} without retain or share treatment"
            ),
            Self::CybersecurityGoalWithoutControls { goal } => write!(
                f,
                "cybersecurity goal {goal} has no linked security controls"
            ),
            Self::CybersecurityGoalUnlinked { goal } => write!(
                f,
                "cybersecurity goal {goal} is not linked to any threat scenario"
            ),
            Self::CybersecurityClaimUnlinked { claim } => write!(
                f,
                "cybersecurity claim {claim} is not linked to any threat scenario"
            ),
            Self::AssetUnbound { asset } => {
                write!(f, "asset {asset} is not bound to any component")
            }
            Self::BoundaryCrossingDataFlowWithoutThreat { flow } => write!(
                f,
                "boundary-crossing data flow {flow} has no threat scenario on its endpoint components"
            ),
            Self::ComponentWithoutSystem { component } => {
                write!(f, "component {component} is in no system")
            }
            Self::SystemBoundaryFlowWithoutThreat { flow } => write!(
                f,
                "data flow {flow} between two systems has no threat scenario on its endpoint components"
            ),
            Self::SystemNameShared { system, first } => {
                write!(f, "system {system} has the same name as system {first}")
            }
        }
    }
}

/// One completeness or consistency finding produced by
/// [`Project::lint`](crate::Project::lint).
///
/// Wraps a [`LintCode`] and exposes its [`LintSeverity`]. Ordered
/// canonically by [`LintCode`] so [`Project::lint`](crate::Project::lint)
/// output is independent of edit history (rule P4).
///
/// ```
/// use aegis_model::{ControlId, LintCode, LintFinding, LintSeverity};
///
/// let id = ControlId::new_v7();
/// let finding = LintFinding::new(LintCode::SecurityControlUnused { control: id });
/// assert_eq!(finding.severity(), LintSeverity::Warning);
/// assert!(finding.is_warning());
/// assert!(!finding.is_error());
/// assert_eq!(finding.code(), LintCode::SecurityControlUnused { control: id });
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LintFinding {
    code: LintCode,
}

impl LintFinding {
    /// Creates a finding for `code`. Time O(1), space O(1).
    ///
    /// ```
    /// use aegis_model::{DamageId, LintCode, LintFinding};
    ///
    /// let d = DamageId::new_v7();
    /// let f = LintFinding::new(LintCode::DamageScenarioUnlinked { damage: d });
    /// assert_eq!(f.code(), LintCode::DamageScenarioUnlinked { damage: d });
    /// ```
    #[must_use]
    pub const fn new(code: LintCode) -> Self {
        Self { code }
    }

    /// Returns the [`LintCode`] of this finding. Time O(1), space O(1).
    ///
    /// ```
    /// use aegis_model::{AssetId, LintCode, LintFinding};
    ///
    /// let a = AssetId::new_v7();
    /// let f = LintFinding::new(LintCode::AssetWithoutThreats { asset: a });
    /// assert_eq!(f.code(), LintCode::AssetWithoutThreats { asset: a });
    /// ```
    #[must_use]
    pub const fn code(&self) -> LintCode {
        self.code
    }

    /// Returns the [`LintSeverity`] of this finding. Time O(1), space O(1).
    ///
    /// ```
    /// use aegis_model::{LintCode, LintFinding, LintSeverity, ThreatId};
    ///
    /// let t = ThreatId::new_v7();
    /// let f = LintFinding::new(LintCode::ThreatWithoutFeasibility { threat: t });
    /// assert_eq!(f.severity(), LintSeverity::Error);
    /// ```
    #[must_use]
    pub const fn severity(&self) -> LintSeverity {
        self.code.severity()
    }

    /// Returns `true` when [`severity`](Self::severity) is
    /// [`LintSeverity::Error`]. Time O(1), space O(1).
    ///
    /// ```
    /// use aegis_model::{LintCode, LintFinding, ThreatId};
    ///
    /// let t = ThreatId::new_v7();
    /// assert!(LintFinding::new(LintCode::ThreatWithoutFeasibility { threat: t }).is_error());
    /// assert!(!LintFinding::new(LintCode::ThreatWithoutTreatment { threat: t }).is_error());
    /// ```
    #[must_use]
    pub const fn is_error(&self) -> bool {
        matches!(self.severity(), LintSeverity::Error)
    }

    /// Returns `true` when [`severity`](Self::severity) is
    /// [`LintSeverity::Warning`]. Time O(1), space O(1).
    ///
    /// ```
    /// use aegis_model::{LintCode, LintFinding, ThreatId};
    ///
    /// let t = ThreatId::new_v7();
    /// assert!(LintFinding::new(LintCode::ThreatWithoutTreatment { threat: t }).is_warning());
    /// assert!(!LintFinding::new(LintCode::ThreatWithoutFeasibility { threat: t }).is_warning());
    /// ```
    #[must_use]
    pub const fn is_warning(&self) -> bool {
        matches!(self.severity(), LintSeverity::Warning)
    }
}

impl fmt::Display for LintFinding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.severity(), self.code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lint_code_size_is_small() {
        assert!(std::mem::size_of::<LintFinding>() <= 40);
    }

    #[test]
    fn lint_severity_and_display_cover_all_variants() {
        let m = crate::testkit::builtin();
        let prop = m.properties().next().expect("property");
        let asset = crate::testkit::asset_id(1);
        let damage = crate::testkit::damage_id(2);
        let threat = crate::testkit::threat_id(3);
        let tree = crate::testkit::tree_id(4);
        let node = crate::testkit::node_id(5);
        let control = crate::testkit::control_id(6);
        let goal = GoalId::new_v7();
        let mut kinds = m.control_kinds();
        let first_kind = kinds.next().expect("kind");
        let second_kind = kinds.next().expect("kind");
        let claim = ClaimId::new_v7();
        let flow = DataFlowId::new_v7();

        let warnings = [
            LintCode::AssetWithoutThreats { asset },
            LintCode::AssetPropertyUntested {
                asset,
                property: prop,
            },
            LintCode::DamageScenarioWithoutImpacts { damage },
            LintCode::ImpactWithoutRationale { damage },
            LintCode::FeasibilityWithoutRationale { threat },
            LintCode::DamageScenarioUnlinked { damage },
            LintCode::ThreatWithoutDamages { threat },
            LintCode::RedundantDirectFeasibility { threat },
            LintCode::ThreatWithoutTreatment { threat },
            LintCode::AttackTreeUnlinked { tree },
            LintCode::AttackTreeStepIncompleteFactors { tree, node },
            LintCode::RedundantStepFeasibility { tree, node },
            LintCode::SecurityControlUnused { control },
            LintCode::ReduceTreatmentWithoutGoal { threat },
            LintCode::RetainOrShareTreatmentWithoutClaim { threat },
            LintCode::GoalOnNonReduceThreat { threat, goal },
            LintCode::ClaimOnNonRetainOrShareThreat { threat, claim },
            LintCode::CybersecurityGoalWithoutControls { goal },
            LintCode::CybersecurityGoalUnlinked { goal },
            LintCode::CybersecurityClaimUnlinked { claim },
            LintCode::AssetUnbound { asset },
            LintCode::BoundaryCrossingDataFlowWithoutThreat { flow },
            LintCode::ComponentWithoutSystem {
                component: ComponentId::new_v7(),
            },
            LintCode::SystemBoundaryFlowWithoutThreat { flow },
            LintCode::SystemNameShared {
                system: ElementId::new_v7(),
                first: ElementId::new_v7(),
            },
            LintCode::ConflictingSettings {
                item: ItemRef::Component(crate::testkit::component_id(1)),
                first: first_kind,
                second: second_kind,
            },
            LintCode::ConflictingSettings {
                item: ItemRef::Interface(crate::testkit::interface_id(2)),
                first: first_kind,
                second: second_kind,
            },
        ];
        for code in warnings {
            let f = LintFinding::new(code);
            assert!(f.is_warning());
            assert!(!f.is_error());
            assert_eq!(f.severity(), LintSeverity::Warning);
            assert!(f.to_string().starts_with("warning: "));
        }

        let errors = [
            LintCode::ThreatWithoutFeasibility { threat },
            LintCode::ReduceTreatmentWithoutControls { threat },
            LintCode::AttackTreeGateEmpty { tree, node },
            LintCode::AttackTreeStepUnrated { tree, node },
        ];
        for code in errors {
            let f = LintFinding::new(code);
            assert!(f.is_error());
            assert!(!f.is_warning());
            assert_eq!(f.severity(), LintSeverity::Error);
            assert!(f.to_string().starts_with("error: "));
        }
    }
}
