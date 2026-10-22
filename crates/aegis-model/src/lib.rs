//! Domain model for `AegisTARA`.
//!
//! This crate holds two parts of a TARA: attack trees, and the
//! methodology-neutral core of assets, damage scenarios, and threat
//! scenarios (see [Methodologies](#methodologies) below).
//!
//! An attack tree lives in an arena: nodes live in one `Vec` and
//! refer to each other by a typed index, [`NodeIdx`]. Every node also has a
//! stable public ID, [`NodeId`] (a `UUIDv7`), that files, APIs, and history use.
//! [`AttackTree`] keeps one map from [`NodeId`] to [`NodeIdx`], so lookups by
//! public ID cost O(1) expected time.
//!
//! A subtree can be shared between gates with [`AttackTree::link`], which
//! turns the tree into a directed acyclic graph (DAG). `link` rejects an edge
//! that would close a cycle ([`ModelError::WouldCreateCycle`]), so the model
//! is always a DAG. [`AttackTree::topological_order`] orders all nodes
//! parents-first in O(V + E) for evaluation, and re-checks for cycles as
//! defense in depth. A node has one [`owner`](AttackTree::owner) and may have
//! more [`parents`](AttackTree::parents). Errors name nodes by their stable
//! [`NodeId`], never by label.
//!
//! [`evaluate`] computes a value for every node in one O(V + E) pass over
//! that order, in an [`AttributeDomain`] chosen by the caller (how OR, AND,
//! and sequential AND combine child values), with the leaf values supplied
//! by the caller. Each node is evaluated once, also when it is shared. The
//! result, an [`Evaluation`], is derived data kept apart from the tree.
//! [`MaxMin`] (feasibility levels) and [`MinTime`] are built in.
//!
//! Every value explains itself: the evaluation records a [`Derivation`] per
//! node (a caller-supplied leaf, one [`Selected`](Derivation::Selected)
//! deciding child, or several children [`Combined`](Derivation::Combined)).
//! [`Evaluation::explain`] returns a node's rule, inputs, and deciding
//! child, and [`Evaluation::why_path`] follows the deciding children from
//! the root down to the step that dominates the result.
//!
//! See `docs/adr/0001-arena-and-typed-ids.md` for why the model is built this
//! way, `docs/adr/0002-dag-links-and-cycle-detection.md` for links and
//! cycle detection, `docs/adr/0005-tree-evaluation.md` for evaluation, and
//! `docs/adr/0006-explain-why-traces.md` for explain-why traces.
//!
//! # Example
//!
//! ```
//! use aegis_model::{AttackTree, GateKind, Node, NodeId};
//!
//! # fn main() -> Result<(), aegis_model::ModelError> {
//! let root_id = NodeId::new_v7();
//! let mut tree = AttackTree::new(root_id, Node::gate(GateKind::Or, "Steal car"))?;
//! let step = tree.add_child(tree.root(), NodeId::new_v7(), Node::step("Relay key fob"))?;
//!
//! assert_eq!(tree.len(), 2);
//! assert_eq!(tree.owner(step), Some(tree.root()));
//! assert_eq!(tree.idx_of(&root_id), Some(tree.root()));
//! # Ok(())
//! # }
//! ```

