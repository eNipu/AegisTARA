# UI spike: iced

The SPEC app (`../SPEC.md`) built with **iced 0.14**, the Elm-style Rust GUI library.

| Crate | Version | License | Role |
|---|---|---|---|
| `iced` | =0.14.0 | MIT | Application, widgets, canvas. Features: `wgpu`, `tiny-skia`, `canvas`, `x11`, `crisp`, `thread-pool`, `fira-sans`, `sysinfo` |
| `iced_core`, `iced_graphics` | =0.14.0 | MIT | Already in iced's tree; used directly only to measure text width for the badge pills |
| resolved: `iced_widget` 0.14.2, `iced_winit` 0.14.1, `iced_wgpu` 0.14.0, `iced_tiny_skia` 0.14.1 | | MIT | |
| resolved: `wgpu` 27.0.1, `winit` 0.30.13, `tiny-skia` 0.11.4, `cosmic-text` 0.15.0 | | MIT/Apache-2.0, BSD-3-Clause (tiny-skia) | |
| `x11rb` (Linux only) | 0.13 | MIT OR Apache-2.0 | Test harness only: sets X input focus when no window manager runs (see below) |

**iced_aw is not used.** It is the usual companion crate for menus. I checked its source
(0.14.1): `ContextMenuOverlay` closes on any left-button release and has no `Overlay::overlay`
method, so nested overlays never open: no submenus and no `pick_list` inside it. Its `MenuBar`
cascades, but only from a menu-bar button, not as a popup at a point.

All code is in `src/main.rs`: 978 lines, formatted with `rustfmt.toml` at max_width 120. Every
factor, level, label, value and rating color comes from `spikes/shared/pack.json` at runtime.
The only fixed strings in the UI are the verbs "Gate type", "Add child step", "Delete",
"Edit feasibility…", "Rationale", "Preview", "OK" and "Cancel". AND and OR come from
`Kind::label()`.

Licenses (`cargo metadata`, Linux target, 204 packages including the two local crates): MIT, Apache-2.0, BSD-2/3-Clause, ISC,
Zlib, CC0-1.0, Unlicense OR MIT, and Unicode-3.0. There is no GPL-only crate. `self_cell` is
"Apache-2.0 OR GPL-2.0-only", so use it under Apache-2.0. The embedded Fira Sans font
(`fira-sans` feature) is OFL-1.1, so ship its notice.

## Build and run

The target folder below is only an example; any folder outside the repo works.

```sh
# Linux (tested on Ubuntu 24.04). Build: Rust >= 1.88 (tested 1.97) and a C toolchain.
# Runtime: X11 (or XWayland), libxkbcommon-x11-0, and a Vulkan or GL driver; with no GPU at all,
# iced falls back to its CPU renderer by itself. The release binary links only libc, libm and
# libgcc_s; everything else is loaded with dlopen at runtime.
cd spikes/iced
CARGO_TARGET_DIR=/tmp/targets/iced cargo run --release
NODES=2000 cargo run --release           # scale test
READONLY=1 cargo run --release           # read-only mode
ICED_BACKEND=tiny-skia cargo run --release   # force the CPU (software) renderer
```

