# 014-provenance: Provenance on every element (D6)

Builds on jobs 009 (`docs/adr/0008-methodology-neutral-domain-model.md`) and 013 (`docs/adr/0012-threat-attack-trees.md`). Decision D6 (`docs/decisions.md`) and the AI backlog (`docs/ai-backlog.md`, sections 3 and 7) require provenance on every element in v1 so that human edits, imports, catalog instantiations, and accepted AI suggestions can be tagged, audited, and round-tripped through undo.

## Open questions

1. **Element provenance shape.** Should each domain element (`Asset`, `DamageScenario`, `ThreatScenario`, and `AttackTree` in `Project`) store a single `Provenance` record representing who created or last changed it, when, and from what source, or separate `created` and `modified` records? **Default: a single `Provenance` record on each element, set on creation (`Add*`), updated on every in-place mutation (`Rename*`, `Set*`, `AddCatalogRef`, `RemoveCatalogRef`, `LinkDamage`, `UnlinkDamage`), and restored to the previous `Provenance` when the inverse operation is applied (undo).** The full history of creation and intermediate modifications lives in the append-only op log (D4, ADR 0009).
2. **Operation representation and memory layout.** How should typed operations carry provenance while keeping per-element memory within the 3x server budget (ADR 0010)? **Default: `Provenance` wraps `Option<Arc<ProvenanceInner>>` (8 bytes on 64-bit targets; `Provenance::default()` is `None` with zero heap allocation and canonical equality for `("", 0, ProvenanceSource::Human)`), and is carried as a `provenance: Provenance` field on all 16 creating and mutating `Op` variants. The 4 entity-removal variants (`RemoveAsset`, `RemoveDamageScenario`, `RemoveThreatScenario`, `RemoveAttackTree`) delete the entity and return `Add*` carrying the removed entity's stored `Provenance`.**
3. **Store wire format.** Should `aegis-project` and `aegis-oplog` v1 wire formats be bumped in this job or kept at v1? **Default: keep v1 wire formats and golden fixtures unchanged in this job; `Project::save` and `OpLogWriter::append` reject non-default provenance with `StoreError::UnencodableOp` (so save/load never silently drops provenance), and format v2 will encode both attack trees and provenance together.**

## Goal

Every element in a `Project` (`Asset`, `DamageScenario`, `ThreatScenario`, and `AttackTree`) records its `Provenance`: who created or last changed it (`actor: &str`), when (`timestamp_ms: u64`, Unix epoch milliseconds in UTC), and from what source (`ProvenanceSource`: `Human`, `Import`, `Catalog`, or `Ai`). Every creating or mutating `Op` variant carries a `Provenance`, `Project::validate` validates the actor string (`EmptyActor`, `ActorTooLong` over 256 Unicode scalar values), and `Project::apply` updates the target element's provenance and returns an inverse `Op` carrying the element's prior `Provenance` so that applying the inverse (`undo`) restores both the element's domain state and its previous provenance.

## Scope

In:
- `ProvenanceSource` (`Human`, `Import`, `Catalog`, `Ai`) and `Provenance` in `crates/aegis-model/src/provenance.rs`, re-exported at the crate root.
- Compact 8-byte representation of `Provenance` (`Option<Arc<ProvenanceInner>>`), where `Provenance::default()` (`actor: ""`, `timestamp_ms: 0`, `source: ProvenanceSource::Human`) requires zero heap allocation and `Provenance::new("", 0, ProvenanceSource::Human)` canonicalizes to the same representation.
- `Asset::provenance(&self) -> &Provenance`, `DamageScenario::provenance(&self) -> &Provenance`, `ThreatScenario::provenance(&self) -> &Provenance`, and `Project::attack_tree_provenance(&self, idx: TreeIdx) -> Option<&Provenance>`.
- `provenance: Provenance` field on all 16 creating and mutating `Op` variants, plus `Op::provenance(&self) -> Option<&Provenance>`.
- Provenance actor validation in `Project::validate`: if `actor` is non-empty, it must not be whitespace-only (`ModelError::EmptyActor`) and must not exceed `MAX_NAME_LEN` (256 Unicode scalar values, `ModelError::ActorTooLong`). Error messages never echo the actor string (R6).
- Undo preservation in `Project::apply`: mutating operations replace the element's `Provenance` and return an inverse `Op` carrying the prior `Provenance`; `Remove*` operations return `Add*` carrying the removed element's `Provenance`.
- Logical `Project` equality (`PartialEq`) includes `Provenance` on every live asset, damage scenario, threat scenario, and attack tree.
- Store v1 guards: `Project::save` and `OpLogWriter::append` return `Err(StoreError::UnencodableOp)` if any live element or operation carries non-default provenance, preserving `load(save(p)) == p` and `read(write(op)) == op` on v1 files.
- Unit tests, integration tests (`tests/project.rs`, `tests/ops_properties.rs`, `tests/store.rs`), doctests, and ADR 0013 (`docs/adr/0013-element-provenance.md`).

