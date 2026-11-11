VERDICT: PASS
## For the owner
What changed: Windows cells in the spike results now say "not measured (deferred to backlog: no Windows machine yet)" (32 cells).
A new record, ADR 0066, gives a provisional result: on macOS GPUI is within every measured budget.
D37 gets one dated line saying GPUI is neither chosen nor rejected and job 182 waits for Windows.
Look at: docs/adr/0066-workbench-shell-toolkit.md:33-45 (the Decision wording: provisional, nothing chosen or rejected); docs/adr/0066-workbench-shell-toolkit.md:68-70 (licence reading: r-efi and self_cell are dual-licensed with a permissive option, so not "GPL-only"); docs/decisions.md:294 (the single D37 line).
Risk: none. Docs only. macOS numbers and cells match RESULTS.md; 11 insertions and 11 deletions there; no spike code touched; no Windows number invented. The 50 ms click budget was never measured and the ADR says so.
View: git -C factory/worktrees/181-shell-spike diff 692f1e3d1f26d02e3d25cec4bce51c3c6c45d32d..HEAD
## Findings
- none
## Lesson
none
NEXT: human
