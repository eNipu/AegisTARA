# ui review — ROUND 1

## Verdict
PASS

## Checked
- Determinism (P4) and stable ordering (D7).
- Caps and limits (R3, R6) and typed errors (Q3).
- 1. Every static string in the .slint files uses @tr and is bundled; 2. aegis-client has en/ja/pseudo Fluent catalogs; 3. Settings.language defaults to the OS and selects the bundled catalog; 4. a test fails when a locale lacks a key; 5. a pseudo-locale exists.
- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --workspace`, `scripts/deny.sh` all pass.

## Findings
None blocking.
