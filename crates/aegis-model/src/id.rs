//! Typed identifiers: the internal arena index and the stable public ID.
//!
//! Every entity kind has a pair: a `u32` arena index (`NodeIdx`, `AssetIdx`,
//! ...) that is only meaningful inside one container, and a stable public ID
//! (`NodeId`, `AssetId`, ...) that is always a `UUIDv7`. The version check
//! lives in one place, [`check_v7`], and every ID type goes through it.

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
        check_v7(u).map(NodeId)
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
        parse_v7(s).map(NodeId)
    }
}

impl fmt::Display for NodeId {
    /// Writes the hyphenated lowercase form, for example
    /// `0190a1b2-c3d4-7e5f-8a9b-0c1d2e3f4a5b`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0.hyphenated(), f)
    }
}

/// Checks that `u` is a version 7 UUID with the RFC 9562 variant. This is
/// the only place the rule lives; every public ID type calls it. Time O(1).
fn check_v7(u: Uuid) -> Result<Uuid, ModelError> {
    if u.get_version() == Some(Version::SortRand) && u.get_variant() == Variant::RFC4122 {
        Ok(u)
    } else {
        Err(ModelError::NotVersion7)
    }
}

/// Parses UUID text and checks it with [`check_v7`]. Time O(1).
fn parse_v7(s: &str) -> Result<Uuid, ModelError> {
    let u = Uuid::parse_str(s).map_err(|_| ModelError::InvalidUuid)?;
    check_v7(u)
}

/// An arena index type: built from a `u32` by the crate only, read back as a
/// `usize` position. Implemented by the project arena indices.
pub(crate) trait ArenaIndex: Copy {
    /// Wraps a raw arena position.
    fn from_u32(i: u32) -> Self;
    /// Returns the arena position.
    fn position(self) -> usize;
}