//!
//! # Methodologies
//!
//! The TARA side of the model (assets, damage scenarios, threat scenarios,
//! and project attack trees) is methodology-neutral: the core names no
//! impact category, rating scale, feasibility scale, security property,
//! threat taxonomy, or catalog. A [`MethodologyDef`] is
//! plain data that declares them, and [`Methodology::new`] validates it.
//! A methodology ships as a pack file (format `aegis-methodology-pack`
//! v1, JSON): [`Methodology::read_pack`] decodes one with capped, strict
//! decoding ([`PackError`]) and validates it through the same `new`, and
//! [`MethodologyDef::write_pack`] writes one. ISO/SAE 21434 + UN R155 ships
//! as the first methodology, [`Methodology::iso21434_r155`], a pack
//! embedded in the binary. A second methodology is one more pack file, not
//! a code change. See `docs/adr/0011-methodology-pack-files.md`.
//!
//! ```
//! use aegis_model::{Methodology, MethodologyDef};
//!
//! # fn main() -> Result<(), aegis_model::PackError> {
//! let pack = r#"{"format": "aegis-methodology-pack", "version": 1, "methodology": {
//!   "key": "tiny", "version": "1", "label": "Tiny",
//!   "impact_categories": [{"key": "harm", "label": "Harm", "levels": [
//!     {"key": "low", "label": "Low"}, {"key": "high", "label": "High"}]}],
//!   "feasibility_levels": [{"key": "low", "label": "Low"}, {"key": "high", "label": "High"}],
//!   "properties": [{"key": "integrity", "label": "Integrity"}],
//!   "threat_categories": [{"key": "corruption", "label": "Corruption",
//!                          "violates": ["integrity"]}],
//!   "catalog_schemes": []}}"#;
//! let m = Methodology::read_pack(pack.as_bytes())?;
//! assert_eq!(m.key(), "tiny");
//!
//! // Packs round-trip; the writer's output is canonical.
//! let def = MethodologyDef::read_pack(pack.as_bytes())?;
//! let mut canonical = Vec::new();
//! def.write_pack(&mut canonical)?;
//! assert_eq!(Methodology::read_pack(canonical.as_slice())?, m);
//! # Ok(())
//! # }
//! ```
//!
//! A [`Project`] is created over one `Arc<Methodology>`, fixed for its
//! lifetime. Every change is a typed [`Op`]; [`Project::apply`] checks it
//! against the methodology and the referential rules, commits it only if
//! every check passes, and returns the inverse op for undo. Ops carry small
//! typed vocabulary indices ([`ImpactCategory`], [`Level`],
//! [`FeasibilityLevel`], [`Property`], [`ThreatCategory`], [`CatalogScheme`])
//! resolved once from text keys. See
//! `docs/adr/0008-methodology-neutral-domain-model.md` and
//! `docs/adr/0012-threat-attack-trees.md`.
//!
//! ```
//! use std::sync::Arc;
//! use aegis_model::{AssetId, DamageId, Methodology, Op, Project, Provenance, ThreatId};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let m = Arc::new(Methodology::iso21434_r155()?);
//! let mut project = Project::new(Arc::clone(&m));
//!
//! // Resolve vocabulary keys once, at the boundary.
//! let integrity = m.property("integrity").ok_or("no property")?;
//! let tampering = m.threat_category("tampering").ok_or("no category")?;
//! let safety = m.impact_category("safety").ok_or("no category")?;
//! let severe = m.level(safety, "severe").ok_or("no level")?;
//! let annex5 = m.catalog_scheme("r155-annex5").ok_or("no scheme")?;
//!
//! let ecu = AssetId::new_v7();
//! project.apply(Op::AddAsset {
//!     id: ecu,
//!     name: "Brake ECU".into(),
//!     properties: [integrity].into_iter().collect(),
//!     provenance: Provenance::default(),
//! })?;
//! let crash = DamageId::new_v7();
//! project.apply(Op::AddDamageScenario {
//!     id: crash,
//!     name: "Vehicle cannot brake".into(),
//!     impacts: vec![(safety, severe)],
//!     provenance: Provenance::default(),
//! })?;
//! let threat = ThreatId::new_v7();
//! let undo = project.apply(Op::AddThreatScenario {
//!     id: threat,
//!     name: "Modify brake firmware".into(),
//!     asset: ecu,
//!     category: tampering,
//!     catalog_refs: vec![m.parse_catalog_ref(annex5, "4.1")?],
//!     damages: vec![crash],
//!     provenance: Provenance::default(),
//! })?;
//!
//! // The targeted property is derived from the category.
//! let idx = project.threat_scenario_idx(&threat).ok_or("missing")?;
//! assert_eq!(project.targets(idx), Some(m.violates(tampering)));
//!
//! // Undo is the inverse op.
//! project.apply(undo)?;
//! assert_eq!(project.threat_scenario_count(), 0);
//! # Ok(())
//! # }
//! ```

