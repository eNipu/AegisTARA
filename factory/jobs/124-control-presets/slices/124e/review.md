VERDICT: PASS
## For the owner
What changed: the linter now warns when a security control made from a pack preset has rules that no longer match that preset.
The model can also answer "does this preset fit this component or interface?" (it answers only, it never refuses).
The Problems page shows the warning as: Setting "X" has rules that differ from its pack preset "Secure boot".
Look at:
- crates/aegis-model/src/project.rs:1729-1735 (`control_differs_from_kind`): compares the control's rule list with the preset's rule list directly. That is correct only because both lists are kept in the same order (by attack type), as earlier slices set up. The tests show that changing a rule and changing it back clears the warning.
- crates/aegis-model/src/project.rs:1689-1700 (`control_kind_fits`): a preset with no interface kinds counts as a component preset; otherwise the interface's kind must be in the preset's list. An unknown preset gives false.
- crates/aegis-model/src/project.rs doctest at about line 1724: just giving a control a preset kind does not copy the preset's rules, so a new control with a kind and no rules is warned at once. That is the specified behaviour, but slice 124f (client edits) should copy the rules when it applies a preset, or users will see this warning straight away.
Risk: low. It is only a warning: no error, no change to any risk number, no change to the snapshot format. It reads only data already in the project, with no new input boundary and no new dependency. The gate passed the existing lint tests.
View: git -C factory/worktrees/124-control-presets diff 8d6cfdcbe27bbf07d47e276bcbe58fabd482e219..HEAD
## Findings
- none. All five "Done when" items are met, and each new test would fail without the change: the model tests cover the same rules, a changed rule, setting it back, an extra rule, a missing rule, and a control with no kind; the fit test covers every case the prompt listed, including IDs that are not in the project; the client test checks the warning row text and that `error == false`. One small gap: no test uses an unknown preset in `control_kind_fits`, but the code handles it with the `control_kind_key` check.
## Lesson
none
NEXT: human
