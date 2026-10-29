//! Deterministic version diff and change impact analysis between two
//! [`Project`](crate::Project) snapshots over the same methodology (rule A7).
//!
//! [`Project::diff`](crate::Project::diff) compares a baseline project against
//! a current project and produces a [`ProjectDiff`] containing:
//!
//! 1. **Per-entity-kind structural changes ([`EntityDiff`]):** `added`,
//!    `removed`, and `modified` public IDs in canonical sorted order for
//!    [`AssetId`], [`DamageId`], [`ThreatId`], [`TreeId`], and [`ControlId`].
//! 2. **Change impact and risk deltas ([`ThreatRiskDelta`]):** every threat
//!    scenario affected by any entity change (added, removed, or modified
//!    threat scenario, or a threat scenario referencing a modified asset,
//!    damage scenario, attack tree, or security control), together with its
//!    baseline and current inherent and residual [`RiskDelta`]
//!    ([`RiskSnapshot`] and [`RiskChange`]).
//!
//! Diff types hold only stable public IDs (`UUIDv7`) and vocabulary indices
//! ([`FeasibilityLevel`], [`RiskLevel`]), never user-supplied entity names,
//! labels, or actor text (rule R6).

use std::cmp::Ordering;
use std::fmt;

use crate::id::{
    AssetId, ClaimId, ComponentId, ControlId, DamageId, DataFlowId, GoalId, InterfaceId, ThreatId,
    TreeId, TrustBoundaryId,
};
use crate::methodology::{FeasibilityLevel, RiskLevel};

/// Added, removed, and modified entity IDs of one entity kind between a
/// baseline project and a current project, each in canonical ascending ID
/// order (rule P4).
///
/// ```
/// use aegis_model::{AssetId, EntityDiff};
///
/// let empty = EntityDiff::<AssetId>::default();
/// assert!(empty.is_empty());
/// assert_eq!(empty.len(), 0);
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntityDiff<Id> {
    pub(crate) added: Box<[Id]>,
    pub(crate) removed: Box<[Id]>,
    pub(crate) modified: Box<[Id]>,
}

impl<Id> Default for EntityDiff<Id> {
    fn default() -> Self {
        Self {
            added: Box::new([]),
            removed: Box::new([]),
            modified: Box::new([]),
        }
    }
}

impl<Id> EntityDiff<Id> {
    /// Returns the IDs present in the current project that were absent in the
    /// baseline project, in ascending order.
    ///
    /// Time O(1), space O(1).
    ///
    /// ```
    /// use aegis_model::{AssetId, EntityDiff};
    ///
    /// let diff = EntityDiff::<AssetId>::default();
    /// assert!(diff.added().is_empty());
    /// ```
    #[must_use]
    pub fn added(&self) -> &[Id] {
        &self.added
    }

    /// Returns the IDs present in the baseline project that were removed in
    /// the current project, in ascending order.
    ///
    /// Time O(1), space O(1).
    ///
    /// ```
    /// use aegis_model::{AssetId, EntityDiff};
    ///
    /// let diff = EntityDiff::<AssetId>::default();
    /// assert!(diff.removed().is_empty());
    /// ```
    #[must_use]
    pub fn removed(&self) -> &[Id] {
        &self.removed
    }

    /// Returns the IDs present in both projects whose content, references, or
    /// provenance changed, in ascending order.
    ///
    /// Time O(1), space O(1).
    ///
    /// ```
    /// use aegis_model::{AssetId, EntityDiff};
    ///
    /// let diff = EntityDiff::<AssetId>::default();
    /// assert!(diff.modified().is_empty());
    /// ```
    #[must_use]
    pub fn modified(&self) -> &[Id] {
        &self.modified
    }

