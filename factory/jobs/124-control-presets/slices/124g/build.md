Commit: 7f6dee0 List settings and presets for Add setting and name the preset in the control detail (124g)
Files: crates/aegis-client/src/pages.rs, crates/aegis-client/tests/control_presets.rs, docs/adr/0070-control-presets.md, docs/architecture.md
Done when:
1. List content per target/filter: `cargo test --locked -p aegis-client --test control_presets` -> 15 passed
2. Every listed preset accepted by AddPresetSetting: same command, `every_listed_preset_is_accepted` passes
3. ControlDetail preset / differs_from_preset: same command, passes
4. Workspace: `scripts/ci.sh` -> fmt, clippy, test all ok (run before the final one-line ADR link fix, docs only)
5. Source lines: `git diff --numstat HEAD~1 -- crates/aegis-client/src` -> 121 added, 4 removed (cap 250)
Notes:
- `format_control_rules` now delegates to a shared `format_rules(&Methodology, &[ControlRule])`; output unchanged.
- ADR 0011 is `0011-methodology-pack-files.md`; ADR links to it by that name.
- architecture.md: added the lint code to the list and one "Presets" paragraph.
- Nothing in the prompt's decisions disagreed with the code I read (SecurityControlHasKind, MAX_CONTROL_KINDS = 64 confirmed).
NEXT: reviewer
