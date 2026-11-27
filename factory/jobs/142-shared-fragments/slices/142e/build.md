Commit: 38c3033de5c66d15aba8807b3ae33ce00d7299aa Incrementally invalidate fragment users in dependency order (142e)
Files:
- /Users/lamda/Documents/GitHub/AegisTARA/crates/aegis-model/src/project.rs
- /Users/lamda/Documents/GitHub/AegisTARA/crates/aegis-model/benches/eval.rs
- /Users/lamda/Documents/GitHub/AegisTARA/crates/aegis-model/tests/shared_fragments.rs
- /Users/lamda/Documents/GitHub/AegisTARA/crates/aegis-model/tests/ops_properties.rs
- /Users/lamda/Documents/GitHub/AegisTARA/crates/aegis-model/tests/ops_properties.proptest-regressions (saved reproduction, kept)
- /Users/lamda/Documents/GitHub/AegisTARA/crates/aegis-model/tests/common/fragment_oracle.rs (new, test-only)
- /Users/lamda/Documents/GitHub/AegisTARA/crates/aegis-model/tests/common/mod.rs (adds `Seed::new` so the saved seeds can be replayed by value)

Done when:
- `cargo fmt --all`: exit 0.
- `scripts/ci.sh crates/aegis-model`: exit 0 (fmt ok, clippy ok, test ok).
- `cargo test --locked -p aegis-model --test shared_fragments --test ops_properties`: exit 0; ops_properties 12 passed, shared_fragments 31 passed.
- `cargo bench --locked -p aegis-model --bench eval -- fragment_single_edit_50`: exit 0, fresh process, 100 samples. Fastest 43.04 µs, median 45.1 µs, mean 47.59 µs, slowest 93.16 µs, all within the 5 ms budget. Each edit recomputes 378 nodes and 20 threats (50 fragments, 10 using trees, both modes).

Notes:
- Production diff against 5e124b3: 226 added and 54 removed in project.rs, 280 counted lines against the 560 cap. Tests and benches are excluded.
- Sensitivity oracle repair (test-only): `tests/common/fragment_oracle.rs` folds the requested closure from public inputs only: tree edges, `effective_step_feasibility` for own and shared steps (never for references), `step_fragment_id`, applicability, step mitigations, and control rules, targets and status (an interface's owning component is included). It memoizes fragment roots per tree and mode, treats an excluded root as excluded, and returns an error for unrated inputs or empty gates. It visits every child, and a tipping point overrides only the host's resolved leaf. `check_sensitivity_and_choke_points` now takes the perturbed root from this fold, and compares the complete sorted tipping tuples with an independent adjacent-level enumeration in both modes. No production evaluation result is used as an expected value.
- The saved seeds (cc f85db38e…) are replayed in `saved_fragment_sensitivity_regression_in_both_methodologies`, which also asserts that the fixture contains a reference. This test passes in both methodologies. The independent fold agreed with production everywhere, and no expectation was tuned to production. Per the planner's source comparison, the full Project evaluation and tipping-point path did not change, so this was an inherited mismatch in the oracle. No baseline runtime replay was performed.
- Known-value fixture `tipping_a_reference_matches_hand_derived_and_independent_tips`: host OR(low, ref→41 = AND(high, ref→42 = medium), ref→42 lowered by a planned very-low mitigation). The root is medium in both modes. Inherent tips: {40002 and 40003 medium→high}. Residual tips: {40002 medium→low, root low; 40002 medium→high, root high}. The hand-derived roots and tuples, the oracle, and the reported tips all agree.
- Deterministic counter evidence (recomputed nodes, threats):
  - Diamond rating edit: 20, 2, which is 10 folds per mode with tree 30 folded once and the unrelated tree 31 untouched.
  - Planned mitigation (residual only): 10, 1.
  - Implementing the control: 11, 1.
  - Why-only edits (fragment root unchanged): 4, 0 each. The cross-tree `fragment_why_path` and the cached tree-34 `why_path` both move from step 34001 to 34002.
  - Metadata renames: 0, 0.
  - A shared step used inside both fragments: one batch gives 16, 2 (separate propagation per user would give 20), and the undo also gives 16, 2.
- The forward/inverse loop checks, after every op and every inverse: cache against Project, the independent fold, and a fresh cache, plus direct users from a fresh scan of every step. It covers fragment-root rating, gate, mitigation and control-status edits; shared-step unrate and rerate; unlink and relink; a second reference to one fragment; an index-shifting insertion; tree-level and step-level conditions; and user deletion. User deletion means unneeding the step condition, unlinking, removing the step, removing a step before the references so their indices shift down, and unlinking and removing the whole using tree. All inverses restore the project exactly.
- Guards found while testing: a step with a rating, condition or controls cannot be removed, and neither can a tree holding references. The tests unlink first.
- Benchmark toggle fixed: the first timed sample now applies the other level, so every sample changes the sink's root (fastest sample went from 10.8 µs to 43.0 µs).
- Project-wide applicability changes still rebuild in full, fragments first, as the prompt allows. A fragment that becomes unrated makes `update_tree_mode` rebuild that one tree.
- Not committed and preserved: factory/board.json, factory/lessons.md, rework.md and the slice artifacts.

NEXT: reviewer
