VERDICT: PASS
## For the owner
What changed: a new `pages::add_setting_list` gives the "Add setting" list for one component or interface: settings already in the project that fit, pack presets that fit and are not created yet, and the typed text as a custom name.
The control detail now says which preset a setting came from and whether its rules differ from the pack's current values.
ADR 0070 records the job's decisions, and architecture.md has a "Presets" paragraph plus the new lint code.
Look at:
- crates/aegis-client/src/pages.rs:755-760: a preset is hidden as soon as any control of that kind exists, even one renamed so the filter no longer finds it. This is what the prompt asked for (one "Secure boot" covers several ECUs), but a renamed preset setting can only be found under its new name.
- crates/aegis-client/src/pages.rs:736-741: an existing setting is offered only if it is not on this target yet and, if it came from a preset, the preset fits the target. This is what stops "Debug port locked" from being offered on an ECU.
- docs/adr/0070-control-presets.md: the decisions as you agreed them. I checked the caps (64 presets, 1 to 16 rules) against the model code.
Risk: low. This is read-only view data. The test `every_listed_preset_is_accepted` passes every preset the list offers on the ECU, JTAG and CAN to the real `Edit::AddPresetSetting`, so the list cannot offer a preset the command would refuse.
View: git -C factory/worktrees/124-control-presets diff ee4a6cafe8442d39a27696460d559fbac6db035a..HEAD
## Findings
- No defects found. All three "Done when" items have tests in tests/control_presets.rs, and each test would fail if its part of the change were removed: exact preset lists per target, the "not applied yet" filter (`same.existing.is_empty()`), the fit filter (`ids == [custom]`), the trimmed custom text, the language fallback, and the detail checking `differs_from_preset` both ways. `format_control_rules` now calls a shared `format_rules` helper and its output is unchanged. The filter text is not size-capped. That is acceptable because it is only searched, never stored; the commands that create a setting validate the name.
## Lesson
none
NEXT: human
