# security review — ROUND 1

## Verdict
PASS

## Checked
- Determinism (P4) and stable ordering (D7).
- Caps and limits (R3, R6) and typed errors (Q3).
- Static SVG sanitizer rejects scripts, event handlers (`on*`), `foreignObject`, `<!DOCTYPE`, `<!ENTITY`, `<image>`, and external URLs; per-file (5 MiB), dimension (16,384 px), and per-project (64 figures / 50 MiB) caps enforced before storage.
- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --workspace`, `scripts/deny.sh` all pass.

## Findings
None blocking.
