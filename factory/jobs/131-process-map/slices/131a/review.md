VERDICT: PASS
## For the owner
What changed: Methodology packs can now carry an optional `clause_refs` map (page key -> list of ISO/CRA reference strings), exposed on `MethodologyDef` and `Methodology` with two accessors.
Round 2 gave the cap its own `Limit::ClauseRefs` (32 refs per page, 32 pages) instead of reusing the impact-category limit, and fixed the last construction sites so all test targets and doc tests compile.
New tests pin the round-trip with refs, the exact over-cap error in both positions, and that builtin packs without refs still decode to an empty map and re-serialize byte-identically.
Look at: crates/aegis-model/src/methodology/pack.rs:168 — the new wire field; one cap type serves both the outer map and the inner lists, and `deny_unknown_fields` means old binaries refuse packs that use it (format-compatibility decision lives here).
Look at: crates/aegis-model/src/methodology/pack.rs:630 — skip-when-empty serialize branch; this is what keeps every existing pack's bytes unchanged, pinned by `builtin_pack_is_canonical`.
Look at: crates/aegis-model/tests/pack_limits.rs:272 — `clause_refs_caps` asserts the exact `Limit::ClauseRefs { max: 32 }` for 33 refs on one page and for 33 pages; the old `Limit::ImpactCategories` reuse would fail it.
Risk: Packs with non-empty `clause_refs` are refused by older binaries (unknown field); checked that empty-map packs serialize byte-identically to the old format (`builtin_pack_is_canonical` compares against the fixed PACK bytes) and that the field is `serde(default)` so old packs still decode. Untrusted input is bounded at the wire boundary: page keys <= 64 bytes (`Key`), refs <= `MAX_LABEL_BYTES` (`Label`), <= 32 entries and <= 32 refs per page (`ClauseRefsCap`, checked in the visitors before allocation), duplicate keys rejected; the error text is the bounded "more than {max} clause refs" with no input echoed. Round-trip determinism checked by writing twice and comparing bytes plus full-def equality (derived `PartialEq` includes `clause_refs`), so a dropped field fails the test. Every changed line of the 129-line diff read (high risk); all 11 files inside the widened allow list; round-1 commits 45495c5/0be824d untouched, one round-2 commit a9b2cbc on top.
View: git -C factory/worktrees/131-process-map diff c7b1f86..HEAD
## Findings
- none
## Lesson
[planner] Adding a required field to a public struct forces an edit at every construction site, doc examples and test fixtures included; put those files in the allow list when cutting the slice, not after the gate fails. Evidence: job 131 slice 131a (error.rs doc tests and catalog.rs fixture out of scope, gate FAIL, re-cut and round 2).
NEXT: human