    /// Returns `true` if no entity of this kind was added, removed, or
    /// modified.
    ///
    /// Time O(1), space O(1).
    ///
    /// ```
    /// use aegis_model::{AssetId, EntityDiff};
    ///
    /// assert!(EntityDiff::<AssetId>::default().is_empty());
    /// ```
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.modified.is_empty()
    }

    /// Returns the total number of added, removed, and modified entities of
    /// this kind.
    ///
    /// Time O(1), space O(1).
    ///
    /// ```
    /// use aegis_model::{AssetId, EntityDiff};
    ///
    /// assert_eq!(EntityDiff::<AssetId>::default().len(), 0);
    /// ```
    #[must_use]
    pub fn len(&self) -> usize {
        self.added.len() + self.removed.len() + self.modified.len()
    }
}

/// Summary of a threat scenario's evaluated feasibility and worst-case risk
/// level at one point in time.
///
/// ```
/// use aegis_model::{Methodology, RiskSnapshot};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let m = Methodology::iso21434_r155()?;
/// let high = m.feasibility_level("high").ok_or("no level")?;
/// let r5 = m.risk_level("r5");
/// let snap = RiskSnapshot::new(high, r5);
/// assert_eq!(snap.feasibility(), high);
/// assert_eq!(snap.max_risk(), r5);
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RiskSnapshot {
    feasibility: FeasibilityLevel,
    max_risk: Option<RiskLevel>,
}

impl RiskSnapshot {
    /// Creates a risk snapshot from `feasibility` and `max_risk`.
    ///
    /// Time O(1), space O(1).
    ///
    /// ```
    /// use aegis_model::{Methodology, RiskSnapshot};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Methodology::iso21434_r155()?;
    /// let low = m.feasibility_level("low").ok_or("no level")?;
    /// let snap = RiskSnapshot::new(low, None);
    /// assert_eq!(snap.feasibility(), low);
    /// assert_eq!(snap.max_risk(), None);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub const fn new(feasibility: FeasibilityLevel, max_risk: Option<RiskLevel>) -> Self {
        Self {
            feasibility,
            max_risk,
        }
    }

    /// Returns the evaluated feasibility level of the threat scenario.
    ///
    /// Time O(1), space O(1).
    ///
    /// ```
    /// use aegis_model::{Methodology, RiskSnapshot};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Methodology::iso21434_r155()?;
    /// let med = m.feasibility_level("medium").ok_or("no level")?;
    /// assert_eq!(RiskSnapshot::new(med, None).feasibility(), med);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub const fn feasibility(self) -> FeasibilityLevel {
        self.feasibility
    }

    /// Returns the worst-case risk level across all rated damage impacts, or
    /// `None` if the threat scenario had no rated damage impacts.
    ///
    /// Time O(1), space O(1).
    ///
    /// ```
    /// use aegis_model::{Methodology, RiskSnapshot};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Methodology::iso21434_r155()?;
    /// let med = m.feasibility_level("medium").ok_or("no level")?;
    /// let r3 = m.risk_level("r3");
    /// assert_eq!(RiskSnapshot::new(med, r3).max_risk(), r3);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub const fn max_risk(self) -> Option<RiskLevel> {
        self.max_risk
    }
}

/// Direction of change between a baseline [`RiskSnapshot`] and a current
/// [`RiskSnapshot`].
///
/// ```
/// use aegis_model::RiskChange;
///
/// assert_eq!(RiskChange::Increased.as_str(), "increased");
/// assert_eq!(RiskChange::Decreased.to_string(), "decreased");
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum RiskChange {
    /// The worst-case risk level (or feasibility when no damage impacts are
    /// rated) is unchanged between baseline and current.
    Unchanged,
    /// The worst-case risk level (or feasibility when no damage impacts are
    /// rated) increased in the current project.
    Increased,
    /// The worst-case risk level (or feasibility when no damage impacts are
    /// rated) decreased in the current project.
    Decreased,
    /// The threat scenario was not evaluable (or had no rated damage impacts)
    /// in the baseline project and is now evaluated in the current project.
    NewlyEvaluated,
    /// The threat scenario was evaluated in the baseline project and is no
    /// longer evaluable (or no longer has rated damage impacts) in the current
    /// project.
    NoLongerEvaluated,
}