//!
//! # Saving and loading
//!
//! [`Project::save`] writes a project as a versioned JSON snapshot and
//! [`Project::load`] reads it back over a caller-supplied methodology.
//! [`OpLogWriter`] appends every accepted [`Op`] to a JSON Lines log, and
//! [`OpLogReader`] reads it back for replay onto an empty project or onto a
//! loaded snapshot. Files never hold vocabulary indices, only stable keys,
//! so a methodology whose vocabularies are reordered or extended still
//! reads them with the same meaning. Decoding is capped while it runs
//! ([`Limit`]), before any model check, and every failure is a typed
//! [`StoreError`]. See `docs/adr/0009-save-load-format.md`.
//!
//! ```
//! use std::sync::Arc;
//! use aegis_model::{AssetId, Methodology, Op, OpLogReader, OpLogWriter, Project, Provenance};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let m = Arc::new(Methodology::iso21434_r155()?);
//! let mut project = Project::new(Arc::clone(&m));
//! let mut log = OpLogWriter::create(Vec::new(), Arc::clone(&m))?;
//!
//! let ecu = AssetId::new_v7();
//! for op in [
//!     Op::AddAsset {
//!         id: ecu,
//!         name: "Brake ECU".into(),
//!         properties: m.all_properties(),
//!         provenance: Provenance::default(),
//!     },
//!     Op::RenameAsset {
//!         id: ecu,
//!         name: "Brake control unit".into(),
//!         provenance: Provenance::default(),
//!     },
//! ] {
//!     project.apply(op.clone())?;
//!     log.append(&op)?;
//! }
//!
//! // A snapshot loads back as an equal project.
//! let mut snapshot = Vec::new();
//! project.save(&mut snapshot)?;
//! assert_eq!(Project::load(snapshot.as_slice(), Arc::clone(&m))?, project);
//!
//! // Replaying the log onto an empty project gives the same project.
//! let mut replayed = Project::new(Arc::clone(&m));
//! for record in OpLogReader::open(log.into_inner()?.as_slice(), m)? {
//!     replayed.apply(record?.op)?;
//! }
//! assert_eq!(replayed, project);
//! # Ok(())
//! # }
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![warn(clippy::pedantic)]

// Lets the shared test kit (`tests/common/mod.rs`) name this crate as
// `aegis_model` when it is compiled into the unit tests.
#[cfg(test)]
extern crate self as aegis_model;

mod asset;
mod damage;
mod error;
mod eval;
mod id;
mod methodology;
mod op;
mod project;
mod provenance;
mod store;
#[cfg(test)]
#[path = "../tests/common/mod.rs"]
mod testkit;
mod threat;
mod topo;
mod tree;

pub use asset::{Asset, PropertySet};
pub use damage::DamageScenario;
pub use error::{EntityKind, MethodologyError, ModelError, PackError, StoreError};
pub use eval::{
    AttributeDomain, Derivation, Evaluation, Explanation, Kept, MaxMin, MinTime, evaluate,
};
pub use id::{
    AssetId, AssetIdx, DamageId, DamageIdx, NodeId, NodeIdx, ThreatId, ThreatIdx, TreeId, TreeIdx,
};
pub use methodology::{
    CatalogRef, CatalogScheme, CatalogSchemeDef, CatalogSyntax, FeasibilityLevel, ImpactCategory,
    ImpactCategoryDef, Level, LevelDef, Methodology, MethodologyDef, Property, PropertyDef,
    SegmentDef, ThreatCategory, ThreatCategoryDef,
};
pub use op::Op;
pub use project::{MAX_NAME_CHARS, Project};
pub use provenance::{Provenance, ProvenanceSource};
pub use store::{
    Limit, LoggedOp, MAX_ACTOR_BYTES, MAX_CATALOG_REF_BYTES, MAX_KEY_BYTES, MAX_LABEL_BYTES,
    MAX_METHODOLOGY_VERSION_BYTES, MAX_NAME_BYTES, MAX_OP_RECORD_BYTES, MAX_PACK_FILE_BYTES,
    MAX_PROJECT_FILE_BYTES, OpLogReader, OpLogWriter, VocabKind,
};
pub use threat::{MAX_CATALOG_REFS, MAX_DAMAGE_LINKS, ThreatScenario};
pub use tree::{AttackTree, GateKind, MAX_ATTACK_TREE_NODES, Node};
