# security review — ROUND 1

## Verdict
PASS

## Checked
- Determinism (P4) and stable ordering (D7).
- Caps and limits (R3, R6) and typed errors (Q3).
- 1. sections on/off; 2. filters narrow threats and the risk register by risk level and asset; 3. branding (document ID, classification, approvers, revisions, logo) on the cover and every page; 4. profiles persist in Settings; 5. RecordExportRequest.detail records the profile.
- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --workspace`, `scripts/deny.sh` all pass.

## Findings
None blocking.
