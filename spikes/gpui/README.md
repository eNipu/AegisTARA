# UI spike: GPUI (Zed) via GPUI Kit

The SPEC app (`../SPEC.md`) built with **GPUI**, Zed's UI framework, through Longbridge's
**GPUI Kit** stack:

| Crate | Version | License | Role |
|---|---|---|---|
| `gpui-kit` | =0.7.1 | Apache-2.0 | Facade. It is the only UI dependency in `Cargo.toml`. |
| `gpui-component` / `gpui-base` / `gpui-kit-assets` | 0.7.1 | Apache-2.0 (Lucide icons ISC) | Styled widgets, unstyled behaviour, icons |
| `gpui-pre`, `gpui-pre-platform`, `-wgpu`, `-linux`, `-windows`, `-macos` | =0.3.8 | Apache-2.0 | Weekly snapshots of Zed's `gpui` (zed@279fe07), published on crates.io |

Everything comes from crates.io, with no git dependencies. The official `gpui` crate on crates.io
(frozen at 0.2.2) is **not** used. `Cargo.lock` has 867 packages and is committed so the
snapshot pins stay reproducible.

All code is in `src/main.rs` (about 670 lines). Every factor, level, label, value and rating color
comes from `spikes/shared/pack.json` at runtime. The only fixed strings in the UI are verbs:
"Gate type", "Add child step", "Delete", "Edit feasibility…", "Rationale", "OK" and "Cancel".
AND and OR come from `Kind::label()`.

## Build and run

The target folder in the commands below is only an example; any folder outside the repo works.

```sh
# Linux (Ubuntu 24.04)
sudo apt install gcc g++ clang pkg-config libfontconfig-dev libfreetype-dev libwayland-dev \
  libxkbcommon-x11-dev libx11-xcb-dev libvulkan1 mesa-vulkan-drivers
cd spikes/gpui
CARGO_TARGET_DIR=/tmp/targets/gpui cargo run --release
NODES=2000 cargo run --release     # scale test
READONLY=1 cargo run --release     # read-only mode
```

- **Windows 10/11.** Install Visual Studio 2022 Build Tools with the "Desktop development with
  C++" workload (MSVC and the Windows SDK) and use the MSVC Rust toolchain. Then run
  `cargo run --release` in `spikes\gpui`. Release builds compile HLSL shaders with `fxc.exe`
  from the Windows SDK. The build script finds it on PATH or in the SDK, or you can set
  `GPUI_FXC_PATH`; debug builds compile the shaders at runtime instead. gpui-kit's docs also ask
  for CMake, but no `cmake` crate is in this spike's dependency graph. **Verified here:**
  `cargo check --target x86_64-pc-windows-msvc` passes (1m41s). Not built, linked or run on real
  Windows.
- **macOS 12+.** Install the Xcode command-line tools, then run `cargo run --release`. gpui-kit
  turns on `runtime_shaders`, so a full Xcode install is not needed for the Metal shaders. Not
  tested here.

Environment variables:

| Variable | Effect |
|---|---|
| `NODES` | Number of generated nodes (handled by spike-model) |
| `READONLY=1` | Read-only mode (handled by spike-model) |
| `RUST_LOG=info` | Show the GPU adapter GPUI picks |
| `PERF=1` | Print a `FRAME visible=.. edges=.. <time>` line per frame |
| `A11Y_DUMP=1` (or `full`) | Print AccessKit status (or the whole tree) |
| `AEGIS_SPIKE_DATA` | Folder with `pack.json` and `tree.json` |

The app prints `NODE`/`RESULT` lines at start, `CHANGE`/`RESULT` lines after every edit, and
`MENU_OPEN`, `KBMENU_OPEN`, `EDITOR_OPEN`, `PREVIEW`, `EDITOR_OK`, `SELECT`, `ZOOM`, `PAN` and
`FIRST_FRAME` lines for the tests.

## Approach

**Canvas (custom-built, as there is no node-graph widget).** One `Canvas` view holds the `Model`,
`zoom` and `pan`. Each node in view is an absolutely positioned `div`: white fill, 1 px `#D0D4DC`
border, radius 8, a 14 px title, an AND/OR chip on gates, and a badge pill in the rating color.
GPUI has no element transforms, so zoom scales positions, sizes, text, padding and corner radii
by hand (`screen = world * zoom + pan`). Edges are drawn in a single `canvas()` element behind the
nodes as cubic Béziers (`PathBuilder::stroke` + `cubic_bezier_to`) from the parent's right edge
to the child's left edge.

Culling skips every node and edge whose screen bounds miss the view. Below zoom 0.45 a node draws
no text, only a strip in its rating color (level of detail). Layout is cached and recomputed only
when `model.generation()` changes.

