VERDICT: PASS

Surface reviewed: library persistence API (`Project::save`, `Project::load`, `OpLogWriter::append`, `OpLogReader`), `AttackTree::from_parts`, wire-level backward compatibility, and `StoreError` / `ModelError` diagnostics. No UI, CLI, or server surface in this change.

Flow:
- `Project::save`/`Project::load` and `OpLogWriter::append`/`OpLogReader` round-trip attack trees (including DAG step sharing), step feasibility ratings, threat-to-tree links, the four tree `Op` variants (`AddAttackTree`, `RemoveAttackTree`, `SetStepFeasibility`, `SetThreatAttackTree`), and non-default `Provenance` with zero extra steps or configuration flags.
- Backward compatibility is seamless: optional/default fields (`attack_trees` when empty, `attack_tree` when `None`, `provenance` when `Provenance::is_default()`) are omitted on serialization and defaulted on deserialization, keeping v1 golden fixtures byte-for-byte identical.
- `AttackTree::from_parts` validates node count (`1..=MAX_ATTACK_TREE_NODES`), non-empty bounded labels, child index bounds, and DAG acyclicity in $O(V + E)$ when reconstructing persisted trees.
- Undo/redo across `AddAttackTree`, `RemoveAttackTree`, `SetStepFeasibility`, and `SetThreatAttackTree` remains an exact single-step inverse even after op-log replay.

Errors:
- `ModelError::TooManyAttackTreeNodes { tree, max: 32 }` rejects oversized attack trees in `Project::apply` before persistence so every valid `Op::AddAttackTree` fits inside `MAX_OP_RECORD_BYTES` (64 KiB) without surprising `StoreError::RecordTooLarge` failures at save time.
- Wire-level caps (`Limit::ActorBytes`, `Limit::AttackTreeNodes`, `Limit::StepFeasibility`) reject oversized payloads during serde decoding before allocation, and `StoreError::UnknownVocabularyKey { kind: VocabKind::FeasibilityLevel, key }` reports unknown feasibility keys with safe 64-byte truncation.
- Explicit default provenance in wire payloads (`provenance: { source: "human" }` with no actor) is rejected cleanly as non-canonical rather than silently normalized.
