# UI spike: Slint

The SPEC app (`../SPEC.md`) built with **Slint 1.18.1** (the latest release on crates.io, published
2026-09-21), used under the **Slint Royalty-free License 2.0**.

| Crate | Version | License | Role |
|---|---|---|---|
| `slint` | =1.18.1 | GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0 | Runtime: widgets, menus, winit backend, femtovg (OpenGL) and software renderers, AccessKit |
| `slint-build` | =1.18.1 | same | Build script: compiles `ui/app.slint` to Rust (`fluent-light` style) |
| `libloading` | 0.8 (Linux only) | ISC | Probes for the OpenGL libraries at startup (see "Software rendering") |

Features enabled: `std`, `compat-1-2`, `backend-winit`, `renderer-femtovg`, `renderer-software`,
`software-renderer-path` (needed so the software renderer draws `Path`, i.e. the edges) and
`accessibility`. Everything comes from crates.io.

Code: `ui/app.slint` (about 430 lines, the declarative UI) and `src/main.rs` (about 440 lines, which
feeds the UI from `spike-model` and handles callbacks). Every factor, level, label, value and rating
color comes from `spikes/shared/pack.json` at runtime. The only fixed strings in the UI are verbs:
"Gate type", "Add child step", "Edit feasibility…", "Delete", "Rationale", "OK", "Cancel" and
"About Slint". AND and OR come from `Kind::label()`.

## Build and run

The target folder below is only an example; any folder outside the repo works.

```sh
# Linux (Ubuntu 24.04)
sudo apt install build-essential pkg-config libfontconfig-dev libfreetype-dev \
  libxkbcommon-dev libxkbcommon-x11-dev libwayland-dev libgl1-mesa-dri libegl1
cd spikes/slint
CARGO_TARGET_DIR=/tmp/targets/slint cargo run --release
NODES=2000 cargo run --release              # scale test
READONLY=1 cargo run --release              # read-only mode
SLINT_BACKEND=winit-software cargo run --release   # force the CPU renderer
```

