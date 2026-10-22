//! `Project::save` and `Project::load`: the `aegis-project` v1 snapshot.

use std::io::{BufReader, BufWriter, Read, Write};
use std::sync::Arc;

use serde::{Deserialize, Serialize, Serializer};

use super::MAX_PROJECT_FILE_BYTES;
use super::wire::{
    self, CappedReader, CatalogRefsByKey, FORMAT_VERSION, ImpactsByKey, KeyOrder, MethodologyView,
    PropertyKeys, Required, SnapshotWire, Text,
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

/// Size of the read buffer between the input and the parser.
const READ_BUFFER_BYTES: usize = 64 * 1024;

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
    /// if the project contains attack trees (which v1 snapshots do not yet
    /// encode) or if the project broke its own invariants (an index not in
    /// its methodology), which the model prevents.
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
        if self.attack_tree_count() > 0
            || self
                .threat_scenarios()
                .any(|(_, _, t)| t.attack_tree().is_some())
        {
            return Err(StoreError::UnencodableOp);
        }
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
    /// The input is streamed through a 64 KiB buffer, never read whole.
    /// Decoding is capped before any model check: at most
    /// [`MAX_PROJECT_FILE_BYTES`] of input, and every string and list
    /// within its cap ([`Limit`]). The file's methodology key must equal
    /// `methodology.key()`, checked as soon as it is read (before any
    /// vocabulary key when `methodology` comes before the entity lists, as
    /// `save` writes it; else a foreign file may fail with
    /// [`StoreError::UnknownKey`] instead); its methodology
    /// version may differ, since the file holds keys, not indices. Every
    /// vocabulary key is resolved to its index the moment it is read, so a
    /// key the methodology does not have fails with
    /// [`StoreError::UnknownKey`] inside the entity that holds it, and no
    /// key string is kept. The project is then built only through
    /// [`Project::apply`], with `AddAsset`, then `AddDamageScenario`, then
    /// `AddThreatScenario` ops, so every model rule is checked exactly as
    /// for an edit.
    ///
    /// The snapshot is lenient about JSON whitespace between tokens and
    /// about a missing final `\n` (the op log is not); a re-save is always
    /// canonical.
    ///
    /// Time O(B + n log n + L log n), with B input bytes, n entities, and L
    /// damage links plus catalog refs. Extra space O(n + L + the longest
    /// string) besides the project: the decoded entities hold names and
    /// 1-byte indices only. Measured heap, as a factor of the file size
    /// (`benches/store_memory.rs`, ADRs 0009 and 0010), as peak during the
    /// load and kept (the returned project): 1.03x peak and 0.83x kept for
    /// the typical project; 0.02x peak when keys are unknown, since the
    /// load fails inside the first entity. The worst case is a file of
    /// minimal entities of one kind, and it is mostly the loaded project,
    /// not the decoder (the decoder adds at most about 0.9x). With the
    /// built-in methodology it peaks at 2.73x (about 175 MiB at the 64 MiB
    /// cap) and keeps at most 1.85x. With a methodology at every vocabulary
    /// limit (64 properties, 32 impact categories) it peaks at 3.53x (about
    /// 226 MiB) and keeps at most 2.65x, both from damage scenarios' one
    /// rating slot per impact category. Kept stays within 3x for every
    /// shape measured.
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
        let mut input = CappedReader::new(r, MAX_PROJECT_FILE_BYTES);
        let decoded = wire::decode(&methodology, None, || {
            let buffered = BufReader::with_capacity(READ_BUFFER_BYTES, &mut input);
            let mut de = serde_json::Deserializer::from_reader(buffered);
            let snapshot = SnapshotWire::deserialize(&mut de)?;
            de.end()?;
            Ok(snapshot)
        });
        if input.exceeded() {
            return Err(StoreError::LimitExceeded(Limit::PROJECT_FILE));
        }
        let snapshot = decoded?;
        let mut project = Project::new(methodology);
        for a in snapshot.assets {
            project.apply(Op::AddAsset {
                id: a.id.0,
                name: a.name.0,
                properties: a.properties.0,
            })?;
        }
        for d in snapshot.damage_scenarios {
            project.apply(Op::AddDamageScenario {
                id: d.id.0,
                name: d.name.0,
                impacts: d.impacts.0,
            })?;
        }
        for t in snapshot.threat_scenarios {
            project.apply(Op::AddThreatScenario {
                id: t.id.0,
                name: t.name.0,
                asset: t.asset.0,
                category: t.category.0,
                catalog_refs: t.catalog_refs.0.into_iter().map(|r| r.0).collect(),
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
    fn capped_reader_allows_exactly_the_cap() {
        let mut sink = std::io::sink();
        let cap = MAX_PROJECT_FILE_BYTES;
        let mut at = CappedReader::new(std::io::repeat(b' ').take(cap), cap);
        assert_eq!(std::io::copy(&mut at, &mut sink).ok(), Some(cap));
        assert!(!at.exceeded());
        let mut over = CappedReader::new(std::io::repeat(b' '), cap);
        assert!(std::io::copy(&mut over, &mut sink).is_err());
        assert!(over.exceeded());
        assert_eq!(over.bytes_read(), cap + 1);
    }

    #[test]
    fn load_reports_io_errors_as_io() {
        struct Broken;
        impl Read for Broken {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("disk gone"))
            }
        }
        assert!(matches!(
            Project::load(Broken, testkit::second()),
            Err(StoreError::Io(_))
        ));
    }
    #[test]
    fn empty_project_round_trips() {
        let m = testkit::second();
        let p = Project::new(Arc::clone(&m));
        let mut bytes = Vec::new();
        assert!(p.save(&mut bytes).is_ok());
        // The test methodology is named only through the test kit, so no
        // `src` file names it (job 012, AC6).
        let (key, version) = (m.key(), m.version());
        assert_eq!(
            String::from_utf8_lossy(&bytes),
            format!(
                "{{\"format\":\"aegis-project\",\"version\":1,\"methodology\":{{\"key\":\"{key}\",\"version\":\"{version}\"}},\"assets\":[],\"damage_scenarios\":[],\"threat_scenarios\":[]}}\n"
            )
        );
        assert_eq!(Project::load(bytes.as_slice(), m).ok(), Some(p));
    }
}
