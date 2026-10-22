//! Provenance metadata for domain elements and operations (D6).
//!
//! Every live element in a [`Project`](crate::Project) ([`Asset`](crate::Asset),
//! [`DamageScenario`](crate::DamageScenario),
//! [`ThreatScenario`](crate::ThreatScenario), and [`AttackTree`](crate::AttackTree))
//! records who created or last changed it ([`Provenance::actor`]), when the
//! change happened ([`Provenance::timestamp_ms`], Unix epoch milliseconds in
//! UTC), and where it came from ([`Provenance::source`]: [`ProvenanceSource`]).
//!
//! [`Provenance`] wraps `Option<Arc<...>>` so it is one pointer wide (8 bytes
//! on 64-bit targets), [`Provenance::default`] (`""`, `0`,
//! [`ProvenanceSource::Human`]) allocates nothing on the heap, and cloning a
//! [`Provenance`] across a batch of operations or into an element is O(1).

use std::fmt;
use std::sync::Arc;

/// Origin of an element creation or modification (D6).
///
/// ```
/// use aegis_model::ProvenanceSource;
///
/// assert_eq!(ProvenanceSource::default(), ProvenanceSource::Human);
/// assert_eq!(ProvenanceSource::Ai.as_str(), "ai");
/// assert_eq!(ProvenanceSource::Catalog.to_string(), "catalog");
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ProvenanceSource {
    /// Direct human authoring or editing.
    #[default]
    Human,
    /// Imported from an external file or tool.
    Import,
    /// Instantiated from a threat or mitigation catalog.
    Catalog,
    /// Proposed by an AI assistant and accepted into the project.
    Ai,
}

impl ProvenanceSource {
    /// Returns the canonical lowercase key for this source (`"human"`,
    /// `"import"`, `"catalog"`, or `"ai"`). Time O(1), space O(1).
    ///
    /// ```
    /// use aegis_model::ProvenanceSource;
    ///
    /// assert_eq!(ProvenanceSource::Human.as_str(), "human");
    /// assert_eq!(ProvenanceSource::Import.as_str(), "import");
    /// assert_eq!(ProvenanceSource::Catalog.as_str(), "catalog");
    /// assert_eq!(ProvenanceSource::Ai.as_str(), "ai");
    /// ```
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Human => "human",
            Self::Import => "import",
            Self::Catalog => "catalog",
            Self::Ai => "ai",
        }
    }
}

impl fmt::Display for ProvenanceSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct ProvenanceInner {
    actor: Arc<str>,
    timestamp_ms: u64,
    source: ProvenanceSource,
}

/// Who created or last changed an element, when, and from what source (D6).
///
/// Carried by every creating or mutating [`Op`](crate::Op) and stored on every
/// element in a [`Project`](crate::Project). When an operation modifies an
/// element, [`Project::apply`](crate::Project::apply) updates the element's
/// provenance and returns an inverse [`Op`](crate::Op) carrying the element's
/// previous provenance, so applying the inverse (`undo`) restores both the
/// domain state and the prior provenance.
///
/// [`Provenance::default`] represents an unrecorded human provenance (`actor`
/// `""`, `timestamp_ms` `0`, `source` [`ProvenanceSource::Human`]) and
/// performs no heap allocation. [`Provenance::new`] canonicalizes that exact
/// triple to the same representation so `PartialEq`, `Eq`, `Ord`, and `Hash`
/// depend only on `(actor, timestamp_ms, source)`.
///
/// ```
/// use aegis_model::{Provenance, ProvenanceSource};
///
/// let p = Provenance::ai("alice", 1_729_555_200_000);
/// assert_eq!(p.actor(), "alice");
/// assert_eq!(p.timestamp_ms(), 1_729_555_200_000);
/// assert_eq!(p.source(), ProvenanceSource::Ai);
/// assert!(!p.is_default());
///
/// assert_eq!(
///     Provenance::new("", 0, ProvenanceSource::Human),
///     Provenance::default(),
/// );
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Provenance(Option<Arc<ProvenanceInner>>);

#[cfg(target_pointer_width = "64")]
const _: [(); 8] = [(); std::mem::size_of::<Provenance>()];