impl RiskChange {
    /// Returns the canonical lowercase string for this risk change direction.
    ///
    /// Time O(1), space O(1).
    ///
    /// ```
    /// use aegis_model::RiskChange;
    ///
    /// assert_eq!(RiskChange::Unchanged.as_str(), "unchanged");
    /// assert_eq!(RiskChange::NewlyEvaluated.as_str(), "newly-evaluated");
    /// assert_eq!(RiskChange::NoLongerEvaluated.as_str(), "no-longer-evaluated");
    /// ```
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unchanged => "unchanged",
            Self::Increased => "increased",
            Self::Decreased => "decreased",
            Self::NewlyEvaluated => "newly-evaluated",
            Self::NoLongerEvaluated => "no-longer-evaluated",
        }
    }

    /// Computes the directional change between `before` and `after`.
    #[must_use]
    pub(crate) fn between(before: Option<RiskSnapshot>, after: Option<RiskSnapshot>) -> Self {
        match (before, after) {
            (None, None) => Self::Unchanged,
            (None, Some(_)) => Self::NewlyEvaluated,
            (Some(_), None) => Self::NoLongerEvaluated,
            (Some(b), Some(a)) => match (b.max_risk, a.max_risk) {
                (Some(rb), Some(ra)) => match ra.cmp(&rb) {
                    Ordering::Greater => Self::Increased,
                    Ordering::Less => Self::Decreased,
                    Ordering::Equal => Self::Unchanged,
                },
                (None, Some(_)) => Self::NewlyEvaluated,
                (Some(_), None) => Self::NoLongerEvaluated,
                (None, None) => match a.feasibility.cmp(&b.feasibility) {
                    Ordering::Greater => Self::Increased,
                    Ordering::Less => Self::Decreased,
                    Ordering::Equal => Self::Unchanged,
                },
            },
        }
    }
}

impl fmt::Display for RiskChange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Baseline vs. current [`RiskSnapshot`] and directional [`RiskChange`] for
/// one evaluation mode (inherent or residual).
///
/// ```
/// use aegis_model::{Methodology, RiskChange, RiskDelta, RiskSnapshot};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let m = Methodology::iso21434_r155()?;
/// let low = m.feasibility_level("low").ok_or("no level")?;
/// let high = m.feasibility_level("high").ok_or("no level")?;
/// let r2 = m.risk_level("r2");
/// let r5 = m.risk_level("r5");
/// let delta = RiskDelta::new(
///     Some(RiskSnapshot::new(high, r5)),
///     Some(RiskSnapshot::new(low, r2)),
/// );
/// assert_eq!(delta.change(), RiskChange::Decreased);
/// assert!(delta.is_changed());
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RiskDelta {
    before: Option<RiskSnapshot>,
    after: Option<RiskSnapshot>,
    change: RiskChange,
}

impl RiskDelta {
    /// Creates a `RiskDelta` from `before` and `after` snapshots.
    ///
    /// Time O(1), space O(1).
    ///
    /// ```
    /// use aegis_model::{RiskChange, RiskDelta};
    ///
    /// let delta = RiskDelta::new(None, None);
    /// assert_eq!(delta.change(), RiskChange::Unchanged);
    /// assert!(!delta.is_changed());
    /// ```
    #[must_use]
    pub fn new(before: Option<RiskSnapshot>, after: Option<RiskSnapshot>) -> Self {
        Self {
            before,
            after,
            change: RiskChange::between(before, after),
        }
    }

    /// Returns the baseline [`RiskSnapshot`], or `None` if the threat scenario
    /// was absent or not evaluable in the baseline project.
    ///
    /// Time O(1), space O(1).
    ///
    /// ```
    /// use aegis_model::RiskDelta;
    ///
    /// assert_eq!(RiskDelta::new(None, None).before(), None);
    /// ```
    #[must_use]
    pub const fn before(self) -> Option<RiskSnapshot> {
        self.before
    }

