# UI spike: Freya 0.4.3 (Skia)

The SPEC app (`../SPEC.md`) built with **Freya**, a pure-Rust UI library that renders with Skia.

| Crate | Version | License | Role |
|---|---|---|---|
| `freya` | =0.4.3 (stable, 2026-08-30) | MIT | Facade. Feature `engine` re-exports the Skia API for the canvas. |
| `freya-winit` | =0.4.3 | MIT | Window, event loop, graphics drivers. Used directly for the plugin that logs the renderer. |
| `freya-components`, `freya-core`, `freya-edit`, `ragnarok`, `torin` | 0.4.3 | MIT | Widgets, reactive core, text editing, event routing, layout |
| `freya-skia-safe` / `freya-skia-bindings` | 0.98.1 | MIT (Skia itself BSD-3-Clause) | Fork of rust-skia. **Downloads prebuilt Skia binaries at build time** |
| `winit` 0.30.13, `accesskit` 0.24.1, `accesskit_winit` 0.33.2, `softbuffer` 0.4.8 | | Apache-2.0 / MIT | Windowing, accessibility, CPU presentation |
| `x11rb` | 0.13 | MIT OR Apache-2.0 | Linux only: the headless-test focus helper (see below) |

The 0.5 betas were not used. All app code is in `src/main.rs` (about 800 lines, including about 60
lines of test logging and the X11 focus helper). `examples/fast_click.rs` is a 30-line repro of a
toolkit issue (see Test notes). Every factor, level, label, value and rating color comes from
`spikes/shared/pack.json` at runtime. The only fixed strings in the UI are verbs: "Gate type",
"Add child step", "Delete", "Edit feasibility…", "Rationale", "Preview", "OK" and "Cancel". AND and
OR come from `Kind::label()`.

## Build and run

The target folder below is only an example; any folder outside the repo works.

```sh
# Linux (Ubuntu 24.04)
sudo apt install build-essential pkg-config libfontconfig-dev libfreetype-dev libegl-dev libgl-dev \
  libxkbcommon-x11-0 libvulkan1 mesa-vulkan-drivers
cd spikes/freya
CARGO_TARGET_DIR=/tmp/targets/freya cargo run
NODES=2000 cargo run                 # scale test
READONLY=1 cargo run                 # read-only mode
FREYA_RENDERER=software cargo run    # force the CPU renderer
```

- **Build needs network access to GitHub**: `freya-skia-bindings` downloads
  `skia-binaries-<key>.tar.gz` from `github.com/marc2332/rust-skia/releases` (the Freya maintainer's
  fork, not upstream rust-skia). For offline or air-gapped CI, mirror the archive and set
  `SKIA_BINARIES_URL`, or build Skia from source (needs Python, ninja and a long build).
- The binary links `libfontconfig`, `libfreetype`, `libEGL` and `libGL` (so those must exist at
  runtime, even with the software renderer); X11, Wayland, xkbcommon and Vulkan are loaded at
  runtime. This container also had clang, cmake and the X11/Wayland dev packages, so the apt line
  above is the expected minimum, not a verified one.
- A clean debug build took about 5 minutes here (3 jobs, 4 shared CPUs). Rebuilding only the app
  crate takes 1–2 s.
