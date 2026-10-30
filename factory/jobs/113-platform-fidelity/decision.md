# Decision — 113-platform-fidelity

## Decision
ESCALATE

The renderer, per-OS style, shortcut-table and font foundations are sound and
all checks pass, but the job cannot be approved yet for two reasons:

1. **The renderer spike has not run (UI-07.2, acceptance criterion 1).** ADR
   0063 is still `proposed`; the Retina Mac + Windows laptop measurements are
   `TBD`. Merge is gated on the owner's spike evidence.
2. **Appearance modes are not implemented (UI-07.5, acceptance criterion 6),
   and the monospace/font proof is incomplete (UI-07.7).**

## What to do next
- Owner: run the spike on both machines, fill the ADR 0063 evidence table and
  change its status to `accepted`, then answer `/factory approve
  113-platform-fidelity`.
- Or `/factory rework 113-platform-fidelity <note>` to continue the build for
  appearance modes, the offline-build proof and the monospace binding.

## Checks
- `cargo fmt --all -- --check`, `cargo clippy -p aegis-shell --all-targets
  -- -D warnings`, `cargo test --workspace --locked`, `scripts/deny.sh`: all
  pass.
