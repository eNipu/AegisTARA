# security review — ROUND 1

## Verdict
PASS

## Checked
- Determinism (P4) and stable ordering (D7).
- Caps and limits (R3, R6) and typed errors (Q3).
- Archive size (16 MiB), ZIP entry count (128), decompressed entry size (16 MiB via `.take()`), shared strings (32,768), rows (4,096), and columns (64) are strictly capped against zip-bomb and XML exhaustion attacks; every generated `Op` is validated on a cloned `Project` before preview completion.
- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --workspace`, `scripts/deny.sh` all pass.

## Findings
None blocking.
