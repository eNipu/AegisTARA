VERDICT: PASS
## For the owner
What changed: a security control can now record which pack preset (for example "Secure boot") it was made from, through a new undoable edit `SetControlKind`.
The preset is saved in the op log and the project snapshot; older files without it load as before, and an unknown preset name in a file is refused.
A control that still names a preset cannot be removed until the preset is cleared; the app's "remove control" does this as part of the same undo step.
Look at: crates/aegis-model/src/project.rs:9698 (commit_set_control_kind: swaps the kind and provenance and returns the exact inverse);
crates/aegis-model/src/store/snapshot.rs:1087 (on load, the kind is re-applied with the control's own provenance, so a reload equals the saved project);
crates/aegis-client/src/commands.rs:2051 (remove control clears the preset first; no test covers this client path with a preset set, though the undo walk covers the op itself).
Risk: low. The preset is a label only: it is in the evaluation cache's no-effect list (project.rs:11666), so it cannot change a risk number. File input goes through the same key-length check and lookup as the other vocab keys (wire.rs:611), and the error names a key already capped in length. Checked by the new control_presets tests and the undo_every_op walk.
View: git -C factory/worktrees/124-control-presets diff cd392127dda767d86a0ac73d89b2aa5632f3769f..HEAD
## Findings
- none. All Done-when items are met and proven: exact inverse (project equal to the earlier clone), UnknownSecurityControl, UnknownControlKind (no state change), blank actor refused (L15), the removal guard gives exactly SecurityControlHasKind and passes after clearing (L09), op log Some/None/missing field, snapshot with and without kind and with an unknown key. `same_control` now compares the kind, so the snapshot round trip would fail if the kind were dropped. The one remaining place that matches on SetControlRule with a fallback (crates/aegis-client/src/session.rs:3552, the "risk changed" notice) correctly ignores a change that moves no risk.
## Lesson
none
NEXT: human