- **Windows 10/11.** Use the MSVC toolchain (Visual Studio 2022 Build Tools, "Desktop development
  with C++"); Freya's README warns that `windows-gnu` does not compile. Then `cargo run` in
  `spikes\freya`. **Verified here:** `cargo check --target x86_64-pc-windows-msvc` passes with no
  warnings (1m45s; the Windows Skia binaries were downloaded). Not linked or run on real Windows.
- **macOS 12+.** Install the Xcode command-line tools, then `cargo run`. Renders with Metal.
  **Verified here:** `cargo check --target aarch64-apple-darwin` passes with no warnings (52 s). Not
  linked or run on a Mac.

Environment variables:

| Variable | Effect |
|---|---|
| `NODES` | Number of generated nodes (handled by spike-model) |
| `READONLY=1` | Read-only mode (handled by spike-model) |
| `FREYA_RENDERER` | `software` forces the CPU renderer, `opengl` skips Vulkan on Linux, `vulkan` opts into Vulkan on Windows |
| `PERF=1` | Print `DRAW <ms> nodes=<n>` for each canvas paint (our Skia callback only) |
| `DEBUG_INPUT=1` | Print key and pointer events |
| `RUST_LOG=...` | tracing output (for example `freya_winit=warn` shows renderer fallbacks) |
| `NO_X11_FOCUS=1` | Disable the headless focus helper |
| `AEGIS_SPIKE_DATA` | Folder with `pack.json` and `tree.json` |

The app prints `NODE`/`RESULT` lines at start, `CHANGE`/`RESULT` lines after every edit, and
`RENDERER`, `FIRST_FRAME`, `SELECT`, `MENU_OPEN <id> mouse|keyboard`, `EDITOR_OPEN`, `PREVIEW`,
`EDITOR_OK`, `EDITOR_CANCEL`, `ZOOM` and `PAN` lines for the tests.

## Approach

**Canvas (custom-built; there is no node-graph widget).** The whole tree is one
`canvas(RenderCallback)` element that draws straight to the Skia canvas: white rounded rects with a
1 px `#D0D4DC` border (2 px `#3D6DE8` when selected), titles as Skia paragraphs (14 px, two lines,
ellipsis), an AND/OR chip on gates, the badge pill in the rating color, and cubic Bézier edges from
the parent's right edge to the child's left edge. Pan and zoom are a `translate` + `scale` on the
Skia canvas, so text stays sharp at any zoom. Nodes and edges outside the view are culled. Layout,
parent links and badge strings are cached and rebuilt only when `model.generation()` changes.
Hit-testing (inverse view transform against `layout()`), wheel zoom around the cursor (0.2–3.0) and
drag-to-pan on empty canvas are app code. One quirk: `RenderCallback` always compares equal, so the
canvas gets a fresh `key` each render, or Freya keeps painting the old closure.

**Context menu (built-in menu widgets).** `Menu`, `SubMenu`, `MenuItem` and `MenuButton` from
freya-components. The menu is rebuilt from the model and the pack on every render: one `SubMenu`
per factor (label plus the current level), one `MenuItem` per level with the label and the value,
and the current one shown with `MenuItem::selected(true)` plus a ● mark. Gates get "Gate type"
(AND/OR, current one marked), "Add child step", and "Delete" for every non-root node. Hover-to-open
submenus, Escape (closes the submenu first, then the menu), click-outside-to-close and shifting a
menu back inside the window are built in. Choosing a level calls `set_factor`.

- Placement: the menu sits in a `Layer::Overlay` rect at a global position. This is the same 6 lines
  the built-in `ContextMenuViewer` uses. I did not use `ContextMenu::open_from_event` because
  `ContextMenu::open` can only open at the *last mouse position*, so the keyboard path (open at the
  selected node) needs its own placement anyway, and one code path is simpler. The built-in
  `ContextMenu` was checked separately in a small scratch app and works, including submenu items.
- The menu opens on right-button **release**. On press, the release would reach the new menu's
  global-press handler and close it right away (`ContextMenuViewer` has a "skip the first close"
  flag for the same reason).
- Radio rows: the built-in `RadioItem` is always focusable, so inside a menu row it becomes an
  extra stop for the arrow keys (Enter on it chose the wrong item). The rows draw a ● / ○ glyph
  instead.

**Disabled items (custom, about 10 lines).** `MenuItem` has no `disabled` property. Read-only
rows are greyed and get a press handler that only calls `prevent_default()`. That cancels the
global press, so clicking a greyed item leaves the menu open and changes nothing.

**Edit popover (built-in parts, custom placement and text field).** "Edit feasibility…" opens a
panel in an overlay layer next to the node: to the right, flipped to the left if there is no room,
and clamped vertically using its measured height (`on_sized`). Freya's `Attached` attaches to an
element and `Popup` is a centered dialog; the anchor here is a pixel rect on the canvas, so
placement is about 10 lines of app code. Inside: one built-in **`Select`** per factor (options are
`MenuItem`s with "label (value)"), a live preview pill `"<AP> · <rating>"` from `model.preview()`
that updates on every Select change, and built-in **`Button`**s for Cancel and OK (`set_factors`).
Escape also cancels.

**Rationale field (custom, about 75 lines).** Freya 0.4.3 has no multi-line text area; the
built-in `Input` is single-line (`max_lines(1)`, Enter submits). The field wires freya-edit's
`use_editable` to a `paragraph` element, the way `Input` does internally: keyboard editing, caret,
selection highlight and mouse click/drag selection all come from freya-edit.

**Keyboard (built-in focus moves, custom glue of about 30 lines).** Shift+F10 and the Menu key open
the same menu at the selected node. Freya's built-in keyboard support moves focus with Tab, and with
Up/Down *inside the focused accessibility group* (each menu container is a group); Enter or Space
"presses" the focused item, which opens a submenu or chooses a level. The glue does three things:

1. When a menu opens, it focuses the menu container, so the first Down lands on the first item.
2. Right moves into the open submenu: focus the last menu group, then its first item.
3. Freya applies **only the last focus request per frame** (`request_focus` overwrites), so step 2
   needs a `use_side_effect` that sends the second request after the first one has landed.

Left does not close a submenu (Escape does). Ctrl+Z calls `undo` (ignored while the popover is
open, where the text field has the keyboard).

## Feature status

All features were tested headless with `spikes/tools/gui_run.sh` (Xvfb, lavapipe Vulkan), debug
binary. The screenshots in `screenshots/` are cropped to 1400x900.

| ID | Status | Built-in or custom | Evidence |
|---|---|---|---|
| F1 Canvas | works | custom (Skia `canvas`) | `01_initial.png`: SPEC style, curved edges, AND/OR chips, badges in pack colors |
| F2 Right-click menu | works | built-in `Menu`, custom overlay placement | `02_menu.png`: opens at the cursor above the canvas; right-click also selects the node |
| F3 Factor submenus | works | built-in `SubMenu` + `MenuItem::selected` | `03_submenu.png`: 5 submenus with pack labels and current level, levels with values, "Specialized 4" marked. Choosing Standard logs `RESULT s6 11 High`, `g2 11 High`, `g0 11 High` |
| F4 Live update | works | model + reactive state | `04_changed.png`: red High badges on s6, g2 and g0 |
| F5 Gate menu | works | built-in | `09_gate_menu.png`: Gate type (OR marked), Add child step, Delete. AND logs `RESULT g3 55 Very low`, root unchanged. Add child step created `n1` (`0 High`); Delete removed it; three Ctrl+Z restored the start state |
| F6 Edit popover | works | built-in `Select`/`Button`, custom placement and multi-line field | `06_popover.png`, `07_popover_edited.png`: preview `14 · Medium`. OK logs `RESULT s6 14 Medium`, `g2 14 Medium`, `g0 14 Medium` and the rationale (`08_after_ok.png`). Cancel and Escape close without a change |
| F7 Pan and zoom | works | custom | `11_zoom_pan.png`: 3 wheel notches around s6 (zoom 1.331), then a drag on empty canvas (`PAN -585 -300`). A right-click on the zoomed s6 still hits it |
| F8 Keyboard | works (with glue) | built-in focus moves and Enter, custom Shift+F10 / Menu key / Right / first-focus | `10_keyboard_menu.png`: click s2, Shift+F10, Down, Enter, Right: focus ring on "≤ 1 day" inside the submenu. Down Down Enter set s2 to `15 Medium`; Menu key on g3, Down, Enter, Right, Enter set g3 to AND. Escape closes the submenu, then the menu. Left does not close a submenu |
| F9 Read-only | works | custom (greyed + `prevent_default`) | `12_readonly.png`: menu opens, submenus open, levels, Edit and Delete greyed, current level still marked, "(read-only)" in the header and READ-ONLY in the status bar. Clicking a greyed level does nothing and the menu stays open. Hovered greyed rows still get the hover background |
| F10 Undo | works | model `undo` + global key handler | `05_undo.png`: `RESULT g0 15 Medium`. Also undid gate, add and delete changes |
| F11 Scale | works | custom culling | `13_scale.png`: `NODES=2000` (2114 nodes), zoom 0.62, menu and Equipment submenu on generated step f4; choosing a level by keyboard logged `CHANGE set_factors f4`. First frame 0.10 s. Canvas paint median 1.4 ms, p90 2.9 ms; at most 50 nodes drawn even at zoom 0.2 |
| F12 Text input | works | custom field on built-in `use_editable` | Typed "Needs only a laptop", Ctrl+Left, Ctrl+Shift+Right selected "laptop" (highlight visible), typed over it, Enter, a second line, Ctrl+Shift+Left selected "dongle" (`07_popover_edited.png`). Log: `rationale="Needs only a cheap laptop\nand an OBD dongle"`. Mouse drag selection and Ctrl+A work. **Ctrl+C / Ctrl+V did not paste in Xvfb** (not investigated further). IME not tested |
| F13 Accessibility | partial (as reported) | built-in AccessKit | See below. Checked through AT-SPI with pyatspi |

`14_edge.png`: right-click on s9 near the bottom, then hover Equipment. The menu does not flip; it
shifts up to stay inside the window (built-in `overflow_offset`), and the submenu shifts up too.
Near the right edge, submenus also shift left instead of flipping. Menus are drawn inside the
window, so they cannot extend past the window frame.

### Expected values (SPEC)

| Action | RESULT lines logged |
|---|---|
| Initial | `s6 15 Medium`, `g2 15 Medium`, `g0 15 Medium` |
| s6, Equipment = Standard | `s6 11 High`, `g2 11 High`, `g0 11 High` |
| then Ctrl+Z | `g0 15 Medium` |
| s6 popover: Elapsed time = ≤ 1 day, OK | `s6 14 Medium`, `g2 14 Medium`, `g0 14 Medium` |
| g3, Gate type = AND | `g3 55 Very low`, root unchanged (`g0 15` from the start state, `g0 14` after the popover step) |

### Test notes (important for re-testing)

- **Fast clicks are lost (a Freya 0.4.3 event-handling issue).** `xdotool click` sends button down
  and up with no gap. Freya measures each winit event right away, but applies the "pressed node"
  state only when the queued events are executed, so an up that arrives in the same batch as its
  down is dropped as a press: `on_press` never fires on a menu item, button or Select. The menu
  still closes, because the global press handler does fire. Traced with `RUST_LOG=ragnarok=info`
  (the node is "Marked as pressed" but no `PointerPress` is emitted). Reproduced with built-in
  components only: `examples/fast_click.rs` (one `Button`; `cargo run --example fast_click`) gave no
  press for 3 instant clicks and 2 presses for 2 clicks held 0.4 s; a scratch app with the built-in
  `ContextMenu` behaved the same on a submenu item. A real mouse click (about 50–150 ms) works. Risk: synthetic input (UI automation, some remote-desktop or KVM clients, possibly
  touchpad taps) may lose clicks. The test steps therefore click with `drag x y x y`, a 0.4 s press.
  Right-clicks are unaffected because the app opens the menu on `on_mouse_up`.
- **Headless keyboard focus.** Xvfb has no window manager, so nobody gives the window X input focus
  and no key events arrive (verified with `NO_X11_FOCUS=1`). On Linux the app calls
  `XSetInputFocus` once at startup, **only when no window manager is running**
  (`_NET_SUPPORTING_WM_CHECK` is missing). This is a test helper, not toolkit functionality.
- The harness wheel steps sometimes deliver one extra wheel event (5 notches gave 6 `ZOOM` lines),
  so derive coordinates after zooming from the `ZOOM`/`PAN` lines.

## Rendering backends and software (CPU) rendering

Freya picks the driver in `freya-winit` (`drivers/mod.rs`); the app prints it as `RENDERER ...`.

| Platform | Default order | Software path |
|---|---|---|
| Linux | Vulkan → OpenGL (EGL/GLX via glutin) → Software | softbuffer + Skia raster |
| Windows | OpenGL → Software (`FREYA_RENDERER=vulkan` tries Vulkan first) | softbuffer + Skia raster |
| macOS | Metal only | only with `FREYA_RENDERER=software` |

Tested here (all under Xvfb, all with the SPEC steps menu → Standard → `RESULT s6 11 High`):

- **Vulkan on lavapipe (CPU):** default in the harness, `RENDERER Vulkan`. All screenshots.
- **`FREYA_RENDERER=software`:** `RENDERER Software`. The output matches the GPU path apart from
  anti-aliasing: tree, menus, submenu, popover, Select and badges all render, and only about 560 of
  1.6 million pixels differ by more than 10 % from the OpenGL screenshots. First frame 0.06 s;
  canvas paint median 1.2 ms.
- **`FREYA_RENDERER=opengl`:** `RENDERER OpenGL` (Mesa llvmpipe), same output.
- **No GPU at all (automatic fallback):** Xvfb without GLX, the EGL vendor and the Vulkan ICD
  pointed at nothing. Log: `Vulkan initialization failed, falling back to OpenGL: Unable to find a
  Vulkan driver`, then `OpenGL initialization failed, falling back to software: failed to create
  EGLDisplay`, then `RENDERER Software`, and the app works normally.

This matters for remote desktops: the CPU path is Freya's own (Skia's raster backend + softbuffer),
so it does not depend on Mesa, WARP or any driver. On Windows over RDP or Citrix the default is
OpenGL. A GPU-less RDP session typically offers only Microsoft's OpenGL 1.1; in the driver code, a
failed Skia GL context returns an error and falls back to software, but a GL display with no
matching pixel format would hit `expect("at least one OpenGL config")` and panic. Neither was
**tested** on Windows. Setting `FREYA_RENDERER=software` removes the guesswork. Whole-frame times on
the CPU path were not measured (only the canvas callback, below); in the headless runs every menu,
submenu and hover state was on screen well within the 1–1.5 s screenshot waits. 4K on a weak VDI
host was not tested.

