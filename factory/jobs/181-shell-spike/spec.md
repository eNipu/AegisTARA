## Open questions

1. **What counts as a pass?** Recommended: GPUI passes if, on the Windows laptop, it meets the budgets in `docs/engineering-standards.md` section 2 (cold start to first frame ≤ 1.0 s; canvas frame time ≤ 16 ms while panning the 100,000 node tree; resident memory ≤ 300 MB), and over RDP it starts, draws and pans with frame interval p95 ≤ 50 ms (the interaction latency budget). Binary size has no budget: it is recorded only. D37 already accepts "no CPU-only rendering", so a machine with no GPU at all is not a fail.
2. **What if GPUI fails?** Recommended: this job does not build a second toolkit. The decision record states the failure with the numbers, the Slint shell keeps shipping (D37), and the planner opens a follow-up spike of iced (the tested fallback named in D7) with the same four parts and the same measurement script.
3. **Who runs Windows and RDP?** Recommended: the owner, between slices 181c and 181d, on a real Windows laptop (build with `cargo build --release` there; MSVC build tools needed), once locally and once from the Mac through an RDP client into that laptop. The builder agent cannot do this. The owner pastes the printed lines into `spikes/gpui-shell/RESULTS.md`.

## Title

Workbench shell toolkit spike: GPUI with gpui-component (job 181, WBX-01)

## Design Reference

- `docs/design/workbench-breakdown.md` group 5, row `181-shell-spike`, requirement WBX-01.
- `docs/decisions.md` D37 (workbench, GPUI first candidate, spike on macOS, a Windows laptop and Windows over RDP), D40 and D43 (GUI first; edits go through the business logic), D8 (spike code is not promoted to production), D5 (no code copied from Zed's GPL crates).
- `docs/design/workbench-architecture.md` section 9, "Shell toolkit" bullet.
- `docs/engineering-standards.md` section 2 (budgets).
- `spikes/RESULTS.md` (the 2026-10 toolkit spike; GPUI was rejected then for no CPU renderer and third-party snapshots).
- `crates/aegis-canvas` (`Scene`, `Viewport`, `render`, `DisplayList`, `Item`, `SvgTheme::hex`): the spike paints this display list, which is what the real tree tab (job 187, WBX-05) will do.

## Problem & Goal

D37 replaces the page-based Slint shell with a workbench (docks, tabs, tree canvas, inspector). GPUI with gpui-component is the first candidate, but in 2026-10 it was rejected for no CPU renderer and an unofficial release. Before job 182 starts a new crate on it, we need numbers on the machines our customers use: automotive engineers often work on Windows laptops and over remote desktop.

Business value: one week now avoids building the whole workbench (groups 5 to 7, about 15 jobs) on a toolkit that fails on a customer's remote desktop. Each part of the spike earns its place:
- **Dock with tree, tabs, inspector:** the workbench layout itself (WBX-02, WBX-03, WBX-06); if gpui-component's dock cannot do this, we write our own, which changes job 182's size.
- **Tree canvas from the `aegis-canvas` display list:** the product's core view; proves the toolkit-neutral adapter path and the 16 ms pan budget at 100,000 nodes.
- **Text editor tab:** item definitions are edited as text sections (D38, D40); Japanese input (D13) must work.
- **Measurement mode and scripts:** the decision record needs numbers that the owner can reproduce on Windows without the builder.

Spike code lives in `spikes/gpui-shell/`, its own Cargo root outside the workspace. No crate under `crates/` depends on it, and it is not promoted to production (D8). Edits in the spike change only in-memory view state; there is no model editing, so D43 is not exercised here.

## Acceptance Criteria

1. `spikes/gpui-shell/` builds on macOS with exact-pinned `gpui` and `gpui-component` from crates.io; `scripts/ci.sh spikes/gpui-shell` passes (fmt, clippy `-D warnings`, tests).
2. No crate in the spike's dependency tree has a GPL, LGPL or AGPL license (checked by the listing in `spikes/gpui-shell/README.md`, produced by `cargo metadata`).
3. The window shows a left dock with a project tree (workspace, system, component, assets, threats, attack trees, with counts), a centre tab area with a "Attack tree" tab and a "Item definition" text tab, and a right dock with an inspector that shows the current selection.
4. The "Attack tree" tab paints the `aegis-canvas` display list (rectangles, curves, polylines, text) of a generated tree of `--nodes N` nodes (default 13, fan-out 4); drag pans, the wheel zooms, a click selects a node and the inspector shows its label and kind.
5. The "Item definition" tab is a multi-line editor over a generated 2,000-line text; typing, selection, undo and scrolling work.
6. `--measure` prints `FIRST_FRAME_MS`, `FRAME_INTERVAL_MS p50 p95 max`, `CANVAS_MS p50 p95 max` and `ITEMS` lines after an automatic pan of `--frames F` frames, then `MEASURE_DONE`, and quits 3 seconds later.
7. `spikes/gpui-shell/measure.sh` (macOS) and `spikes/gpui-shell/measure.ps1` (Windows) build nothing, run the release binary with `--measure --nodes 100000`, and add resident memory (`RSS_MB`) and binary size (`BINARY_MB`) lines.
8. `spikes/gpui-shell/RESULTS.md` has the macOS numbers filled in, and rows for Windows laptop and Windows over RDP filled in by the owner, plus a yes/no checklist (starts, draws, pan, zoom, select, dock resize, editor typing, Japanese IME).
9. `docs/adr/0066-workbench-shell-toolkit.md` records the choice with the numbers from all three environments, and D37 in `docs/decisions.md` has a dated "Spike result" line naming the choice and the ADR.

## Out of scope

- Editing the model, ops, undo through the model, saving (D43 is exercised from job 182 on).
- A command palette, settings, themes beyond the light theme, accessibility tree checks (D37 accepts a weaker screen-reader path for now).
- Building or measuring a second toolkit, or re-measuring the Slint shell.
- Any change under `crates/`, `scripts/`, `deny.toml` or CI workflows.
- Moving the spike to an archive branch (done when job 182 starts, as with the 2026-10 spikes).
