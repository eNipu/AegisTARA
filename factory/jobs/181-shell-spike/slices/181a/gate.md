GATE: PASS
Slice: 181a  Base: 60bc411ac15c17a53b734c6acc551dfa4abfdeff


Diff: 398 counted lines, 9 files
ok   scripts/ci.sh spikes/gpui-shell
ok   cargo build --locked --release --manifest-path spikes/gpui-shell/Cargo.toml
ok   grep -q '## Licenses' spikes/gpui-shell/README.md
ok   grep -q 'gpui-pre-platform' spikes/gpui-shell/Cargo.toml