### Performance (headless, 4 shared CPUs)

| Run | First frame (process start to first canvas paint) | Canvas paint (our Skia callback) |
|---|---|---|
| 13 nodes, Vulkan (lavapipe), debug | 0.08–0.11 s | median 0.9 ms, p90 1.4 ms |
| 13 nodes, Vulkan (lavapipe), release | 0.08 s | median 0.8 ms, p90 1.5 ms |
| 13 nodes, Software, debug | 0.06 s | median 1.2 ms, p90 1.4 ms |
| 13 nodes, OpenGL (llvmpipe), debug | 0.10 s | median 0.8 ms, p90 1.1 ms |
| `NODES=2000`, Vulkan, debug | 0.10 s | median 1.4 ms, p90 2.9 ms |
| `NODES=2000`, Vulkan, release | 0.11 s | median 1.3 ms, p90 2.4 ms |

The max was always the first paint (30–34 ms). These numbers cover only the canvas
callback, not Freya's layout and compositing; the canvas is repainted only when state changes, not
every frame. No lag was visible in any run. Debug and release are close because Skia comes
prebuilt and optimized and the dependencies already build at opt-level 1. Binary size: debug
43.5 MB (`debug = 0`), release 39.1 MB (31.3 MB stripped). A release build from a warm debug cache
took 3m56s.