- **Windows 10/11.** Use the MSVC Rust toolchain with Visual Studio Build Tools ("Desktop
  development with C++"), then run `cargo run --release` in `spikes\iced`. No other SDKs are
  needed. wgpu uses DX12, Vulkan or GL, and the tiny-skia fallback presents through
  `softbuffer`. **Verified here:** `cargo check --target x86_64-pc-windows-msvc` passes with no
  warnings (2m31s cold). Not built, linked or run on real Windows.
- **macOS.** Install the Xcode command-line tools, then run `cargo run --release` (Metal
  through wgpu). **Verified here:** `cargo check --target aarch64-apple-darwin` passes. Cargo adds
  a future-incompatibility note for `block` 0.1.6, which is in the macOS dependency tree. Not run.
- **Wayland.** The `wayland` feature is left off to reuse the probe's build cache, so on a
  Wayland desktop the app runs through XWayland. For native Wayland, add `"wayland"` to the iced
  features.

Environment variables:

| Variable | Effect |
|---|---|
| `NODES=<n>` | Adds n generated steps (spike-model) |
| `READONLY=1` | Read-only mode (spike-model) |
| `ICED_BACKEND=tiny-skia` (or `wgpu`) | Chooses the renderer (iced). Default: try wgpu, then tiny-skia |
| `WGPU_BACKEND=vulkan\|gl\|dx12\|metal` | Restricts wgpu's API (wgpu) |
| `PERF=1` | Prints `PAINT <ms> zoom=.. drawn=..` for each canvas geometry rebuild |
| `RUST_LOG` | Default `warn,iced_wgpu=info`, which logs the adapter wgpu selects |
| `AEGIS_SPIKE_DATA` | Folder with `pack.json` and `tree.json` |

Stdout: `NODE` and `RESULT` lines at start, `CHANGE`/`RESULT` after every edit, plus `RENDERER`,
`FIRST_DRAW`, `MENU_OPEN`, `KBMENU_OPEN`, `POPOVER_OPEN`, `PREVIEW`, `POPOVER_OK`,
`POPOVER_CANCEL`, `SELECT`, `ZOOM`, `PAN`, `UNDO nothing` and `REJECTED` for the tests. The
status bar shows the selection, zoom, node count, read-only state and the active renderer, for
example `Vulkan (llvmpipe …)` or `tiny-skia (CPU)`, from `iced::system::information()`.

## Approach

**Canvas (built-in `canvas` widget, custom drawing).** `canvas::Program` draws the tree into a
`canvas::Cache`. The cache is cleared only when the model generation, zoom, pan or selection
changes. Nodes are rounded rectangles with a 14 px title wrapped to the node width, a rating
pill whose width comes from measuring the text (`iced_graphics::text::Paragraph`, cached), and
an AND/OR chip on gates. Edges are cubic Béziers. Pan (left-drag on empty canvas), zoom around
the cursor (wheel), hit-testing, culling of nodes and edges outside the view, and level of
detail (no text below 35 % zoom) are all hand-written. iced has no node-graph or scene widget.
Everything is drawn in screen space (`world * zoom + pan`) instead of with `Frame::scale`: see
"What was hard".

**Context menu (custom, built from built-in primitives).** iced 0.14 has no context menu,
submenu, menu item or popover widget. A right-press in the canvas hit-tests and publishes
`OpenMenu(node, cursor)`. `view()` then adds layers on top of the canvas in a `stack`:
- a full-window `mouse_area` backdrop that closes the menu on any press;
- the menu panel, `pin(opaque(container(column(rows)))).x(..).y(..)`;
- the submenu panel, built the same way.

The rows come from a small data model (`menu_items()`), rebuilt from the model and pack on every
`view()`. Steps get one submenu per pack factor (label, current level as detail), each with one
radio-style row per level ("● label" plus value, current level marked), then "Edit
feasibility…" and "Delete". Gates get "Gate type" (AND/OR, current one marked), "Add child step"
and "Delete" (not on the root). Each row is a `mouse_area(container(row![marker, label, detail,
›]))`: hover highlights it and opens its submenu, and release runs it. Disabled rows are greyed
and have no release handler. The menu flips left or up at the window edge, and the submenu flips
to the left side. Keyboard support is custom: `keyboard::listen()` (keys no widget captured)
handles Shift+F10 and the Menu key (anchored at the selected node), Up/Down, Right/Enter/Space
into a submenu, Enter to choose, and Left/Escape to go back or close. A keyboard-opened menu
ignores hover until the mouse really moves. Without that, the resting cursor would steal focus,
because `mouse_area` fires `on_enter` on any event once its bounds change.

**Edit popover (built-in widgets in a custom anchored layer).** Another `pin(opaque(..))` layer,
placed right of the node's screen rectangle. It flips to the left when there is no room and is
clamped to the window. It recomputes on every view, so it stays attached while the canvas pans.
Content: one built-in `pick_list` per factor (options "label (value)" from the pack), a built-in
multi-line `text_editor` for the rationale, a live preview pill `"<AP> · <rating>"` from
`model.preview()` in the rating color, and built-in `button`s for Cancel and OK. OK calls
`set_factors`, and Escape cancels. The popover is not modal; the canvas stays usable.

## Feature status

Tested headless with `spikes/tools/gui_run.sh` (Xvfb, lavapipe/llvmpipe). The final screenshots
in `screenshots/` come from the release binary on Vulkan (lavapipe). The same main procedure
also passed with `ICED_BACKEND=tiny-skia` and with the debug binary.

| ID | Status | Built-in or custom | Evidence |
|---|---|---|---|
| F1 Canvas | works | custom drawing on built-in `canvas` | `01_initial.png`: SPEC colors, radius 8, curved edges, pack-colored badges, AND/OR chips |
| F2 Right-click menu | works | custom (stack + pin + mouse_area + opaque) | `02_menu.png`: opens at the cursor above the canvas; a header shows the node |
| F3 Factor submenus | works | custom, data-driven | `03_submenu.png`: 5 submenus with pack labels; levels with values, "● Specialized" marked. Standard gives `RESULT s6 11 High`, `g2 11 High`, `g0 11 High` |
| F4 Live update | works | model + cache clear | `04_changed.png`: red High badges on s6, g2, g0 |
| F5 Gate menu | works | custom | `09_gate_menu.png`: Gate type (OR marked), Add child step, Delete; the menu flips up at the bottom. AND gives `RESULT g3 55 Very low`, root unchanged. Add child step on g1 gave `n1 0 High`; Delete on n1 removed it |
| F6 Edit popover | works | built-in pick_list, text_editor, button; custom anchoring | `06_popover.png`, `07_popover_edited.png`: Elapsed time ≤ 1 day, live preview `14 · Medium`. OK gives `RESULT s6 14 Medium`, `g2 14 Medium`, `g0 14 Medium` (`08_after_ok.png`) and the rationale "Needs OBD physical access". Escape (on s5) and Cancel (after a change with preview 14) close without a `CHANGE` |
| F7 Pan and zoom | works | custom | `11_zoom_pan.png`: wheel zoom around the cursor, drag on empty canvas (`PAN -790 -475`). Menus still hit the right node when transformed |
| F8 Keyboard | works | custom (`keyboard::listen`) | `10_keyboard_menu.png`: click s2, Shift+F10 opens the menu at the node with the first row focused. Down×4, Right, Down, Enter sets s2 Equipment = Specialized (`RESULT s2 16 Medium`). The Menu key opens the same menu, Escape closes it. Only `text_editor`/`text_input` take keyboard focus in iced 0.14; pick_list and buttons in the popover cannot be tabbed to |
| F9 Read-only | works | custom (greyed rows without handlers) | `12_readonly.png`: the menu and submenus open, current level still marked; levels, Edit and Delete are greyed. Clicks and Enter on them do nothing (no `CHANGE`). Ctrl+Z prints `UNDO nothing` |
| F10 Undo | works | model `undo` + `keyboard::listen` | `05_undo.png`: `RESULT g0 15 Medium`; it also undid the g3 gate change. Ctrl+Z is ignored while the popover is open, so it cannot undo the model behind the editor (text_editor has no undo of its own in 0.14) |
| F11 Scale | works | custom culling + LOD | `13_scale.png`: `NODES=2000` (2114 nodes). Zoom 56 %, the menu on generated step f1 flips up and its submenu flips left; choosing a level gives `CHANGE set_factors f1`. First visible frame 0.38 s (Vulkan) / 0.16 s (tiny-skia). Geometry rebuild 0.7–2.3 ms per frame with about 15–50 nodes in view. The menu also works at 22 % zoom |
| F12 Text input | works | built-in `text_editor` | `07_popover_edited.png`: typed "Needs OBD access", Ctrl+Left, inserted "physical ", Home, Shift+Ctrl+Right selects "Needs". In a second run, Shift+Home, Ctrl+C, End, Ctrl+V gave the rationale "abcabc", so the clipboard works. Multi-line and wrapping are built in. IME support is listed in the 0.14 changelog but was not tested here. No undo inside the field in 0.14 |
| F13 Accessibility | not_done (toolkit gap) | none | iced 0.14 has no AccessKit, UI Automation or NSAccessibility support. There is no `accesskit` in the dependency tree and none in the iced crate sources. Issue #552 is open; only the pop-os/iced fork (COSMIC) has AccessKit work. Screen readers see nothing, and canvas content would be invisible anyway |

Extra: `14_edge.png` shows a right-click on s9 near the bottom: the menu flips up and the
submenu stays in the window. In another run (pan right, then right-click s9 at x = 1240), the
menu flips left and the submenu opens on the left side.

## Renderers and the no-GPU case

iced builds both renderers into one binary and picks one at startup: wgpu first, then
tiny-skia. The status bar and the `RENDERER` line show which one is active. Results under Xvfb
(Mesa 25.2, 4 shared vCPUs), measured from exec to the first frame on screen:

| Setting | Active renderer | First frame (release) | Result |
|---|---|---|---|
| default | `Vulkan (llvmpipe)` (lavapipe) | 0.35–0.39 s | full procedure passes |
| `WGPU_BACKEND=gl` | `Gl (llvmpipe)` | 0.32 s | renders, s6 change correct |
| `ICED_BACKEND=tiny-skia` | `tiny-skia (CPU)` | 0.17 s | full procedure passes, same RESULT lines |
| `VK_ICD_FILENAMES=/nonexistent.json WGPU_BACKEND=vulkan` (no GPU) | `tiny-skia (CPU)`, automatic fallback | 0.17 s | renders, s6 change correct |
| no Vulkan ICD and no EGL vendor (no GPU, all APIs) | `tiny-skia (CPU)`, automatic fallback | 0.17 s | renders, s6 change correct |

tiny-skia output looks the same as Vulkan: the same layout, shadows and rounded corners; only
text anti-aliasing pixels differ. So the CPU renderer works for remote desktops without a GPU,
with no code change. Caveats: in 0.14 the backend is chosen only at startup and only through
`ICED_BACKEND`. If wgpu starts on a slow software adapter (for example WARP or llvmpipe),
nothing switches automatically. An app can set `ICED_BACKEND` itself before `run()`, for
example when it detects an RDP session. The research probe measured tiny-skia at about 40 fps
with about 50 visible nodes, but only 10–12 fps when zoomed out to about 160 nodes (release,
1280x800). Level of detail matters on CPU.

## What was hard

- **No menu widgets at all.** The context menu, submenu, radio marks, disabled rows, edge
  flipping, keyboard navigation and the popover anchoring are about 300 lines of custom code.
  The building blocks (`stack`, `pin`, `opaque`, `mouse_area`) work well, and the Elm
  architecture makes data-driven menus trivial. But every native-menu behaviour (focus
  handling, type-ahead, mnemonics, submenu hover delay, menus outside the window) is yours to
  write and maintain. Menus can never leave the main window.
- **A renderer difference in canvas text.** With `Frame::scale`, `iced_wgpu` 0.14.0 scales
  the font size but not `Text::max_width`, while `iced_tiny_skia` 0.14.1 scales both. Zoomed-in
  titles wrapped too early on Vulkan only. The fix is drawing in screen space. It is a real
  upstream inconsistency to expect elsewhere.
- **Canvas has no text measuring.** The badge pill width needs `iced_graphics::text::Paragraph`
  (or the `advanced` feature).
- **Keyboard focus model.** Only text inputs are focusable. Keys reach the app through
  `keyboard::listen()` only when no widget captured them. Hover and keyboard focus fight, as
  described above.
- **Headless testing.** With no window manager, the window never gets X input focus.
  winit's `focus_window` only sends `_NET_ACTIVE_WINDOW`, which needs a WM. The app therefore
  sets the focus itself through x11rb when `_NET_SUPPORTING_WM_CHECK` is missing (about 25
  lines, Linux only, a no-op on real desktops). Also, each `xdotool click 4` produces two
  wheel events in winit 0.30: the XTEST button press plus the server's emulated XI2 scroll
  valuator. So 3 notches in the test give zoom 1.1^6 = 1.77. A real wheel's emulated button
  event is skipped by winit, so this is a test artifact.
- **The pick_list dropdown opens upward** when there is more room above the field. That is
  built-in behaviour, but test coordinates must account for it.

## Verdict

iced can do the key interaction well. The right-click menu with data-driven factor submenus,
the live badge updates up to the root, and the anchored editor with pick_lists, a multi-line
rationale and a live preview all work and look clean, on both GPU and CPU renderers. The CPU
fallback (tiny-skia) is a real strength for remote desktops: it is automatic, fast to start and
visually identical. The cost: iced 0.14 ships **no context menu, submenu or popover**, and
iced_aw cannot nest them, so the core interaction is custom widget plumbing that you own.
It is also exposed to the breaking overlay/widget changes already on master (0.15-dev).
Keyboard focus is basic, and there is **no accessibility support** at all. If screen-reader
support or native-feeling menus are requirements, that weighs heavily against iced; if not, it
is a solid, small, permissively licensed option.
