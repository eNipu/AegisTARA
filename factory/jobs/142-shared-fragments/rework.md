# 142e bounded follow-up: repair the sensitivity oracle, then finish the slice

Decision: NEXT: builder 142e. No requirement conflict; no plan, allow-list, cap or semantic change is authorized.

## Diagnosis and evidence

Checkout: `/Users/lamda/Documents/GitHub/AegisTARA`, branch `factory/142-shared-fragments`; predecessor `5e124b3`. Preserve all incomplete edits, the saved regression, board and predecessor artifacts.

`crates/aegis-model/tests/ops_properties.rs:130-149` supplies unchanged leaves to its perturbed full-tree fold through `effective_step_feasibility`. That accessor explicitly returns `None` for fragment references (`project.rs:1922-1934,4270-4288`). A reference other than the perturbed leaf consequently produces MissingLeafValue, even though the project's dependency-closure evaluation has a valid value. The production tipping-point path already obtains reference roots through `closure_evals`/`lifted_leaf` (`project.rs:5879-5938`); it does not invoke ProjectEvalCache.

The saved reproduction is `cc f85db38e039d089d086dc1c4ecb38fd0c94cc04a79648334fb5637ff3abf3da7` in `crates/aegis-model/tests/ops_properties.proptest-regressions`: two kind-22 AddAttackTree operations followed by kind-89 UseFragment. Generator arm 22..=23 can create a Fragment, and arm 89 accepts a link to it. `/tmp/142e-required-tests.log` records the unchanged-reference failure in both methodologies, node `0199c83c-0248-7000-8000-0000000f4248`.

Source-path baseline verification: `git diff 5e124b3 -- crates/aegis-model/tests/ops_properties.rs crates/aegis-model/tests/common/mod.rs` is empty; every production hunk starts at baseline line 15024 or later, in cache-related code. The full Project evaluation and tipping-point path used by this reproduction is unchanged. Thus this is an inherited oracle mismatch, not evidence against the new cache invalidation. No baseline runtime replay or new build was performed by the planner; do not claim one. Keep the reproduction as a deterministic regression and document this source comparison.

The proposed repair stays in `crates/aegis-model/tests/*`, already allowed by plan.json and the prompt. Tests and benches are excluded by `factory/verify.py:25-26`. Current production count against `5e124b3` is 226 added + 54 removed = 280/560; the oracle repair adds zero counted production lines. Recount the finished slice; the cap remains 560.

## Narrow repair, with independent expectations

1. Add only the test-local input resolver/fold needed by the existing sensitivity property, in `ops_properties.rs` or a narrowly named module under `tests/common/`. Do not alter production accessors, tipping-point semantics, generic evaluation, generator acceptance or case counts. Do not skip trees containing references, swallow oracle errors, clear the regression, or use a project/cache Evaluation's leaf/root as the expected value.
2. Independently resolve the unperturbed requested closure from public inputs: tree edges, own/shared ratings and factor resolution, `step_fragment_id`, applicability, step mitigations and control rules/targets/status. Existing own/shared rating resolution can remain the ordinary-leaf input accessor; it must never resolve a reference. For a reference, recursively fold the referenced tree from those inputs, then apply the referencing node's local ceilings. Memoize unperturbed fragment results by tree and mode; follow only the requested closure, not unrelated invalid trees or cached reverse indexes. `shared_fragments.rs:1533-1615` demonstrates the existing independent fixture fold, but its Spec is not a Project resolver.
3. Rules: excluded tree/node => Excluded; referenced Excluded root stays Excluded; unrated/invalid reference => missing/error, never a fabricated number. OR takes max of applicable children (all excluded => Excluded); AND/SEQ takes min when all children apply and is Excluded if any child is excluded. Preserve missing-value/empty-gate errors rather than inventing neutral values. Fold all reachable children so missing inputs are not hidden by short-circuiting. Own/shared and reference leaves are lowered by applicable local ceilings: Implemented counts in both modes, Planned only residual, Off neither. Rule targets include a via interface's owning component. Use public input scans, not production ceiling/explanation results.
4. A tipping point is a perturbation of the host's already-resolved leaf value, not an edit to a source fragment/shared block. Override only `(host TreeId, host NodeId)` with the adjacent level, after normal baseline resolution/ceilings; do not reapply a ceiling to the override or alter other references to the same source. Independently refold the host. Never substitute `tip.to_root()`, `eval.get(...)`, `Project::evaluate_*`, or cache results into this expectation.
5. Keep the current assertions and use the independent fold for the perturbed-root assertion. Add a fixed test for the saved three Seed values in both methodologies. Add a small known-value fixture where a host has two references and perturbing one leaves the other resolved, including a nested reference and a reference-local ceiling. Assert hand-derived baseline and perturbed roots. To prove falsifiability, independently enumerate adjacent levels from resolved host leaves and compare the complete sorted expected tipping tuples with the reported tips; this catches incorrect roots, extra tips and omitted tips, not just a panic. Use the same resolver in residual mode when checking that fixture.

## Finish only the already requested 142e work

Complete the outstanding deterministic evidence listed in build.md and prompt Do 5: simultaneous shared-user counter bounds, metadata zero folds, why-only derivation changes, conditions and user deletion, with forward/inverse checks and fresh scans of direct users. Keep the independent `oracle(spec, ...)` property. Cache-versus-Project/fresh-cache equality alone is not an independent numeric oracle; deterministic fixtures need hand-derived values or the independent input fold as well.

Fix the benchmark toggle so the first timed edit actually changes the diagnostic rating; keep construction outside timing. Preserve the 50-fragment fixture and 5 ms budget.

Run commands serially from the checkout, never concurrent builds:

- `cargo fmt --all`
- `scripts/ci.sh crates/aegis-model`
- `cargo test --locked -p aegis-model --test shared_fragments --test ops_properties`
- `cargo bench --locked -p aegis-model --bench eval -- fragment_single_edit_50`

Record exits, independent regression/fixture results, final production count, benchmark timing and measured nodes/threats in `factory/jobs/142-shared-fragments/slices/142e/build.md`. Review staged diff/status for unrelated edits before the single `(142e)` commit with the prescribed co-author trailer. Do not stage the board or predecessor artifacts. Preserve the generated regression in the slice's test changes.

If the independent resolver disagrees with production after correcting the input omission, report the exact fixture and values with NEXT: planner; do not tune the expectation to production. Otherwise finish the original handoff with NEXT: reviewer. No gate edits or scope expansion.