    /// Returns the current [`RiskSnapshot`], or `None` if the threat scenario
    /// is absent or not evaluable in the current project.
    ///
    /// Time O(1), space O(1).
    ///
    /// ```
    /// use aegis_model::RiskDelta;
    ///
    /// assert_eq!(RiskDelta::new(None, None).after(), None);
    /// ```
    #[must_use]
    pub const fn after(self) -> Option<RiskSnapshot> {
        self.after
    }

    /// Returns the directional [`RiskChange`] between [`before`](Self::before)
    /// and [`after`](Self::after).
    ///
    /// Time O(1), space O(1).
    ///
    /// ```
    /// use aegis_model::{RiskChange, RiskDelta};
    ///
    /// assert_eq!(RiskDelta::new(None, None).change(), RiskChange::Unchanged);
    /// ```
    #[must_use]
    pub const fn change(self) -> RiskChange {
        self.change
    }

    /// Returns `true` if `before != after` (either `feasibility`, `max_risk`,
    /// or evaluability changed).
    ///
    /// Time O(1), space O(1).
    ///
    /// ```
    /// use aegis_model::{Methodology, RiskDelta, RiskSnapshot};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Methodology::iso21434_r155()?;
    /// let low = m.feasibility_level("low").ok_or("no level")?;
    /// let snap = Some(RiskSnapshot::new(low, None));
    /// assert!(!RiskDelta::new(snap, snap).is_changed());
    /// assert!(RiskDelta::new(None, snap).is_changed());
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn is_changed(self) -> bool {
        self.before != self.after
    }
}

/// Inherent and residual [`RiskDelta`] for one affected threat scenario
/// between a baseline project and a current project (rule A7).
///
/// ```
/// use aegis_model::{RiskDelta, ThreatId, ThreatRiskDelta};
///
/// let t = ThreatId::new_v7();
/// let delta = ThreatRiskDelta::new(t, RiskDelta::new(None, None), RiskDelta::new(None, None));
/// assert_eq!(delta.threat(), t);
/// assert!(!delta.is_risk_changed());
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ThreatRiskDelta {
    threat: ThreatId,
    inherent: RiskDelta,
    residual: RiskDelta,
}

impl ThreatRiskDelta {
    /// Creates a `ThreatRiskDelta` for `threat` with `inherent` and `residual`
    /// deltas.
    ///
    /// Time O(1), space O(1).
    ///
    /// ```
    /// use aegis_model::{RiskDelta, ThreatId, ThreatRiskDelta};
    ///
    /// let t = ThreatId::new_v7();
    /// let d = ThreatRiskDelta::new(t, RiskDelta::new(None, None), RiskDelta::new(None, None));
    /// assert_eq!(d.threat(), t);
    /// ```
    #[must_use]
    pub const fn new(threat: ThreatId, inherent: RiskDelta, residual: RiskDelta) -> Self {
        Self {
            threat,
            inherent,
            residual,
        }
    }

    /// Returns the public ID of the affected threat scenario.
    ///
    /// Time O(1), space O(1).
    ///
    /// ```
    /// use aegis_model::{RiskDelta, ThreatId, ThreatRiskDelta};
    ///
    /// let t = ThreatId::new_v7();
    /// let d = ThreatRiskDelta::new(t, RiskDelta::new(None, None), RiskDelta::new(None, None));
    /// assert_eq!(d.threat(), t);
    /// ```
    #[must_use]
    pub const fn threat(&self) -> ThreatId {
        self.threat
    }

    /// Returns the inherent (unmitigated) [`RiskDelta`] of this threat
    /// scenario.
    ///
    /// Time O(1), space O(1).
    ///
    /// ```
    /// use aegis_model::{RiskChange, RiskDelta, ThreatId, ThreatRiskDelta};
    ///
    /// let t = ThreatId::new_v7();
    /// let d = ThreatRiskDelta::new(t, RiskDelta::new(None, None), RiskDelta::new(None, None));
    /// assert_eq!(d.inherent().change(), RiskChange::Unchanged);
    /// ```
    #[must_use]
    pub const fn inherent(&self) -> RiskDelta {
        self.inherent
    }