## Accessibility (F13)

AccessKit 0.24 is built in (accesskit_winit 0.33: UIA on Windows, NSAccessibility on macOS, AT-SPI
on Linux). Built-in widgets set roles (`Menu`, `MenuItem`, `ListBox` for Select, `RadioButton`,
`Button`), are focusable, and Freya moves focus with Tab and with Up/Down inside groups.

Tested on Linux with a D-Bus session, the AT-SPI bus, `org.a11y.Status IsEnabled=true` and pyatspi,
with the s6 menu and the Equipment submenu open:

```
application 'aegis-spike-freya'
  frame ''
    unknown ''                     <- the canvas: one opaque element
    label ''                       <- status bar
    menu '' [focused]
      label ''                     <- header row
      menu item ''  (x4)           <- factor rows: label + current level in a nested rect
      menu item ''                 <- Equipment
        menu ''                    <- submenu, correctly nested under its item
          menu item ''  (x4)       <- level rows
      menu item 'Edit feasibility…'
      menu item 'Delete'
```

What works: roles, real parent/child structure for submenus, keyboard focus moving into menus,
focusable state. Gaps:

- **Names.** Freya names an accessible node after its *direct* label children. Items whose only
  child is a label are named ("Edit feasibility…", "Delete"); the factor and level rows, which put
  several labels in a row container, have **empty names**. Labels themselves expose no text. A
  real product would need one label per item or a name API on `MenuItem` (not in 0.4.3).
