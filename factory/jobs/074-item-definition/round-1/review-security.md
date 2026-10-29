# security review — ROUND 1

## Verdict
PASS

## Checked
- Determinism (P4) and stable ordering (D7).
- Caps and limits (R3, R6) and typed errors (Q3).
- Component hierarchy depth capped at `MAX_COMPONENT_DEPTH = 16` with cycle detection; trust boundary members capped at `MAX_TRUST_BOUNDARY_MEMBERS = 64`; decode-time `CapSeq` limits enforced in `store/snapshot.rs` and `store/op_log.rs`.
- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --workspace`, `scripts/deny.sh` all pass.

## Findings
None blocking.
