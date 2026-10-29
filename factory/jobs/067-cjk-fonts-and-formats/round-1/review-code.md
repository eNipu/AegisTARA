# code review — ROUND 1

## Verdict
PASS

## Checked
- Determinism (P4) and stable ordering (D7).
- Caps and limits (R3, R6) and typed errors (Q3).
- 1. Noto Sans JP (OFL-1.1) is embedded as the PDF fallback for characters Inter lacks; 2. mixed Latin+Japanese runs are split per font; 3. format_ms_lang writes Japanese dates; 4. a test proves Japanese glyphs come from the CJK face, not ?.
- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --workspace`, `scripts/deny.sh` all pass.

## Findings
None blocking.