Pan and zoom are custom too. A left drag on empty canvas pans (nodes are found by hit-testing
against `layout()`), and the wheel zooms around the cursor.

**Context menu (built-in).** `gpui_kit::component::menu::ContextMenuExt::context_menu(...)` is
attached to each node div. The builder runs on every open, so it reads fresh model data. One
`PopupMenu::submenu` per pack factor holds one `PopupMenuItem` per level with
`.checked(current)`, `.disabled(read_only)` and `.on_click(...)`, and the click calls
`set_factor`. Gates get a "Gate type" submenu (AND/OR, current one checked), "Add child step"
and "Delete". Submenus, check marks, disabled styling, hover, arrow, Enter and Escape handling,
and clamping to the window edge are all built in.

**Keyboard-opened menu (custom glue, about 25 lines).** `ContextMenuExt` reacts only to the right
mouse button. Shift+F10 and the Menu key are bound to an `OpenNodeMenu` action in the `Canvas`
key context. The handler builds the same menu with `PopupMenu::build` from a data snapshot,
focuses it, and shows it in `deferred(anchored().snap_to_window_with_margin(..))`. A
`DismissEvent` subscription hands focus back to the canvas. The snapshot is needed because GPUI
panics if a view reads itself while it is being updated.

**Edit popover (built-in parts).** `gpui_kit::base::Positioner::side(node_bounds)`
`.placement(Right)` inside `deferred()` anchors the panel next to the node. It flips side when
there is no room and clamps to the window. The panel holds one built-in `Select` per factor, a
built-in multi-line `Textarea` for the rationale, a live preview pill ("`<AP> · <rating>`" from
`model.preview()`, updated on `SelectEvent::Confirm`), and built-in `Button`s for OK and Cancel.
OK calls `set_factors`. Escape cancels. Focus goes back to the canvas afterwards, otherwise
Ctrl+Z stops working.

## Feature status

All features were tested headless with `spikes/tools/gui_run.sh` (Xvfb + lavapipe). The final
screenshots in `screenshots/` come from the release binary. The same steps also passed with the
debug binary.

| ID | Status | Built-in or custom | Evidence |
|---|---|---|---|
| F1 Canvas | works | custom (divs + `canvas()` paths) | `01_initial.png`: SPEC style, curved edges, badges in pack colors |
| F2 Right-click menu | works | built-in `ContextMenuExt` | `02_menu.png`: opens at the cursor, above the canvas. Right-click also selects the node |
| F3 Factor submenus | works | built-in `PopupMenu::submenu` + `.checked()` | `03_submenu.png`: 5 submenus with pack labels, "Specialized (4)" checked. Choosing Standard gives `RESULT s6 11 High`, `g2 11 High`, `g0 11 High` |
| F4 Live update | works | model + `cx.notify()` | `04_changed.png`: red High badges on s6, g2 and g0 |
| F5 Gate menu | works | built-in | `09_gate_menu.png`: Gate type with AND/OR (OR checked), Add child step, Delete. AND gives `RESULT g3 55 Very low` and leaves the root unchanged. Add child step created `n1` (`0 High`), and Delete removed it |
| F6 Edit popover | works | built-in Select/Textarea/Button, placed by `Positioner` | `06_popover.png`, `07_popover_edited.png`: live preview `14 · Medium`. OK gives `RESULT s6 14 Medium`, `g2 14 Medium`, `g0 14 Medium` (`08_after_ok.png`). Escape and Cancel close it without changes, and Ctrl+Z works afterwards |
| F7 Pan and zoom | works | custom | `11_zoom_pan.png`: 3 wheel notches around the cursor (zoom 1.331), then a drag on empty canvas (`PAN -424 -58`) |
| F8 Keyboard | works (with one quirk) | built-in menu navigation; custom Shift+F10 / Menu key glue | `10_keyboard_menu.png`: s2 selected, Shift+F10, Down, Right. The Menu key works too. Down/Right/Return in the gate menu set g3 to AND. Escape closes. **Quirk:** the first Down selects index 0 even when it is a non-clickable label item, which broke navigation until the menu header label was removed |
| F9 Read-only | works | built-in `.disabled()` | `12_readonly.png`: the menu opens, levels, Edit and Delete are greyed, and the current level stays checked. Clicking a greyed level does nothing. The status bar shows READ-ONLY |
| F10 Undo | works | model `undo` + custom `ctrl-z` binding | `05_undo.png`: `RESULT g0 15 Medium`. Also undid the g3 gate change |
| F11 Scale | works | custom culling + LOD | `13_scale.png`: `NODES=2000` (2114 nodes), zoom 0.51, menu and submenu on generated step f5 (the submenu flipped left). First frame 0.13 s (release). The menu also opens at zoom 0.24 |
| F12 Text input | works | built-in `Textarea` | Typed "Needs only a laptop", Ctrl+Left, Ctrl+Shift+Right selected "laptop" (highlight visible), typed over it, then Enter and a second line. The log shows `rationale="Needs only a cheap laptop\nand an OBD dongle"`. IME not tested |
| F13 Accessibility | partial (as reported) | built-in AccessKit in GPUI core; roles and labels added on nodes | See below. Checked through AT-SPI with pyatspi |

