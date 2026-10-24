//! `Project::save` and `Project::load`: the `aegis-project` v1 snapshot.

use std::io::{BufReader, BufWriter, Read, Write};
use std::sync::Arc;

use serde::{Deserialize, Serialize, Serializer};

use super::MAX_PROJECT_FILE_BYTES;
use super::wire::{
    self, CappedReader, CatalogRefsByKey, FORMAT_VERSION, ImpactsByKey, KeyOrder, MethodologyView,
    NodesView, PropertyKeys, ProvenanceView, Required, RiskTreatmentWire, SnapshotWire,
    StepFactorsByNode, StepFeasibilityByNode, StepMitigationsByNode, Text,
};
use crate::error::StoreError;
use crate::id::{AssetId, ClaimId, ControlId, DamageId, GoalId, ThreatId, TreeId};
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
    #[serde(skip_serializing_if = "Trees::is_empty")]
    attack_trees: Trees<'a>,
    #[serde(skip_serializing_if = "Controls::is_empty")]
    security_controls: Controls<'a>,
    #[serde(skip_serializing_if = "Goals::is_empty")]
    cybersecurity_goals: Goals<'a>,
    #[serde(skip_serializing_if = "Claims::is_empty")]
    cybersecurity_claims: Claims<'a>,
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
struct Trees<'a>(Ctx<'a>);
struct Controls<'a>(&'a Project);
struct Goals<'a>(&'a Project);
struct Claims<'a>(&'a Project);

impl Trees<'_> {
    fn is_empty(&self) -> bool {
        self.0.p.attack_tree_count() == 0
    }
}

impl Controls<'_> {
    fn is_empty(&self) -> bool {
        self.0.security_control_count() == 0
    }
}

impl Goals<'_> {
    fn is_empty(&self) -> bool {
        self.0.cybersecurity_goal_count() == 0
    }
}

impl Claims<'_> {
    fn is_empty(&self) -> bool {
        self.0.cybersecurity_claim_count() == 0
    }
}

#[derive(Serialize)]
struct AssetView<'a> {
    id: Text<AssetId>,
    name: &'a str,
    properties: PropertyKeys<'a>,
    #[serde(skip_serializing_if = "Option::is_none")]
    provenance: Option<ProvenanceView<'a>>,
}

#[derive(Serialize)]
struct DamageView<'a> {
    id: Text<DamageId>,
    name: &'a str,
    impacts: ImpactsByKey<'a>,
    #[serde(skip_serializing_if = "Option::is_none")]
    provenance: Option<ProvenanceView<'a>>,
}

#[derive(Serialize)]
struct ThreatView<'a> {
    id: Text<ThreatId>,
    name: &'a str,
    asset: Required<Text<AssetId>>,
    category: Required<&'a str>,
    catalog_refs: CatalogRefsByKey<'a>,
    damages: LinkedDamages<'a>,
    #[serde(skip_serializing_if = "Option::is_none")]
    direct_feasibility: Option<Required<&'a str>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    attack_tree: Option<Required<Text<TreeId>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    treatment: Option<RiskTreatmentWire>,
    #[serde(skip_serializing_if = "Option::is_none")]
    goal: Option<Required<Text<GoalId>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    claim: Option<Required<Text<ClaimId>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    provenance: Option<ProvenanceView<'a>>,
}

#[derive(Serialize)]
struct TreeView<'a> {
    id: Text<TreeId>,
    nodes: NodesView<'a>,
    step_feasibility: StepFeasibilityByNode<'a>,
    #[serde(skip_serializing_if = "StepFactorsByNode::is_empty")]
    step_factors: StepFactorsByNode<'a>,
    #[serde(skip_serializing_if = "StepMitigationsByNode::is_empty")]
    step_mitigations: StepMitigationsByNode<'a>,
    #[serde(skip_serializing_if = "Option::is_none")]
    provenance: Option<ProvenanceView<'a>>,
}

#[derive(Serialize)]
struct ControlView<'a> {
    id: Text<ControlId>,
    name: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    provenance: Option<ProvenanceView<'a>>,
}