impl Provenance {
    /// Creates a provenance record.
    ///
    /// If `actor` is empty, `timestamp_ms` is `0`, and `source` is
    /// [`ProvenanceSource::Human`], returns the zero-allocation default
    /// representation.
    ///
    /// Time O(1) when `actor` is already an `Arc<str>` (or O(|actor|) when
    /// converting from `&str` or `String`), space O(|actor|).
    ///
    /// ```
    /// use aegis_model::{Provenance, ProvenanceSource};
    ///
    /// let p = Provenance::new("importer-v1", 1_700_000_000_000, ProvenanceSource::Import);
    /// assert_eq!(p.actor(), "importer-v1");
    /// assert_eq!(p.timestamp_ms(), 1_700_000_000_000);
    /// assert_eq!(p.source(), ProvenanceSource::Import);
    /// ```
    #[must_use]
    pub fn new(actor: impl Into<Arc<str>>, timestamp_ms: u64, source: ProvenanceSource) -> Self {
        let actor = actor.into();
        if actor.is_empty() && timestamp_ms == 0 && source == ProvenanceSource::Human {
            Self(None)
        } else {
            Self(Some(Arc::new(ProvenanceInner {
                actor,
                timestamp_ms,
                source,
            })))
        }
    }

    /// Creates a [`ProvenanceSource::Human`] provenance record.
    ///
    /// Time O(1) when `actor` is an `Arc<str>` (or O(|actor|) from `&str`),
    /// space O(|actor|).
    ///
    /// ```
    /// use aegis_model::{Provenance, ProvenanceSource};
    ///
    /// let p = Provenance::human("alice", 1_700_000_000_000);
    /// assert_eq!(p.source(), ProvenanceSource::Human);
    /// assert_eq!(p.actor(), "alice");
    /// ```
    #[must_use]
    pub fn human(actor: impl Into<Arc<str>>, timestamp_ms: u64) -> Self {
        Self::new(actor, timestamp_ms, ProvenanceSource::Human)
    }

    /// Creates a [`ProvenanceSource::Import`] provenance record.
    ///
    /// Time O(1) when `actor` is an `Arc<str>` (or O(|actor|) from `&str`),
    /// space O(|actor|).
    ///
    /// ```
    /// use aegis_model::{Provenance, ProvenanceSource};
    ///
    /// let p = Provenance::import("csv-import", 1_700_000_000_000);
    /// assert_eq!(p.source(), ProvenanceSource::Import);
    /// assert_eq!(p.actor(), "csv-import");
    /// ```
    #[must_use]
    pub fn import(actor: impl Into<Arc<str>>, timestamp_ms: u64) -> Self {
        Self::new(actor, timestamp_ms, ProvenanceSource::Import)
    }

    /// Creates a [`ProvenanceSource::Catalog`] provenance record.
    ///
    /// Time O(1) when `actor` is an `Arc<str>` (or O(|actor|) from `&str`),
    /// space O(|actor|).
    ///
    /// ```
    /// use aegis_model::{Provenance, ProvenanceSource};
    ///
    /// let p = Provenance::catalog("capec", 1_700_000_000_000);
    /// assert_eq!(p.source(), ProvenanceSource::Catalog);
    /// assert_eq!(p.actor(), "capec");
    /// ```
    #[must_use]
    pub fn catalog(actor: impl Into<Arc<str>>, timestamp_ms: u64) -> Self {
        Self::new(actor, timestamp_ms, ProvenanceSource::Catalog)
    }

    /// Creates a [`ProvenanceSource::Ai`] provenance record.
    ///
    /// Time O(1) when `actor` is an `Arc<str>` (or O(|actor|) from `&str`),
    /// space O(|actor|).
    ///
    /// ```
    /// use aegis_model::{Provenance, ProvenanceSource};
    ///
    /// let p = Provenance::ai("alice", 1_700_000_000_000);
    /// assert_eq!(p.source(), ProvenanceSource::Ai);
    /// assert_eq!(p.actor(), "alice");
    /// ```
    #[must_use]
    pub fn ai(actor: impl Into<Arc<str>>, timestamp_ms: u64) -> Self {
        Self::new(actor, timestamp_ms, ProvenanceSource::Ai)
    }

