# AegisTARA UI spike: egui + eframe

The SPEC.md app built with **egui / eframe 0.36.2** (immediate mode, pure Rust, winit + wgpu or OpenGL).
The key interaction is right-clicking an attack node and changing its attack feasibility values. It works with
egui's built-in context menu, submenus, radio items, popup, combo boxes and text field. Every factor, level, label,
value and rating color is read from `spikes/shared/pack.json` at runtime. The UI code contains no factor or level names.

Everything below was tested headless (Xvfb + Mesa software rendering) with `spikes/tools/gui_run.sh`.
Windows and macOS were only type-checked (`cargo check --target x86_64-pc-windows-msvc` and
`--target aarch64-apple-darwin` both pass). The spike was not run on them.

## Build and run

Needs Rust 1.95 or newer (egui 0.36 MSRV). Versions are pinned exactly (`=0.36.2`) because every egui 0.x minor
release breaks the API.

```sh
cd spikes/egui
cargo run --release              # or: cargo build --release && target/release/aegis-spike-egui
```

| OS | Notes |
|---|---|
| Linux | X11 and Wayland are both compiled in. X11, Wayland, Vulkan and GL are loaded with `dlopen` at runtime, so the research probe needed no `-dev` packages to build (not re-checked on a clean machine). At runtime it needs a Vulkan or OpenGL driver. Mesa's llvmpipe/lavapipe software drivers are enough. |
| Windows | `cargo build --release` with the MSVC toolchain. The default renderer is wgpu on DX12. With no GPU (RDP/VDI), DX12 exposes the WARP software adapter, which wgpu can use (not tested here). `RENDERER=glow` needs OpenGL 2.1 or newer, so it fails on plain RDP (GDI OpenGL 1.1, egui issue #957). |
| macOS | `cargo build --release`. wgpu uses Metal. No extra steps. |

Environment switches:

| Variable | Effect |
|---|---|
| `NODES=2000` | adds generated steps (model crate) |
| `READONLY=1` | read-only model (model crate) |
| `RENDERER=glow` | OpenGL renderer instead of wgpu |
| `WGPU_BACKEND=gl\|vulkan\|dx12\|metal` | forces the wgpu backend (read by egui-wgpu) |
| `BENCH=1` | repaints continuously and prints `PERF` lines (frame CPU time, visible nodes) |
| `DEBUG_INPUT=1` | prints key and wheel events with the focused widget id |
| `RUST_LOG=warn` | eframe, winit and wgpu logging |

Headless test (as used for the screenshots):

```sh
OUT=<scratch>/run; BIN=<target>/release/aegis-spike-egui
LANG=C.UTF-8 LC_ALL=C.UTF-8 spikes/tools/gui_run.sh $OUT spikes/egui/steps/main.steps -- env RUST_LOG=warn $BIN
# also: steps/edge.steps, steps/readonly.steps (READONLY=1), steps/scale.steps (NODES=2000),
#       steps/renderer.steps (RENDERER=glow or WGPU_BACKEND=gl), steps/bench.steps (NODES=2000 BENCH=1)
```

`LANG=C.UTF-8` is needed for `xdotool type`.

## Approach

- **Canvas (custom).** egui has no graph widget. Everything is painted with `ui.painter()`: rounded rects, two-line
  wrapped titles, a gate chip, the rating pill, and cubic Bézier edges. Each visible node gets one
  `ui.interact(rect, id, Sense::CLICK)`. The background interaction is registered first, so nodes win the hit test.
  Pan and zoom use the spike's own transform (`screen = origin + pan + world * zoom`). Zoom snaps to 1.1^n around
  the cursor. `egui::Scene` was not used: its defaults fight the spec (max zoom 1.0, plain wheel pans, right-drag
  pans). Nodes outside the view are culled. Below 40 % zoom a level-of-detail rule draws only the boxes and rating
  color bars. The layout is cached and rebuilt only when `model.generation()` changes.
- **Menu (built-in).** `response.context_menu(|ui| node_menu(..))` opens at the pointer. Inside a menu,
  `ui.menu_button(label)` becomes a submenu automatically. Factor submenus are generated from the pack, one per
  factor, with `ui.radio(is_current, "<label>  (<value>)")` items. Read-only wraps the editing items in
  `ui.add_enabled_ui(false, ..)`, so the menu opens with those items greyed. The same `node_menu` function serves the
  keyboard menu, which is a plain `egui::Popup` with `PopupKind::Menu`, anchored under the selected node.
- **Popover (built-in).** An `egui::Popup` anchored to the node rect with `RectAlign::RIGHT_START`. It flips or
  shifts at window edges, and `IgnoreClicks` keeps it open while a combo list is used. It contains a `Grid` of
  `ComboBox`es (one per factor), `TextEdit::multiline` for the rationale, a live preview pill from
  `model.preview(&draft)` (recomputed every frame), and OK / Cancel. OK calls `set_factors`. Escape closes it.
- **State.** UI closures only push `Action`s. The model is changed after the frame is built, which avoids
  borrow fights in immediate mode. After a change, every badge simply reflects the model on the next frame.

## Feature status

| ID | Status | Built-in / custom | Evidence and notes |
|---|---|---|---|
| F1 Canvas | works | custom | `01_initial`. Painter primitives match the spec style. Start state is zoom 1.0 and no pan; NODE lines match the window coordinates. |
| F2 Right-click menu | works | built-in | `02_menu`. `Response::context_menu`, opens at the cursor on top of the canvas. |
| F3 Factor submenus | works | built-in | `03_submenu`. Labels, levels and values come from the pack; radio items mark the current level. |
| F4 Live update | works | built-in (immediate mode) | `04_changed`. RESULT `s6 11 High`, `g2 11 High`, `g0 11 High`. |
| F5 Gate menu | works | built-in | `09_gate_menu`. Gate type AND/OR (current marked), Add child step, Delete (not on the root). `g3` AND gives `g3 55 Very low` with the root unchanged. Add then Delete verified (`steps/edge.steps`). |
| F6 Edit popover | works | built-in | `06_popover`, `07_popover_edited`, `08_after_ok`. Combos, rationale, live preview (`14 · Medium`), OK and Cancel. RESULT `s6/g2/g0 14 Medium`. |
| F7 Pan and zoom | works | custom | `11_zoom_pan`. Drag on empty canvas pans; the wheel zooms around the cursor; the menu still works on transformed nodes. Under xdotool, winit reports each wheel notch twice (see below), so the 3 notches in the test give 177 %. |
| F8 Keyboard | works | custom glue on built-in menus | `10_keyboard_menu`. Left-click selects. Shift+F10 **and the Menu key** open the menu at the selected node with the first item focused. Down/Up move, Right or Enter opens a submenu on its current level, Left goes back, Enter picks (verified: s2 Equipment set to Specialized, RESULT `s2 16 Medium`), Escape closes. Needed about 70 lines of glue (see "What was hard"). The focus highlight inside submenus is subtle. |
| F9 Read-only | works | built-in | `12_readonly`. The menu opens; every editing item and submenu is greyed and does not open; Ctrl+Z is ignored; the status bar shows READ-ONLY. |
| F10 Undo | works | built-in | `05_undo`. Ctrl+Z calls `undo` (RESULT `g0 15 Medium`). It is ignored while the rationale field has focus, so TextEdit keeps its own undo. |
| F11 Scale | works | custom culling and LOD | `13_scale`. NODES=2000 (2114 nodes); first frame in about 0.13 s; the menu, submenu and change all work. See the numbers below. |
| F12 Text input | works | built-in | `07_popover_edited`. Typing, Ctrl+Left word jump, Home, and Shift+Ctrl+Right selection verified; the committed text is logged (`COMMIT s6 rationale="Needs OBD physical access"`). IME and clipboard were not tested here. |
| F13 Accessibility | partial | built-in AccessKit | AccessKit is always compiled into egui 0.36 and connected by eframe (UIA on Windows, NSAccessibility on macOS, AT-SPI on Linux). Roles present: Button, RadioButton (with toggled state), ComboBox, MultilineTextInput, disabled flags. Canvas nodes get a Button role and a name ("STEP Open diagnostic session: 15 · Medium") via `widget_info`. Missing in 0.36.2: Menu, MenuItem and MenuItemRadio roles, expanded state, and Dialog for the popover (checked in the source; egui 0.37, unreleased, adds them in PR #8581). No screen reader was available to test with. |

Edge behavior (`14_edge`): the menu on s9 flips upward, its submenu opens to the right, and the popover shifts up
to stay inside the window. Menus and popovers are layers inside the one OS window, so they can never extend past it.

## Software (CPU) rendering

eframe has no CPU renderer of its own. It renders through GPU APIs, and all of these worked on Mesa's software
drivers in this container (`RENDERER` line in `app.log`):

| Path | How | Result |
|---|---|---|
| wgpu → Vulkan → lavapipe | default (harness sets `VK_ICD_FILENAMES`) | renders; `backend=Vulkan device_type=Cpu "llvmpipe"`; egui logs "Software rasterizer detected" |
| glow → OpenGL 4.5 → llvmpipe | `RENDERER=glow` | renders all steps (`renderer.steps`) |
| wgpu → GL backend → llvmpipe | `WGPU_BACKEND=gl` | renders all steps |
| wgpu with no Vulkan driver | `VK_ICD_FILENAMES=/nonexistent.json` | wgpu fell back **automatically** to its GL backend on llvmpipe and rendered |
| no Vulkan and no GL driver | Mesa driver paths pointed to nothing | exits: `WGPU error: Failed to create surface for any enabled backend`. eframe does not fall back to glow automatically. |
| `egui_software_backend` 0.0.3 (third-party CPU rasterizer) | not tried | needs egui `^0.34`, so it is incompatible with 0.36 |

For remote desktops without a GPU: on Linux, Mesa llvmpipe/lavapipe works (verified here). On Windows, the path
would be wgpu DX12 on WARP (not tested); glow will not work over plain RDP.

## Performance (release build, Xvfb, 4 shared vCPUs, another agent busy)

- Startup to first frame: 0.11 to 0.14 s release, 0.2 to 0.25 s debug (from `main()`, `STARTUP` line).
- NODES=2000 with `BENCH=1` continuous repaint, at 100 %, 9 % (LOD) and 62 % zoom:
  - wgpu/lavapipe: 48 to 82 fps.
  - glow/llvmpipe: 65 to 134 fps.
  - App CPU per frame (eframe `cpu_usage`) is mostly 1 to 5 ms, with spikes of 20 to 27 ms on frames right after a
    zoom step. egui lays text out again at each new font size (issue #8093); snapping zoom to 1.1^n limits this.
- Culling keeps at most about 60 nodes visible on screen. egui repaints only on input, so it is idle otherwise.

## What was hard

1. **Keyboard navigation into submenus.** Shift+F10, Up/Down and Enter worked out of the box once the first item
   got `request_focus()`. Right arrow, however, moved focus onto the submenu's popup *area* (popups sense clicks,
   and `Sense::click` includes FOCUSABLE), so Down and Enter then did nothing. I found this with the `DEBUG_INPUT`
   focus trace. The fix is the `submenu()` helper (about 30 lines): Right or Enter opens the submenu through
   `menu::MenuState` (it must be `mark_shown` first, or egui resets it), focuses the current item, and cancels
   egui's spatial move. Left goes back to the parent item.
2. **Menu (Apps) key.** `egui::Key` has no ContextMenu variant, and egui-winit drops `NamedKey::ContextMenu`.
   I start eframe through `eframe::create_native` on my own winit event loop and wrap `EframeWinitApplication` in a
   tiny `ApplicationHandler` that catches the raw key (about 40 lines). This works.
3. **Headless keyboard input (harness only).** With no window manager, the window never gets X input focus, so
   key events are lost. `x11_focus_if_no_wm` calls `XSetInputFocus` through x11rb, only when no EWMH window manager
   is running. A flaky run showed that the call fails (BadMatch) until the window is mapped, so it now retries
   every 100 ms until it succeeds.
4. **Wheel events under xdotool.** winit 0.30.13 on X11 emits a wheel event on both press and release of XTEST
   buttons 4 to 7 (real mice send XI2 valuator motion instead), so one test notch equals two zoom steps. This is a
   harness artifact; the code is not changed for it.
5. **Pan and zoom.** `egui::Scene` would need reconfiguring (zoom range, pan buttons, wheel behavior), so a
   20-line custom transform was simpler. Text in zoomed views is re-laid out at each scale, which causes the
   hitches above.
6. Immediate mode itself was easy for the data-driven menu: the menu body is a closure over the pack, and nothing
   needs to be kept in sync.

## Verdict

egui handles the core requirement well, mostly with built-in widgets. Right-click → data-driven factor submenus
with radio items → live badge updates → anchored edit popover with a live preview took little code (F2 to F6 are
about 200 lines). Read-only, undo and text editing are also built in. The weak spots are keyboard and screen-reader
support for menus: getting arrow keys into submenus and the Menu key needed custom glue, and 0.36 exposes no menu
roles to AccessKit (0.37 adds them). Menus are drawn by egui: consistent across operating systems, but not native,
and confined to the window. Software rendering works through Mesa on Linux. On Windows it should use WARP; that is
untested.

Recommendation: a strong candidate if a custom-drawn look is acceptable. Pin exact versions, plan one API migration
per egui minor release, and plan for the keyboard menu glue and accessibility checks (re-test on 0.37).

## Versions and licenses

egui, eframe, egui-wgpu, egui_glow, egui-winit and epaint 0.36.2; wgpu 30.0.1; glow 0.17.0; glutin 0.32.3;
winit 0.30.13; accesskit 0.24.1; accesskit_winit 0.32.2; x11rb 0.13.2 (Linux only, harness helper).

A `cargo metadata` scan of the resolved Linux graph (268 crates) found only permissive licenses (MIT, Apache-2.0,
BSD, ISC, Zlib, 0BSD, Unicode-3.0, Unlicense OR MIT), with these notes:
- `self_cell` is "Apache-2.0 OR GPL-2.0-only": choose Apache-2.0.
- `epaint_default_fonts` bundles fonts under OFL-1.1 and the Ubuntu Font License. Both allow bundling, but they are
  not on the SPEC list; disable the `default_fonts` feature and ship a different font if that matters.

## Files

- `src/main.rs`: the whole spike (about 770 lines including comments).
- `steps/*.steps`: harness scripts used for the evidence.
- `screenshots/`: 14 PNGs (1400x900) from the release build on wgpu/lavapipe.
