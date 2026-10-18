//! Typed identifiers: the internal arena index and the stable public ID.

use std::fmt;
use std::str::FromStr;

use uuid::{Uuid, Variant, Version};

use crate::error::ModelError;

/// Internal index of a node in an [`AttackTree`](crate::AttackTree) arena.
///
/// A `NodeIdx` is small (4 bytes) and cheap to copy. It is only meaningful
/// for the tree that handed it out. There is no public constructor: only
/// [`AttackTree`](crate::AttackTree) creates indices.
///
/// `NodeIdx` and [`NodeId`] are distinct types, so passing one where the other
/// is expected does not compile:
///
/// ```compile_fail
/// use aegis_model::{AttackTree, Node, NodeId};
///
/// # fn main() -> Result<(), aegis_model::ModelError> {
/// let id = NodeId::new_v7();
/// let tree = AttackTree::new(id, Node::step("root"))?;
/// let _ = tree.get(id); // error: expected `NodeIdx`, found `NodeId`
/// # Ok(())
/// # }
/// ```
///
/// The correct call takes the index:
///
/// ```
/// use aegis_model::{AttackTree, Node, NodeId};
///
/// # fn main() -> Result<(), aegis_model::ModelError> {
/// let tree = AttackTree::new(NodeId::new_v7(), Node::step("root"))?;
/// assert_eq!(tree.root().index(), 0);
/// assert!(tree.get(tree.root()).is_some());
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeIdx(u32);

impl NodeIdx {
    /// Returns the position of this node in the arena, for indexing slices.
    ///
    /// ```
    /// use aegis_model::{AttackTree, GateKind, Node, NodeId};
    ///
    /// # fn main() -> Result<(), aegis_model::ModelError> {
    /// let mut tree = AttackTree::new(NodeId::new_v7(), Node::gate(GateKind::And, "root"))?;
    /// let child = tree.add_child(tree.root(), NodeId::new_v7(), Node::step("a"))?;
    /// assert_eq!(child.index(), 1);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn index(self) -> usize {
        // A `u32` always fits in `usize` on the 32- and 64-bit targets we
        // support; widening never loses information.
        self.0 as usize
    }

    /// Converts an arena position into an index. This is the only place that
    /// turns a `usize` into a `NodeIdx`.
    ///
    /// Returns `None` if `i` does not fit in a `u32`.
    pub(crate) fn try_from_usize(i: usize) -> Option<Self> {
        u32::try_from(i).ok().map(NodeIdx)
    }
}

/// Stable public ID of a node: an RFC 9562 version 7 UUID.
///
/// Files, APIs, and history refer to nodes by `NodeId`. A `NodeId` can only be
/// built from a version 7 UUID, so holding one proves the value was checked
/// ("parse, don't validate").
///
/// ```
/// use aegis_model::NodeId;
///
/// # fn main() -> Result<(), aegis_model::ModelError> {
/// let id = NodeId::new_v7();
/// let text = id.to_string();
/// assert_eq!(text.parse::<NodeId>()?, id);
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(Uuid);

impl NodeId {
    /// Creates a fresh `UUIDv7` from the system clock and random bits.
    ///
    /// ```
    /// use aegis_model::NodeId;
    ///
    /// assert_ne!(NodeId::new_v7(), NodeId::new_v7());
    /// ```
    #[must_use]
    pub fn new_v7() -> Self {
        NodeId(Uuid::now_v7())
    }

    /// Wraps a UUID after checking that it is version 7 with the RFC 9562
    /// variant.
    ///
    /// # Errors
    ///
    /// Returns [`ModelError::NotVersion7`] for any other version or variant.
    ///
    /// ```
    /// use aegis_model::{ModelError, NodeId};
    /// use uuid::Uuid;
    ///
    /// let v4 = Uuid::parse_str("67e55044-10b1-426f-9247-bb680e5fe0c8").ok();
    /// assert_eq!(v4.map(NodeId::from_uuid), Some(Err(ModelError::NotVersion7)));
    /// assert!(NodeId::from_uuid(Uuid::now_v7()).is_ok());
    /// ```
    pub fn from_uuid(u: Uuid) -> Result<Self, ModelError> {
        if u.get_version() == Some(Version::SortRand) && u.get_variant() == Variant::RFC4122 {
            Ok(NodeId(u))
        } else {
            Err(ModelError::NotVersion7)
        }
    }

