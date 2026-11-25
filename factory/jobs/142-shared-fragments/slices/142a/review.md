VERDICT: PASS
## For the owner
What changed: attack trees now carry an immutable role, either Threat (the default) or Fragment. The role is fixed by `Op::AddAttackTree { role, .. }`, and no op changes it.
Fragments show as `TTnn` and threat trees as `ATnn`. Both draw on one shared number sequence (AT00, TT01, AT02), and a number is never reused. Removing a tree and undoing the removal restores both its role and its number.
Snapshots, op logs and workspace files store `"role": "fragment"` and leave the field out for Threat, so older files load as threat trees. A visual ID or tree file name whose prefix contradicts the stored role is refused, and so is an unknown role value.
Look at: crates/aegis-model/src/store/snapshot.rs:1604. This check refuses a snapshot whose visual ID prefix (AT or TT) disagrees with the stored role. A shared `used` set keeps AT01 and TT01 from both claiming number 1 (DuplicateEntry).
crates/aegis-model/src/store/workspace.rs:269. `tree_file_number` reads the role from the raw JSON before the snapshot loader runs, so the error names the tree file. It must stay in step with the serde names in `TreeRole` ("threat" and "fragment"); a role value it does not recognise also returns WorkspaceTreeName.
crates/aegis-model/src/project.rs:13548 and :11819. These put the role into tree equality (which `Project::eq` and the diff both use) and into the undo op that `RemoveAttackTree` returns.
Risk: low. Nothing in the client or server creates a fragment yet: every construction site passes `Default::default()`, which is Threat. Because of that, an older peer cannot receive `role: "fragment"` until 142b adds the fragment ops and bumps the protocol version.
A threat scenario can still link to a fragment tree. The prompt assigns that guard to 142b, so it is not counted as a defect here.
How it was checked: I read every changed line. All 9 tests in `shared_fragments.rs` fail if their feature is removed: role in `same_tree`, role in the undo op, the `skip_serializing_if` default omission, the snapshot prefix check, the workspace name check, and the serde default for legacy files.
The random op generator (tests/common/mod.rs:1509) makes about one AddAttackTree in four a fragment, so `ops_properties` covers role round trips and undo.
No code still uses `VisualPrefix::Tree` directly for display or parsing. Nothing depends on the order of `VisualPrefix` variants, so adding `Fragment` in the middle is safe. `Op` has no serde derive, so its only wire format is the op log, which handles the role.
No new dependency was added, and the format version was not changed.
View: git -C /Users/lamda/Documents/GitHub/AegisTARA diff a5877d6dd907e4c67c77a3bcc9834b6410c3d429..7dd201c
## Findings
- none
## Lesson
none
NEXT: human