    /// Returns the residual (mitigated) [`RiskDelta`] of this threat scenario.
    ///
    /// Time O(1), space O(1).
    ///
    /// ```
    /// use aegis_model::{RiskChange, RiskDelta, ThreatId, ThreatRiskDelta};
    ///
    /// let t = ThreatId::new_v7();
    /// let d = ThreatRiskDelta::new(t, RiskDelta::new(None, None), RiskDelta::new(None, None));
    /// assert_eq!(d.residual().change(), RiskChange::Unchanged);
    /// ```
    #[must_use]
    pub const fn residual(&self) -> RiskDelta {
        self.residual
    }

    /// Returns `true` if either the inherent or residual [`RiskSnapshot`]
    /// (`feasibility`, `max_risk`, or evaluability) changed.
    ///
    /// Time O(1), space O(1).
    ///
    /// ```
    /// use aegis_model::{Methodology, RiskDelta, RiskSnapshot, ThreatId, ThreatRiskDelta};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Methodology::iso21434_r155()?;
    /// let low = m.feasibility_level("low").ok_or("no level")?;
    /// let snap = Some(RiskSnapshot::new(low, None));
    /// let t = ThreatId::new_v7();
    /// let d = ThreatRiskDelta::new(t, RiskDelta::new(None, snap), RiskDelta::new(None, snap));
    /// assert!(d.is_risk_changed());
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn is_risk_changed(&self) -> bool {
        self.inherent.is_changed() || self.residual.is_changed()
    }
}

/// Complete structural diff and change impact report between a baseline
/// [`Project`](crate::Project) and a current [`Project`](crate::Project),
/// returned by [`Project::diff`](crate::Project::diff) (rule A7).
///
/// ```
/// use std::sync::Arc;
/// use aegis_model::{Methodology, Project};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let m = Arc::new(Methodology::iso21434_r155()?);
/// let p = Project::new(m);
/// let diff = p.diff(&p)?;
/// assert!(diff.is_empty());
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProjectDiff {
    pub(crate) components: EntityDiff<ComponentId>,
    pub(crate) interfaces: EntityDiff<InterfaceId>,
    pub(crate) data_flows: EntityDiff<DataFlowId>,
    pub(crate) trust_boundaries: EntityDiff<TrustBoundaryId>,
    pub(crate) assets: EntityDiff<AssetId>,
    pub(crate) damage_scenarios: EntityDiff<DamageId>,
    pub(crate) threat_scenarios: EntityDiff<ThreatId>,
    pub(crate) attack_trees: EntityDiff<TreeId>,
    pub(crate) security_controls: EntityDiff<ControlId>,
    pub(crate) cybersecurity_goals: EntityDiff<GoalId>,
    pub(crate) cybersecurity_claims: EntityDiff<ClaimId>,
    pub(crate) risk_deltas: Box<[ThreatRiskDelta]>,
    pub(crate) notes_changed: bool,
}

impl ProjectDiff {
    /// Whether the project notes differ. Description changes of elements
    /// show as `modified` in the element's own list. Time O(1).
    ///
    /// ```
    /// use aegis_model::ProjectDiff;
    ///
    /// assert!(!ProjectDiff::default().notes_changed());
    /// ```
    #[must_use]
    pub fn notes_changed(&self) -> bool {
        self.notes_changed
    }

    /// Returns the added, removed, and modified [`ComponentId`]s in canonical
    /// order. Time O(1), space O(1).
    #[must_use]
    pub fn components(&self) -> &EntityDiff<ComponentId> {
        &self.components
    }

