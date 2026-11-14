VERDICT: PASS
## For the owner
What changed: A methodology can now hold "setting presets" (control kinds). Each one has a key, a label, the interface kinds it is for (none means components) and 1 to 16 default rules.
`Methodology::new` checks them: at most 64 presets, unique keys, known interface kinds listed once, known attack types and feasibility levels. Read-only accessors return them as typed values, and a translated label is used when there is one.
Pack files do not read or write presets yet (that is slice 124b). The only change to `pack.rs` is the empty fields it needs to compile.
Look at: crates/aegis-model/src/methodology/mod.rs:1563 — `build_control_kinds`. The checks run in the order the prompt gives, unknown keys are shortened in error text with `error_key_text`, the rule count is checked before the loop over the rules, and the stored rules and interface kinds are sorted.
crates/aegis-model/src/methodology/mod.rs:3274 — the seven accessors. A preset index that is out of range gives `None` or an empty slice, never a panic.
crates/aegis-model/tests/control_kinds.rs — there is one test for each of the seven errors, each checking the exact error value, plus tests at the limits: exactly 64 presets and exactly 16 rules load, and 65 presets or 17 rules are refused.
Risk: Low. Nothing that already exists changes behaviour. The new fields are empty for every current pack, and `builtin_pack` still passes. Presets will come from untrusted pack files once 124b lands, and that input is already bounded here: the preset count is capped before the loop, the rule count is checked before rules are resolved, and the interface-kind loop stops at the first unknown or repeated key, so it cannot run more than 33 times. The unknown keys that end up in error messages are shortened.
View: git -C factory/worktrees/124-control-presets diff e56e783766407b0a25020a86d0dadff89b8474e5..HEAD
## Findings
- none. One minor point, not a defect: in `presets_load_and_come_back` the expected rules and interface kinds are sorted in the test before comparing. So the test only shows that the code sorts if the built-in pack declares those items in a different order from the input. For interface kinds the input is `usb, debug`, which is out of order, so that part is exercised.
## Lesson
none
NEXT: human
