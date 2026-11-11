# ADR 0066: Workbench shell toolkit (GPUI with gpui-component)

- Status: provisional (job 181-shell-spike, slices 181a to 181d; Windows laptop and Windows over RDP deferred to a backlog job)
- Date: 2026-11-11
- Spike: spikes/gpui-shell (not production code, D8)
- Rules: docs/engineering-standards.md section 2
- Decision: D37 in docs/decisions.md
- Builds on: spikes/RESULTS.md (the 2026-10 toolkit spike)

## Context

D37 makes the app a workbench (project tree, tabs, splits, docks) and names
GPUI with gpui-component as the first candidate for the shell. The spike
decides it on macOS, a Windows laptop and Windows over RDP. The 2026-10
spike (spikes/RESULTS.md) rejected GPUI for two reasons: it has no CPU
renderer (it needs Vulkan or GL and crashed without one), and its releases
were pre-1.0 snapshots of Zed published by a third party. D37 accepts the
missing CPU renderer for now and keeps Zed's GPL crates out (D5).

The spike (spikes/gpui-shell) is one window with a left dock (project tree),
a centre tab area (an attack tree tab drawn from the `aegis-canvas` display
list, and a 2,000-line item definition text editor tab) and a right dock
(inspector). Selection in the tree updates the inspector. It has a
`--measure` mode that centres the tree, pans it for 300 frames and prints the
numbers in `RESULTS.md`.

Versions (from `spikes/gpui-shell/RESULTS.md`, Cargo.lock):

- `gpui-pre` =0.3.12 (imported as `gpui`)
- `gpui-pre-platform` =0.3.12, features `font-kit`, `runtime_shaders`
- `gpui-component` =0.9.1

This GPUI snapshot needs the separate `gpui-pre-platform` crate to open a
window. `gpui-pre` alone has no desktop platform layer.

## Decision

No toolkit is chosen or rejected yet. The pass bar is defined on a Windows laptop and over Windows RDP, which were not measured (no Windows machine yet), so it is neither met nor failed.

On macOS, GPUI with gpui-component is within every budget that was measured.

- Cold start (`FIRST_FRAME_MS` 520.7) is under the 1000 ms bar.
- Canvas work (`CANVAS_MS` p95 0.2) is under the 16 ms bar.
- Resident memory (`RSS_MB` 162.3) is under the 300 MB bar.
- Frame interval p95 8.4 ms is under the 50 ms RDP criterion. This is a reference only; the RDP criterion is about Windows.
- The licenses in the locked set contain no GPL-only license (see Measurements).
- Only `starts` and `draws the tree` were checked on macOS. The other seven checklist rows are `not checked`, so click, zoom, editing and IME are unverified.

Job 182 starts only after the Windows follow-up, unless the owner decides otherwise.

## Measurements

Command: `sh spikes/gpui-shell/measure.sh 100000 300` on macOS, after
`cargo build --release --locked --manifest-path spikes/gpui-shell/Cargo.toml`.
`measure.ps1` is prepared for Windows and was never run. Workload: 100,000
nodes, 300 frames, release build. Machine: Apple M2 Max (Mac14,6), Darwin 27.2.0.

| Environment | FIRST_FRAME_MS | FRAME_INTERVAL_MS p50/p95/max | CANVAS_MS p50/p95/max | ITEMS | RSS_MB | BINARY_MB | Machine |
|---|---|---|---|---|---|---|---|
| macOS | 520.7 | 8.3 / 8.4 / 61.4 | 0.2 / 0.2 / 0.9 | 85 | 162.3 | 15.9 | Apple M2 Max (Mac14,6), Darwin 27.2.0 |
| Windows laptop | not measured (deferred to backlog: no Windows machine yet) | not measured (deferred to backlog: no Windows machine yet) | not measured (deferred to backlog: no Windows machine yet) | not measured (deferred to backlog: no Windows machine yet) | not measured (deferred to backlog: no Windows machine yet) | not measured (deferred to backlog: no Windows machine yet) | not measured (deferred to backlog: no Windows machine yet) |
| Windows over RDP | not measured (deferred to backlog: no Windows machine yet) | not measured (deferred to backlog: no Windows machine yet) | not measured (deferred to backlog: no Windows machine yet) | not measured (deferred to backlog: no Windows machine yet) | not measured (deferred to backlog: no Windows machine yet) | not measured (deferred to backlog: no Windows machine yet) | not measured (deferred to backlog: no Windows machine yet) |

The Mac display runs at 120 Hz, so an 8.3 ms interval is the display rate,
not a limit of the app. `CANVAS_MS` covers building the display list and the
paint calls, not the GPU work after them. `ITEMS` is 85 because the wide tree
is sparse at 1:1 zoom in a window of about 1000x900 px, so the measured frame
does not represent a crowded view.