`14_edge.png`: right-click on s9 near the bottom, then Equipment opened with the keyboard. The
root menu does not flip, it shifts up to stay inside the window (`snap_to_window_with_margin`).
The submenu also shifts up. Near the right edge, submenus flip to the left (see `13_scale.png`).
Menus are drawn by GPUI, so they cannot extend past the window frame.

### Expected values (SPEC)

| Action | RESULT lines logged |
|---|---|
| Initial | `s6 15 Medium`, `g2 15 Medium`, `g0 15 Medium` |
| s6, Equipment = Standard | `s6 11 High`, `g2 11 High`, `g0 11 High` |
| then Ctrl+Z | `g0 15 Medium` |
| s6 popover: Elapsed time = ≤ 1 day, OK | `s6 14 Medium`, `g2 14 Medium`, `g0 14 Medium` |
| g3, Gate type = AND | `g3 55 Very low`, root unchanged (`g0 15` from the initial state in a separate run, `g0 14` after the popover step) |

### Test harness note (window focus)

Xvfb has no window manager, so nothing gives the window X input focus and GPUI never marks it
active. Keyboard input still arrives, but gpui-base inputs paint the caret and the selection
highlight only when `window.is_window_active()`. The test runs wrap the binary in a 3-line shim
that runs `xdotool search --sync --name "AegisTARA spike" windowfocus` in the background and then
`exec`s the app, which is what a window manager does. The app and the harness are unchanged. On a
normal desktop this does not come up.

## Rendering backends and software (CPU) rendering

- **Linux:** wgpu 29 with **Vulkan or GL only** (the backends are hard-coded in gpui-pre-wgpu).
  Adapters are sorted discrete > integrated > virtual > CPU, and software adapters are accepted
  at startup.
  - **lavapipe (Vulkan on the CPU): works.** It is the default in the harness:
    `Selected GPU adapter: "llvmpipe (LLVM 20.1.2, 256 bits)" (Vulkan)`. All screenshots are from
    this path.
  - **llvmpipe through GL/EGL: works.** Tested by hiding the Vulkan ICD
    (`VK_ICD_FILENAMES=/nonexistent.json`): `Selected GPU adapter: "llvmpipe ..." (Gl)`. The tree
    rendered correctly and the menu change logged `s6 11 High`.
  - **No adapter at all** (no Vulkan ICD and no EGL vendor): the window cannot open
    (`Failed to create surface for any enabled backend`). GPUI has no built-in pure-CPU
    rasterizer like tiny-skia. On Linux the CPU path is Mesa (lavapipe or llvmpipe), which
    standard distributions ship.
  - After a GPU device loss, GPUI rebuilds its context with software adapters excluded.
- **Windows:** Direct3D 11 through DXGI. The adapters are enumerated in order and the first one
  that creates a D3D11 device at feature level 10.1 or higher wins. On a GPU-less RDP session
  Windows exposes the "Microsoft Basic Render Driver" (WARP, a DXGI software adapter that
  supports D3D11), so GPUI should start on WARP. GPUI's own tests create WARP devices. Zed
  officially does not support the Basic Display Adapter, though, and this was **not tested**. Try
  `GPUI_DISABLE_DIRECT_COMPOSITION=1` if presentation fails over RDP or Citrix.
- **macOS:** Metal only (not tested).

### Performance (headless, llvmpipe, 4 shared CPUs, another build running in parallel)

| Run | First frame (process start to first frame) | Frame time median / p90 / max |
|---|---|---|
| Release, 13 nodes | 0.12–0.15 s | 17–48 / 48–64 / 110–217 ms |
| Release, `NODES=2000` | 0.13–0.17 s | 48–64 / 96–112 / 128–176 ms |
| Debug, 13 nodes | 0.41–0.47 s | 48 / 160 / 176 ms |
| Debug, `NODES=2000` | 0.36–0.45 s | 79 / 160 / 256 ms |

Frame time is measured from the start of `render` to GPUI's next-frame callback, so it includes
waiting for the frame tick. With culling, the node count barely matters. Even at the minimum
zoom of 0.05, the SPEC tree is one tall column with at most about 200 nodes in view (73 were
seen). Menus open within one frame and hover feels responsive. Sizes: the release binary is
58 MB (42 MB stripped) and the debug binary 163 MB. The target dir is 5.0 GB (debug, release and
a Windows check). Rebuilding only the app crate takes 3–7 s.