    /// Returns the added, removed, and modified [`InterfaceId`]s in canonical
    /// order. Time O(1), space O(1).
    #[must_use]
    pub fn interfaces(&self) -> &EntityDiff<InterfaceId> {
        &self.interfaces
    }

    /// Returns the added, removed, and modified [`DataFlowId`]s in canonical
    /// order. Time O(1), space O(1).
    #[must_use]
    pub fn data_flows(&self) -> &EntityDiff<DataFlowId> {
        &self.data_flows
    }

    /// Returns the added, removed, and modified [`TrustBoundaryId`]s in
    /// canonical order. Time O(1), space O(1).
    #[must_use]
    pub fn trust_boundaries(&self) -> &EntityDiff<TrustBoundaryId> {
        &self.trust_boundaries
    }

    /// Returns the added, removed, and modified [`AssetId`]s in canonical
    /// order.
    ///
    /// Time O(1), space O(1).
    ///
    /// ```
    /// use aegis_model::ProjectDiff;
    ///
    /// assert!(ProjectDiff::default().assets().is_empty());
    /// ```
    #[must_use]
    pub fn assets(&self) -> &EntityDiff<AssetId> {
        &self.assets
    }

    /// Returns the added, removed, and modified [`DamageId`]s in canonical
    /// order.
    ///
    /// Time O(1), space O(1).
    ///
    /// ```
    /// use aegis_model::ProjectDiff;
    ///
    /// assert!(ProjectDiff::default().damage_scenarios().is_empty());
    /// ```
    #[must_use]
    pub fn damage_scenarios(&self) -> &EntityDiff<DamageId> {
        &self.damage_scenarios
    }

    /// Returns the added, removed, and modified [`ThreatId`]s in canonical
    /// order.
    ///
    /// Time O(1), space O(1).
    ///
    /// ```
    /// use aegis_model::ProjectDiff;
    ///
    /// assert!(ProjectDiff::default().threat_scenarios().is_empty());
    /// ```
    #[must_use]
    pub fn threat_scenarios(&self) -> &EntityDiff<ThreatId> {
        &self.threat_scenarios
    }

    /// Returns the added, removed, and modified [`TreeId`]s in canonical
    /// order.
    ///
    /// Time O(1), space O(1).
    ///
    /// ```
    /// use aegis_model::ProjectDiff;
    ///
    /// assert!(ProjectDiff::default().attack_trees().is_empty());
    /// ```
    #[must_use]
    pub fn attack_trees(&self) -> &EntityDiff<TreeId> {
        &self.attack_trees
    }

    /// Returns the added, removed, and modified [`ControlId`]s in canonical
    /// order.
    ///
    /// Time O(1), space O(1).
    ///
    /// ```
    /// use aegis_model::ProjectDiff;
    ///
    /// assert!(ProjectDiff::default().security_controls().is_empty());
    /// ```
    #[must_use]
    pub fn security_controls(&self) -> &EntityDiff<ControlId> {
        &self.security_controls
    }

    /// Returns the added, removed, and modified [`GoalId`]s in canonical
    /// order.
    ///
    /// Time O(1), space O(1).
    ///
    /// ```
    /// use aegis_model::ProjectDiff;
    ///
    /// assert!(ProjectDiff::default().cybersecurity_goals().is_empty());
    /// ```
    #[must_use]
    pub fn cybersecurity_goals(&self) -> &EntityDiff<GoalId> {
        &self.cybersecurity_goals
    }

    /// Returns the added, removed, and modified [`ClaimId`]s in canonical
    /// order.
    ///
    /// Time O(1), space O(1).
    ///
    /// ```
    /// use aegis_model::ProjectDiff;
    ///
    /// assert!(ProjectDiff::default().cybersecurity_claims().is_empty());
    /// ```
    #[must_use]
    pub fn cybersecurity_claims(&self) -> &EntityDiff<ClaimId> {
        &self.cybersecurity_claims
    }

