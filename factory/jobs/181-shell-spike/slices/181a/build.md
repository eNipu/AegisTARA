# Build 181a: GPUI spike window with docks, project tree, inspector and text tab

Commit: d27869d Add the GPUI shell spike window with docks, tree, inspector and editor (181a)
Files:
- spikes/gpui-shell/Cargo.toml
- spikes/gpui-shell/Cargo.lock
- spikes/gpui-shell/README.md
- spikes/gpui-shell/src/main.rs
- spikes/gpui-shell/src/shell.rs
- spikes/gpui-shell/src/selection.rs
- spikes/gpui-shell/src/project_tree.rs
- spikes/gpui-shell/src/inspector.rs
- spikes/gpui-shell/src/editor_tab.rs

Done when:
1. `cargo build --locked --release --manifest-path spikes/gpui-shell/Cargo.toml`: passes. The stripped release binary is 16 MB.
2. `scripts/ci.sh spikes/gpui-shell`: fmt ok, clippy ok, test ok, "all steps passed in 1 crate(s)". `cargo fmt --all` and `cargo fmt --manifest-path spikes/gpui-shell/Cargo.toml` also ran clean.
3. `grep -n '## Licenses' spikes/gpui-shell/README.md`: found. 760 packages, no GPL-only entry.
4. Manual run: only partly checked (see Notes). `cargo run --release` opened one window, 1400 x 932 (900 plus the 32 px title bar), at a centred position. CGWindowList confirmed this, and the process stayed up with an empty stderr log until I stopped it. I could not click a tree row, type in the editor, drag a splitter or take a screenshot. This agent session has no Accessibility permission (`osascript` returned "not allowed assistive access", -1719) and no Screen Recording permission (`screencapture` returned "could not create image from display"). `cliclick` is not installed. **The owner needs to check these by hand:** click a tree row and see the inspector change, type in the editor tab, and drag a dock splitter. I did not commit a screenshot.

## Notes

- Versions in Cargo.lock, one of each: `gpui-pre 0.3.12` (as `gpui`), `gpui-pre-platform 0.3.12` (as `gpui_platform`, features `font-kit` and `runtime_shaders`), and `gpui-component 0.9.1`. It also pulls in `gpui-base 0.9.1`, but only through gpui-component.
- Lock file: 759 packages locked, and `cargo metadata` lists 760 including the spike itself. `r-efi` (MIT OR Apache-2.0 OR LGPL-2.1-or-later) and `self_cell` (Apache-2.0 OR GPL-2.0-only) are the only licenses that mention GPL, and both have a permissive alternative. Every `gpui-*` crate is Apache-2.0, except `gpui-pre-reqwest`, which is MIT OR Apache-2.0.
- Dock: the spike uses the gpui-component dock, not the resizable-panel fallback. `DockSkin::dock_area("shell", None, ..)` builds it. The centre is `set_center(tabs[Attack tree, Item definition])`, and the left and right docks are `set_dock(Left|Right, tabs[..])`. The dock widths are set with `set_dock_size(.., px(260.) / px(300.))`, because `set_dock` itself takes no size and a new dock starts at `PANEL_MIN_SIZE * 2`. Every panel goes in through `panel_handle(entity)`. The `Rc<DockSkin>` and the `Entity<Selection>` are kept in `Shell`.
- Editor: the tab uses `EditorState` + `Editor` (the code editor with line numbers), not Textarea. `EditorState::new(window, cx).default_value(sample_text(2000))` built on the first try.
- Selection flow: `ProjectTree` updates `Entity<Selection>` and calls `cx.notify()`. `Inspector` keeps a `cx.observe(&selection, ..)` subscription and redraws. This compiles, but the click path was not exercised (see Done when 4).
- Tree: I drew the rows by hand as indented `h_flex` rows with `on_click`. I did not use gpui-component's `tree` or `list` modules, because the prompt asked for a flat indented outline. Whether `tree::Tree` would suit the real workbench is still open for 181b/181c or the owner.
- What was hard in the toolkit (spike evidence): almost nothing once the API facts were known. The whole spike compiled with zero errors on the first build. The cost sits in discovery. The 0.9.1 API (DockSkin, the BasePanel/Panel split, panel_handle, the `#[doc(hidden)]` Editor) is unlike older tutorials, and the crate ships no examples, so the source is the only documentation. The dock has no size argument on `set_dock`, which is easy to miss. A clean release build of the 760-crate tree is slow (minutes), and clippy plus test in debug took about 4 minutes on the shared target.
- `main` does not unwrap. If `open_window` fails, it prints the error and quits.

### Findings so far (from the earlier stopped attempt, kept)

- Versions: `gpui-component 0.9.1`, `gpui-pre 0.3.12` (crates.io snapshot of zed@d2a4fb6, Apache-2.0). Plain `gpui` on crates.io is 0.2.2, and gpui-component does not use it.
- `gpui-pre` alone has no desktop platform. `Application::with_platform` needs `gpui-pre-platform`, which the planner then allowed as the third pin.
- Worktree trap: without its own `[workspace]` table, Cargo walks up from the worktree to the main checkout's root `Cargo.toml` and refuses to build. The spike keeps an empty `[workspace]` table. The slint spike would hit the same problem in a worktree.
- The 0.9.1 dock API has no `DockItem`. It uses `DockLayout::{h_split, v_split, tabs}`, `.panel_view(panel_handle(..), cx)` and `DockArea::set_center` / `set_dock`. A panel without `panel_handle` shows no title.
- Text: `InputState` is single-line only. `TextareaState` and `EditorState` are the multi-line options.

## Rework 1

Gate item: "too big to review: 490 changed lines, cap 400". Fixed in a new commit:
2d6c844 Shrink the GPUI shell spike under the review cap with a panel macro (181a)

- The counted lines are now 398: Cargo.toml 22, editor_tab.rs 55, inspector.rs 49, main.rs 60, project_tree.rs 118, selection.rs 18 and shell.rs 76. Cargo.lock and README.md are not counted. I checked this with `git diff --numstat 60bc411 HEAD`.
- The biggest cut is a `dock_panel!(Type, "panel_name", "Title")` macro in main.rs. It implements `BasePanel`, `Panel`, `EventEmitter<PanelEvent>` and `Focusable` for all four panels, so that code is written once instead of four times. Spike finding: every gpui-component dock panel needs four trait impls of about 20 lines that never change. The real workbench will want the same kind of helper.
- Smaller cuts: the hand-written `WhenSelected` trait is replaced by GPUI's `FluentBuilder::when`. `sample_text` and `select` are shorter. `AttackTreeTab::new` is gone. Cargo.toml comments and doc comments are shorter. I dropped two extra test asserts. The required ones stay: the 8 depths, row 0's kind `workspace`, 2000 lines, and the text starting with `# Item definition`.
- Behaviour changes only at the edge: `sample_text(0)` now returns the header line (the doc comment says this). The spike only calls `sample_text(2000)`.
- I kept every "Done when" item and checked them again:
  1. `cargo build --locked --release --manifest-path spikes/gpui-shell/Cargo.toml` passes.
  2. `scripts/ci.sh spikes/gpui-shell` passes fmt, clippy and test. `cargo fmt --all` and `cargo fmt --manifest-path spikes/gpui-shell/Cargo.toml` are clean.
  3. `grep -q '## Licenses' spikes/gpui-shell/README.md` passes.
  4. I started the release binary again. It stayed up for 6 s with an empty stderr. Clicking a tree row, typing and dragging a splitter still need the owner to check by hand, for the same permission reasons as before.

NEXT: reviewer
