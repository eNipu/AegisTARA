VERDICT: PASS
## For the owner
What changed: the spike's "Attack tree" tab now draws the real aegis-canvas tree (boxes, curves, labels) with GPUI.
Drag pans, the wheel zooms, a click selects a node and sends its kind, label and number to the inspector.
`--nodes N` (default 13, 1..1,000,000) picks the size of a generated tree; bad values exit with code 2.
Look at:
- The manual check (Done when 3 and 4) has NOT been done. The builder had no screen access. Run `cargo run --release --manifest-path spikes/gpui-shell/Cargo.toml -- --nodes 13` and then `-- --nodes 100000` yourself. Check that boxes, curves and labels show, that drag pans, the wheel zooms and a click fills the inspector, and that the 100k tree pans. Note how smooth it feels.
- spikes/gpui-shell/src/canvas_tab.rs:87-95: the zoom direction is a guess (`dy > 0` zooms in). If it feels backwards, flip the sign.
- spikes/gpui-shell/src/canvas_tab.rs:101-109: prepaint saves the canvas origin and size on the view so mouse positions can be converted later. This is the adapter pattern job 187 will copy.
Risk: low; this is spike code only. Nothing under `crates/` changed. The new dependencies are only the two path crates plus rstar and heapless, which come through aegis-canvas. No gpui-pre or gpui-component version changed in Cargo.lock. Command-line input is capped at 1,000,000 nodes, and the error text quotes one argv value. The drawing is unverified until you do the manual run.
View: git -C factory/worktrees/181-shell-spike diff 2d6c8442c39b2859e02c87aaba009eb08f0afa44..HEAD
## Findings
- No blocking defects. Notes, not required:
- spikes/gpui-shell/src/canvas_tab.rs:51-69: a click on empty space clears the highlight but leaves the old node in the inspector. The prompt did not ask for this to be cleared. Fine for the spike; job 187 should decide.
- spikes/gpui-shell/src/canvas_tab.rs:72-80: if the mouse is released outside the tab, `drag_from` stays set. A later drag that starts outside and moves in would jump once. Minor in a spike.
- The deviations are reasonable and stated in build.md: `wide_tree` returns `Result`, the tree is built in main, `on_scroll`/`ScrollEvent` are used, and the unit tests moved to tests/unit.rs via `#[path]`. Test check: every item-7 test calls the new functions directly, so each would fail if the change were removed. Text keeps the right byte length after `\n` is replaced by a space, because both are one byte. Scene vertex indices match `NodeIdx::index()` (scene.rs 301-312, render.rs 88), so the inspector lookup is right.
## Lesson
[planner] A "Done when" item that needs eyes on a GUI cannot be met by a headless builder; plan it as an owner check (or give the builder screen-recording permission) instead of making it a builder criterion.
NEXT: human