    /// Returns the underlying UUID.
    ///
    /// ```
    /// use aegis_model::NodeId;
    ///
    /// let id = NodeId::new_v7();
    /// assert_eq!(id.as_uuid().get_version_num(), 7);
    /// ```
    #[must_use]
    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl FromStr for NodeId {
    type Err = ModelError;

    /// Parses any textual UUID form accepted by [`Uuid::parse_str`], then
    /// checks that it is version 7.
    ///
    /// # Errors
    ///
    /// [`ModelError::InvalidUuid`] for malformed text,
    /// [`ModelError::NotVersion7`] for a valid UUID of another version.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let u = Uuid::parse_str(s).map_err(|_| ModelError::InvalidUuid)?;
        NodeId::from_uuid(u)
    }
}

impl fmt::Display for NodeId {
    /// Writes the hyphenated lowercase form, for example
    /// `0190a1b2-c3d4-7e5f-8a9b-0c1d2e3f4a5b`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0.hyphenated(), f)
    }
}

#[cfg(test)]
mod parse {
    use super::*;

    const V7: &str = "0190a1b2-c3d4-7e5f-8a9b-0c1d2e3f4a5b";
    const V4: &str = "67e55044-10b1-426f-9247-bb680e5fe0c8";

    #[test]
    fn from_uuid_accepts_v7() {
        let u = Uuid::now_v7();
        assert_eq!(NodeId::from_uuid(u).map(|id| *id.as_uuid()), Ok(u));
    }

    #[test]
    fn from_uuid_rejects_v4() {
        let u = Uuid::parse_str(V4).expect("valid v4 literal");
        assert_eq!(NodeId::from_uuid(u), Err(ModelError::NotVersion7));
    }

    #[test]
    fn from_uuid_rejects_nil_and_max() {
        assert_eq!(NodeId::from_uuid(Uuid::nil()), Err(ModelError::NotVersion7));
        assert_eq!(NodeId::from_uuid(Uuid::max()), Err(ModelError::NotVersion7));
    }

    #[test]
    fn from_uuid_rejects_v7_with_wrong_variant() {
        // Version nibble 7, but variant bits 110 (Microsoft) instead of 10.
        let u = Uuid::parse_str("0190a1b2-c3d4-7e5f-ca9b-0c1d2e3f4a5b").expect("valid literal");
        assert_eq!(u.get_version_num(), 7);
        assert_ne!(u.get_variant(), Variant::RFC4122);
        assert_eq!(NodeId::from_uuid(u), Err(ModelError::NotVersion7));
    }

    #[test]
    fn from_str_accepts_v7() {
        let id: NodeId = V7.parse().expect("valid v7");
        assert_eq!(id.as_uuid().get_version_num(), 7);
    }

    #[test]
    fn from_str_rejects_v4() {
        assert_eq!(V4.parse::<NodeId>(), Err(ModelError::NotVersion7));
    }

    #[test]
    fn from_str_rejects_malformed() {
        for bad in [
            "",
            "not a uuid",
            "0190a1b2-c3d4-7e5f-8a9b",
            "0190a1b2-c3d4-7e5f-8a9b-0c1d2e3f4a5z",
        ] {
            assert_eq!(
                bad.parse::<NodeId>(),
                Err(ModelError::InvalidUuid),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn display_is_hyphenated_lowercase() {
        let id: NodeId = V7.to_uppercase().parse().expect("valid v7");
        assert_eq!(id.to_string(), V7);
    }

    #[test]
    fn display_from_str_round_trips() {
        for _ in 0..100 {
            let id = NodeId::new_v7();
            assert_eq!(id.to_string().parse::<NodeId>(), Ok(id));
        }
    }

    #[test]
    fn new_v7_is_version_7() {
        let id = NodeId::new_v7();
        assert_eq!(id.as_uuid().get_version(), Some(Version::SortRand));
        assert_eq!(id.as_uuid().get_variant(), Variant::RFC4122);
    }
}

#[cfg(test)]
mod capacity {
    use super::*;

    #[test]
    fn try_from_usize_zero() {
        assert_eq!(NodeIdx::try_from_usize(0).map(NodeIdx::index), Some(0));
    }

    #[test]
    fn try_from_usize_accepts_u32_max() {
        let max = u32::MAX as usize;
        assert_eq!(NodeIdx::try_from_usize(max).map(NodeIdx::index), Some(max));
    }

    #[cfg(target_pointer_width = "64")]
    #[test]
    fn try_from_usize_rejects_u32_max_plus_one() {
        assert_eq!(NodeIdx::try_from_usize(u32::MAX as usize + 1), None);
        assert_eq!(NodeIdx::try_from_usize(usize::MAX), None);
    }
}
