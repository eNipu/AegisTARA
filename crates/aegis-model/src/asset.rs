//! Assets and sets of security properties.

use std::sync::Arc;

use crate::methodology::Property;
use crate::provenance::Provenance;

/// A set of security properties: one bit per [`Property`], 8 bytes.
///
/// A methodology declares at most 64 properties, so every property fits.
/// There is no raw-bits constructor: a set is built from properties that a
/// [`Methodology`](crate::Methodology) handed out. All operations are O(1)
/// except iteration, which is O(1) per member.
///
/// ```
/// use aegis_model::{Methodology, PropertySet};
///
/// # fn main() -> Result<(), aegis_model::PackError> {
/// let m = Methodology::iso21434_r155()?;
/// let set: PropertySet = m.properties().take(2).collect();
/// assert_eq!(set.len(), 2);
/// assert!(set.is_subset(m.all_properties()));
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PropertySet(u64);

/// The bit for `p`, or 0 for a position outside a `u64` (never handed out
/// by a methodology).
fn bit(p: Property) -> u64 {
    u32::try_from(p.index())
        .ok()
        .and_then(|i| 1u64.checked_shl(i))
        .unwrap_or(0)
}

impl PropertySet {
    /// The empty set. Time O(1).
    ///
    /// ```
    /// use aegis_model::PropertySet;
    ///
    /// assert!(PropertySet::empty().is_empty());
    /// ```
    #[must_use]
    pub const fn empty() -> Self {
        PropertySet(0)
    }

    /// Returns the set with `p` added. Time O(1).
    ///
    /// ```
    /// use aegis_model::{Methodology, PropertySet};
    ///
    /// # fn main() -> Result<(), aegis_model::PackError> {
    /// let m = Methodology::iso21434_r155()?;
    /// let Some(p) = m.property("integrity") else { return Ok(()) };
    /// let set = PropertySet::empty().with(p);
    /// assert!(set.contains(p));
    /// assert_eq!(set.with(p), set);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn with(self, p: Property) -> Self {
        PropertySet(self.0 | bit(p))
    }

    /// Returns `true` if `p` is in the set. Time O(1).
    ///
    /// ```
    /// use aegis_model::{Methodology, PropertySet};
    ///
    /// # fn main() -> Result<(), aegis_model::PackError> {
    /// let m = Methodology::iso21434_r155()?;
    /// let (Some(c), Some(i)) = (m.property("confidentiality"), m.property("integrity")) else {
    ///     return Ok(());
    /// };
    /// let set = PropertySet::empty().with(c);
    /// assert!(set.contains(c));
    /// assert!(!set.contains(i));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn contains(self, p: Property) -> bool {
        let b = bit(p);
        b != 0 && self.0 & b == b
    }

    /// Returns `true` if every member of `self` is in `other`. Time O(1).
    ///
    /// ```
    /// use aegis_model::{Methodology, PropertySet};
    ///
    /// # fn main() -> Result<(), aegis_model::PackError> {
    /// let m = Methodology::iso21434_r155()?;
    /// let some: PropertySet = m.properties().take(3).collect();
    /// assert!(some.is_subset(m.all_properties()));
    /// assert!(!m.all_properties().is_subset(some));
    /// assert!(PropertySet::empty().is_subset(some));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn is_subset(self, other: PropertySet) -> bool {
        self.0 & !other.0 == 0
    }

    /// Iterates the members in ascending index order. Time O(1) per
    /// member.
    ///
    /// ```
    /// use aegis_model::{Methodology, PropertySet};
    ///
    /// # fn main() -> Result<(), aegis_model::PackError> {
    /// let m = Methodology::iso21434_r155()?;
    /// let props: Vec<_> = m.properties().collect();
    /// let set: PropertySet = props.iter().rev().copied().collect();
    /// assert_eq!(set.iter().collect::<Vec<_>>(), props);
    /// # Ok(())
    /// # }
    /// ```
    pub fn iter(self) -> impl Iterator<Item = Property> {
        let mut rest = self.0;
        std::iter::from_fn(move || {
            if rest == 0 {
                return None;
            }
            let i = rest.trailing_zeros();
            rest &= rest - 1;
            u8::try_from(i).ok().map(Property::new)
        })
    }

    /// Returns the number of members. Time O(1).
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), aegis_model::PackError> {
    /// assert_eq!(Methodology::iso21434_r155()?.all_properties().len(), 6);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn len(self) -> usize {
        // At most 64, so the cast never truncates.
        self.0.count_ones() as usize
    }

    /// Returns `true` if the set has no members. Time O(1).
    ///
    /// ```
    /// use aegis_model::PropertySet;
    ///
    /// assert!(PropertySet::empty().is_empty());
    /// ```
    #[must_use]
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }
}

