Commit: f7c2ffd Record the provisional macOS shell result and defer Windows (181d)
Files: docs/adr/0066-workbench-shell-toolkit.md (new), docs/decisions.md (one line under D37), spikes/gpui-shell/RESULTS.md (32 `TODO(owner)` cells replaced)
Done when:
1. `cargo fmt --all` - ran, no changes to tracked files beyond this slice.
2. `scripts/ci.sh` - all steps passed in 8 crate(s) (fmt, clippy, test).
3. `test -f docs/adr/0066-workbench-shell-toolkit.md` - ok.
4. `test "$(grep -c '^## ' docs/adr/0066-workbench-shell-toolkit.md)" -eq 6` - ok.
5. `grep -qi 'provisional' docs/adr/0066-workbench-shell-toolkit.md` - ok.
6. `grep -q 'Spike result' docs/decisions.md` - ok.
7. `grep -q '0066-workbench-shell-toolkit' docs/decisions.md` - ok.
8. `! grep -q 'TODO(owner)' spikes/gpui-shell/RESULTS.md` - ok.
9. `grep -o 'not measured (deferred to backlog: no Windows machine yet)' spikes/gpui-shell/RESULTS.md | wc -l` - 32.
10. `git diff --stat 692f1e3 -- spikes/gpui-shell/RESULTS.md` - 11 insertions, 11 deletions.
11. `git diff --stat 692f1e3 -- docs/decisions.md` - 1 insertion.
12. `git diff --quiet 692f1e3 HEAD -- spikes/gpui-shell/src spikes/gpui-shell/Cargo.toml spikes/gpui-shell/measure.sh spikes/gpui-shell/measure.ps1` - exit 0 (no spike code changed).
Notes:
- Provisional macOS result: within every measured budget. FIRST_FRAME_MS 520.7 (bar 1000), CANVAS_MS p95 0.2 (bar 16), RSS_MB 162.3 (bar 300), FRAME_INTERVAL_MS p95 8.4 (reference, bar 50). No criterion missed on macOS. Binary 15.9 MB recorded. Licenses: no GPL-only entry; `r-efi` and `self_cell` have permissive alternatives.
- Checklist on macOS: `starts` and `draws the tree` are yes. The other seven rows stay `not checked` (no one at the keyboard).
- Windows laptop and Windows over RDP are deferred to a backlog job (no Windows machine yet). Nothing was measured or filled in for Windows.
- The 50 ms interaction budget (click, selection) was not measured on any machine; it is listed under "Left for later" in ADR 0066.
- The ADR says nothing is chosen or rejected. D37 gets the one dated provisional line.
- Commit message: the subject ends with `(181d)` as the prompt asks; a Co-Authored-By trailer follows it, per the attribution rule.
NEXT: reviewer