Per criterion, macOS result:

- `FIRST_FRAME_MS` 520.7 against 1000: within the bar.
- `CANVAS_MS` p95 0.2 against 16: within the bar.
- `RSS_MB` 162.3 against 300: within the bar.
- `FRAME_INTERVAL_MS` p95 8.4 against 50: within (reference only).
- Binary size 15.9 MB: recorded, no budget.
- Licenses: no GPL-only license. `r-efi` is `MIT OR Apache-2.0 OR LGPL-2.1-or-later` and `self_cell` is `Apache-2.0 OR GPL-2.0-only`; both offer a permissive alternative. Every `gpui-*` crate is Apache-2.0, except `gpui-pre-reqwest` (MIT OR Apache-2.0). Source: `spikes/gpui-shell/README.md` section Licenses, 760 packages.
- Windows laptop criteria (`FIRST_FRAME_MS`, `CANVAS_MS` p95, `RSS_MB`): not measured.
- Windows over RDP criterion (`FRAME_INTERVAL_MS` p95 at most 50, starts, draws and pans): not measured.

No macOS criterion missed.

## Checklist

| Check | macOS | Windows laptop | Windows over RDP |
|---|---|---|---|
| starts | yes | not measured (deferred to backlog: no Windows machine yet) | not measured (deferred to backlog: no Windows machine yet) |
| draws the tree | yes | not measured (deferred to backlog: no Windows machine yet) | not measured (deferred to backlog: no Windows machine yet) |
| pan | not checked (no one at the keyboard; the automatic pan in `--measure` ran) | not measured (deferred to backlog: no Windows machine yet) | not measured (deferred to backlog: no Windows machine yet) |
| zoom | not checked | not measured (deferred to backlog: no Windows machine yet) | not measured (deferred to backlog: no Windows machine yet) |
| click selects and the inspector follows | not checked | not measured (deferred to backlog: no Windows machine yet) | not measured (deferred to backlog: no Windows machine yet) |
| dock splitter resize | not checked | not measured (deferred to backlog: no Windows machine yet) | not measured (deferred to backlog: no Windows machine yet) |
| editor typing | not checked | not measured (deferred to backlog: no Windows machine yet) | not measured (deferred to backlog: no Windows machine yet) |
| editor undo | not checked | not measured (deferred to backlog: no Windows machine yet) | not measured (deferred to backlog: no Windows machine yet) |
| Japanese IME in the editor (type にほんご) | not checked | not measured (deferred to backlog: no Windows machine yet) | not measured (deferred to backlog: no Windows machine yet) |

The macOS `not checked` rows need a person at the keyboard: the agent had no
Accessibility or Screen Recording permission, so it could not drive the mouse
or see the window.

## Left for later

- Windows follow-up (a backlog job): build the spike on a Windows laptop or VM, run `measure.ps1` locally and through an RDP client, fill the Windows rows and checklist in `spikes/gpui-shell/RESULTS.md`, then set this record's status and D37's result to pass or fail by the bar in Decision.
- The macOS checklist rows still `not checked`: pan, zoom, click selection with the inspector, dock splitter resize, editor typing, editor undo and Japanese IME.
- The click budget (interaction at most 50 ms) was not measured on any machine.
- The screen reader path is weaker than the Slint shell; D37 accepts this for now.
- CPU-only rendering: GPUI needs a GPU; D37 accepts this until the first customer deployment.
- A crowded view: the measured frame drew only 85 items (`ITEMS`), so `CANVAS_MS` says little about a dense tree.
- Dashed strokes: the code uses `dash_array` and `BorderStyle::Dashed`, but no one saw them render. The generated tree has no dashed items.
- The viewport starts at origin with zoom 1 and no fit-to-view.
- `aegis-canvas` has no toolkit-neutral palette type; the adapter reads colours from `SvgTheme`. Job 187 may want a palette table in the canvas crate.
- `tree::Tree` from gpui-component was not evaluated; the spike draws its rows by hand.

## Consequences

- Job 182 (the `aegis-workbench` crate) waits for the Windows follow-up unless the owner says otherwise.
- If the follow-up passes: `aegis-workbench` pins these exact three versions (`gpui-pre` =0.3.12, `gpui-pre-platform` =0.3.12, `gpui-component` =0.9.1), `deny.toml` must allow them in job 182, and Zed's GPL crates are still not copied (D5).
- If the follow-up fails: Slint keeps shipping, and a follow-up job spikes iced (the tested fallback in D7). That iced spike reuses `measure.sh`, `measure.ps1` and `RESULTS.md`.
- `spikes/gpui-shell` stays in place until the follow-up is done, and moves to an archive branch when job 182 starts.
