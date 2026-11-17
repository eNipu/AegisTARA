Commit: 9f782bb1e09bee23f5474352bfc83621182b8114 Test multi-link picker pick, unpick, chips, IME and 200-item window (150b)

Files:
- crates/aegis-client/tests/multi_link_picker.rs (new, 445 lines)

Done when:
- `cargo test --locked -p aegis-client --test multi_link_picker` — PASS, 7 tests run, 7 passed, 0 failed.

Before you commit (all run from the worktree root):
- `cargo fmt --all` — PASS (no diff).
- `scripts/ci.sh` (no arguments) — PASS: `ok workspace fmt`, `ok workspace clippy`, `ok workspace test`, all steps passed in 8 crate(s).
- `cargo test --locked -p aegis-client --test multi_link_picker` — PASS, 7/7.

Tests added:
1. `threat_damages_pick_and_unpick_are_one_undo_step_each`
2. `goal_controls_pick_and_unpick_are_one_undo_step_each`
3. `chip_removal_unlinks_with_one_step`
4. `escape_close_keeps_committed_filter`
5. `two_hundred_items_filter_and_window`
6. `ime_preedit_does_not_filter_until_committed`
7. `ascii_filter_is_case_insensitive`

Notes:
- The 200-item fixture alternates Japanese (`データ漏えい-NN`) and ASCII (`Data breach-NN`) names. This is needed for the prompt's own assertions that filtering by `データ` / `漏えい` leaves fewer than 200 visible items while `data` matches the ASCII half; an all-Japanese list could never shrink under `データ`.
- Both fixtures build the project through the prompt's `run(ws, Edit::…)` helper, then rebase it onto a fresh `Workspace::new(project.clone(), 0)`. This clears the setup edits from the undo stack so `can_undo()` is genuinely false after the toggles are undone. The asset carries all six properties because the spoofing threat category (index 0) requires authenticity on the asset.
- The pick/unpick tests undo the pick immediately and compare against the pre-pick clone as written. Because that undo removes the link, a single `Edit::LinkDamage/LinkGoalControl { linked: true }` setup edit re-creates it before the unpick; the final `undo()` of that setup edit is what empties the undo stack for the `can_undo()` assertion.
- The ASCII case-insensitive check is its own 7th test so the "all seven tests run" criterion is met literally; the IME test still covers composition, commit, and nested filter behaviour.
- No source file, dependency, `.slint`, snapshot, op-log or model file was touched. Only the one new test file exists in this slice.

NEXT: reviewer