    /// Returns the identifier of who created or changed the element (`""`
    /// when unrecorded in [`Provenance::default`]). Time O(1), space O(1).
    ///
    /// ```
    /// use aegis_model::Provenance;
    ///
    /// assert_eq!(Provenance::default().actor(), "");
    /// assert_eq!(Provenance::human("bob", 42).actor(), "bob");
    /// ```
    #[must_use]
    pub fn actor(&self) -> &str {
        match &self.0 {
            Some(inner) => &inner.actor,
            None => "",
        }
    }

    /// Returns the UTC timestamp in milliseconds since the Unix epoch (`0`
    /// when unrecorded in [`Provenance::default`]). Time O(1), space O(1).
    ///
    /// ```
    /// use aegis_model::Provenance;
    ///
    /// assert_eq!(Provenance::default().timestamp_ms(), 0);
    /// assert_eq!(Provenance::human("bob", 1_234).timestamp_ms(), 1_234);
    /// ```
    #[must_use]
    pub fn timestamp_ms(&self) -> u64 {
        match &self.0 {
            Some(inner) => inner.timestamp_ms,
            None => 0,
        }
    }

    /// Returns the origin source of the change. Time O(1), space O(1).
    ///
    /// ```
    /// use aegis_model::{Provenance, ProvenanceSource};
    ///
    /// assert_eq!(Provenance::default().source(), ProvenanceSource::Human);
    /// assert_eq!(Provenance::ai("bob", 1).source(), ProvenanceSource::Ai);
    /// ```
    #[must_use]
    pub fn source(&self) -> ProvenanceSource {
        match &self.0 {
            Some(inner) => inner.source,
            None => ProvenanceSource::Human,
        }
    }

    /// Returns `true` if this is the default unrecorded human provenance
    /// (`actor` `""`, `timestamp_ms` `0`, `source`
    /// [`ProvenanceSource::Human`]). Time O(1), space O(1).
    ///
    /// ```
    /// use aegis_model::{Provenance, ProvenanceSource};
    ///
    /// assert!(Provenance::default().is_default());
    /// assert!(Provenance::new("", 0, ProvenanceSource::Human).is_default());
    /// assert!(!Provenance::new("", 1, ProvenanceSource::Human).is_default());
    /// assert!(!Provenance::new("", 0, ProvenanceSource::Ai).is_default());
    /// ```
    #[must_use]
    pub const fn is_default(&self) -> bool {
        self.0.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_default_and_accessors() {
        let d = Provenance::default();
        assert!(d.is_default());
        assert_eq!(d.actor(), "");
        assert_eq!(d.timestamp_ms(), 0);
        assert_eq!(d.source(), ProvenanceSource::Human);
        assert_eq!(Provenance::new("", 0, ProvenanceSource::Human), d);
        assert_eq!(Provenance::human("", 0), d);

        let non_zero_time = Provenance::human("", 1);
        assert!(!non_zero_time.is_default());
        assert_eq!(non_zero_time.actor(), "");
        assert_eq!(non_zero_time.timestamp_ms(), 1);
        assert_eq!(non_zero_time.source(), ProvenanceSource::Human);

        let non_human = Provenance::ai("", 0);
        assert!(!non_human.is_default());
        assert_eq!(non_human.actor(), "");
        assert_eq!(non_human.timestamp_ms(), 0);
        assert_eq!(non_human.source(), ProvenanceSource::Ai);
    }

    #[test]
    fn ordering_matches_tuple_order() {
        let samples = [
            Provenance::default(),
            Provenance::new("", 0, ProvenanceSource::Import),
            Provenance::new("", 0, ProvenanceSource::Catalog),
            Provenance::new("", 0, ProvenanceSource::Ai),
            Provenance::new("", 1, ProvenanceSource::Human),
            Provenance::human("alice", 0),
            Provenance::human("alice", 10),
            Provenance::ai("alice", 10),
            Provenance::import("bob", 5),
            Provenance::catalog("cat", 100),
        ];
        for a in &samples {
            for b in &samples {
                let tuple_a = (a.actor(), a.timestamp_ms(), a.source());
                let tuple_b = (b.actor(), b.timestamp_ms(), b.source());
                assert_eq!(a.cmp(b), tuple_a.cmp(&tuple_b));
            }
        }
    }
}
