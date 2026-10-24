VERDICT: PASS

Surface reviewed: library API (`EntityKind` re-export, `SegmentDef::fmt`, `CatalogSyntax::fmt`, `CatalogRef::fmt`), `ModelError` diagnostics, and property-test harness ergonomics (`tests/properties.rs`). No UI, CLI, or server surface in this change.

Flow:
- `EntityKind` (`Asset`, `DamageScenario`, `ThreatScenario`, `AttackTree`, `TreeNode`, and subsequently `SecurityControl`) is re-exported at the `aegis_model` crate root with `EntityKind::as_str()` and `fmt::Display`, allowing callers to pattern-match and display entity categories directly without reaching into `error`.
- `SegmentDef`, `CatalogSyntax`, and `CatalogRef` implement `fmt::Display` (`<1..=32>[.<1..=99>]`, `<0001..=9999>`, `4.1`) without duplicating formatting logic, making catalog syntax descriptions and references directly usable in logs and user-facing diagnostics.
- `tests/properties.rs` persists proptest counterexamples to `proptest-regressions/properties.txt` and respects `PROPTEST_CASES` (defaulting to 64 locally and scaling to 512 in CI), keeping local feedback well within interactive budgets.

Errors:
- `ModelError::EmptyName { kind }`, `ModelError::NameTooLong { kind, max }`, and `ModelError::ProjectCapacityExceeded { kind }` clearly identify which entity kind failed and the enforced bound (`65535` capacity, `256` character max) without echoing confidential entity names or labels.
- `ModelError::InvalidCatalogId { scheme, syntax }` displays the expected `<lo..=hi>` segment syntax directly via `CatalogSyntax::fmt`, and `ModelError::CatalogRefNotFound` / `ModelError::DuplicateCatalogRef` format both the `ThreatId` and `CatalogRef` in readable dotted/padded notation.
