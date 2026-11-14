VERDICT: PASS
## For the owner
What changed: The three built-in packs now ship example setting presets with Japanese labels: 12 in ISO, 8 in CRA, 8 in STRIDE.
These are data only, with no Rust under src/ changed and pack versions unchanged. Every key, label, interface kind, rule and ja label matches the prompt tables.
New tests pin the ISO list to an expected table. They also check the order of the CRA and STRIDE presets, that those presets apply to components only, and a few chosen values.
Look at: build.md "Presets for the owner to review". These values are the ones customers will see as defaults, and the merge waits on your sign-off of them.
  crates/aegis-model/packs/iso21434-r155.json:560 (control_kinds block): the ISO presets are the only ones tied to interface kinds (debug; can/ethernet/usb; ethernet/cellular; wireless).
  crates/aegis-model/tests/builtin_pack.rs:111 (CONTROL_KINDS): one table feeds both expected() and the ja map, so the ISO pack is compared field by field.
Risk: low. The ISO presets are compared in full against expected(), and all three packs must equal the writer's output byte for byte. The CRA and STRIDE tests check keys, order, component-only and two sample values (datacenter-access very-low, mfa ja label). They do not pin every CRA/STRIDE rule, so your table review is the check on those values. I compared them against the prompt tables by eye and they match.
View: git -C factory/worktrees/124-control-presets diff 14f4a3838f82384f4e10f007e22a84d79c745e16..HEAD
## Findings
none
## Lesson
none
NEXT: human
