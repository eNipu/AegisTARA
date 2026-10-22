## Round 1
Commits: feb1aa1 Add element provenance on entities and operations (D6)
Files:
- new: `crates/aegis-model/src/provenance.rs`, `docs/adr/0013-element-provenance.md`
- changed: `crates/aegis-model/src/lib.rs` (re-export `Provenance`, `ProvenanceSource`, `MAX_ACTOR_CHARS`), `src/asset.rs` (`provenance: Provenance` on `Asset`, `Asset::provenance()`), `src/damage.rs` (`provenance: Provenance` on `DamageScenario`, `DamageScenario::provenance()`), `src/threat.rs` (`provenance: Provenance` on `ThreatScenario`, `ThreatScenario::provenance()`), `src/op.rs` (`provenance: Provenance` on all 16 creating/mutating `Op` variants, `Op::provenance()`), `src/error.rs` (`ModelError::EmptyActor`, `ModelError::ActorTooLong { max }`), `src/project.rs` (`provenance: Provenance` on `TreeEntry`, `Project::attack_tree_provenance(TreeIdx)`, `check_provenance`, validation, commit/undo provenance swaps, `PartialEq`, `check_invariants`), `src/id.rs` (doctests), `src/store/snapshot.rs` & `src/store/op_log.rs` (`StoreError::UnencodableOp` refusal on non-default provenance; `Provenance::default()` on v1 load/decode), `benches/store_kit/mod.rs`, `tests/common/mod.rs`, `tests/pack.rs`, `tests/project.rs`, `tests/second_methodology.rs`, `tests/store.rs`, `tests/store_limits.rs`, `docs/adr/0008-methodology-neutral-domain-model.md`, `docs/adr/0009-save-load-format.md`, `docs/adr/0012-threat-attack-trees.md`

Acceptance criteria:
- [x] AC1 — `Provenance` and `ProvenanceSource` (`Human`, `Import`, `Catalog`, `Ai`) are implemented in `src/provenance.rs`; `size_of::<Provenance>() == 8` and `size_of::<ProvenanceSource>() == 1`; `Provenance::default()` allocates 0 heap bytes and `Provenance::new("", 0, ProvenanceSource::Human)` canonicalizes to `None`.
- [x] AC2 — `Asset::provenance()`, `DamageScenario::provenance()`, `ThreatScenario::provenance()`, and `Project::attack_tree_provenance(TreeIdx)` return the element's current `&Provenance`.
- [x] AC3 — All 16 creating and mutating `Op` variants carry `provenance: Provenance`; `Op::provenance()` returns `Some(&Provenance)` for creating/mutating ops and `None` for the 4 removal ops; `Remove*` returns `Add*` carrying the removed entity's stored `Provenance`.
- [x] AC4 — `check_provenance` accepts `Provenance::default()` and validates non-default provenance (`EmptyActor` when `actor.trim().is_empty()`, `ActorTooLong { max: 256 }` when `actor.chars().count() > 256`).
- [x] AC5 — Every in-place mutation (`Rename*`, `Set*`, `AddCatalogRef`, `RemoveCatalogRef`, `LinkDamage`, `UnlinkDamage`) updates the element's provenance and returns an inverse `Op` carrying the previous provenance; applying the inverse restores the exact previous provenance (`provenance_lifecycle_and_undo_redo_across_every_op`).
- [x] AC6 — `Project::eq` compares provenance on all 4 element kinds (`project_equality_compares_provenance_on_every_element_kind`).
- [x] AC7 — `tests/ops_properties.rs` and `tests/project.rs` exercise all 20 `Op` variants with generated provenance (`default`, `Human`, `Import`, `Catalog`, `Ai`, and invalid actors) across both methodologies.
- [x] AC8 — `tests/second_methodology.rs` exercises all 4 `ProvenanceSource` variants and all 36 `ModelError` variants (`EmptyActor`, `ActorTooLong` included).
- [x] AC9 — `Project::save` and `OpLogWriter::append` refuse non-default provenance with `StoreError::UnencodableOp` without writing bytes (`save_and_append_refuse_non_default_provenance_with_unencodable_op`); v1 golden fixtures are unchanged and `benches/store_memory.rs` passes (`damages_only_max` kept heap 2.78x <= 3.0x).
- [x] AC10 — `errors_never_contain_names` in `tests/project.rs` verifies `EmptyActor` and `ActorTooLong` never echo actor text or entity names.
- [x] AC11 — No `unwrap()`, `expect()`, `panic!`, `todo!`, or `unimplemented!` in `crates/aegis-model/src` outside `#[cfg(test)]`.
- [x] AC12 — `cargo test --doc` (255 doctests) and `cargo clippy --all-targets -- -D warnings` pass with zero warnings.
- [x] AC13 — `docs/adr/0013-element-provenance.md` written and cross-referenced from ADRs 0008, 0009, and 0012.
- [x] AC14 — `scripts/ci.sh crates/aegis-model`, `cargo test --manifest-path crates/aegis-model/Cargo.toml --benches`, and `scripts/deny.sh` pass.

Checks:
- `cargo fmt --check` → pass
- `cargo clippy --all-targets -- -D warnings` → pass
- `cargo test` → pass (159 lib, all integration tests, 255 doctests)
- `cargo test --benches` → pass
- `scripts/ci.sh crates/aegis-model` → pass
- `scripts/deny.sh` → pass
