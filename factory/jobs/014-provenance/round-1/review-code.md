VERDICT: PASS

## Check results

Run from `factory/worktrees/014-provenance/` (branch `factory/014-provenance`, commit feb1aa1).

| Command | Result |
|---|---|
| `cargo fmt --check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass |
| `cargo test` | pass (159 unit, all integration suites, 255 doctests) |
| `cargo test --manifest-path crates/aegis-model/Cargo.toml --benches` | pass (`damages_only_max` kept heap 2.78x <= 3.0x) |
| `scripts/ci.sh crates/aegis-model` | pass (fmt, clippy, test) |
| `scripts/deny.sh` | pass (aegis-model and aegis-model/fuzz) |
| AC11 no-panic grep | zero hits outside `#[cfg(test)]` |
| AC9 golden fixtures diff | exit 0 |

## Acceptance criteria

All 14 acceptance criteria (AC1–AC14) in `spec.md` are met and covered by unit, integration, property, benchmark, and doc tests:
- `Provenance` (8 B) and `ProvenanceSource` (1 B) in `src/provenance.rs`, with canonical `None` for default provenance (`actor: ""`, `timestamp_ms: 0`, `source: Human`).
- `provenance` field and getter on `Asset`, `DamageScenario`, `ThreatScenario`, and `TreeEntry` (`Project::attack_tree_provenance`).
- `provenance: Provenance` on all 16 creating/mutating `Op` variants; `Op::provenance()`; exact provenance preservation across mutations, removals, and undo (`provenance_lifecycle_and_undo_redo_across_every_op`).
- `ModelError::EmptyActor` and `ModelError::ActorTooLong { max: 256 }` checked in `Project::validate` and `check_invariants`.
- `Project::eq` compares provenance across all 4 element kinds (`project_equality_compares_provenance_on_every_element_kind`).
- Explicit `StoreError::UnencodableOp` refusal in `Project::save` and `OpLogWriter::append` when non-default provenance is present.
- `docs/adr/0013-element-provenance.md` written and cross-referenced from ADRs 0008, 0009, and 0012.

## Findings

None blocking.
