GATE: PASS
Slice: 181d  Base: 692f1e3d1f26d02e3d25cec4bce51c3c6c45d32d


Diff: 0 counted lines, 3 files
ok   scripts/ci.sh
ok   test -f docs/adr/0066-workbench-shell-toolkit.md
ok   test "$(grep -c '^## ' docs/adr/0066-workbench-shell-toolkit.md)" -eq 6
ok   grep -qi 'provisional' docs/adr/0066-workbench-shell-toolkit.md
ok   grep -q 'Spike result' docs/decisions.md
ok   grep -q '0066-workbench-shell-toolkit' docs/decisions.md
ok   ! grep -q 'TODO(owner)' spikes/gpui-shell/RESULTS.md
ok   test "$(grep -o 'not measured (deferred to backlog: no Windows machine yet)' spikes/gpui-shell/RESULTS.md | wc -l)" -eq 32
ok   git diff --quiet 692f1e3 HEAD -- spikes/gpui-shell/src spikes/gpui-shell/Cargo.toml spikes/gpui-shell/measure.sh spikes/gpui-shell/measure.ps1