- **Windows 10/11.** Install the MSVC Rust toolchain (Visual Studio Build Tools, "Desktop
  development with C++"). Then `cargo run --release` in `spikes\slint`. No extra SDKs; femtovg uses
  OpenGL through WGL. On Windows and macOS Slint shows `ContextMenuArea` menus as **native OS menus**
  (through the `muda` crate). Set `SLINT_NO_MUDA=1` to get the Slint-drawn menus seen in the
  screenshots. **Verified here:** `cargo check --target x86_64-pc-windows-msvc` passes. Not built,
  linked or run on real Windows.
- **macOS 11+.** Xcode command-line tools, then `cargo run --release`. **Verified here:**
  `cargo check --target aarch64-apple-darwin` passes. Not run on a Mac.

Cold builds here (3 jobs on shared CPUs): debug 6 min 50 s, release 4 min 41 s (about 350 crates).
Rebuilding after a `.slint` or Rust change takes 4 to 75 s. Release binary 30 MB (22.6 MB stripped),
debug 41 MB. The target dir is 3.3 GB (debug, release, and Windows and macOS checks).

Environment variables:

| Variable | Effect |
|---|---|
| `NODES` | Number of generated nodes (handled by spike-model) |
| `READONLY=1` | Read-only mode (handled by spike-model) |
| `SLINT_BACKEND` | `winit-software` forces the CPU renderer; `winit-femtovg` forces OpenGL |
| `SLINT_NO_MUDA=1` | Windows and macOS: Slint-drawn menus instead of native ones |
| `PERF=1` | Prints a `FRAME <ms>` line per rendered frame (OpenGL renderer only) |
| `AEGIS_SPIKE_DATA` | Folder with `pack.json` and `tree.json` |

The app prints `NODE`/`RESULT` lines at start, `CHANGE`/`RESULT` after every edit, and `SELECT`,
`MENU_OPEN`, `KBMENU_OPEN`, `EDITOR_OPEN`, `PREVIEW`, `EDITOR_OK`, `EDITOR_CANCEL`, `ZOOM`, `PAN`,
`GRAPHICS_API`, `RENDERER` and `FIRST_FRAME` lines for the tests.

## Approach

**Canvas (custom, built from Slint elements).** Slint has no node-graph or canvas widget. Rust
computes the layout, culls it to the window and puts the visible nodes into a
`VecModel<NodeView>`. The `.slint` file draws them with `for node in nodes : NodeItem`. Each node is
a `Rectangle` (white, 1 px `#D0D4DC` border, radius 8; 2 px `#3D6DE8` when selected) with a 14 px
title `Text`, an AND/OR chip and a badge pill filled with the pack's rating color. Edges are one
`Path` element whose SVG command string (`M … C …` cubic Béziers from the parent's right edge to
the child's left edge) Rust builds for the visible edges.

Slint has no transform on arbitrary elements, so zoom and pan are applied by hand: `x = wx * zoom +
pan`, and font sizes, radii and paddings are multiplied by `zoom`. Below zoom 0.45 a node draws no
text, only a strip in its rating color (level of detail). The row list is replaced only when the set
of visible nodes changes; otherwise the rows are updated in place. A left drag on empty canvas pans,
and the wheel zooms around the cursor (a `TouchArea` with `scroll-event`).

**Context menu (built-in).** Each node holds a `ContextMenuArea` with a `Menu`. The menu tree is
**data-driven inside the `.slint` file**: `for level[fi] in n.levels : Menu { title:
Bridge.factors[fi].label; for title[li] in Bridge.factors[fi].titles : MenuItem { … } }`. So there
is one submenu per pack factor and one item per level ("label  (value)"). The current level has
`checkable: true; checked: level == li`, and `enabled: !Bridge.read-only` greys the items in
read-only mode. Gates get a "Gate type" submenu (AND/OR, current one checked), "Add child step" and
"Delete" (not on the root). `MenuSeparator` separates groups. Submenus, hover, arrow keys, Enter,
Escape, check marks, disabled styling and keeping the menu inside the window are all built in. A
`TouchArea` on the node catches the right press, selects the node and calls `menu.show(position)`,
so a right-click also selects.

**Keyboard-opened menu (small custom glue).** `ContextMenuArea` opens on the Menu key only when a
`FocusScope` inside it has focus, and on Windows only also on Shift+F10 (checked in
`i-slint-core` `items.rs`). The nodes here have no focus of their own, so the canvas `FocusScope` has a
`KeyBinding { keys: @keys(Shift + F10) }` and a Menu-key handler. Both call Rust, which bumps a
`menu-request` counter. The selected node reacts with `changed menu-request => { menu.show(…) }`
and opens its menu at its bottom-left corner. Ctrl+Z is another `KeyBinding`.

**Edit popover (built-in parts, custom placement).** A `PopupWindow` with
`close-policy: no-auto-close`. Rust computes its position (right of the node, or left if it does
not fit, clamped to the window). Slint has no "anchor to element" API. The panel holds one built-in
`ComboBox` per factor (generated with `for` from the pack), a built-in multi-line `TextEdit` for the
rationale, a live preview pill (`model.preview()` on every `ComboBox.selected`, in the rating
color), and built-in `Button`s for Cancel and OK (`primary`). OK calls `set_factors`. Escape and
Cancel close it without changes.

**About dialog.** "About Slint" in the status bar opens a popup with the built-in `AboutSlint`
widget, as the Royalty-free license requires.

## Feature status

All features were tested headless with `spikes/tools/gui_run.sh` (Xvfb + Mesa llvmpipe) using the
steps files in `steps/` (`steps/run_all.sh <binary> <out_dir>` runs them all). Final screenshots in
`screenshots/` come from the release binary on the OpenGL (femtovg) renderer. The core steps file
also passed with the debug binary and with the software renderer.

| ID | Status | Built-in or custom | Evidence |
|---|---|---|---|
| F1 Canvas | works | custom (Rectangle/Text per node, one `Path` for edges) | `01_initial.png`: SPEC style, curved edges, badges in pack colors |
| F2 Right-click menu | works | built-in `ContextMenuArea` + `menu.show()` | `02_menu.png`: opens at the cursor over the canvas. Right-click also selects the node |
| F3 Factor submenus | works | built-in `Menu`/`MenuItem` with `for` loops, `checkable`/`checked` | `03_submenu.png`: 5 submenus with pack labels, "Specialized (4)" checked. Choosing Standard logs `RESULT s6 11 High`, `g2 11 High`, `g0 11 High` |
| F4 Live update | works | model + `VecModel` row updates | `04_changed.png`: red High badges on s6, g2 and g0 |
| F5 Gate menu | works | built-in | `09_gate_menu.png`: Gate type (AND/OR, OR checked), Add child step, Delete. AND logs `RESULT g3 55 Very low`, root unchanged. Add child step created `n1` (`0 High`), Delete removed it (`c` steps) |
| F6 Edit popover | works | built-in `PopupWindow`, `ComboBox`, `TextEdit`, `Button`; placement custom | `06_popover.png`, `07_popover_edited.png`: preview `14 · Medium`. OK logs `RESULT s6 14 Medium`, `g2 14 Medium`, `g0 14 Medium` (`08_after_ok.png`). Cancel and Escape discard; reopening shows fresh values (`f` steps) |
| F7 Pan and zoom | works | custom | `11_zoom_pan.png`: 3 wheel notches around the cursor, then a drag on empty canvas (`PAN -940 -465`). The menu still opens on the zoomed node. Under xdotool winit reports each notch twice, so the test reaches zoom 1.77 instead of 1.33 (same in the egui and iced spikes) |
| F8 Keyboard | works | built-in menu navigation; custom Shift+F10 / Menu key glue | `10_keyboard_menu.png`: click s2, Shift+F10, Down, Right opens "Elapsed time". The Menu key works too. Shift+F10, Down, Right, Down, Return on g3 set AND (keyboard only). Escape closes the submenu, then the menu |
| F9 Read-only | works | built-in `enabled` on `MenuItem`, model check | `12_readonly.png` (`READONLY=1`): the menu opens, levels, Edit feasibility and Delete are greyed, the current level stays checked. Clicking a greyed level or Edit does nothing; Ctrl+Z logs `UNDO nothing` |
| F10 Undo | works | model `undo` + `KeyBinding` | `05_undo.png`: `CHANGE undo`, `RESULT g0 15 Medium` |
| F11 Scale | works | custom culling + level of detail | `13_scale.png`: `NODES=2000` (2114 nodes, 24 drawn at zoom 0.47), menu and submenu on generated step f16, Equipment changed by keyboard. First frame 0.13 s. Render time per frame median 9.7 ms, p90 16 ms, max 38 ms (44 frames). At zoom 0.07, 107 nodes drawn |
| F12 Text input | works | built-in `TextEdit` | Typed "Needs only a laptop", Ctrl+Left, Ctrl+Shift+Right selected "laptop" (highlight visible), typed over it, Enter, second line. Logged `rationale="Needs only a cheap laptop\nand an OBD dongle"`. IME not tested |
| F13 Accessibility | partial (as reported) | built-in AccessKit; roles/labels added on nodes | See below |

`14_edge.png`: right-click on s9 near the bottom, then Equipment by keyboard. The root menu does not
flip; it is shifted up to stay inside the window, and so is the submenu. At the right edge
(right-click on s1 at x=950) the submenu does not flip to the left either: it is pushed left until
it fits and then overlaps its parent menu. Usable but less polished than a flip. On Linux the menus
are drawn inside the window, so they cannot extend past it.

### Expected values (SPEC)

| Action | RESULT lines logged |
|---|---|
| Initial | `s6 15 Medium`, `g2 15 Medium`, `g0 15 Medium` |
| s6, Equipment = Standard | `s6 11 High`, `g2 11 High`, `g0 11 High` |
| then Ctrl+Z | `g0 15 Medium` |
| s6 popover: Elapsed time = ≤ 1 day, OK | `s6 14 Medium`, `g2 14 Medium`, `g0 14 Medium` |
| g3, Gate type = AND | `g3 55 Very low`, root unchanged (`g0 14`, carried over from the popover step) |

### Test harness note (window focus)

Xvfb has no window manager, so nothing gives the window X input focus, and winit then drops all key
events (an earlier run without focus logged the click but no Shift+F10 or Ctrl+Z). `steps/run_all.sh`
wraps the binary in `steps/focus-shim.sh`, which waits until the window is mapped and runs
`xdotool windowfocus` on it, as a window manager would. The first version of the shim did not wait
for the map and lost the race once on a cold start (Ctrl+Z and typing were ignored). The app and the
harness are unchanged. On a normal desktop this does not come up.

## Rendering backends and software (CPU) rendering

- **femtovg on OpenGL (default): works.** Under Mesa llvmpipe here (`GRAPHICS_API NativeOpenGL`).
  First frame 0.11 to 0.15 s after process start (release), window mapped after 0.16 s. Debug: first
  frame 0.13 to 0.17 s, mapped after 0.5 s.
- **Slint software renderer (`SLINT_BACKEND=winit-software`): works, everything rendered.** The tree,
  curved edges (`Path`, needs the `software-renderer-path` feature), badges, menus with submenus and
  check marks, the popover with ComboBoxes and TextEdit, and the 2000-node scale test. The full core
  steps file (`a_sw`) logged the same RESULT lines as OpenGL. Window mapped after 53 ms (75 ms with
  `NODES=2000`). This is a pure CPU rasterizer inside Slint; it needs no GL, Vulkan or Mesa. At
  half size the screenshots look the same as with femtovg. The software renderer has no rendering
  notifier, so `PERF` frame times are not available there.
- **No OpenGL at all: Slint does not fall back by itself.** Tested by bind-mounting empty files over
  `libEGL`, `libGL` and `libGLX` in a private mount namespace. The default build then shows a **black
  window** with no error message: the winit backend only falls back when creating the renderer
  object fails, and femtovg fails later, at window creation. The spike therefore probes for
  `libEGL.so.1`/`libGL.so.1` at startup on Linux and selects the software renderer with
  `slint::BackendSelector` when neither loads (`RENDERER no OpenGL library found, selecting the
  software renderer`). With that, the no-GL run rendered correctly and the menu opened. This probe
  does not catch a GL library that loads but cannot create a context.
- **Windows over RDP without a GPU:** not tested. Windows always has `opengl32.dll`, but without a
  GPU driver it offers only GDI OpenGL 1.1, which femtovg cannot use. Expect the same black window
  unless the app selects the software renderer (for example a setting or a
  `SLINT_BACKEND=winit-software` shortcut). Slint's `renderer-skia` feature (D3D on Windows) would be
  the other option; not tried here.
- **macOS:** femtovg on OpenGL (deprecated by Apple but working), or skia/Metal with
  `renderer-skia`. Not tested.

## Accessibility (F13)

Slint has AccessKit built in (`accessibility` feature; UIA on Windows, NSAccessibility on macOS,
AT-SPI on Linux). Built-in widgets set roles. The spike gives each node `accessible-role: list-item`,
`accessible-label: "<KIND> <name>: <AP> · <rating>"`, `accessible-item-selected` and a default
action that selects it.

Tested on Linux with a private D-Bus session, the AT-SPI bus with `IsEnabled=true`, and pyatspi:

```
application 'aegis-spike-slint'
  frame 'AegisTARA spike - Slint' [focused]
    list item 'OR Unintended braking via spoofed brake request: 15 · Medium'
      label 'Unintended braking via spoofed brake request', label 'OR', label '15 · Medium'
    ... (13 list items), label '<status bar>', label 'About Slint'
  -- with the menu and the Equipment submenu open --
    list item 'Open diagnostic session: 15 · Medium' [selected]
      frame 'aegis-spike-slint'
        label 'Elapsed time', label '', label 'Specialist expertise', label '', ...
    frame 'aegis-spike-slint' [focused]
      label 'Standard  (0)', label '', label '✓', label 'Specialized  (4)', label '', ...
```

What works: node roles, names and selected state, pack-driven labels, focus moving into the menu.
Gaps on Linux: the Slint-drawn menu is exposed as **plain labels**, with no menu or menu-item roles,
the checked level appears only as a separate `'✓'` label, separators show as empty labels, and the
tree is flat. The disabled state of read-only menu items was not checked. On Windows and macOS the context menu is a native OS
menu (muda), which should give real menu semantics including checked state, but that was not tested.
Not tried with a real screen reader (NVDA, Narrator, Orca).

## What was hard

- **Deleting the item whose menu is calling you.** "Add child step" and "Delete" change the node
  list, which destroys the `NodeItem` that owns the active menu. Model changes from menu callbacks
  therefore run in a zero-delay `Timer::single_shot` after the callback returns.
- **`checkable` toggles `checked`.** Activating a checkable item flips `checked` and breaks its
  binding. It is harmless here because the menu tree is rebuilt on every `show()`, but it is
  surprising. `checkable: true` is needed for the native menus to show the check mark.
- **Focus bookkeeping.** After an item in a submenu is chosen, or after the popover closes, focus
  does not return to the canvas by itself, so Ctrl+Z and Shift+F10 stopped working until the code
  calls `canvas-keys.focus()` explicitly.
- **No transforms and no anchoring.** Zoom means scaling every length by hand; the popover position
  is computed in Rust. Culling and level of detail are the app's job.
- **Silent black window without OpenGL** (see above); found only by testing a GL-less setup.
- **Data flow between Rust and `.slint`.** Each view needs a struct (`NodeView`, `FactorView`), a
  global with properties and callbacks, and index-based mapping back to pack ids. It is clean but
  verbose. Compile errors in `.slint` are clear and point at the line.

What was easy: data-driven submenus with `for` loops in the menu, check marks, disabled items,
ComboBox/TextEdit/Button in the popover, Ctrl+Z and Shift+F10 via `KeyBinding`, and switching to the
software renderer. The Fluent light style looks clean and consistent out of the box.

## Licenses

`cargo metadata` over the resolved graph (437 packages on Linux, 325 for Windows, 346 for macOS):
apart from Slint itself everything is MIT, Apache-2.0, BSD-2/3-Clause, ISC, Zlib, Unicode-3.0,
Unlicense-or-MIT, 0BSD-or-MIT or CC0-or-Apache. There is no MPL, LGPL or GPL-only crate.

**Slint** (the 14 `slint`/`i-slint-*` crates) is triple-licensed: GPL-3.0-only, the Slint
Royalty-free License 2.0, or the commercial Slint Software License 3.0. This spike assumes the
**Royalty-free License 2.0** (text in the crate's `LICENSES/`): free for desktop, mobile and web
applications. Conditions: either show the `AboutSlint` widget in an About dialog reachable from the
app's top-level menu, or show the "Made with Slint" badge on a public web page (for example the
download page). It does not allow embedded systems, distributing Slint on its own, or an app that
exposes Slint's APIs. The spike shows `AboutSlint` in an About popup ("About Slint" in the status
bar); a real app would put it in Help > About. Legal should review the text before shipping.

## Verdict

Slint **meets the core requirement**. The right-click menu on a node, factor submenus generated
from the pack with the current level checked, greyed read-only items, an edit popover with
ComboBoxes, a multi-line TextEdit and a live preview, live badge updates, undo and keyboard access
all work, mostly with built-in widgets and very little glue. The `.slint` language makes the
data-driven menu short and readable. The software renderer is a real strength: it renders the whole
app with no GPU and no Mesa, which matters for remote desktops.

Weak points:

- the canvas, pan, zoom, culling and popover placement are hand-built;
- submenus at the window edge overlap rather than flip;
- the Linux menu accessibility tree is poor (labels, no menu roles, no checked state);
- there is no automatic fallback to the software renderer when OpenGL is missing (the app must pick
  it, as this spike does on Linux);
- the license is not a standard permissive one: the Royalty-free License 2.0 requires visible
  attribution.

Recommendation: a strong candidate, especially for GPU-less users. Confirm the license with legal,
test native menus and NVDA on Windows, and test the software renderer over RDP on Windows before
committing.
