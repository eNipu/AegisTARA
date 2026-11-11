Commit: 84bf538 Paint the aegis-canvas display list in the attack tree tab (181b)

Files:
- spikes/gpui-shell/Cargo.toml (two path dependencies: aegis-canvas, aegis-model)
- spikes/gpui-shell/Cargo.lock (adds aegis-canvas, aegis-model, rstar, heapless 0.8 and friends; no gpui-pre*/gpui-component line changed, checked with `git diff spikes/gpui-shell/Cargo.lock | grep -i gpui`)
- spikes/gpui-shell/src/sample_tree.rs (new)
- spikes/gpui-shell/src/palette.rs (new)
- spikes/gpui-shell/src/args.rs (new)
- spikes/gpui-shell/src/canvas_tab.rs (new, `AttackTreeTab`)
- spikes/gpui-shell/src/main.rs (parses `--nodes`, builds the tree, exit 2 on error)
- spikes/gpui-shell/src/shell.rs (placeholder removed; `AttackTreeTab` in the centre tabs via `panel_handle`)

Size: about 340 changed source lines without tests and the lock file (cap 400).

Done when:
1. `cargo build --locked --release --manifest-path spikes/gpui-shell/Cargo.toml`: Finished release profile, no warnings.
2. `scripts/ci.sh spikes/gpui-shell`: ok fmt, ok clippy, ok test (all steps passed). `cargo fmt --all` and `cargo fmt --manifest-path spikes/gpui-shell/Cargo.toml` also run, no changes left.
3. 13-node tree, pan, zoom, click, inspector: NOT VERIFIED BY ME. `--nodes 13` starts and opens its window (seen in the CGWindowList), but this agent session has no screen-recording or event-posting permission (`CGPreflightScreenCaptureAccess` and `CGPreflightPostEventAccess` both false; `screencapture -x` says "could not create image from display"). So I could neither look at the drawing nor drive the mouse, and there is no `screenshots/181b.png`. The owner or reviewer has to do the step 8 manual check by hand.
4. 100,000-node tree: `--nodes 100000` starts, opens its window, about 157 MB resident a moment after start. I could not pan it, so I have no smoothness impression; 181c measures it.
Also checked: `--nodes 0` prints "--nodes must be between 1 and 1000000, got 0" and exits 2; `--nodes x` prints "--nodes: `x` is not a whole number" and exits 2.

Notes:
- GPUI paint calls used: `gpui::canvas(prepaint, paint)` inside a `div` that has the mouse and scroll listeners; `window.paint_quad(gpui::quad(bounds, radius, background, border_width, border_color, BorderStyle))`; `PathBuilder::stroke(px(w))` + `move_to` / `cubic_bezier_to(to, c1, c2)` / `line_to` + `build()` (returns `Result`; an error skips that item) + `window.paint_path(path, colour)`; `window.text_system().shape_line(..)` then `ShapedLine::paint(top_left, line_height = size, TextAlign::Left, None, window, cx)`; width for `Align::End` from `line.width`. Font is `window.text_style().font()`, with `FontWeight::BOLD` when bold.
- Differences from the prompt's facts: the wheel listener is `on_scroll` with `ScrollEvent` (`ScrollWheelEvent`/`on_scroll_wheel` are deprecated aliases in 0.3.12); I zoom in when `delta.pixel_delta(16px).y > 0`. The direction is a guess I could not test; if it feels inverted, swap the sign. `ShapedLine::paint` returns `Result`; a failure is ignored so the other items still paint.
- Dashes: dashed strokes go through `.dash_array(&[px(4.0), px(3.0)])`; dashed rect borders use `BorderStyle::Dashed`. Whether they render is not seen (no screen access). The generated tree has no shared subtrees, so it has no dashed items anyway.
- The prepaint closure holds `cx.entity()` and calls `entity.update` to write `viewport.size` and the canvas origin (used to turn window positions into canvas points), then calls `render`. That works but is awkward: the canvas API gives the bounds only during prepaint, so the view has to keep the last origin to convert mouse positions.
- Deviation 1: `wide_tree` returns `Result<AttackTree, ModelError>` rather than `AttackTree`, so the spike has no `unwrap`/`expect` (`AttackTree::new` and `add_child` both return `Result`). The unit test checks `.len()` on the `Ok` value.
- Deviation 2: `main.rs` builds the tree with `wide_tree(n, 4)` right after parsing the arguments, so a build error can exit with code 2 before the app starts. It passes `&AttackTree` to `Shell::new`, and `AttackTreeTab::new` builds the `Scene` from it once. The tree is dropped when the launch closure ends, so only the `Scene` stays in memory.
- `wide_tree` also stops when the frontier is empty, so `k = 0` returns the root instead of looping forever (the test helper would hang).
- Nothing was missing from `aegis-canvas`: `Scene::hit_test`, `kind`, `label`, `Viewport::to_world/pan_by_screen/zoom_at` and `SvgTheme::Light.hex` were enough. The adapter has to lean on `SvgTheme` for colours because there is no toolkit-neutral palette type. This is fine for the spike, but job 187 may want a `Paint -> [u8; 3]` table in the canvas crate.
- The viewport starts at origin (0, 0), zoom 1, with no fit-to-view (not asked for).

## Rework 1
Gate: 465 counted lines, cap 400.

Commit: 2484126 Move the spike unit tests to tests/unit.rs to fit the size cap (181b)

Fix: the three `#[cfg(test)] mod tests` blocks in `args.rs`, `palette.rs` and `sample_tree.rs` moved unchanged into `spikes/gpui-shell/tests/unit.rs`. The spike is a binary with no lib target, so the test file includes the three pure modules with `#[path = "../src/<name>.rs"] mod <name>;` (each with `#[allow(dead_code)]`, because the test does not call every item, e.g. `palette::color`). No product code changed; every test of item 7 is still there (7 tests in `tests/unit.rs`, all pass).

Counted lines now (`git diff --numstat 2d6c844 -- spikes/gpui-shell/src spikes/gpui-shell/Cargo.toml`): 364 (Cargo.toml 2, args 23, canvas_tab 241, main 22, palette 19, sample_tree 26, shell 31).

Before you commit, rerun:
- `cargo fmt --all`, `cargo fmt --manifest-path spikes/gpui-shell/Cargo.toml`: no changes left.
- `scripts/ci.sh spikes/gpui-shell`: ok fmt, ok clippy, ok test.
- `cargo build --locked --release --manifest-path spikes/gpui-shell/Cargo.toml`: Finished release profile.

Done when items 3 and 4 (manual run) are unchanged from the first round: still not verified by me for lack of screen access; the owner or reviewer must do step 8 by hand.

NEXT: reviewer
