//! Threat scenarios: how an asset's properties can be violated.

use std::sync::Arc;

use crate::id::{AssetIdx, DamageIdx};
use crate::methodology::{CatalogRef, ThreatCategory};

/// Most catalog refs on one threat scenario.
pub const MAX_CATALOG_REFS: usize = 16;

/// A threat scenario of a [`Project`](crate::Project).
///
/// A read-only view. It targets one asset, has one threat category (the
/// targeted properties are derived from it through
/// [`Project::targets`](crate::Project::targets), so they can never
/// disagree), cites 0 to 16 catalog refs, and links to damage scenarios.
/// `catalog_refs` is sorted and unique; `damages` is sorted by the damage
/// scenario's public ID and unique. Neither order depends on edit history.
///
/// ```
/// use std::sync::Arc;
/// use aegis_model::{AssetId, Methodology, Op, Project, ThreatId};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let m = Arc::new(Methodology::iso21434_r155()?);
/// let mut project = Project::new(Arc::clone(&m));
/// let asset = AssetId::new_v7();
/// project.apply(Op::AddAsset { id: asset, name: "ECU".into(), properties: m.all_properties() })?;
/// let category = m.threat_category("tampering").ok_or("no category")?;
/// project.apply(Op::AddThreatScenario {
///     id: ThreatId::new_v7(),
///     name: "Flash malicious firmware".into(),
///     asset,
///     category,
///     catalog_refs: vec![],
///     damages: vec![],
/// })?;
/// let (_, _, t) = project.threat_scenarios().next().ok_or("missing")?;
/// assert_eq!(t.category(), category);
/// assert_eq!(Some(t.asset()), project.asset_idx(&asset));
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThreatScenario {
    pub(crate) name: Arc<str>,
    pub(crate) asset: AssetIdx,
    pub(crate) category: ThreatCategory,
    /// Sorted, unique, at most [`MAX_CATALOG_REFS`].
    pub(crate) catalog_refs: Vec<CatalogRef>,
    /// Sorted by the target's `DamageId`, unique.
    pub(crate) damages: Vec<DamageIdx>,
}

impl ThreatScenario {
    /// Returns the name. Time O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{AssetId, Methodology, Op, Project, ThreatId};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let m = Arc::new(Methodology::iso21434_r155()?);
    /// # let mut project = Project::new(Arc::clone(&m));
    /// # let asset = AssetId::new_v7();
    /// # project.apply(Op::AddAsset { id: asset, name: "ECU".into(), properties: m.all_properties() })?;
    /// # let category = m.threat_categories().next().ok_or("none")?;
    /// project.apply(Op::AddThreatScenario {
    ///     id: ThreatId::new_v7(), name: "Replay".into(), asset, category,
    ///     catalog_refs: vec![], damages: vec![],
    /// })?;
    /// let (_, _, t) = project.threat_scenarios().next().ok_or("missing")?;
    /// assert_eq!(t.name(), "Replay");
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the targeted asset. Time O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{AssetId, Methodology, Op, Project, ThreatId};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let m = Arc::new(Methodology::iso21434_r155()?);
    /// # let mut project = Project::new(Arc::clone(&m));
    /// let asset = AssetId::new_v7();
    /// project.apply(Op::AddAsset { id: asset, name: "ECU".into(), properties: m.all_properties() })?;
    /// # let category = m.threat_categories().next().ok_or("none")?;
    /// project.apply(Op::AddThreatScenario {
    ///     id: ThreatId::new_v7(), name: "Replay".into(), asset, category,
    ///     catalog_refs: vec![], damages: vec![],
    /// })?;
    /// let (_, _, t) = project.threat_scenarios().next().ok_or("missing")?;
    /// assert_eq!(project.asset_id(t.asset()), Some(asset));
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn asset(&self) -> AssetIdx {
        self.asset
    }

    /// Returns the threat category. Time O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{AssetId, Methodology, Op, Project, ThreatId};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let m = Arc::new(Methodology::iso21434_r155()?);
    /// # let mut project = Project::new(Arc::clone(&m));
    /// # let asset = AssetId::new_v7();
    /// # project.apply(Op::AddAsset { id: asset, name: "ECU".into(), properties: m.all_properties() })?;
    /// let category = m.threat_category("spoofing").ok_or("none")?;
    /// project.apply(Op::AddThreatScenario {
    ///     id: ThreatId::new_v7(), name: "Fake key fob".into(), asset, category,
    ///     catalog_refs: vec![], damages: vec![],
    /// })?;
    /// let (_, _, t) = project.threat_scenarios().next().ok_or("missing")?;
    /// assert_eq!(t.category(), category);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn category(&self) -> ThreatCategory {
        self.category
    }

    /// Returns the catalog refs, sorted and unique. Time O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{AssetId, Methodology, Op, Project, ThreatId};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let m = Arc::new(Methodology::iso21434_r155()?);
    /// # let mut project = Project::new(Arc::clone(&m));
    /// # let asset = AssetId::new_v7();
    /// # project.apply(Op::AddAsset { id: asset, name: "ECU".into(), properties: m.all_properties() })?;
    /// # let category = m.threat_categories().next().ok_or("none")?;
    /// let scheme = m.catalog_scheme("r155-annex5").ok_or("none")?;
    /// let (a, b) = (m.parse_catalog_ref(scheme, "5")?, m.parse_catalog_ref(scheme, "4.1")?);
    /// project.apply(Op::AddThreatScenario {
    ///     id: ThreatId::new_v7(), name: "Replay".into(), asset, category,
    ///     catalog_refs: vec![a, b], damages: vec![],
    /// })?;
    /// let (_, _, t) = project.threat_scenarios().next().ok_or("missing")?;
    /// assert_eq!(t.catalog_refs(), [b, a]);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn catalog_refs(&self) -> &[CatalogRef] {
        &self.catalog_refs
    }

    /// Returns the linked damage scenarios, sorted by public ID and unique.
    /// Time O(1).
    ///
    /// ```
    /// # use std::sync::Arc;
    /// # use aegis_model::{AssetId, DamageId, Methodology, Op, Project, ThreatId};
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let m = Arc::new(Methodology::iso21434_r155()?);
    /// # let mut project = Project::new(Arc::clone(&m));
    /// # let asset = AssetId::new_v7();
    /// # project.apply(Op::AddAsset { id: asset, name: "ECU".into(), properties: m.all_properties() })?;
    /// # let category = m.threat_categories().next().ok_or("none")?;
    /// let damage = DamageId::new_v7();
    /// project.apply(Op::AddDamageScenario { id: damage, name: "Crash".into(), impacts: vec![] })?;
    /// project.apply(Op::AddThreatScenario {
    ///     id: ThreatId::new_v7(), name: "Replay".into(), asset, category,
    ///     catalog_refs: vec![], damages: vec![damage],
    /// })?;
    /// let (_, _, t) = project.threat_scenarios().next().ok_or("missing")?;
    /// assert_eq!(t.damages(), [project.damage_scenario_idx(&damage).ok_or("missing")?]);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn damages(&self) -> &[DamageIdx] {
        &self.damages
    }
}
