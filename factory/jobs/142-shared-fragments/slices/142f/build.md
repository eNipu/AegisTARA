Commit: e4882cf Expose undo-safe fragment edits and the fragment used-in relation (142f)
Files:
- crates/aegis-client/src/commands.rs
- crates/aegis-client/src/tree_diff.rs
- crates/aegis-client/src/relations.rs
- crates/aegis-client/src/inspector.rs
- crates/aegis-client/locales/en/main.ftl
- crates/aegis-client/locales/ja/main.ftl
- crates/aegis-client/locales/pseudo/main.ftl
- crates/aegis-client/tests/shared_fragments.rs (new)
- crates/aegis-client/tests/common/fragments.rs (new)
- crates/aegis-client/tests/inspector_view.rs
- crates/aegis-client/tests/relations.rs

Done when:
1. `cargo test --locked -p aegis-client --test shared_fragments --test undo_every_op --test remove_loaded --test relations --test inspector_view` — PASS (26 passed, 0 failed).
2. `scripts/ci.sh crates/aegis-client` — PASS (fmt, clippy, test all ok).
3. `cargo fmt --all` — PASS (clean).

Notes:
- 192 production changed lines (src + locales), under the 440 cap; tests excluded per prompt.
- Edit::NewFragment creates an OR-root Fragment-role tree with no threat link and `created()` selection `Selection::Tree(id)`; the NewTree to_ops arm is shared, with role chosen by variant. RenameFragment resolves the tree root and emits RenameTreeNode only when the role is Fragment (else EditError::Missing). UseFragment/UnuseFragment emit the single typed op; attachment never clears rated/shared leaves — the model guard refuses the batch atomically (asserted in tests).
- tree_diff: clear_step_ops emits UnuseFragment for reference leaves and skips own rating/via/type resets for both shared and fragment references; transfer_step_ops preserves the reference via UseFragment and additionally restores the source's step conditions, mitigations and review mark at the destination (shared references keep the prior behavior of skipping own-field resets).
- relations.rs registers exactly one `fragment-used-in` row (Tree -> Step, label `inspector-used-in`, many, read-only) reading live Fragment-role trees through Project::fragment_users, sorted stable tree/node IDs; no step-fragment row. THREAT_TREE candidates are now restricted to Threat-role trees.
- inspector.rs: reverse_relation_label arm FRAGMENT_USED_IN -> `inspector-rev-fragment-used-in`; shared_note for a fragment-referencing step shows "<Fragment noun> <TT visual ID>: <root label>" using the localized `fragment` key. New keys `fragment` and `inspector-rev-fragment-used-in` added to EN (Fragment), JA (フラグメント) and pseudo (⟦Frâgmént␣⟧).
- New shared fixture `tests/common/fragments.rs`: loaded (save/reload byte-exact) project with non-default provenance holding fragments A/B, an unused fragment, direct users in two threat trees plus fragment B, a transitive B user, ordinary and shared-step leaves, conditions/mitigation/review mark on a reference, and an extra link edge. Reused by shared_fragments.rs and inspector_view.rs.
- Tests cover exact Project and snapshot-byte equality after one undo and redo for create/use/unuse/rename, reference node removal, whole using-tree removal, outline replacement, branch/move/insert-parent (node-index shifts) and ordinary removal; in-use fragment removal, use on shared/rated leaves and wrong-role RenameFragment all fail atomically (project and bytes unchanged, no undo step). Registry candidates and direct Used-in identities are independently asserted; relations.rs fixture/oracle extended with a fragment and two users. inspector_view.rs asserts per EN/JA/pseudo the label identity/difference vs inspector-rev-conditions, key resolution, the single forward row on A with exactly three sorted direct targets, one reverse row per referencing step with Selection::Tree(A) and a note containing A's visual ID and root name, unused/Threat trees yielding no chips, ordinary/shared leaves unchanged, and Unuse/undo/redo row updates.
- undo_every_op fixtures were already current (UseFragment/UnuseFragment covered by predecessor slices); no changes needed there.
- Pre-existing dirty factory/board.json and factory/lessons.md and untracked job artifacts were left untouched and uncommitted.

NEXT: reviewer