- **The canvas is invisible** to screen readers: nodes are pixels in one Skia element. An
  accessible tree would need a hidden element tree or per-node elements.
- The current level is **not exposed as checked**, and the window frame has no name.
- Not tried with NVDA, Narrator, VoiceOver or Orca.

## What was hard

- **Lost fast clicks** (see test notes): a submenu item press "just did nothing" in the headless
  tests. It took tracing ragnarok's pressed-node bookkeeping to find that only the click timing was
  the problem.
- **Keyboard menus:** arrow keys only move within a focus group, and only one focus request per
  frame is honored, so moving into a submenu needs a two-frame dance. A focusable `RadioItem` inside
  a menu row silently broke Enter.
- **No multi-line text field:** built from `use_editable` + `paragraph` (works well once wired).
- **No `disabled` on menu items:** greyed styling plus a `prevent_default` press guard.
- **Canvas repaint:** `RenderCallback` compares equal, so the canvas must be re-keyed to repaint.
- **Docs:** good API docs with examples per component, but behavior details (event batching, focus
  groups, overlay placement) had to be read from the source.

What was easy: the Skia canvas (fast, sharp text, transforms), data-driven submenus, Select, the
reactive state model (`use_state` handles are `Copy`), the automatic software fallback, and fast
incremental builds. Out of the box the menus, Select and buttons look clean (shadows, hover,
rounded corners, Select animation).

