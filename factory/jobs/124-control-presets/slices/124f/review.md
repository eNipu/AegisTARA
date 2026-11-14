VERDICT: PASS
## For the owner
What changed: two new commands. "Add preset setting" creates a control from a pack preset, copies the preset's rules, and applies it, all in one undo step. "Use the pack's values" resets a changed preset control.
Applying a preset control to the wrong kind of element (for example a debug-port preset on a whole ECU) is now refused with a sentence the user can read. This holds for new and for existing controls.
Duplicating a preset control keeps its preset, so the copy is still a preset-based setting.
Look at: crates/aegis-client/src/commands.rs:870 (preset_fit_error builds the three refusal sentences; a missing target returns Missing instead of a sentence)
  crates/aegis-client/src/commands.rs:2015 (ApplyControl now refuses a wrong fit; controls with no preset and unknown controls behave as before)
  crates/aegis-client/src/commands.rs:2071 (UsePackValues removes extra rules before it sets any, so a control never goes over the 16-rule limit partway through)
Risk: low. Adding a setting, resetting it, removing it and duplicating it each undo in one step, and tests check this by comparing the whole project before and after. Project equality compares each control's full content, kind included. The refusal sentences contain only labels from the pack, never text the user typed.
View: git -C factory/worktrees/124-control-presets diff 9d8494457941e42246e7dd5172620c5d600c42a3..HEAD
## Findings
- none blocking. All 5 "Done when" items are met. Each new behaviour has a test that would fail if the change were removed: the exact sentences, the ApplyControl refusal, the copy keeping its kind, the clears-first order and the one-step undo.
- crates/aegis-client/tests/control_presets.rs (duplicate_and_remove_keep_the_kind): the test only checks that the control comes back with its kind by comparing whole projects. It does not assert `kind()` directly after the undo. This is covered, because control equality includes kind. Optional.
## Lesson
none
NEXT: human
