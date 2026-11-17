Commit: 24a2a10c9c75fe85b4af209b3ebacc9368eb1660 Add bidirectional relation reachability tests (149b)
Files: crates/aegis-client/tests/relations.rs (new, 471 lines; the only file in 149b's allow list)
Done when:
- `cargo test --locked -p aegis-client --test relations` -> exits 0; all four tests listed and passing: keys_and_labels_are_valid, many_matches_the_fixture, every_model_link_has_a_registry_row_and_vice_versa, every_link_is_reachable_both_ways.
- Temporarily deleted the THREAT_CLAIM row from `RELATIONS`, re-ran the same command -> 3 failures including `every_model_link_has_a_registry_row_and_vice_versa`, whose message printed the missing Threat->Claim pair as the symmetric difference; restored `relations.rs` with `git checkout --` before committing (final commit touches only `tests/relations.rs`).
- `cargo fmt --all` -> exit 0.
- `scripts/ci.sh` -> `ok workspace fmt`, `ok workspace clippy`, `ok workspace test`, "all steps passed in 8 crate(s)".
Notes:
- The 149a reviewer's extra requirement (architecture III.9.2: one instance of every link kind, each reachable through the registry in both directions) is exactly this slice's deliverable and is fully implemented. 149b's allow list contains only `crates/aegis-client/tests/relations.rs`, which is the sole file changed, so the requirement fits within scope; no `NEXT: planner` needed.
- Deviation from the prompt's literal type: `model_links`/`registry_links` return `BTreeSet<(Sel, Sel)>` with a local `Sel` newtype instead of `BTreeSet<(Selection, Selection)>`. `Selection` derives only `PartialEq, Eq, Hash` (view.rs:611), not `Ord`, and `BTreeSet` requires `Ord`; the orphan rule forbids implementing `Ord` for a foreign type in this external test crate. `Sel` provides a deterministic total order (variant rank + Debug of the inner ID), preserving the prompt's intent of sorted symmetric differences in failure output. No library code was changed.
- `every_link_is_reachable_both_ways` disambiguates DataFlow->Component links by the row whose `read` actually contains the target: `DATAFLOW_FROM` and `DATAFLOW_TO` share the same `(from, to)` ElementKind pair, so matching on kinds alone yields two rows, not the "exactly one" the prompt anticipates. After that filter the test still asserts exactly one row, plus forward, reverse, and `linked` reachability for both ends.
- Fixture yields 16 distinct link pairs (12 rows: 4 `many = true` rows with two targets each), satisfying the >= 12 assertion. `many_matches_the_fixture` finds exactly one owning source per row and checks `row.many == (count > 1)`.
NEXT: reviewer
