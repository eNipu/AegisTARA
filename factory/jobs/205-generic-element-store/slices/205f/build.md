Commit: 07ebaa6 Diff generic elements, record ADR 0068 and the architecture entry (205f)

Files:
- crates/aegis-model/src/element.rs (ElementStore::diff, imports)
- crates/aegis-model/src/diff.rs (ProjectDiff field, elements() accessor with doc test, is_empty)
- crates/aegis-model/src/project.rs (Project::diff sets elements)
- crates/aegis-model/tests/elements.rs (four diff tests)
- crates/aegis-model/tests/ops_properties.rs (205e filter removed, element assertions in check_diff_pair)
- crates/aegis-model/tests/common/mod.rs (Seed::is_element_op and its impl block removed)
- docs/adr/0068-generic-element-store.md (new)
- docs/architecture.md (element.rs row, ElementId, Op count 79, project.rs and diff.rs rows)

Done when:
1. `cargo test --locked -p aegis-model --test elements`: 34 passed, 0 failed.
2. `cargo test --locked -p aegis-model --test ops_properties`: 11 passed, 0 failed, including diff_soundness_first_methodology and diff_soundness_second_methodology with the element ops in the generator.
3. `grep -rn "is_element_op" crates/`: no output, exit 1.
4. `scripts/ci.sh`: fmt, clippy and test ok for the workspace, "all steps passed in 8 crate(s)", exit 0.
5. `grep -n "Fuzz" docs/adr/0068-generic-element-store.md`: line 148 "## Fuzz result", with the three runs recorded.
6. `git diff --numstat HEAD~1 -- crates/aegis-model/src`: 19+2 diff.rs, 49 element.rs, 1 project.rs = 71 changed lines (cap 200).

Notes:
- Diff complexity: ElementStore::diff walks kind_of in ID order as the prompt asks, but for IDs present in both it compares the elements with get(), which is a log-time lookup. The cost is O((n + m) log n), not O(n + m). Making it linear would need a different walk; I kept the prompt's walk and documented the cost in the doc comment.
- The diff property tests cover the six element ops in check_diff_sequence. No filter, seed range or case count was changed.
- Seed::is_element_op was used only by the removed filter, so the impl block is gone.
- Fuzz: `crates/aegis-model/fuzz/run.sh 60 <target>` built the binaries but failed to run them. The script looks for them in the fuzz crate's own target folder, while CARGO_TARGET_DIR (set as the rules require) sent the build to the shared folder. I ran the same binaries from the shared folder with the script's arguments (`-max_total_time=60`, the same corpus and seeds folders). Results: snapshot 1,468,292 runs, op_log 995,857 runs, workspace 1,371,114 runs, each 61 s, exit 0, no artifacts. The fuzz targets and run.sh are unchanged. The ADR records this. The script's path issue is a pre-existing problem with CARGO_TARGET_DIR and is not fixed here.
- architecture.md: beyond the rows the prompt lists, I changed the diff.rs row from "across all 11 arenas" to also name the generic elements, so the row does not go stale.
- Op variant count: I counted the variants of the `Op` enum in op.rs (79). The old row said 63, which was stale before this job.
- The ADR's MAX_WORKSPACE_FILES value (10,011) is taken from store/mod.rs: 10 + KINDS.len() + 10,000.
- Nothing was pushed. The work is one commit on branch factory/205-generic-element-store.

NEXT: reviewer
