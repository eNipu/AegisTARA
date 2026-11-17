Commit: 1e024e754c9bc31a599da70de08335f5a0276dec Store damage-link rationales in snapshots while keeping v1 bytes (146b)
Files: crates/aegis-model/src/store/wire.rs, crates/aegis-model/src/store/snapshot.rs, crates/aegis-model/tests/pair_rationale.rs
Done when:
- `cargo test --locked -p aegis-model --test store` — PASS (20 passed, 1 ignored), including `golden_v1_snapshot_loads_and_resaves_byte_for_byte`.
- `cargo test --locked -p aegis-model --test pair_rationale` — PASS (10 passed), including `v1_fixture_resaves_byte_for_byte_without_rationale_key`, `object_form_round_trips_byte_for_byte` and `empty_rationale_reverts_to_v1_string_array_form`.
Before you commit (all run from the worktree root with `CARGO_TARGET_DIR` set to the shared target dir):
- `cargo fmt --all` — PASS (no extra files changed).
- `scripts/ci.sh crates/aegis-model` — PASS (`ok aegis-model fmt`, `ok aegis-model clippy`, `ok aegis-model test`; all steps passed in 1 crate).
- `cargo test --locked -p aegis-model --test store` — PASS.
- `cargo test --locked -p aegis-model --test pair_rationale` — PASS.
Notes: `wire.rs` gains a `WireDamageLink` (`deny_unknown_fields`, rationale via `some`) and an untagged `DamageLinksWire` with `Ids`/`Links` variants; both reuse `LinksCap` (and `DescriptionCap` for the rationale), no new `Limit`. `snapshot.rs` `LinkedDamages` now carries the threat's sparse `damage_rationales`; serialization emits the original ID-string array when every rationale is empty and a `DamageLinkView { damage, rationale }` object array otherwise, always writing `rationale` so bytes are deterministic. `apply_snapshot_threat` splits the decoded form into plain IDs (passed to `AddThreatScenario` as before) and `(DamageId, Arc<str>)` rationales applied via `Op::SetDamageLinkRationale` with `provenance.clone()` immediately after that op. The untagged-enum first variant fails on object content with a type mismatch (no fault raised), so the existing fault channel stays clean. No scope changes, no dependency added, no `unwrap`/`expect`/`unsafe` in library code. L06 covered by `object_form_round_trips_byte_for_byte`, which checks both `Project` equality and `save(&loaded) == bytes`.
NEXT: reviewer
