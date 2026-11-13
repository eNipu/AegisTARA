Commit: efcc00a Apply through and type from the outline as ops (122db)
Files: crates/aegis-client/src/outline.rs, commands.rs, session.rs, tests/outline_apply.rs (new), tests/step_via.rs
Done when:
1. `cargo test --locked -p aegis-client --test outline_apply` - 11 passed
2. `cargo test --locked -p aegis-client --test step_via` - 5 passed
3. `grep -n to_outline_annotated crates/aegis-client/src/session.rs` - one line (3332)
4. `scripts/ci.sh crates/aegis-client crates/aegis-shell` - all steps passed
5. Step 8 tests pass; `grep -c is_note_value outline.rs` prints 3
6. `git diff --numstat HEAD~1 -- crates/aegis-client/src` - 44+99+1 = 144 added lines (cap 250)
Notes:
- The ReplaceTree arm stayed short; the new ops are built in a private `note_ops` in commands.rs.
- `Project` has no `PartialEq`, so "project equal to before" in tests compares the annotated outline, node count and step values; the error test also checks the undo label is unchanged.
- `Edit::SetStepAttackType` takes a `usize` position, so tests use the position in `attack_types()`.
- The removed-component test: `Edit::RemoveComponent` was accepted with no step going through it.
- No shell or other client test broke.
NEXT: reviewer
