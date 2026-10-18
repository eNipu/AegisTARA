# UI Toolkit Spike: Customizable Attack Nodes

Goal: decide which native Rust UI toolkit AegisTARA uses, based on one key interaction:
**right-click an attack node and change its attack feasibility values.**

Every spike builds the same small app with the same data and the same math.
Only the UI toolkit differs.

## Shared parts (do not change)

| Path | What |
|---|---|
| `spikes/shared/pack.json` | Methodology pack: 5 factors with levels and values, 4 rating bands with colors |
| `spikes/shared/tree.json` | Example attack tree (13 nodes) |
| `spikes/spike-model/` | UI-free model crate: load, compute, layout, undo, read-only, logging |
| `spikes/tools/gui_run.sh` | Headless test harness: Xvfb + xdotool + screenshots |

The UI must take every factor, level, label, value, and rating color **from the pack at runtime**.
No hard-coded factor names or levels in UI code. This proves menus can be generated from data.

Use the model crate as a path dependency: `spike-model = { path = "../spike-model" }`.
API summary: `Model::load_default()`, `layout()`, `result(id)`, `rating_of(id)`, `level_of(node, factor)`,
`preview(levels)`, `set_factor`, `set_factors`, `set_kind`, `add_child_step`, `delete`, `undo`, `can_undo`,
`read_only`, `generation()`, `log_layout(dx, dy)`, `log_results()`, constants `WINDOW_W`, `WINDOW_H`.

## The app

- One window, **1400 x 900**, titled `AegisTARA spike - <toolkit>`.
- The canvas fills the window. A thin status bar at the bottom is allowed (max 28 px).
- Light theme. Shared visual style:
  - Background `#F6F7F9`.
  - Nodes: white, 1 px border `#D0D4DC`, corner radius 8, size and position from `layout()`.
  - Node title: 14 px text. Gates also show a small `AND` or `OR` chip.
  - A rating badge pill on each node: `"<AP> · <rating label>"` filled with the rating color, white text.
  - Edges: smooth curves from parent right edge to child left edge, color `#9AA3B2`.
  - Selected node: 2 px border `#3D6DE8`.
- At start: zoom 1.0, no pan, so world coordinates from `layout()` are window coordinates.
- At start, call `model.log_layout(0.0, 0.0)` and `model.log_results()` (prints `NODE` and `RESULT` lines to stdout).

## Features to build

Priority order. F1 to F6 are the core ask. Do them first.

| ID | Feature | Detail |
|---|---|---|
| F1 | Canvas | Draws the tree with the style above |
| F2 | Right-click menu on a node | Opens at the cursor, on top of the canvas |
| F3 | Data-driven factor submenus | For a step: one submenu per factor (label from pack), radio-style items per level showing label and value, current level marked. Choosing one calls `set_factor`. |
| F4 | Live update | After a change, badges update on the step and all ancestors up to the root |
| F5 | Gate menu | For AND/OR nodes: "Gate type" with AND and OR (current marked), "Add child step", and for any non-root node "Delete" |
| F6 | Edit popover | Menu item `Edit feasibility…` opens a panel anchored next to the node: one dropdown (or segmented control) per factor, a multi-line "Rationale" text field, a live preview `"<AP> · <rating>"` that updates while editing (`preview`), and OK / Cancel. OK calls `set_factors`. |
| F7 | Pan and zoom | Drag empty canvas to pan. Mouse wheel zooms around the cursor. |
| F8 | Keyboard | Left-click selects a node. Shift+F10 or the Menu key opens the same menu for the selected node. Arrow keys move in the menu, Enter chooses, Escape closes. |
| F9 | Read-only | With `READONLY=1`, the menu still opens but editing items are disabled (greyed) |
| F10 | Undo | Ctrl+Z calls `undo` |
| F11 | Scale | With `NODES=2000`, the app starts, stays responsive, and the menu still works. Cull nodes outside the view. |
| F12 | Text input | Typing, cursor movement, and selection work in the Rationale field |
| F13 | Accessibility | Report what the toolkit exposes to screen readers (for example AccessKit). No need to build extra. |

## Expected values (from the model tests)

Initial: `s6` = 15 Medium, `g2` = 15 Medium, root `g0` = 15 Medium (via `g2`).

| Action from the initial state | Expected `RESULT` lines |
|---|---|
| s6, Equipment = Standard | `s6 11 High`, `g2 11 High`, `g0 11 High` |
| then Ctrl+Z | `g0 15 Medium` |
| s6 popover: Elapsed time = ≤ 1 day, OK | `s6 14 Medium`, `g2 14 Medium`, `g0 14 Medium` |
| g3, Gate type = AND | `g3 55 Very low`, root unchanged |

## Test procedure (headless)

The harness starts a private Xvfb (1600 x 1000, no window manager, so the window sits at 0,0),
Mesa llvmpipe for OpenGL and lavapipe for Vulkan, runs the app, and executes a steps file.
Read `spikes/tools/gui_run.sh` for the step commands (`sleep`, `shot`, `move`, `click`, `drag`, `key`, `type`, `scroll`).
Use the `NODE` lines in `app.log` for click targets.

Suggested screenshots (crop to the window with `convert in.png -crop 1400x900+0+0 out.png`):

1. `01_initial`
2. `02_menu`: right-click on s6
3. `03_submenu`: hover "Equipment"
4. `04_changed`: after choosing "Standard" (expect red High badges on s6, g2, g0)
5. `05_undo`: after Ctrl+Z
6. `06_popover`: after "Edit feasibility…"
7. `07_popover_edited`: Elapsed time changed and rationale typed, preview visible
8. `08_after_ok`
9. `09_gate_menu`: right-click on g3
10. `10_keyboard_menu`: select s2, then Shift+F10
11. `11_zoom_pan`
12. `12_readonly`: `READONLY=1`, right-click s1
13. `13_scale`: `NODES=2000`
14. `14_edge`: right-click s9 (near the bottom) to see if the menu flips or gets clipped

## Rules

- Code in `spikes/<toolkit>/` as a standalone Cargo project (no workspace).
- Build output outside the repo: `CARGO_TARGET_DIR=<scratchpad>/targets/<toolkit>`.
- Permissive licenses only (MIT, Apache-2.0, BSD, ISC, Zlib, Unicode, BSL-1.0, CC0). Slint under its royalty-free license is allowed but must be noted.
- Keep it small and idiomatic. This is a spike, not product code.
- Do not commit.