    /// Returns the [`ThreatRiskDelta`]s for all threat scenarios affected by
    /// the diff (added, removed, or modified threat scenarios, or threat
    /// scenarios referencing a modified asset, damage scenario, attack tree,
    /// or security control), in canonical [`ThreatId`] order (rule A7).
    ///
    /// Time O(1), space O(1).
    ///
    /// ```
    /// use aegis_model::ProjectDiff;
    ///
    /// assert!(ProjectDiff::default().risk_deltas().is_empty());
    /// ```
    #[must_use]
    pub fn risk_deltas(&self) -> &[ThreatRiskDelta] {
        &self.risk_deltas
    }

    /// Iterates over the subset of [`risk_deltas`](Self::risk_deltas) whose
    /// inherent or residual [`RiskSnapshot`] (`feasibility`, `max_risk`, or
    /// evaluability) changed.
    ///
    /// Time O(1) per item, space O(1).
    ///
    /// ```
    /// use aegis_model::ProjectDiff;
    ///
    /// assert_eq!(ProjectDiff::default().changed_risks().count(), 0);
    /// ```
    pub fn changed_risks(&self) -> impl Iterator<Item = &ThreatRiskDelta> {
        self.risk_deltas.iter().filter(|d| d.is_risk_changed())
    }

    /// Returns `true` if no entity of any kind was added, removed, or
    /// modified between the baseline and current projects.
    ///
    /// Time O(1), space O(1).
    ///
    /// ```
    /// use aegis_model::ProjectDiff;
    ///
    /// assert!(ProjectDiff::default().is_empty());
    /// ```
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.components.is_empty()
            && self.interfaces.is_empty()
            && self.data_flows.is_empty()
            && self.trust_boundaries.is_empty()
            && self.assets.is_empty()
            && self.damage_scenarios.is_empty()
            && self.threat_scenarios.is_empty()
            && self.attack_trees.is_empty()
            && self.security_controls.is_empty()
            && self.cybersecurity_goals.is_empty()
            && self.cybersecurity_claims.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::methodology::Methodology;

    #[test]
    fn risk_change_between_covers_all_transitions() {
        let m = Methodology::iso21434_r155().expect("builtin");
        let low = m.feasibility_level("low").expect("low");
        let high = m.feasibility_level("high").expect("high");
        let r2 = m.risk_level("r2").expect("r2");
        let r5 = m.risk_level("r5").expect("r5");

        let s_low_r2 = Some(RiskSnapshot::new(low, Some(r2)));
        let s_high_r5 = Some(RiskSnapshot::new(high, Some(r5)));
        let s_low_none = Some(RiskSnapshot::new(low, None));
        let s_high_none = Some(RiskSnapshot::new(high, None));

        assert_eq!(RiskChange::between(None, None), RiskChange::Unchanged);
        assert_eq!(
            RiskChange::between(None, s_low_r2),
            RiskChange::NewlyEvaluated
        );
        assert_eq!(
            RiskChange::between(s_low_r2, None),
            RiskChange::NoLongerEvaluated
        );
        assert_eq!(
            RiskChange::between(s_low_r2, s_high_r5),
            RiskChange::Increased
        );
        assert_eq!(
            RiskChange::between(s_high_r5, s_low_r2),
            RiskChange::Decreased
        );
        assert_eq!(
            RiskChange::between(s_low_r2, s_low_r2),
            RiskChange::Unchanged
        );
        assert_eq!(
            RiskChange::between(s_low_none, s_low_r2),
            RiskChange::NewlyEvaluated
        );
        assert_eq!(
            RiskChange::between(s_low_r2, s_low_none),
            RiskChange::NoLongerEvaluated
        );
        assert_eq!(
            RiskChange::between(s_low_none, s_high_none),
            RiskChange::Increased
        );
        assert_eq!(
            RiskChange::between(s_high_none, s_low_none),
            RiskChange::Decreased
        );
        assert_eq!(
            RiskChange::between(s_low_none, s_low_none),
            RiskChange::Unchanged
        );
    }
}
