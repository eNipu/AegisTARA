//! Applicability conditions: the project-level list of states and events that
//! element links may name as needed.
//!
//! A condition is a named, described boolean fact. Conditions change only
//! through [`Op`](crate::Op)s on a [`Project`](crate::Project); they carry no
//! risk number and have no links in this slice.

use std::sync::Arc;

use crate::provenance::Provenance;

/// Maximum number of live conditions in one project.
///
/// ```
/// assert_eq!(aegis_model::MAX_CONDITIONS, 256);
/// ```
pub const MAX_CONDITIONS: usize = 256;

/// Maximum number of conditions one element or step may need (job 133).
pub const MAX_CONDITIONS_PER_ELEMENT: usize = 16;

/// A condition in a [`Project`](crate::Project), changed only through
/// [`Project::apply`](crate::Project::apply).
///
/// ```
/// use std::sync::Arc;
/// use aegis_model::{ConditionId, Methodology, Op, Project, Provenance};
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let mut p = Project::new(Arc::new(Methodology::iso21434_r155()?));
/// let id = ConditionId::new_v7();
/// let prov = Provenance::human("alice", 1_700_000_000_000);
/// p.apply(Op::AddCondition {
///     id,
///     name: "Airbag deploys".into(),
///     description: "The airbag deploys in a frontal crash".into(),
///     present: true,
///     provenance: prov.clone(),
/// })?;
/// let c = p.condition(p.condition_idx(&id).ok_or("missing")?).ok_or("missing")?;
/// assert_eq!(c.name(), "Airbag deploys");
/// assert_eq!(c.description(), "The airbag deploys in a frontal crash");
/// assert!(c.present());
/// assert_eq!(c.provenance(), &prov);
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Condition {
    pub(crate) name: Arc<str>,
    pub(crate) description: Arc<str>,
    pub(crate) present: bool,
    pub(crate) provenance: Provenance,
}

impl Condition {
    /// The condition's name. Time O(1). See the [`Condition`] example.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The condition's plain-text description. Time O(1).
    /// See the [`Condition`] example.
    #[must_use]
    pub fn description(&self) -> &str {
        &self.description
    }

    /// Whether the condition is present in the project. Time O(1).
    /// See the [`Condition`] example.
    #[must_use]
    pub fn present(&self) -> bool {
        self.present
    }

    /// Provenance of the last change to this condition. Time O(1).
    /// See the [`Condition`] example.
    #[must_use]
    pub fn provenance(&self) -> &Provenance {
        &self.provenance
    }
}
