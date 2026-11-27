VERDICT: PASS
## For the owner
What changed: References now consume dependency roots with mode-specific host ceilings and applicability.
Full evaluation and cache builds fold dependencies first; reference-bearing calculation changes conservatively rebuild.
Stable-ID explanations cross fragment roots; engine semantics is 2, with historical compatibility preserved.
Look at:
- /Users/lamda/Documents/GitHub/AegisTARA/crates/aegis-model/src/project.rs:4407 — local exclusion precedes root consumption; excluded roots stay excluded and missing/error roots stay unrated.
- /Users/lamda/Documents/GitHub/AegisTARA/crates/aegis-model/src/project.rs:1529 — iterative cross-tree path preserves tree-local indices and requested mode, with at most eight crossings.
- /Users/lamda/Documents/GitHub/AegisTARA/crates/aegis-model/tests/shared_fragments.rs:1554 — independent recursive oracle, checked against every node/mode after forward, inverse and redo operations.
Risk: Reference-bearing calculation edits currently rebuild the whole cache; this is intentional until 142e. Explanations deliberately cross a reference even when its local ceiling decides the value: 142g must resolve host ceiling causes from the selected cached Evaluation, not deciding_step. Reviewed every changed line against 4ea7108. The independent oracle does not call production ordering/evaluation/cache: it folds OR=max, AND/SEQ=min, exclusion and unrated states separately, with explicit mode-counting. Assertions compare live/cache values and errors before any reload, then check snapshot/workspace reloads; explicit path, unlink/undo, exclusion and unrelated-tree regressions would fail without the feature. Existing boundary guards validate IDs, roles, cycles, eight-hop depth and 256 references per tree; new traversal uses ordered maps and bounded typed errors, with no new external input, paths, secrets or dependencies. The narrow lint change correctly avoids marking every valid reference unrated while target trees remain linted. Historical v1, current semantics-2 rejection and future semantics-3 fixtures remain distinct; protocol/minimum stay 2 and file format stays 1. Accepted the supplied script gate PASS (287 production changed lines); no checks rerun.
View: git -C /Users/lamda/Documents/GitHub/AegisTARA diff 4ea7108..HEAD
## Findings
none
## Lesson
none
NEXT: human