Out:
- Format v2 of `aegis-project` and `aegis-oplog` (deferred to the store v2 job alongside attack trees).
- Per-node provenance inside `AttackTree` (nodes are immutable structural parts of an `AttackTree`; the `AttackTree` entry in `Project` carries provenance for tree creation and step feasibility updates).
- Cryptographic signing of provenance records or hash-chaining of the audit log.

## Acceptance criteria

- **AC1 (`ProvenanceSource` and `Provenance` API):** `ProvenanceSource` defines `Human` (`#[default]`), `Import`, `Catalog`, and `Ai`, with `as_str(&self) -> &'static str` (`"human"`, `"import"`, `"catalog"`, `"ai"`) and `Display`. `Provenance` provides `new`, `human`, `import`, `catalog`, `ai`, `default`, `actor(&self) -> &str`, `timestamp_ms(&self) -> u64`, `source(&self) -> ProvenanceSource`, and `is_default(&self) -> bool`.
- **AC2 (Compact 8-byte layout and canonical equality):** `std::mem::size_of::<Provenance>() == std::mem::size_of::<usize>()`, `Provenance::default()` performs no heap allocation, and `Provenance::new("", 0, ProvenanceSource::Human) == Provenance::default()`. All 18 cases in `benches/store_memory.rs` pass within `SERVER_BUDGET` (`3.0x`) and their existing per-case gates.
- **AC3 (Provenance on every element):** Every live `Asset`, `DamageScenario`, `ThreatScenario`, and `AttackTree` in `Project` exposes its `Provenance` (`asset.provenance()`, `damage.provenance()`, `threat.provenance()`, `project.attack_tree_provenance(idx)`).
- **AC4 (Carried by typed operations):** All 16 creating and mutating `Op` variants (`AddAsset`, `RenameAsset`, `SetAssetProperties`, `AddDamageScenario`, `RenameDamageScenario`, `SetImpact`, `AddThreatScenario`, `RenameThreatScenario`, `SetThreatCategory`, `AddCatalogRef`, `RemoveCatalogRef`, `LinkDamage`, `UnlinkDamage`, `AddAttackTree`, `SetStepFeasibility`, `SetThreatAttackTree`) carry `provenance: Provenance`, and `Op::provenance(&self)` returns `Some(&Provenance)` for them and `None` for the 4 `Remove*` variants.
- **AC5 (Kept through undo and redo):** For every accepted `Op`, `let inv = project.apply(op.clone())?;` sets or updates the affected element's `Provenance` to `op`'s provenance, and `let inv2 = project.apply(inv)?;` restores the element's exact prior `Provenance` (`assert_eq!(project, before)`), with `inv2` restoring `op`'s `Provenance` on redo.
- **AC6 (Validation & no actor leakage):** `Project::validate` and `Project::apply` reject a non-empty whitespace-only actor with `ModelError::EmptyActor` and an actor longer than `MAX_NAME_LEN` (256 Unicode scalar values) with `ModelError::ActorTooLong`. Neither error's `Display` or `Debug` output contains the actor string (R6).
- **AC7 (Project logical equality):** Two `Project` values with identical entities except for differing `Provenance` on any asset, damage scenario, threat scenario, or attack tree compare unequal (`!=`).
- **AC8 (Store v1 safety):** `Project::save` and `OpLogWriter::append` succeed with identical bytes on v1 fixtures (`Provenance::default()`) and fail with `StoreError::UnencodableOp` when any live element or operation carries non-default provenance.
- **AC9 (Engineering standards):** `scripts/ci.sh crates/aegis-model` and `scripts/deny.sh licenses bans sources` pass with zero warnings, `#![forbid(unsafe_code)]`, `#![warn(missing_docs)]`, `#![warn(clippy::pedantic)]`, doctests on every new public item, big-O complexity documented, no `unwrap`/`expect` in non-test library code, and ADR 0013 written.
