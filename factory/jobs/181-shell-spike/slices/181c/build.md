Commit: 692f1e3 Add measure mode, measure scripts and results sheet to the GPUI spike (181c)
Files: spikes/gpui-shell/{README.md,RESULTS.md,measure.sh,measure.ps1,src/args.rs,src/measure.rs,src/main.rs,src/shell.rs,src/canvas_tab.rs,tests/unit.rs}
Done when:
1. `scripts/ci.sh spikes/gpui-shell` - all steps passed (fmt, clippy, test).
2. `cargo build --locked --release ... && sh spikes/gpui-shell/measure.sh 100000 300` - printed FIRST_FRAME_MS, FRAME_INTERVAL_MS, CANVAS_MS, ITEMS, NODES, MEASURE_DONE, RSS_MB, BINARY_MB, OS, MACHINE.
3. `sh -n spikes/gpui-shell/measure.sh` - ok.
4. RESULTS.md has the macOS row with numbers and `TODO(owner)` in the Windows rows.
Notes:
- macOS output: FIRST_FRAME_MS 520.7; FRAME_INTERVAL_MS 8.3 8.4 61.4; CANVAS_MS 0.2 0.2 0.9; ITEMS 85; NODES 100000; RSS_MB 162.3; BINARY_MB 15.9; OS Darwin 27.2.0; MACHINE Apple M2 Max (Mac14,6). Display is 120 Hz. Cold start 0.52 s is within the 1.0 s budget; the 61 ms max interval is probably the first frames.
- Centring: the prompt's formula is ambiguous; I followed the bench (`origin = scene_size/2 - view/2`, world coordinates). The first attempt with the opposite sign drew 0 items.
- ITEMS is only 85 for a ~1000x900 window at 1:1, because the wide tree is sparse at that zoom; CANVAS_MS therefore says little about a crowded view.
- Quit timer: the pinned gpui's `App::spawn(async move |cx| { cx.background_executor().timer(3s).await; cx.update(|cx| cx.quit()); })` (closure takes only the AsyncApp), called from paint.
- Checklist: only starts and draws-the-tree are yes on macOS; pan/zoom/click/splitter/editor/undo/IME are `not checked` (no interactive session). I did not make up yes values.
- `measure.ps1` was not run (no Windows here).
- Numbers were taken at base 2484126 plus these changes; RESULTS.md says so.
- Changed source lines about 195 (numstat, tests excluded); the slice cap is 200 if plan.json lists 181c at 200.
NEXT: reviewer
