VERDICT: PASS
## For the owner
What changed: Added typed create/use/unuse/rename edits for fragment trees with normal provenance, unchanged filtering and creation selection.
Structural removal and transfer now preserve fragment references, conditions, mitigations and review marks through exact undo/redo.
Added one read-only Tree-to-Step `fragment-used-in` relation plus localized reverse labels and fragment notes in EN, JA and pseudo.
Look at: `crates/aegis-client/src/tree_diff.rs:157-350` — reference unlink/transfer ordering and metadata preservation are the main correctness risk.
Look at: `crates/aegis-client/src/relations.rs:170-173,253-269,412-427` — confirms the sole read-only relation, Threat-only candidates and live Fragment-role direct users.
Look at: `crates/aegis-client/tests/shared_fragments.rs:27-230` and `crates/aegis-client/tests/inspector_view.rs:718-830` — loaded non-default-provenance fixtures independently assert exact bytes, relation identities, labels and notes.
Risk: Structural edits could drop links or attached metadata, and deletion could silently remove users; exact Project/snapshot-byte undo/redo coverage and atomic invalid-edit tests check these paths. The registered relation has no `STEP_FRAGMENT` counterpart, and all three catalogs resolve the new labels.
View: git -C /Users/lamda/Documents/GitHub/AegisTARA diff 38c3033..HEAD
## Findings
- none
## Lesson
none
NEXT: human