## Licenses

`cargo metadata` over the resolved graph (332 packages on Linux, 259 on Windows, 264 on macOS):
everything is MIT, Apache-2.0, BSD-2/3-Clause, ISC, Zlib, Unicode-3.0, CC0 or Unlicense-OR-MIT. No
MPL, LGPL, GPL or AGPL. Notes:

- The prebuilt Skia archive is linked statically: Skia (BSD-3-Clause) plus bundled third-party code
  such as HarfBuzz and ICU (permissive licenses, ICU data embedded via the `embed-icudtl` default
  feature). Ship their notices. On Linux, FreeType and fontconfig come from the system.
- Apache-2.0 crates (winit, glutin, accesskit_winit) require attribution and NOTICE handling.
- `spike-model` is marked proprietary (our own code).

## Verdict

Freya **meets the core requirement**. Right-click on a node opens a data-driven menu with one
submenu per pack factor, the current level marked and the value shown; choosing a level updates the
badges up to the root; gates get Gate type / Add child step / Delete; the anchored edit panel has
Selects, a multi-line rationale with selection, a live preview and OK/Cancel; read-only, undo, pan,
zoom and 2000 nodes all work. The menus, Select and buttons are built in and look good. The Skia
canvas is the strongest part: fast, sharp at any zoom, and simple to cull.

Its **main strength for AegisTARA** is the CPU renderer: it falls back automatically when Vulkan
and OpenGL fail, looks the same apart from anti-aliasing, and can be forced with one variable. That
is a strong story for GPU-less remote desktops (still to be confirmed on real Windows RDP).

The risks:

- the lost-fast-click issue in event handling (real mice are fine; automation and some remote
  input paths may not be);
- keyboard menu navigation and disabled items need app glue, and accessibility names for composed
  menu rows are missing;
- the canvas is opaque to screen readers;
- youth and churn: 0.4 was a rewrite (July 2026), 0.5 betas ship weekly, mostly one maintainer,
  and the build downloads Skia binaries from the maintainer's GitHub fork.

Recommendation: a serious finalist if remote-desktop rendering is a top priority. Before committing,
test on a real Windows RDP/Citrix session (default OpenGL vs. `FREYA_RENDERER=software`), report or
patch the fast-click issue upstream, and check that 0.5 does not break the menu and focus APIs.
