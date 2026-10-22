VERDICT: PASS

Surface reviewed: library API (`Provenance`, `ProvenanceSource`, entity accessors, `Op::provenance()`), undo/redo behavior, and `ModelError` messages. No UI, CLI, or server surface in this change.

Flow:
- Callers construct provenance with concise helpers (`Provenance::human`, `Provenance::import`, `Provenance::catalog`, `Provenance::ai`, or `Provenance::default()`) and pass it on any creating or mutating `Op`.
- Every element kind (`Asset`, `DamageScenario`, `ThreatScenario`, and `AttackTree` entry on `Project`) exposes its current `&Provenance`.
- Every in-place mutation swaps the element's provenance and returns an inverse `Op` carrying the previous provenance, and every `Remove*` op returns an `Add*` op carrying the removed element's provenance, so undo restores the exact prior provenance in O(1).

Errors:
- `ModelError::EmptyActor` (`"provenance actor is empty or whitespace"`) and `ModelError::ActorTooLong { max }` (`"provenance actor exceeds 256 characters"`) clearly state the validation rule without echoing untrusted actor strings.
- Saving or logging non-default provenance in the v1 store format fails explicitly with `StoreError::UnencodableOp` rather than silently discarding provenance.
