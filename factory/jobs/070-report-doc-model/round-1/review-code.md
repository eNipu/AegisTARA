# code review — ROUND 1

## Verdict
PASS

## Checked
- Determinism (P4) and stable ordering (D7).
- Caps and limits (R3, R6) and typed errors (Q3).
- 1. HTML and PDF render the same ReportDoc blocks; 2. the HTML has a TOC and anchor ids on every heading; 3. the same-sections test still passes and now checks the HTML headings come from the doc.
- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --workspace`, `scripts/deny.sh` all pass.

## Findings
None blocking.
