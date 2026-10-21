//! `Project::save` and `Project::load`: the `aegis-project` v1 snapshot.

use std::io::{BufWriter, Read, Write};
use std::sync::Arc;

use serde::{Serialize, Serializer};

use super::MAX_PROJECT_FILE_BYTES;
use super::wire::{
    self, CatalogRefsByKey, FORMAT_VERSION, ImpactsByKey, KeyOrder, MethodologyView, PropertyKeys,
    Required, SnapshotWire, Text,
};
use crate::error::StoreError;
use crate::id::{AssetId, DamageId, ThreatId};
use crate::methodology::Methodology;
use crate::op::Op;
use crate::project::Project;
use crate::store::Limit;

/// The whole snapshot, borrowed from the project.
#[derive(Serialize)]
struct SnapshotView<'a> {
    format: &'static str,
    version: u32,
    methodology: MethodologyView<'a>,
    assets: Assets<'a>,
    damage_scenarios: Damages<'a>,
    threat_scenarios: Threats<'a>,
}

/// What every entity view needs.
#[derive(Clone, Copy)]
struct Ctx<'a> {
    p: &'a Project,
    m: &'a Methodology,
    order: &'a KeyOrder,
}

struct Assets<'a>(Ctx<'a>);
struct Damages<'a>(Ctx<'a>);
struct Threats<'a>(Ctx<'a>);

#[derive(Serialize)]
struct AssetView<'a> {
    id: Text<AssetId>,
    name: &'a str,
    properties: PropertyKeys<'a>,
}

#[derive(Serialize)]
struct DamageView<'a> {
    id: Text<DamageId>,
    name: &'a str,
    impacts: ImpactsByKey<'a>,
}

#[derive(Serialize)]
struct ThreatView<'a> {
    id: Text<ThreatId>,
    name: &'a str,
    asset: Required<Text<AssetId>>,
    category: Required<&'a str>,
    catalog_refs: CatalogRefsByKey<'a>,
    damages: LinkedDamages<'a>,
}

/// A threat's damage links as public IDs, in stored (ID) order.
struct LinkedDamages<'a> {
    p: &'a Project,
    links: &'a [crate::id::DamageIdx],
}

impl Serialize for Assets<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Ctx { p, m, order } = self.0;
        s.collect_seq(p.assets().map(|(_, id, a)| AssetView {
            id: Text(id),
            name: a.name(),
            properties: PropertyKeys {
                m,
                order,
                set: a.properties(),
            },
        }))
    }
}

impl Serialize for Damages<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Ctx { p, m, order } = self.0;
        s.collect_seq(p.damage_scenarios().map(|(_, id, d)| DamageView {
            id: Text(id),
            name: d.name(),
            impacts: ImpactsByKey {
                m,
                order,
                damage: d,
            },
        }))
    }
}

impl Serialize for Threats<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Ctx { p, m, order } = self.0;
        s.collect_seq(p.threat_scenarios().map(|(_, id, t)| ThreatView {
            id: Text(id),
            name: t.name(),
            asset: Required(p.asset_id(t.asset()).map(Text)),
            category: Required(m.threat_category_key(t.category())),
            catalog_refs: CatalogRefsByKey {
                m,
                order,
                refs: t.catalog_refs(),
            },
            damages: LinkedDamages {
                p,
                links: t.damages(),
            },
        }))
    }
}

impl Serialize for LinkedDamages<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_seq(
            self.links
                .iter()
                .map(|&d| Required(self.p.damage_scenario_id(d).map(Text))),
        )
    }
}

/// Reads all of `r`, refusing more than [`MAX_PROJECT_FILE_BYTES`]. Reads
/// at most one byte past the cap. Time and space O(bytes).
fn read_capped<R: Read>(r: R) -> Result<Vec<u8>, StoreError> {
    let mut bytes = Vec::new();
    r.take(MAX_PROJECT_FILE_BYTES.saturating_add(1))
        .read_to_end(&mut bytes)?;
    if wire::to_u64(bytes.len()) > MAX_PROJECT_FILE_BYTES {
        return Err(StoreError::LimitExceeded(Limit::PROJECT_FILE));
    }
    Ok(bytes)
}

impl Project {
    /// Writes the project as an `aegis-project` v1 snapshot: one line of
    /// compact JSON ending in `\n` (format in
    /// `docs/adr/0009-save-load-format.md`).
    ///
    /// The output holds only live entities, in public-ID order, with every
    /// vocabulary item written as its key and every list in canonical
    /// order, so two equal projects give identical bytes whatever their
    /// edit history or their methodology's declaration order (P4). Writes
    /// go through an internal `BufWriter`, which is flushed before return.
    ///
    /// Time O(n + L log `L_max` + m log m), with n live entities, L damage
    /// links plus catalog refs, `L_max` the longest list on one threat, and m
    /// the methodology size. Extra space O(m) for the key-sorted
    /// vocabulary order; the project is not copied.
    ///
    /// # Errors
    ///
    /// [`StoreError::Io`] if writing fails. [`StoreError::UnencodableOp`]
    /// only if the project broke its own invariants (an index not in its
    /// methodology), which the model prevents.
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{AssetId, Methodology, Op, Project};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let mut project = Project::new(Arc::clone(&m));
    /// let id = AssetId::new_v7();
    /// let integrity = m.property("integrity").ok_or("no property")?;
    /// project.apply(Op::AddAsset {
    ///     id,
    ///     name: "Brake ECU".into(),
    ///     properties: [integrity].into_iter().collect(),
    /// })?;
    ///
    /// let mut bytes = Vec::new();
    /// project.save(&mut bytes)?;
    /// let text = String::from_utf8(bytes)?;
    /// assert!(text.starts_with(r#"{"format":"aegis-project","version":1,"#));
    /// assert!(text.contains(r#""properties":["integrity"]"#));
    /// assert!(text.ends_with("}\n"));
    /// # Ok(())
    /// # }
    /// ```
    pub fn save<W: Write>(&self, w: W) -> Result<(), StoreError> {
        let m = &**self.methodology();
        let order = KeyOrder::new(m);
        let ctx = Ctx {
            p: self,
            m,
            order: &order,
        };
        let view = SnapshotView {
            format: "aegis-project",
            version: FORMAT_VERSION,
            methodology: MethodologyView::of(m),
            assets: Assets(ctx),
            damage_scenarios: Damages(ctx),
            threat_scenarios: Threats(ctx),
        };
        let mut out = BufWriter::new(w);
        serde_json::to_writer(&mut out, &view).map_err(wire::encode_error)?;
        out.write_all(b"\n")?;
        out.flush()?;
        Ok(())
    }

