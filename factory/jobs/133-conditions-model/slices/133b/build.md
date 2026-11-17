Commit: 958b5f7b6cc9925512549ee26ad9f90328ab46d6 Link conditions to elements and steps with guarded ops (133b)

Files:
- crates/aegis-model/src/applicability.rs
- crates/aegis-model/src/op.rs
- crates/aegis-model/src/project.rs
- crates/aegis-model/src/error.rs
- crates/aegis-model/src/store/wire.rs
- crates/aegis-model/src/store/op_log.rs
- crates/aegis-model/tests/conditions.rs
- crates/aegis-model/tests/common/mod.rs
- crates/aegis-client/tests/undo_every_op.rs

Done when:
1. `cargo test --locked -p aegis-model --test conditions` -> 11 passed, 0 failed. All eight condition ops (four element ops from 133a plus the four new link ops) invert exactly and are compared by `save()` bytes; every guard has an input only it refuses with the variant asserted; `ConditionInUse` covers an element link, a step link and the cleared case; lists are sorted/unique by public ID; insertion-order permutations compare equal and `diff` marks the owning element/tree modified.
2. `cargo test --locked -p aegis-model --test store` -> 20 passed, 1 ignored (the fixture regenerator). `link_ops_round_trip_through_the_op_log` in `tests/conditions.rs` covers the four new `OpWire` round trips through both `OpCodec` and `OpLogWriter`/`OpLogReader`; no snapshot bytes or envelope version changed.
3. `cargo test --locked -p aegis-client --test undo_every_op` -> 2 passed. `walk_conditions` now applies and undoes all eight condition ops.
4. `cargo fmt --all` -> clean. `scripts/ci.sh crates/aegis-model crates/aegis-client` -> fmt, clippy and test all ok for both crates.
5. NOT MET: `git diff --numstat 007c564..HEAD` (source only, excluding tests/docs/lock) is 667 counted lines against the slice's 600-line cap. Every test and quality command passes; only the review-size gate is over. I reduced the count from 848 by replacing doc-tested error variants with one-line docs, removing the per-kind `same_*` signature changes in favour of a resolved public-ID comparison in `Project::eq`, and sharing the element/step link helpers. The remaining size is the four ops with their checks and commits, the side table and step slot wiring, seven error variants, and the op-log codec arms the prompt requires. As in job 205 slice 205b, this needs the planner to raise the cap (or split the slice), not a code change.

Notes:
- Storage deviation: `SmallVec<[ConditionIdx; 2]>` was not used. `smallvec` is not a dependency of `aegis-model`, `plan.json` sets `new_deps: false`, and `Cargo.toml`/`Cargo.lock` are outside this slice's allow list, so adding the crate would fail the gate with `--locked`. Plain `Vec<ConditionIdx>` gives identical behaviour; the public read accessors still return `&[ConditionIdx]` exactly as specified.
- Comparison deviation: instead of extending each `same_*` function and the `diff_arena` closure with an `ElementRef`, `Project::eq` uses `link_map()`/`step_link_map()` which resolve every `ConditionIdx` to a `ConditionId` on both sides (L06, L29), and `merge_condition_link_changes` marks the owning element or tree modified in `diff`. This keeps raw indices out of both comparisons and avoids changing 13 `diff_arena` call sites.
- `check_provenance` runs inside the new validate helpers because those arms `return` early; `NeedCondition`/`UnneedCondition`/`NeedStepCondition`/`UnneedStepCondition` with a blank actor give `EmptyActor`.
- `condition_links` is sparse and canonical: empty element entries are removed, and a `TreeEntry::step_links` block is dropped when every `via`/`attack_type`/`condition` slot is empty.
- `RemoveCondition` now refuses while any element or step still needs it; `condition_usages` is the public (elements, steps) helper.
- The `every-op` walk in `tests/common/mod.rs` (`second_every_op`) exercises the four links; they are deliberately not added to `record_every_op` because conditions have no snapshot fields until 133c, matching the 133a treatment of the condition ops. The generative `Gen::op()` walk was left at 74 kinds because `ops_properties.rs` hard-codes the expected successful-kind count; extending it would fail that out-of-scope test.

NEXT: reviewer
