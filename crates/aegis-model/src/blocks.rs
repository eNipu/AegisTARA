//! Reusable building blocks shared by attack steps.

use std::sync::Arc;

use crate::item::ItemIdx;
use crate::methodology::{AttackType, FactorLevel, FeasibilityFactor, FeasibilityLevel};
use crate::provenance::Provenance;

/// A tree's immutable purpose, chosen when it is added to a project.
/// Legacy trees without a stored role are threat trees.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TreeRole {
    /// A tree that realizes a threat scenario.
    #[default]
    Threat,
    /// A reusable technical attack tree.
    Fragment,
}

impl TreeRole {
    /// The stable visual ID prefix for this role. Time O(1).
    #[must_use]
    pub const fn visual_prefix(self) -> crate::VisualPrefix {
        match self {
            Self::Threat => crate::VisualPrefix::Tree,
            Self::Fragment => crate::VisualPrefix::Fragment,
        }
    }

    // By reference because serde's `skip_serializing_if` passes `&T`.
    #[allow(clippy::trivially_copy_pass_by_ref)]
    pub(crate) fn is_threat(&self) -> bool {
        *self == Self::Threat
    }
}

/// Maximum populated fragment source nodes per tree.
pub const MAX_FRAGMENT_REFERENCES: usize = 256;

/// Maximum dependency edges in a fragment path, including a Threat entry edge.
pub const MAX_FRAGMENT_DEPTH: usize = 8;

/// Maximum number of shared steps in one project.
pub const MAX_SHARED_STEPS: usize = 4_096;

/// A reusable attack step in a [`Project`](crate::Project), changed only
/// through [`Project::apply`](crate::Project::apply).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SharedStep {
    pub(crate) name: Arc<str>,
    pub(crate) feasibility: Option<FeasibilityLevel>,
    pub(crate) factors: Box<[Option<FactorLevel>]>,
    pub(crate) via: Option<ItemIdx>,
    pub(crate) attack_type: Option<AttackType>,
    pub(crate) provenance: Provenance,
}

impl SharedStep {
    pub(crate) fn new(name: Arc<str>, provenance: Provenance) -> Self {
        Self {
            name,
            feasibility: None,
            factors: Box::default(),
            via: None,
            attack_type: None,
            provenance,
        }
    }

    /// The shared step's name. Time O(1).
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The direct feasibility rating, or `None` when unrated. Time O(1).
    #[must_use]
    pub const fn feasibility(&self) -> Option<FeasibilityLevel> {
        self.feasibility
    }

    /// The rating of one attack-potential factor, or `None` when unrated.
    /// Time O(1).
    #[must_use]
    pub fn factor(&self, factor: FeasibilityFactor) -> Option<FactorLevel> {
        self.factors.get(factor.index()).copied().flatten()
    }

    /// All factor ratings in methodology factor order. Time O(1).
    #[must_use]
    pub fn factor_slice(&self) -> &[Option<FactorLevel>] {
        &self.factors
    }

    /// The component or interface this step goes through, if any. Time O(1).
    /// Use [`Project::shared_step_via`](crate::Project::shared_step_via)
    /// to resolve this crate-internal slot to a public [`ItemRef`](crate::ItemRef).
    #[must_use]
    pub(crate) const fn via(&self) -> Option<ItemIdx> {
        self.via
    }

    /// The pack-defined attack type, if any. Time O(1).
    #[must_use]
    pub const fn attack_type(&self) -> Option<AttackType> {
        self.attack_type
    }

    /// Provenance of the last change to this shared step. Time O(1).
    #[must_use]
    pub const fn provenance(&self) -> &Provenance {
        &self.provenance
    }

    /// Whether any rating, factor, through item, or attack type is set.
    #[must_use]
    pub fn is_rated(&self) -> bool {
        self.feasibility.is_some()
            || self.factors.iter().any(Option::is_some)
            || self.via.is_some()
            || self.attack_type.is_some()
    }
}
