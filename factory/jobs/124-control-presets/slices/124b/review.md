VERDICT: PASS
## For the owner
What changed: a pack file can now carry setting presets (`control_kinds`) and their translated labels.
Reading goes through capped wire types: at most 64 presets, 16 rules each, 32 interface kinds each, and capped key and label lengths. A repeated or unknown field is refused.
Writing uses a fixed order (key, label, interface_kinds, rules). Empty lists are left out, so packs without presets come out byte-for-byte the same as before.
Look at:
- crates/aegis-model/src/methodology/pack.rs:242 — `ControlKindWire`, the boundary for untrusted preset data. Every field is bounded and `rules` is required.
- crates/aegis-model/src/methodology/pack.rs:443 — the new `From<ControlKindWire>`. The builder added it so `From<DefWire>` stays under the clippy line limit. It only moves `Arc<str>` handles.
- crates/aegis-model/tests/pack_limits.rs:558 — `with_preset`, which builds every over-cap input. Each one is valid except for the single cap it tests (L09).
Risk: low. The format is additive and optional, and the pack version stays 1. The built-in packs still pass the canonical tests. Each cap has a test at the cap and one over it, and each test checks the exact `Limit`.
View: git -C factory/worktrees/124-control-presets diff b0d395330def3553f92c8d032f9649dfaaeb25bc..HEAD
## Findings
- none. Every "Done when" item has a test that would fail if the change were removed:
  - Round trip: `assert_eq!` on the definition, plus a bytes-equal check after reading back.
  - Shape: exact-line assertions in the written text.
  - Refusals: tested against a baseline `a_control_kind_loads` that uses the same helper, so the refusal is caused by the bad field and not a broken fixture.
  - Missing `rules`: the test expects `Decode`, not a model error, so it pins `rules` as required on the wire.
  - Caps: tested at 64/65 presets, 16/17 rules and 32/33 interface kinds, and on key and label bytes.
- Minor, no change needed: translated preset labels are capped at 64 entries (`ControlKindsCap`), but no test covers that cap. It reuses the same bounded map as the other translation maps.
## Lesson
none
NEXT: human