impl FromIterator<Property> for PropertySet {
    /// Collects properties into a set; repeats are ignored. Time O(items).
    fn from_iter<I: IntoIterator<Item = Property>>(iter: I) -> Self {
        iter.into_iter()
            .fold(PropertySet::empty(), PropertySet::with)
    }
}

/// An asset of a [`Project`](crate::Project): something worth protecting,
/// with the security properties it must keep.
///
/// A read-only view. The property set is non-empty and every member is
/// declared by the project's methodology; the project enforces both.
///
/// ```
/// use std::sync::Arc;
/// use aegis_model::{AssetId, Methodology, Op, Project, PropertySet, Provenance};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let m = Arc::new(Methodology::iso21434_r155()?);
/// let mut project = Project::new(Arc::clone(&m));
/// let id = AssetId::new_v7();
/// let properties: PropertySet = m.properties().collect();
/// project.apply(Op::AddAsset {
///     id,
///     name: "Gateway".into(),
///     properties,
///     provenance: Provenance::default(),
/// })?;
/// let asset = project.asset_idx(&id).and_then(|i| project.asset(i)).ok_or("missing")?;
/// assert_eq!(asset.name(), "Gateway");
/// assert_eq!(asset.properties(), properties);
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Asset {
    pub(crate) name: Arc<str>,
    pub(crate) properties: PropertySet,
    pub(crate) provenance: Provenance,
}

impl Asset {
    /// Returns the name. Time O(1).
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{AssetId, Methodology, Op, Project, Provenance};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let mut project = Project::new(Arc::clone(&m));
    /// let id = AssetId::new_v7();
    /// project.apply(Op::AddAsset {
    ///     id,
    ///     name: "ECU".into(),
    ///     properties: m.all_properties(),
    ///     provenance: Provenance::default(),
    /// })?;
    /// let (_, _, asset) = project.assets().next().ok_or("missing")?;
    /// assert_eq!(asset.name(), "ECU");
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the security properties. Time O(1).
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{AssetId, Methodology, Op, Project, Provenance};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let mut project = Project::new(Arc::clone(&m));
    /// let id = AssetId::new_v7();
    /// project.apply(Op::AddAsset {
    ///     id,
    ///     name: "ECU".into(),
    ///     properties: m.all_properties(),
    ///     provenance: Provenance::default(),
    /// })?;
    /// let (_, _, asset) = project.assets().next().ok_or("missing")?;
    /// assert_eq!(asset.properties().len(), 6);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn properties(&self) -> PropertySet {
        self.properties
    }

    /// Returns the provenance of the last creation or modification of this
    /// asset. Time O(1), space O(1).
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{AssetId, Methodology, Op, Project, Provenance};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let mut project = Project::new(Arc::clone(&m));
    /// let id = AssetId::new_v7();
    /// let prov = Provenance::human("alice", 1_700_000_000_000);
    /// project.apply(Op::AddAsset {
    ///     id,
    ///     name: "ECU".into(),
    ///     properties: m.all_properties(),
    ///     provenance: prov.clone(),
    /// })?;
    /// let (_, _, asset) = project.assets().next().ok_or("missing")?;
    /// assert_eq!(asset.provenance(), &prov);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn provenance(&self) -> &Provenance {
        &self.provenance
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(i: u8) -> Property {
        Property::new(i)
    }

    #[test]
    fn property_set_basics() {
        let s = PropertySet::empty();
        assert!(s.is_empty());
        assert_eq!(s.len(), 0);
        assert_eq!(s.iter().count(), 0);
        let s = s.with(p(3)).with(p(0)).with(p(63)).with(p(3));
        assert_eq!(s.len(), 3);
        assert!(!s.is_empty());
        assert!(s.contains(p(0)) && s.contains(p(3)) && s.contains(p(63)));
        assert!(!s.contains(p(1)));
        assert_eq!(s.iter().collect::<Vec<_>>(), [p(0), p(3), p(63)]);
    }

    #[test]
    fn property_set_subset() {
        let a: PropertySet = [p(1), p(2)].into_iter().collect();
        let b: PropertySet = [p(1), p(2), p(5)].into_iter().collect();
        assert!(a.is_subset(b));
        assert!(!b.is_subset(a));
        assert!(a.is_subset(a));
        assert!(PropertySet::empty().is_subset(PropertySet::empty()));
    }

    #[test]
    fn property_set_from_iter_ignores_repeats() {
        let s: PropertySet = [p(4), p(4), p(2)].into_iter().collect();
        assert_eq!(s.iter().collect::<Vec<_>>(), [p(2), p(4)]);
        assert_eq!(s, PropertySet::default().with(p(2)).with(p(4)));
    }

    #[test]
    fn property_set_ignores_positions_beyond_64() {
        let s = PropertySet::empty().with(p(64)).with(p(200));
        assert!(s.is_empty());
        assert!(!s.contains(p(64)));
    }

    #[test]
    fn property_set_size() {
        assert_eq!(std::mem::size_of::<PropertySet>(), 8);
    }
}
