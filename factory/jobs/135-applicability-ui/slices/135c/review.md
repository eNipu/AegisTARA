VERDICT: PASS
## For the owner
What changed: Added `excluded: Option<String>` to `NodeStyle` and `NodeAccess`, rendered 45-degree `Item::Polyline` hatch lines and a bold Graphite `"excluded"` label on excluded canvas nodes, and appended ` Excluded: <reasons>.` in `access::sentence`. Wired `tree_scene` in `aegis-client` to populate `NodeStyle::excluded` from `Project::applicability()` via `exclusion::reasons_text` while clearing score, tone, and cap on excluded nodes. Added unit and integration tests covering `access::sentence`, canvas/SVG rendering, and partial as well as fully-excluded attack trees.
Look at:
- `crates/aegis-canvas/src/render.rs:159-210` — 45-degree hatch segment clipping inside `rect`, `"excluded"` text emission, and `score_chars` room calculation for `via` chips.
- `crates/aegis-canvas/src/access.rs:156-158` — `" Excluded: <reasons>."` formatting in `sentence`.
- `crates/aegis-client/src/view.rs:1568-1611` — applicability lookup and clearing of `score`, `tone`, and `capped` when a node is excluded.
Risk: none — all logic is deterministic, restricted to the allowed canvas/client files with no new dependencies or `Item` variants, and verified by `scripts/ci.sh` plus the new `access`, `scene`, and `excluded_canvas` tests recorded in `gate.md`.
View: git -C factory/worktrees/135-applicability-ui diff afe855017ea9da04ab4e9f98d013c196ec8cc7e9..HEAD
## Findings
none
## Lesson
none
NEXT: human
