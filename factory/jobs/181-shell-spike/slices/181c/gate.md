GATE: PASS
Slice: 181c  Base: 2484126424c670fc87cbaaa63455c47e652be1d3


Diff: 291 counted lines, 10 files
ok   scripts/ci.sh spikes/gpui-shell
ok   cargo build --locked --release --manifest-path spikes/gpui-shell/Cargo.toml
ok   sh -n spikes/gpui-shell/measure.sh
ok   test -f spikes/gpui-shell/measure.ps1
ok   test -f spikes/gpui-shell/RESULTS.md
