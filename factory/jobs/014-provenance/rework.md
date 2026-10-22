# Rework notes: 014-provenance

## Owner answers to open questions

1. **Element provenance shape:** Follow the project's guiding principles of maintainability, scalability, and accuracy. Single `Provenance` per element (`Asset`, `DamageScenario`, `ThreatScenario`, and `AttackTree` in `Project`), set on creation, updated on every in-place mutation, and restored via the inverse `Op` on undo.
2. **Operation representation and memory layout:** Follow maintainability, scalability, and accuracy. `Provenance` wraps `Option<Arc<ProvenanceInner>>` (8 bytes, zero allocation for `Provenance::default()`) and is carried as `provenance: Provenance` on all 16 creating and mutating `Op` variants; `Remove*` variants return `Add*` carrying the removed entity's `Provenance`.
3. **Store wire format:** Keep `aegis-project` and `aegis-oplog` v1 fixtures unchanged and reject non-default provenance with `StoreError::UnencodableOp` until format v2 encodes attack trees and provenance together.