/// Defines one entity's index and public ID pair. The index is a `u32`
/// newtype with no public constructor; the ID is a `UUIDv7` newtype checked
/// by [`check_v7`].
macro_rules! entity_ids {
    (
        $(#[$idx_meta:meta])*
        pub struct $idx:ident;
        index_example { $(#[$index_meta:meta])* }
        $(#[$id_meta:meta])*
        pub struct $id:ident;
    ) => {
        $(#[$idx_meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $idx(u32);

        impl $idx {
            /// Returns the position of this entity in its arena, for
            /// indexing slices. Time O(1).
            ///
            $(#[$index_meta])*
            #[must_use]
            pub fn index(self) -> usize {
                // A `u32` always fits in `usize` on the 32- and 64-bit
                // targets we support; widening never loses information.
                self.0 as usize
            }
        }

        impl ArenaIndex for $idx {
            fn from_u32(i: u32) -> Self {
                $idx(i)
            }
            fn position(self) -> usize {
                self.index()
            }
        }

        $(#[$id_meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $id(Uuid);

        impl $id {
            /// Creates a fresh `UUIDv7` from the system clock and random
            /// bits. Time O(1).
            ///
            /// ```
            #[doc = concat!("use aegis_model::", stringify!($id), ";")]
            ///
            #[doc = concat!("let a = ", stringify!($id), "::new_v7();")]
            #[doc = concat!("assert_ne!(a, ", stringify!($id), "::new_v7());")]
            /// assert_eq!(a.as_uuid().get_version_num(), 7);
            /// ```
            #[must_use]
            pub fn new_v7() -> Self {
                $id(Uuid::now_v7())
            }

            /// Wraps a UUID after checking that it is version 7 with the
            /// RFC 9562 variant. Time O(1).
            ///
            /// # Errors
            ///
            /// Returns [`ModelError::NotVersion7`] for any other version or
            /// variant.
            ///
            /// ```
            #[doc = concat!("use aegis_model::{ModelError, ", stringify!($id), "};")]
            /// use uuid::Uuid;
            ///
            /// let v4 = Uuid::parse_str("67e55044-10b1-426f-9247-bb680e5fe0c8").ok();
            #[doc = concat!("assert_eq!(v4.map(", stringify!($id), "::from_uuid), Some(Err(ModelError::NotVersion7)));")]
            #[doc = concat!("assert!(", stringify!($id), "::from_uuid(Uuid::now_v7()).is_ok());")]
            /// ```
            pub fn from_uuid(u: Uuid) -> Result<Self, ModelError> {
                check_v7(u).map($id)
            }

            /// Returns the underlying UUID. Time O(1).
            ///
            /// ```
            #[doc = concat!("use aegis_model::", stringify!($id), ";")]
            ///
            #[doc = concat!("let id = ", stringify!($id), "::new_v7();")]
            /// assert_eq!(id.as_uuid().get_version_num(), 7);
            /// ```
            #[must_use]
            pub fn as_uuid(&self) -> &Uuid {
                &self.0
            }
        }

        impl FromStr for $id {
            type Err = ModelError;

            /// Parses any textual UUID form accepted by
            /// [`Uuid::parse_str`], then checks that it is version 7.
            ///
            /// # Errors
            ///
            /// [`ModelError::InvalidUuid`] for malformed text,
            /// [`ModelError::NotVersion7`] for a valid UUID of another
            /// version.
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                parse_v7(s).map($id)
            }
        }

        impl fmt::Display for $id {
            /// Writes the hyphenated lowercase form.
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                fmt::Display::fmt(&self.0.hyphenated(), f)
            }
        }
    };
}

entity_ids! {
    /// Internal index of an asset in a [`Project`](crate::Project) arena.
    ///
    /// 4 bytes, `Copy`, and only meaningful for the project that handed it
    /// out. There is no public constructor.
    ///
    /// ```
    /// assert_eq!(std::mem::size_of::<aegis_model::AssetIdx>(), 4);
    /// ```
    pub struct AssetIdx;
    index_example {
        /// ```
        /// use std::sync::Arc;
        /// use aegis_model::{AssetId, Methodology, Op, Project, PropertySet};
        ///
        /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
        /// let m = Arc::new(Methodology::iso21434_r155()?);
        /// let mut project = Project::new(Arc::clone(&m));
        /// let id = AssetId::new_v7();
        /// let properties: PropertySet = m.properties().collect();
        /// project.apply(Op::AddAsset { id, name: "Gateway ECU".into(), properties })?;
        /// assert_eq!(project.asset_idx(&id).map(|i| i.index()), Some(0));
        /// # Ok(())
        /// # }
        /// ```
    }
    /// Stable public ID of an asset: an RFC 9562 version 7 UUID.
    ///
    /// Distinct from the other ID types, so an `AssetId` cannot be passed
    /// where a [`ThreatId`] is expected:
    ///
    /// ```compile_fail
    /// use std::sync::Arc;
    /// use aegis_model::{AssetId, Methodology, Project};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// let id = AssetId::new_v7();
    /// let _ = project.threat_scenario_idx(&id); // error: expected `&ThreatId`
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// The correct call uses the asset lookup:
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{AssetId, Methodology, Project};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// let id = AssetId::new_v7();
    /// assert_eq!(project.asset_idx(&id), None);
    /// assert_eq!(id.to_string().parse::<AssetId>()?, id);
    /// # Ok(())
    /// # }
    /// ```
    pub struct AssetId;
}

entity_ids! {
    /// Internal index of a damage scenario in a [`Project`](crate::Project)
    /// arena.
    ///
    /// 4 bytes, `Copy`, and only meaningful for the project that handed it
    /// out. There is no public constructor.
    ///
    /// ```
    /// assert_eq!(std::mem::size_of::<aegis_model::DamageIdx>(), 4);
    /// ```
    pub struct DamageIdx;
    index_example {
        /// ```
        /// use std::sync::Arc;
        /// use aegis_model::{DamageId, Methodology, Op, Project};
        ///
        /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
        /// let mut project = Project::new(Arc::new(Methodology::iso21434_r155()?));
        /// let id = DamageId::new_v7();
        /// project.apply(Op::AddDamageScenario { id, name: "Loss of braking".into(), impacts: vec![] })?;
        /// assert_eq!(project.damage_scenario_idx(&id).map(|i| i.index()), Some(0));
        /// # Ok(())
        /// # }
        /// ```
    }
    /// Stable public ID of a damage scenario: an RFC 9562 version 7 UUID.
    ///
    /// ```
    /// use aegis_model::DamageId;
    ///
    /// # fn main() -> Result<(), aegis_model::ModelError> {
    /// let id = DamageId::new_v7();
    /// assert_eq!(id.to_string().parse::<DamageId>()?, id);
    /// # Ok(())
    /// # }
    /// ```
    pub struct DamageId;
}

entity_ids! {
    /// Internal index of a threat scenario in a [`Project`](crate::Project)
    /// arena.
    ///
    /// 4 bytes, `Copy`, and only meaningful for the project that handed it
    /// out. There is no public constructor.
    ///
    /// ```
    /// assert_eq!(std::mem::size_of::<aegis_model::ThreatIdx>(), 4);
    /// ```
    pub struct ThreatIdx;
    index_example {
        /// ```
        /// use std::sync::Arc;
        /// use aegis_model::{AssetId, Methodology, Op, Project, PropertySet, ThreatId};
        ///
        /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
        /// let m = Arc::new(Methodology::iso21434_r155()?);
        /// let mut project = Project::new(Arc::clone(&m));
        /// let asset = AssetId::new_v7();
        /// let properties: PropertySet = m.properties().collect();
        /// project.apply(Op::AddAsset { id: asset, name: "ECU".into(), properties })?;
        /// let category = m.threat_categories().next().ok_or("no category")?;
        /// let id = ThreatId::new_v7();
        /// project.apply(Op::AddThreatScenario {
        ///     id, name: "Forge messages".into(), asset, category,
        ///     catalog_refs: vec![], damages: vec![],
        /// })?;
        /// assert_eq!(project.threat_scenario_idx(&id).map(|i| i.index()), Some(0));
        /// # Ok(())
        /// # }
        /// ```
    }
    /// Stable public ID of a threat scenario: an RFC 9562 version 7 UUID.
    ///
    /// ```
    /// use aegis_model::ThreatId;
    ///
    /// # fn main() -> Result<(), aegis_model::ModelError> {
    /// let id = ThreatId::new_v7();
    /// assert_eq!(id.to_string().parse::<ThreatId>()?, id);
    /// # Ok(())
    /// # }
    /// ```
    pub struct ThreatId;
}

entity_ids! {
    /// Internal index of an attack tree in a [`Project`](crate::Project)
    /// arena.
    ///
    /// 4 bytes, `Copy`, and only meaningful for the project that handed it
    /// out. There is no public constructor.
    ///
    /// ```
    /// assert_eq!(std::mem::size_of::<aegis_model::TreeIdx>(), 4);
    /// ```
    pub struct TreeIdx;
    index_example {
        /// ```
        /// use std::sync::Arc;
        /// use aegis_model::{AttackTree, Methodology, Node, NodeId, Op, Project, TreeId};
        ///
        /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
        /// let mut project = Project::new(Arc::new(Methodology::iso21434_r155()?));
        /// let id = TreeId::new_v7();
        /// let tree = AttackTree::new(NodeId::new_v7(), Node::step("Pick lock"))?;
        /// project.apply(Op::AddAttackTree { id, tree, step_feasibility: vec![] })?;
        /// assert_eq!(project.attack_tree_idx(&id).map(|i| i.index()), Some(0));
        /// # Ok(())
        /// # }
        /// ```
    }
    /// Stable public ID of an attack tree: an RFC 9562 version 7 UUID.
    ///
    /// ```
    /// use aegis_model::TreeId;
    ///
    /// # fn main() -> Result<(), aegis_model::ModelError> {
    /// let id = TreeId::new_v7();
    /// assert_eq!(id.to_string().parse::<TreeId>()?, id);
    /// # Ok(())
    /// # }
    /// ```
    pub struct TreeId;
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

#[cfg(test)]
mod entity_ids {
    use super::*;

    const V7: &str = "0190a1b2-c3d4-7e5f-8a9b-0c1d2e3f4a5b";
    const V4: &str = "67e55044-10b1-426f-9247-bb680e5fe0c8";

    /// Runs the same checks on one ID type.
    macro_rules! id_checks {
        ($id:ident) => {{
            let v4 = Uuid::parse_str(V4).expect("valid v4 literal");
            assert_eq!($id::from_uuid(v4), Err(ModelError::NotVersion7));
            assert_eq!($id::from_uuid(Uuid::nil()), Err(ModelError::NotVersion7));
            let wrong_variant =
                Uuid::parse_str("0190a1b2-c3d4-7e5f-ca9b-0c1d2e3f4a5b").expect("valid literal");
            assert_eq!($id::from_uuid(wrong_variant), Err(ModelError::NotVersion7));
            assert_eq!(V4.parse::<$id>(), Err(ModelError::NotVersion7));
            assert_eq!("nope".parse::<$id>(), Err(ModelError::InvalidUuid));
            let id: $id = V7.to_uppercase().parse().expect("valid v7");
            assert_eq!(id.to_string(), V7);
            for _ in 0..100 {
                let fresh = $id::new_v7();
                assert_eq!(fresh.as_uuid().get_version(), Some(Version::SortRand));
                assert_eq!(fresh.to_string().parse::<$id>(), Ok(fresh));
            }
        }};
    }

    #[test]
    fn ids_asset_accepts_only_v7_and_round_trips() {
        id_checks!(AssetId);
    }

    #[test]
    fn ids_damage_accepts_only_v7_and_round_trips() {
        id_checks!(DamageId);
    }

    #[test]
    fn ids_threat_accepts_only_v7_and_round_trips() {
        id_checks!(ThreatId);
    }

    #[test]
    fn ids_tree_accepts_only_v7_and_round_trips() {
        id_checks!(TreeId);
    }

    #[test]
    fn ids_node_id_unchanged() {
        id_checks!(NodeId);
    }

    #[test]
    fn ids_index_round_trips_through_arena_index() {
        assert_eq!(AssetIdx::from_u32(7).index(), 7);
        assert_eq!(DamageIdx::from_u32(0).position(), 0);
        assert_eq!(ThreatIdx::from_u32(u32::MAX).index(), u32::MAX as usize);
        assert_eq!(TreeIdx::from_u32(42).index(), 42);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const V7: &str = "0190a1b2-c3d4-7e5f-8a9b-0c1d2e3f4a5b";
    const V4: &str = "67e55044-10b1-426f-9247-bb680e5fe0c8";

    #[test]
    fn tree_id_and_idx_sizes_and_round_trips() {
        assert_eq!(std::mem::size_of::<TreeIdx>(), 4);
        assert_eq!(std::mem::size_of::<TreeId>(), 16);
        assert_eq!(TreeIdx::from_u32(0).index(), 0);
        assert_eq!(TreeIdx::from_u32(19).position(), 19);

        let v4 = Uuid::parse_str(V4).expect("valid v4 literal");
        assert_eq!(TreeId::from_uuid(v4), Err(ModelError::NotVersion7));
        assert_eq!(TreeId::from_uuid(Uuid::nil()), Err(ModelError::NotVersion7));
        assert_eq!(V4.parse::<TreeId>(), Err(ModelError::NotVersion7));
        assert_eq!("invalid".parse::<TreeId>(), Err(ModelError::InvalidUuid));

        let parsed: TreeId = V7.to_uppercase().parse().expect("valid v7");
        assert_eq!(parsed.to_string(), V7);
        assert_eq!(parsed.as_uuid().get_version_num(), 7);

        let fresh = TreeId::new_v7();
        assert_eq!(fresh.to_string().parse::<TreeId>(), Ok(fresh));
    }
}
