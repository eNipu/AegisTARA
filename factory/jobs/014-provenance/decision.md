APPROVE
All three design questions in the spec (single current `Provenance` on each element with full history in the op log, 8-byte `Option<Arc<ProvenanceInner>>` representation carried on all 16 creating/mutating `Op` variants and restored on undo, and keeping v1 store wire formats unchanged while rejecting non-default provenance with `StoreError::UnencodableOp` until format v2) were resolved using the owner's guiding principle in `rework.md` and recorded in ADR 0013.
The implementation matches `spec.md` across AC1–AC14. All four reviewers (`review-security`, `review-ux`, `review-ui`, `review-code`) passed with no blocking findings.
All checks (`scripts/ci.sh crates/aegis-model`, `scripts/deny.sh`, `cargo test --doc`, `cargo test --benches`, and no-panic grep) pass cleanly.
