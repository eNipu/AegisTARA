# UI Toolkit Spike: Results and Decision

- Date: 2026-10-11
- Question: which native Rust UI toolkit lets a user **right-click an attack node and change its feasibility values**, and still meets the rest of [SPEC.md](SPEC.md)?
- Method: five toolkits built the same app on the same shared model and data. Each one was then **re-tested by a separate agent** that tried to refute the builder's claims. All tests ran headless on Linux (Xvfb, Mesa software rendering).

## TLDR

- **Decision: Slint.** Built-in right-click menus with submenus and check marks, built-in popover widgets, and a stable 1.x API. It is the only finalist that is both mature and runs fully on a CPU-only renderer (36 MB at 2,000 nodes).
- **Condition:** Slint's free license for proprietary desktop apps (Royalty-free 2.0) requires an "About Slint" notice in the app and a badge on the download page. If that is not acceptable, use **iced** (MIT) as the fallback, or buy Slint's commercial license.
- **All five toolkits can do the core interaction.** The re-tests confirmed it every time. The differences are in maturity, the no-GPU story, and how much has to be custom-built.

## What every spike proved

- Right-click a node: a menu opens at the cursor with one submenu per factor. The labels, levels, and values come from the methodology pack at runtime.
- Choosing a level recomputes the attack potential, and badges update up to the root. All four expected-value checks in SPEC matched exactly in every toolkit.
- "Edit feasibility…" opens a panel next to the node with a dropdown per factor, a rationale text field, a live preview, and OK/Cancel.
- Gate menu (AND/OR, add child, delete), pan and zoom, Shift+F10 and the Menu key, read-only mode, undo, and 2,000 nodes all work.
- Each verifier renamed a factor, added a sixth level, and recolored a rating in the data file. The UI followed without code changes.

## Comparison

| | Slint 1.18 | GPUI (gpui-kit 0.7) | iced 0.14 | Freya 0.4 | egui 0.36 |
|---|---|---|---|---|---|
| Right-click menu | Built-in (`ContextMenuArea`) | Built-in (gpui-component) | **Custom** (iced_aw cannot do submenus) | Built-in | Built-in |
| Edit popover widgets | Built-in | Built-in | Built-in widgets, custom placement | Built-in, custom multi-line text | Built-in |
| Features confirmed by re-test | 12 of 13, a11y partial | 12 of 13, a11y partial | 13 of 13 | 11 of 13 | 12 of 13, a11y partial |
| Bugs found by re-test | Minor | Minor | Several in the custom popover and menu | Lost fast clicks; menu reopen bug; no Home/End | Some menu edge cases |
| CPU-only rendering (no GPU, for remote desktops) | **Yes** (software renderer, 36 MB) | **No**: needs Vulkan or GL; crashes if none | **Yes**: automatic fallback (24 MB) | **Yes**: automatic fallback | **No**: needs a GPU or GL emulation; exits if none |
| 2,000 nodes: peak memory | 150 MB (GL), 36 MB (CPU) | 217 MB | 181 MB (GPU), 24 MB (CPU) | 156 MB | 147 MB |
| API stability | **Stable 1.x** | Pre-1.0 snapshots of Zed, published by a third party | 0.x, breaking releases | 0.x, young, one main maintainer | 0.x, breaks every minor release |
| Code size (spike) | 864 lines (437 Rust + 427 .slint) | 671 | 978 | 756 | 770 |
| Look (verifier score, 1 to 5) | 4 | 4.5 | 3.5 | 3.5 | 3 |
| Code clarity (verifier score) | 4 | 4 | 3.5 | 3 | 3.5 |
| License | Royalty-free 2.0 with attribution, or GPL-3, or commercial | Apache-2.0 | MIT | MIT | MIT/Apache (plus OFL fonts) |

Side by side: [menus](compare_03_submenu.png) and [edit popover](compare_07_popover_edited.png).

## Why Slint

1. **Least custom code for the core ask.** The menu, submenus, check marks, disabled items, dropdowns, and text editing are all built-in widgets.
2. **Works without a GPU.** Many automotive engineers work over Citrix, RDP, or VDI. Slint's software renderer drew everything, including curved edges, menus, and the popover. It used the least memory of all the GPU-capable options.
3. **Stable.** It is the only finalist with a 1.x API. This is a long-lived product, so fewer forced rewrites matter.
4. **Clean architecture fit.** UI layout lives in `.slint` files and logic in Rust. This matches our plan to keep a toolkit-independent client core.
5. **Domain fit.** Slint is used in automotive and industrial products, so its vendor understands this market.

## Known Slint gaps (from the re-test)

- No transforms, so zoom scales every length by hand.
- There is no anchoring API: the popover position is computed in Rust.
- If OpenGL is missing, Slint shows a black window instead of falling back. The spike fixes this with a small startup probe that picks the software renderer.
- Shift+F10 opens the context menu natively only on Windows, so other platforms need a little glue.
- Focus has to be returned to the canvas by hand after menus and popovers.
- Read-only menu items are not reported as disabled to screen readers.

None of these blocks the product. All have workarounds in the spike.

## Why not the others

- **GPUI:** best-looking menus, but no CPU renderer, and it crashes when no adapter exists. It is built on unofficial pre-1.0 snapshots published by a third party. Watch it, and revisit if Zed publishes GPUI officially.
- **iced:** fully MIT, with the best no-GPU fallback. But the menus had to be custom-built, and the re-test found several bugs in that code. **This is the fallback if the Slint license condition is rejected.**
- **Freya:** good built-in widgets and a CPU fallback. But it is young, has mostly one maintainer, and has input bugs (fast clicks are lost).
- **egui:** the simplest to write, but the plainest look, no CPU renderer, and frequent breaking changes.

## Limits of this test

- All runs were on Linux with software rendering. Windows was only compile-checked for some toolkits, and not run. **Before locking the choice, run the Slint spike on a real Windows laptop and over an RDP or Citrix session.**
- Screen reader support was checked with an accessibility tree dump only, not with a real screen reader.
- Frame times under software rendering are not GPU performance numbers.

## Run it yourself

Only `slint/` (the chosen toolkit) is kept on `master`. The `gpui/`, `iced/`, `freya/` and `egui/` spikes, each with a README with build and run steps and its screenshots, live on the `archive/ui-spikes` branch (`git switch archive/ui-spikes`). Start there if the Slint license condition is rejected and iced becomes the fallback.
