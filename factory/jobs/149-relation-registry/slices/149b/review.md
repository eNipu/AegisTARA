VERDICT: PASS
## For the owner
What changed: Added one 471-line integration test file, the only file in 149b's allow list.
It builds a Project fixture holding one instance of every registry row (12 rows, 16 links: 4 `many=true` rows with two targets each).
It compares two independent link enumerations (model arenas vs. `RELATIONS`), checks forward/reverse/`linked` reachability for every link, and checks key/label/`many` validity; all four tests and the gate pass.
Look at: tests/relations.rs:251 (`model_links` reads arenas directly — the independence that makes this a real safety net); tests/relations.rs:364 (per-link disambiguation of the two DataFlow->Component rows by which `read` contains the target — worth confirming it cannot mask a miswired read); tests/relations.rs:405 (hardcoded expected-key list is what makes a removed row fail this test too).
Risk: none. The builder's "deleted a row, saw 3 failures" claim is plausible from the code: removing THREAT_CLAIM makes `relation(THREAT_CLAIM)` return None (keys_and_labels_are_valid fails), leaves Threat->Claim in the model but not the registry (every_model_link_has_a_registry_row_and_vice_versa fails), and leaves no matching row for that link (every_link_is_reachable_both_ways fails its `assert_eq!(n, 1)`); many_matches_the_fixture still passes because it iterates only existing rows — exactly 3. Confirmed by tracing the test and relations.rs code, not by rerunning the gate. The documented `Sel` newtype deviation is necessary (`Selection` has no `Ord`, orphan rule) and preserves deterministic sorted diffs. No correctness, security or scope defect.
View: git -C factory/worktrees/149-relation-registry diff aa591238a121b84ce1d7e3328425592b4477ab5f..HEAD
## Findings
none
## Lesson
none
NEXT: human
