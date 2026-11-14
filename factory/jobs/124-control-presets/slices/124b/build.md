Commit: a1dc234 Read and write control kinds in pack files (124b)
Files: crates/aegis-model/src/methodology/pack.rs, crates/aegis-model/src/store/mod.rs, crates/aegis-model/tests/control_kinds.rs, crates/aegis-model/tests/pack.rs, crates/aegis-model/tests/pack_limits.rs
Done when:
1. round trip, byte-stable: cargo test --locked -p aegis-model --test control_kinds: ok (16 tests)
2. missing/unknown/repeated fields refused: --test pack: ok (25)
3. caps enforced while decoding: --test pack_limits: ok (21)
4. generated round trip: --test pack_properties: ok (6)
5. built-in packs canonical: --test builtin_pack --test stride_pack: ok
6. source lines changed: 85 added, 12 removed (pack.rs 69/9, store/mod.rs 16/3), under 250
scripts/ci.sh crates/aegis-model: all steps passed.
Notes:
- control_kinds.rs already existed from 124a; the new pack-file tests are appended to it under a "Pack files (124b)" header (helpers renamed lv, rule_map, pack_presets, pack_text; added mod common).
- Added From<ControlKindWire> for ControlKindDef because From<DefWire> hit clippy too_many_lines (102/100).
- Limit::CONTROL_KINDS etc. are pub(crate), so pack_limits.rs uses the Limit variants directly (ControlKinds{max:64}, ControlRules{max:16}, InterfaceKinds{max:32}).
- Preset label cap test uses 256 four-byte chars at cap and x+256 over cap (the model also limits labels to 256 chars).
- pack.rs tests for repeated rule key / missing rules / unknown field assert PackError::Decode, with a passing baseline test (a_control_kind_loads) using the same helper.
- Amended my own unpushed commit once to include pack_limits.rs.
NEXT: reviewer

## Rework 1
Fixed: `control_kind_interface_kind_cap` no longer pushes `extra` into the top-level `interface_kinds`; only the preset's list goes to 33, so the preset's own cap is what fails. Test file only.
Checks: `--test pack_limits` 21 ok; `scripts/ci.sh crates/aegis-model` all passed.
NEXT: reviewer
