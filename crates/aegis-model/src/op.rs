//! Typed operations on a [`Project`](crate::Project).

use std::sync::Arc;

use crate::asset::PropertySet;
use crate::id::{AssetId, DamageId, ThreatId};
use crate::methodology::{CatalogRef, ImpactCategory, Level, ThreatCategory};

/// One change to a [`Project`](crate::Project).
///
/// Entities are named by public ID; vocabulary by the typed indices of the
/// project's methodology. [`Project::apply`](crate::Project::apply) checks
/// the op, commits it, and returns its inverse, so undo is "apply the
/// inverse" and costs O(size of the op). Costs below use n = entities of
/// the kind, k = damage links or catalog refs on the threat, p <= 64
/// properties, c <= 32 impact categories.
///
/// | Op | Inverse | Cost |
/// |---|---|---|
/// | `AddAsset` | `RemoveAsset` | O(log n) |
/// | `RenameAsset` | `RenameAsset` with the old name | O(log n) |
/// | `SetAssetProperties` | the same with the old set | O(log n + p) |
/// | `RemoveAsset` | `AddAsset` with the old fields | O(log n + p) |
/// | `AddDamageScenario` | `RemoveDamageScenario` | O(log n + c) |
/// | `RenameDamageScenario` | the same with the old name | O(log n) |
/// | `SetImpact` | the same with the old level | O(log n) |
/// | `RemoveDamageScenario` | `AddDamageScenario`, rated pairs in category order | O(log n + c) |
/// | `AddThreatScenario` | `RemoveThreatScenario` | O(k log n + k log k + p) |
/// | `RenameThreatScenario` | the same with the old name | O(log n) |
/// | `SetThreatCategory` | the same with the old category | O(log n + p) |
/// | `AddCatalogRef` | `RemoveCatalogRef` | O(log n + k) |
/// | `RemoveCatalogRef` | `AddCatalogRef` | O(log n + k) |
/// | `LinkDamage` | `UnlinkDamage` | O(log n + k) |
/// | `UnlinkDamage` | `LinkDamage` | O(log n + k) |
/// | `RemoveThreatScenario` | `AddThreatScenario`, fields in stored order | O(log n + k + p) |
///
/// ```
/// use std::sync::Arc;
/// use aegis_model::{AssetId, Methodology, Op, Project};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let m = Arc::new(Methodology::iso21434_r155()?);
/// let mut project = Project::new(Arc::clone(&m));
/// let id = AssetId::new_v7();
/// let inverse = project.apply(Op::AddAsset {
///     id,
///     name: "Telematics unit".into(),
///     properties: m.all_properties(),
/// })?;
/// assert_eq!(inverse, Op::RemoveAsset { id });
/// project.apply(inverse)?;
/// assert_eq!(project, Project::new(m));
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Op {
    /// Adds an asset. The property set must be non-empty.
    AddAsset {
        /// Unused asset ID.
        id: AssetId,
        /// Name, non-empty after trimming, at most 256 chars.
        name: Arc<str>,
        /// Security properties of the asset.
        properties: PropertySet,
    },
    /// Renames an asset.
    RenameAsset {
        /// Existing asset.
        id: AssetId,
        /// New name.
        name: Arc<str>,
    },
    /// Replaces an asset's properties. Must not drop a property that a
    /// threat targets.
    SetAssetProperties {
        /// Existing asset.
        id: AssetId,
        /// New non-empty property set.
        properties: PropertySet,
    },
    /// Removes an asset that no threat targets.
    RemoveAsset {
        /// Existing asset.
        id: AssetId,
    },
    /// Adds a damage scenario. Categories not listed are unrated.
    AddDamageScenario {
        /// Unused damage scenario ID.
        id: DamageId,
        /// Name.
        name: Arc<str>,
        /// Ratings, each category at most once.
        impacts: Vec<(ImpactCategory, Level)>,
    },
    /// Renames a damage scenario.
    RenameDamageScenario {
        /// Existing damage scenario.
        id: DamageId,
        /// New name.
        name: Arc<str>,
    },
    /// Sets or clears (`None` = unrated) one impact rating.
    SetImpact {
        /// Existing damage scenario.
        id: DamageId,
        /// The impact category.
        category: ImpactCategory,
        /// The new rating, or `None` for unrated.
        level: Option<Level>,
    },
    /// Removes a damage scenario that no threat links to.
    RemoveDamageScenario {
        /// Existing damage scenario.
        id: DamageId,
    },
    /// Adds a threat scenario. The asset must carry every property the
    /// category violates.
    AddThreatScenario {
        /// Unused threat scenario ID.
        id: ThreatId,
        /// Name.
        name: Arc<str>,
        /// The targeted asset.
        asset: AssetId,
        /// The threat category.
        category: ThreatCategory,
        /// Catalog refs, unique, at most 16, in any order (stored sorted).
        catalog_refs: Vec<CatalogRef>,
        /// Linked damage scenarios, unique, at most 256, in any order
        /// (stored sorted by ID).
        damages: Vec<DamageId>,
    },
    /// Renames a threat scenario.
    RenameThreatScenario {
        /// Existing threat scenario.
        id: ThreatId,
        /// New name.
        name: Arc<str>,
    },
    /// Changes a threat's category. The asset must carry every property the
    /// new category violates.
    SetThreatCategory {
        /// Existing threat scenario.
        id: ThreatId,
        /// The new category.
        category: ThreatCategory,
    },
    /// Adds a catalog ref to a threat.
    AddCatalogRef {
        /// Existing threat scenario.
        threat: ThreatId,
        /// A ref not yet on the threat.
        catalog_ref: CatalogRef,
    },
    /// Removes a catalog ref from a threat.
    RemoveCatalogRef {
        /// Existing threat scenario.
        threat: ThreatId,
        /// A ref on the threat.
        catalog_ref: CatalogRef,
    },
    /// Links a threat to a damage scenario. A threat links at most 256.
    LinkDamage {
        /// Existing threat scenario.
        threat: ThreatId,
        /// Existing damage scenario, not yet linked.
        damage: DamageId,
    },
    /// Unlinks a threat from a damage scenario.
    UnlinkDamage {
        /// Existing threat scenario.
        threat: ThreatId,
        /// A linked damage scenario.
        damage: DamageId,
    },
    /// Removes a threat scenario.
    RemoveThreatScenario {
        /// Existing threat scenario.
        id: ThreatId,
    },
}
