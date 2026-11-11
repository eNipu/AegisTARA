# GPUI shell spike results (decision D37, WBX-01)

## Budgets (docs/engineering-standards.md)

- Cold start: at most 1.0 s.
- Frame time while panning `large` (100,000 nodes): at most 16 ms.
- Interaction (click, selection): at most 50 ms.
- Client memory: at most 300 MB.
- Binary size: recorded, no budget.

## Numbers

100,000 nodes, 300 frames, release build. Milliseconds except RSS and binary (MB).

| Environment | FIRST_FRAME_MS | FRAME_INTERVAL_MS p50/p95/max | CANVAS_MS p50/p95/max | ITEMS | RSS_MB | BINARY_MB | Machine |
|---|---|---|---|---|---|---|---|
| macOS | 520.7 | 8.3 / 8.4 / 61.4 | 0.2 / 0.2 / 0.9 | 85 | 162.3 | 15.9 | Apple M2 Max (Mac14,6), Darwin 27.2.0 |
| Windows laptop | TODO(owner) | TODO(owner) | TODO(owner) | TODO(owner) | TODO(owner) | TODO(owner) | TODO(owner) |
| Windows over RDP | TODO(owner) | TODO(owner) | TODO(owner) | TODO(owner) | TODO(owner) | TODO(owner) | TODO(owner) |

Notes: the Mac display runs at 120 Hz, so an interval of 8.3 ms is the display rate, not a
limit of the app. `CANVAS_MS` covers building the display list and the paint calls, not the
GPU work after them. `ITEMS` is the display list of the last frame (about 1000x900 px window).

## Checklist

| Check | macOS | Windows laptop | Windows over RDP |
|---|---|---|---|
| starts | yes | TODO(owner) | TODO(owner) |
| draws the tree | yes | TODO(owner) | TODO(owner) |
| pan | not checked (no one at the keyboard; the automatic pan in `--measure` ran) | TODO(owner) | TODO(owner) |
| zoom | not checked | TODO(owner) | TODO(owner) |
| click selects and the inspector follows | not checked | TODO(owner) | TODO(owner) |
| dock splitter resize | not checked | TODO(owner) | TODO(owner) |
| editor typing | not checked | TODO(owner) | TODO(owner) |
| editor undo | not checked | TODO(owner) | TODO(owner) |
| Japanese IME in the editor (type にほんご) | not checked | TODO(owner) | TODO(owner) |

## Versions

- `gpui-pre` =0.3.12
- `gpui-pre-platform` =0.3.12, features `font-kit`, `runtime_shaders`
- `gpui-component` =0.9.1
- Numbers taken at commit 2484126 plus the slice 181c commit (the one that adds this file).