    /// Reads an `aegis-project` v1 snapshot written by
    /// [`save`](Self::save) and rebuilds the project over `methodology`.
    ///
    /// Decoding is capped before any model check: at most
    /// [`MAX_PROJECT_FILE_BYTES`] of input, and every string and list
    /// within its cap ([`Limit`]). The file's methodology key must equal
    /// `methodology.key()`; its methodology version may differ, since the
    /// file holds keys, not indices, and a key the methodology no longer
    /// has fails with [`StoreError::UnknownKey`]. The project is then built
    /// only through [`Project::apply`], with `AddAsset`, then
    /// `AddDamageScenario`, then `AddThreatScenario` ops, so every model
    /// rule is checked exactly as for an edit.
    ///
    /// Time O(B + n log n + L log n), with B input bytes, n entities, and L
    /// damage links plus catalog refs. Extra space O(B): the input is read
    /// into memory, decoded, and dropped before the project is built.
    ///
    /// # Errors
    ///
    /// [`StoreError::Io`] if reading fails;
    /// [`StoreError::LimitExceeded`] if a cap breaks;
    /// [`StoreError::Decode`] for bad JSON or a wrong shape;
    /// [`StoreError::UnsupportedFormat`], [`StoreError::MethodologyMismatch`],
    /// [`StoreError::UnknownKey`], [`StoreError::InvalidId`],
    /// [`StoreError::InvalidCatalogRef`], [`StoreError::DuplicateEntry`];
    /// and [`StoreError::Model`] if the content breaks a model rule (for
    /// example a threat that names a missing asset).
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{DamageId, Methodology, Op, Project};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let mut project = Project::new(Arc::clone(&m));
    /// project.apply(Op::AddDamageScenario {
    ///     id: DamageId::new_v7(),
    ///     name: "Data leak".into(),
    ///     impacts: vec![],
    /// })?;
    /// let mut bytes = Vec::new();
    /// project.save(&mut bytes)?;
    /// let loaded = Project::load(bytes.as_slice(), m)?;
    /// assert_eq!(loaded, project);
    /// # Ok(())
    /// # }
    /// ```
    pub fn load<R: Read>(r: R, methodology: Arc<Methodology>) -> Result<Project, StoreError> {
        let bytes = read_capped(r)?;
        let snapshot: SnapshotWire = wire::from_slice(&bytes, None)?;
        drop(bytes);
        wire::check_methodology(&snapshot.methodology, &methodology)?;
        let shared = Arc::clone(&methodology);
        let m = &*shared;
        let mut project = Project::new(methodology);
        for a in snapshot.assets {
            let properties = wire::resolve_properties(m, &a.properties)?;
            project.apply(Op::AddAsset {
                id: a.id.0,
                name: a.name.0,
                properties,
            })?;
        }
        for d in snapshot.damage_scenarios {
            let impacts = wire::resolve_impacts(m, &d.impacts)?;
            project.apply(Op::AddDamageScenario {
                id: d.id.0,
                name: d.name.0,
                impacts,
            })?;
        }
        for t in snapshot.threat_scenarios {
            let category = wire::resolve_threat_category(m, &t.category)?;
            let catalog_refs = wire::resolve_catalog_refs(m, &t.catalog_refs.0)?;
            project.apply(Op::AddThreatScenario {
                id: t.id.0,
                name: t.name.0,
                asset: t.asset.0,
                category,
                catalog_refs,
                damages: t.damages.0.into_iter().map(|d| d.0).collect(),
            })?;
        }
        Ok(project)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit;

    #[test]
    fn read_capped_allows_exactly_the_cap() {
        let at = std::io::repeat(b' ').take(MAX_PROJECT_FILE_BYTES);
        assert_eq!(
            read_capped(at).map(|b| wire::to_u64(b.len())).ok(),
            Some(MAX_PROJECT_FILE_BYTES)
        );
        let over = std::io::repeat(b' ').take(MAX_PROJECT_FILE_BYTES + 1);
        assert!(matches!(
            read_capped(over),
            Err(StoreError::LimitExceeded(Limit::ProjectFileBytes { .. }))
        ));
    }

    #[test]
    fn empty_project_round_trips() {
        let m = testkit::second();
        let p = Project::new(Arc::clone(&m));
        let mut bytes = Vec::new();
        assert!(p.save(&mut bytes).is_ok());
        assert_eq!(
            String::from_utf8_lossy(&bytes),
            "{\"format\":\"aegis-project\",\"version\":1,\"methodology\":{\"key\":\"med-device-demo\",\"version\":\"2026.1\"},\"assets\":[],\"damage_scenarios\":[],\"threat_scenarios\":[]}\n"
        );
        assert_eq!(Project::load(bytes.as_slice(), m).ok(), Some(p));
    }
}
