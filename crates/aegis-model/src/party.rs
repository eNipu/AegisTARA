//! Parties: the project-level list of us, customers and suppliers.
//!
//! A party is a named organization or role that a goal or claim may name as
//! responsible. Parties change only through [`Op`](crate::Op)s on a
//! [`Project`](crate::Project); they carry no risk number.

use std::sync::Arc;

use crate::provenance::Provenance;

/// Maximum number of live parties in one project.
///
/// ```
/// assert_eq!(aegis_model::MAX_PARTIES, 256);
/// ```
pub const MAX_PARTIES: usize = 256;

/// Maximum number of parties a goal or claim may list as responsible.
pub const MAX_RESPONSIBLE: usize = 16;

/// The role a [`Party`] plays in a project's TARA.
///
/// ```
/// use aegis_model::PartyKind;
/// assert_eq!(PartyKind::Us.as_str(), "us");
/// assert_eq!(PartyKind::from_str("customer"), Some(PartyKind::Customer));
/// assert_eq!(PartyKind::from_str("nope"), None);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PartyKind {
    /// The organization performing the TARA.
    Us,
    /// The customer whose system is assessed.
    Customer,
    /// An external supplier of a component or service.
    Supplier,
    /// Any other named party.
    Other,
}

impl PartyKind {
    /// The lowercase key of this kind. Time O(1).
    ///
    /// ```
    /// assert_eq!(aegis_model::PartyKind::Supplier.as_str(), "supplier");
    /// ```
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Us => "us",
            Self::Customer => "customer",
            Self::Supplier => "supplier",
            Self::Other => "other",
        }
    }

    /// Finds the kind with this key. Time O(1).
    ///
    /// ```
    /// assert_eq!(aegis_model::PartyKind::from_str("us"), Some(aegis_model::PartyKind::Us));
    /// ```
    #[must_use]
    #[allow(clippy::should_implement_trait)] // inherent Option API; the trait would return Result
    pub fn from_str(key: &str) -> Option<Self> {
        match key {
            "us" => Some(Self::Us),
            "customer" => Some(Self::Customer),
            "supplier" => Some(Self::Supplier),
            "other" => Some(Self::Other),
            _ => None,
        }
    }
}

/// A party in a [`Project`](crate::Project), changed only through
/// [`Project::apply`](crate::Project::apply).
///
/// ```
/// use std::sync::Arc;
/// use aegis_model::{Methodology, Op, PartyId, PartyKind, Project, Provenance};
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let mut p = Project::new(Arc::new(Methodology::iso21434_r155()?));
/// let id = PartyId::new_v7();
/// let prov = Provenance::human("alice", 1_700_000_000_000);
/// p.apply(Op::AddParty {
///     id, name: "Bosch".into(), kind: PartyKind::Supplier, provenance: prov.clone(),
/// })?;
/// let x = p.party(p.party_idx(&id).ok_or("missing")?).ok_or("missing")?;
/// assert_eq!(x.name(), "Bosch");
/// assert_eq!(x.kind(), PartyKind::Supplier);
/// assert_eq!(x.provenance(), &prov);
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Party {
    pub(crate) name: Arc<str>,
    pub(crate) kind: PartyKind,
    pub(crate) provenance: Provenance,
}

impl Party {
    /// The party's name. Time O(1). See the [`Party`] example.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The party's role. Time O(1). See the [`Party`] example.
    #[must_use]
    pub fn kind(&self) -> PartyKind {
        self.kind
    }

    /// Provenance of the last change to this party. Time O(1).
    /// See the [`Party`] example.
    #[must_use]
    pub fn provenance(&self) -> &Provenance {
        &self.provenance
    }
}
