VERDICT: CHANGES (small; fixed on review/post-129-fixes, 0a0f56f)
Reviewed: efb5977..cd26f5e against spec.md and UI-18.

Fine: pack `clause_refs` is optional and bounded on decode; old packs load; the tests check real behavior.

Fixed:
1. The sidebar (`session::nav_counts`) and `pages::page_count` held two copies of the counting rules, and the sidebar copy still had the `_ => None` catch-all that AC8 forbids. The sidebar now calls `page_count`.
2. `process_map` ran the lint twice; it now counts the `problems()` rows it already has.
3. `Methodology::new` did not check `clause_refs`, so a def built in code could write a pack that `read_pack` refuses. It now checks the caps, keys and labels (`TooManyClauseRefs`), with a test.
