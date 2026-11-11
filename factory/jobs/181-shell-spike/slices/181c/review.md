VERDICT: PASS
## For the owner
What changed: the GPUI spike gets a `--measure` mode that pans the 100,000-node tree for 300 frames and prints start time, frame times and item count.
Two scripts (macOS, Windows) add memory and binary size. RESULTS.md holds the Mac numbers and empty `TODO(owner)` rows for Windows.
Look at: RESULTS.md:12-16 (the Mac row; 85 items only, so canvas time is a light load); src/canvas_tab.rs:191-232 (timing, pan and quit timer); measure.ps1 (never run, no Windows here).
Risk: low, spike code only. The Windows script is untested; the first run on a Windows laptop may need a small fix. The 8.3 ms frame interval is the 120 Hz display rate, so CANVAS_MS (0.2 ms) is the real headroom number. The view shows only 85 items, so it says little about a crowded view.
View: git -C factory/worktrees/181-shell-spike diff 2484126424c670fc87cbaaa63455c47e652be1d3..HEAD
## Findings
- none blocking. Minor: canvas_tab.rs paint closure spawns a new 3 s quit timer on every paint after `done`; harmless (quit is idempotent) but could be guarded.
- Args are validated with caps (nodes 1..=1,000,000, frames 1..=10,000); no new dependency, no unsafe, no paths from input.
## Lesson
none
NEXT: human