## Accessibility (F13)

GPUI core has AccessKit built in (accesskit 0.24; adapters: UIA on Windows, NSAccessibility on
macOS, and `accesskit_unix` / AT-SPI on Linux). The tree is built only while an assistive
technology is connected. Elements show up when they have an id plus a role or label. The spike
gives each node `.role(Role::TreeItem).aria_label("<KIND> <name>: <AP> · <rating>")`,
`.aria_level(depth)` and `.aria_selected(..)`.

Tested on Linux by starting a D-Bus session and the AT-SPI bus, setting `org.a11y.Status
IsEnabled=true`, and dumping the tree with pyatspi (`A11Y enabled=true active=true`):

```
application 'aegis-spike-gpui'
  frame 'AegisTARA spike - GPUI' [focused]
    tree item 'OR Unintended braking via spoofed brake request: 15 · Medium'
    tree item 'STEP Open diagnostic session: 15 · Medium'  ... (13 tree items)
  -- with the menu open --
    menu '' [focused]
      menu item 'Elapsed time' ... 'Equipment' [selected], '' (separator), 'Edit feasibility…', '' , 'Delete'
    menu ''
      menu item 'Standard  (0)', 'Specialized  (4)', 'Bespoke  (7)', 'Multiple bespoke  (9)'
```

What works: roles, names, the hovered or selected menu item, focus moving into the menu, and
pack-driven labels. Gaps: the checked level is **not** exposed (no checked or radio state on
menu items), submenu expanded state is missing, separators show up as unnamed menu items, menus
have no name, and the tree items are flat (`aria_level` is set, but there is no parent/child
structure, because nodes are sibling divs). Not tried with a real screen reader (NVDA, Narrator,
Orca). On Windows, `gpui_kit::component::native_menu::NativeMenu` is an alternative that gives
native Win32 menu semantics, including checked state.

## What was hard

- **Menu keyboard quirk:** with a header `.label(..)` as the first item, Down selected that
  label and Right did nothing. I found it in `select_down` (it jumps to index 0 without checking
  that the item is clickable) and fixed it by dropping the header.
- **Re-entrancy:** a GPUI view cannot read itself while it is being updated, so the keyboard menu
  path builds from a cloned `MenuData` snapshot.
- **Focus bookkeeping:** after the editor closes, focus must go back to the canvas by hand, or
  Ctrl+Z goes nowhere. The Shift+F10 menu needs a `DismissEvent` subscription for the same
  reason.
- **Headless caret and selection** were invisible until the X window was given focus (see the
  harness note above).
- **No transforms:** zoom means scaling every size by hand. Culling and level of detail are the
  app's job, and GPUI repaints the whole window every frame.
- **Churn:** GPUI Kit had breaking renames in 0.6.0 and 0.7.0 (September 2026) and ships weekly.
  The gpui-pre snapshots are published by one Longbridge maintainer, not by Zed. Pin exact
  versions and upgrade as a whole.

What was easy: data-driven submenus with check marks and disabled items, the popover with
Select and Textarea, the Shift+F10 path (about 25 lines), and the default polish. The menus and
selects look good out of the box, with shadows, hover states, focus rings and animation.

## Licenses

`cargo metadata` over the resolved graph (608 crates on Linux, 513 on Windows, 529 on macOS):
everything is MIT, Apache-2.0, BSD, ISC, Zlib, Unicode-3.0, CC0, BSL-1.0 or bzip2-1.0.6, with
these exceptions:

- `option-ext` is MPL-2.0: file-level copyleft, fine to use unmodified.
- `self_cell` is "Apache-2.0 OR GPL-2.0-only"; take Apache.
- `cbindgen` is MPL-2.0, but only as a macOS build tool.

There is no LGPL, GPL-only or AGPL. Apache-2.0 requires NOTICE and attribution.

## Verdict

GPUI with GPUI Kit **meets the core requirement well**. The right-click menu on a node,
data-driven factor submenus with the current level checked, greyed read-only items, the anchored
edit panel with Select, Textarea and a live preview, live badge updates, undo and keyboard access
all work. Most of it uses built-in widgets, and the result looks polished. The canvas, pan, zoom,
culling and the Shift+F10 path are custom but small.

The risks are not in what it can do:

- the stack is pre-1.0, with weekly snapshot releases and frequent breaking changes;
- Windows software rendering (WARP over RDP or Citrix) is plausible but unverified and not
  supported by Zed;
- the accessibility gaps for checked state and tree structure are real.

Recommendation: a strong candidate if the team accepts pinning and periodic upgrades. Test on
customer VDI and RDP images with NVDA in week 1 before committing.
