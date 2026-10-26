# Build Report — 051-screen-reader-path

## Summary
Provides a complete screen-reader sentence for the selected attack-tree node (`canvas_a11y_label`: node label, gate/step kind, inherent/residual feasibility, and child count) and full keyboard reach and activation on the canvas (gap G14).

## Key Changes
- Design documented in `docs/adr/0046-attack-tree-editing.md`.
- Implemented across `crates/aegis-client/src/view.rs`, `crates/aegis-shell/ui/app.slint`.

## Verification
- `cargo fmt --check`: PASS
- `cargo clippy --all-targets -- -D warnings`: PASS
- `cargo test`: PASS
- `scripts/deny.sh`: PASS