#[derive(Serialize)]
struct GoalView<'a> {
    id: Text<GoalId>,
    name: &'a str,
    controls: LinkedControls<'a>,
    #[serde(skip_serializing_if = "Option::is_none")]
    provenance: Option<ProvenanceView<'a>>,
}

#[derive(Serialize)]
struct ClaimView<'a> {
    id: Text<ClaimId>,
    name: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    provenance: Option<ProvenanceView<'a>>,
}

/// A threat's damage links as public IDs, in stored (ID) order.
struct LinkedDamages<'a> {
    p: &'a Project,
    links: &'a [crate::id::DamageIdx],
}

/// A goal's control links as public IDs, in stored (ID) order.
struct LinkedControls<'a> {
    p: &'a Project,
    links: &'a [crate::id::ControlIdx],
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
            provenance: ProvenanceView::of(a.provenance()),
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
            provenance: ProvenanceView::of(d.provenance()),
        }))
    }
}

impl Serialize for Threats<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Ctx { p, m, order } = self.0;
        s.collect_seq(p.threat_scenarios().map(|(_, id, t)| {
            ThreatView {
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
                direct_feasibility: t
                    .direct_feasibility()
                    .map(|l| Required(m.feasibility_level_key(l))),
                attack_tree: t
                    .attack_tree()
                    .map(|idx| Required(p.attack_tree_id(idx).map(Text))),
                treatment: t.treatment().map(RiskTreatmentWire::from),
                goal: t
                    .goal()
                    .map(|idx| Required(p.cybersecurity_goal_id(idx).map(Text))),
                claim: t
                    .claim()
                    .map(|idx| Required(p.cybersecurity_claim_id(idx).map(Text))),
                provenance: ProvenanceView::of(t.provenance()),
            }
        }))
    }
}

impl Serialize for Trees<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Ctx { p, order, .. } = self.0;
        s.collect_seq(p.attack_trees().map(|(tree_idx, id, tree)| {
            TreeView {
                id: Text(id),
                nodes: NodesView(tree),
                step_feasibility: StepFeasibilityByNode {
                    project: p,
                    tree_idx,
                    tree,
                },
                step_factors: StepFactorsByNode {
                    project: p,
                    order,
                    tree_idx,
                    tree,
                },
                step_mitigations: StepMitigationsByNode {
                    project: p,
                    tree_idx,
                    tree,
                },
                provenance: p
                    .attack_tree_provenance(tree_idx)
                    .and_then(ProvenanceView::of),
            }
        }))
    }
}

impl Serialize for Controls<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let p = self.0;
        s.collect_seq(p.security_controls().map(|(_, id, c)| ControlView {
            id: Text(id),
            name: c.name(),
            provenance: ProvenanceView::of(c.provenance()),
        }))
    }
}

impl Serialize for Goals<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let p = self.0;
        s.collect_seq(p.cybersecurity_goals().map(|(_, id, g)| GoalView {
            id: Text(id),
            name: g.name(),
            controls: LinkedControls {
                p,
                links: g.controls(),
            },
            provenance: ProvenanceView::of(g.provenance()),
        }))
    }
}

