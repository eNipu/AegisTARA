VERDICT: PASS
## For the owner
What changed: Widened `Edit::Unshare` in `commands.rs` to materialize fragment references via `CopyMode::Materialize` in one atomic `Vec<Op>` while keeping `SharedStep` unshare and 1-op `Edit::UnuseFragment` unchanged and leaving reusable sources intact.
Extended `fragments.rs` to resolve and distribute source/host assumption exclusions (`materialized_assumptions`) and preflight node, condition, mitigation, and assumption-exclude caps before mutation.
Added EN/JA/pseudo `unshare-fragment` Fluent labels, multi-op `undo_label::describe` recognition for fragment unshare, and regression, cap, oracle, and snapshot/op-log/workspace-file roundtrip tests in `shared_fragments.rs`.
Look at:
- `crates/aegis-client/src/commands.rs:1391-1470` — `unshare_fragment_ops` preflights assumption-exclude caps, clears outer metadata and `SetAssumptionExcludes` before `SetTreeNodeKind`, inserts materialized ops, and re-applies distributed excludes after new leaves exist.
- `crates/aegis-client/src/fragments.rs:433-537` — `resolve_node` and `push_assumptions` propagate outer/intermediate overlays, tree conditions, and step/tree assumption exclusions to copied leaves while copying raw fields (`feasibility`, `factors`, `via`, `attack_type`).
- `crates/aegis-client/src/undo_label.rs:215-249` — multi-op `Op::UnuseFragment` match distinguishes fragment unshare from `Edit::UnuseFragment` and node/tree removals (unsharing a bare unrated single-step fragment with zero metadata emits 1 op and keeps `UnuseFragment` labeling).
Risk: Low — Rework 1 isolated unrated leaves from `frag_excl_id` and `single_host` so `IndependentFold` and `eval_outcome_pair` now verify every exclusion mechanism, merged control ceiling (`low`), distributed condition transition, and single-step shared-field evaluation without `Unrated` masking; 1-op bare unrated single-step fragment unshare shares its single `Op::UnuseFragment` shape with `Edit::UnuseFragment` so `undo_label::describe` labels both as `unuse fragment` (verified by code inspection).
View: git -C /Users/lamda/Documents/GitHub/AegisTARA diff e9f51b4b0148f0ec0052cc2a13be9f5ee51cdcf7..HEAD
## Findings
none
## Lesson
[builder] Putting an unrated leaf in the same `OR` gate as excluded or merged-overlay branches forces evaluation to `Unrated` and masks failures in copied exclusions or overlays; keep unrated cases in a separate tree.
NEXT: human