impl Serialize for Claims<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let p = self.0;
        s.collect_seq(p.cybersecurity_claims().map(|(_, id, cl)| ClaimView {
            id: Text(id),
            name: cl.name(),
            provenance: ProvenanceView::of(cl.provenance()),
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

impl Serialize for LinkedControls<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_seq(
            self.links
                .iter()
                .map(|&c| Required(self.p.security_control_id(c).map(Text))),
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
    /// if the project broke its own invariants (an index not in its
    /// methodology), which the model prevents.
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{AssetId, Methodology, Op, Project, Provenance};
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
    ///     provenance: Provenance::default(),
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
            attack_trees: Trees(ctx),
            security_controls: Controls(self),
            cybersecurity_goals: Goals(self),
            cybersecurity_claims: Claims(self),
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
    /// use aegis_model::{DamageId, Methodology, Op, Project, Provenance};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let mut project = Project::new(Arc::clone(&m));
    /// project.apply(Op::AddDamageScenario {
    ///     id: DamageId::new_v7(),
    ///     name: "Data leak".into(),
    ///     impacts: vec![],
    ///     provenance: Provenance::default(),
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
                provenance: wire::unwrap_provenance(a.provenance),
            })?;
        }
        for d in snapshot.damage_scenarios {
            project.apply(Op::AddDamageScenario {
                id: d.id.0,
                name: d.name.0,
                impacts: d.impacts.0,
                provenance: wire::unwrap_provenance(d.provenance),
            })?;
        }
        for c in snapshot.security_controls {
            project.apply(Op::AddSecurityControl {
                id: c.id.0,
                name: c.name.0,
                provenance: wire::unwrap_provenance(c.provenance),
            })?;
        }
        for g in snapshot.cybersecurity_goals {
            project.apply(Op::AddCybersecurityGoal {
                id: g.id.0,
                name: g.name.0,
                controls: g.controls.0.into_iter().map(|c| c.0).collect(),
                provenance: wire::unwrap_provenance(g.provenance),
            })?;
        }
        for cl in snapshot.cybersecurity_claims {
            project.apply(Op::AddCybersecurityClaim {
                id: cl.id.0,
                name: cl.name.0,
                provenance: wire::unwrap_provenance(cl.provenance),
            })?;
        }
        for tr in snapshot.attack_trees {
            let provenance = wire::unwrap_provenance(tr.provenance);
            let id = tr.id.0;
            project.apply(Op::AddAttackTree {
                id,
                tree: tr.nodes.0,
                step_feasibility: tr.step_feasibility.0,
                provenance: provenance.clone(),
            })?;
            for (node, factors) in tr.step_factors.0 {
                for (factor, level) in factors {
                    project.apply(Op::SetStepFactor {
                        tree: id,
                        node,
                        factor,
                        level: Some(level),
                        provenance: provenance.clone(),
                    })?;
                }
            }
            for (node, mitigations) in tr.step_mitigations.0 {
                for (control, feasibility) in mitigations {
                    project.apply(Op::SetStepMitigation {
                        tree: id,
                        node,
                        control,
                        feasibility: Some(feasibility),
                        provenance: provenance.clone(),
                    })?;
                }
            }
        }
        for t in snapshot.threat_scenarios {
            apply_snapshot_threat(&mut project, t)?;
        }
        Ok(project)
    }
}

fn apply_snapshot_threat(project: &mut Project, t: wire::ThreatWire) -> Result<(), StoreError> {
    let provenance = wire::unwrap_provenance(t.provenance);
    let id = t.id.0;
    let direct_feasibility = t.direct_feasibility.map(|w| w.0);
    let attack_tree = t.attack_tree.map(|w| w.0);
    let treatment = t.treatment.map(Into::into);
    let goal = t.goal.map(|w| w.0);
    let claim = t.claim.map(|w| w.0);
    project.apply(Op::AddThreatScenario {
        id,
        name: t.name.0,
        asset: t.asset.0,
        category: t.category.0,
        catalog_refs: t.catalog_refs.0.into_iter().map(|r| r.0).collect(),
        damages: t.damages.0.into_iter().map(|d| d.0).collect(),
        provenance: provenance.clone(),
    })?;
    if let Some(level) = direct_feasibility {
        project.apply(Op::SetThreatFeasibility {
            threat: id,
            level: Some(level),
            provenance: provenance.clone(),
        })?;
    }
    if let Some(tree) = attack_tree {
        project.apply(Op::SetThreatAttackTree {
            threat: id,
            tree: Some(tree),
            provenance: provenance.clone(),
        })?;
    }
    if let Some(treatment) = treatment {
        project.apply(Op::SetRiskTreatment {
            threat: id,
            treatment: Some(treatment),
            provenance: provenance.clone(),
        })?;
    }
    if let Some(goal) = goal {
        project.apply(Op::SetThreatGoal {
            threat: id,
            goal: Some(goal),
            provenance: provenance.clone(),
        })?;
    }
    if let Some(claim) = claim {
        project.apply(Op::SetThreatClaim {
            threat: id,
            claim: Some(claim),
            provenance,
        })?;
    }
    Ok(())
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
