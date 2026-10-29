# Build Report — 073-figures

## Summary
Implemented figure attachments (`ARC-01`) and report image rendering (`RPT-06`) across `aegis-proto`, `aegis-server`, `aegis-client`, and `aegis-shell`.

## Key Changes
- `crates/aegis-server/src/store/blob.rs`: PNG/JPEG header and dimension validation, strict static SVG allowlist sanitizer, SHA-256 hashing, and per-file (5 MiB) / per-project (64 files, 50 MiB) caps.
- `crates/aegis-server/src/store/{memory,folder,postgres}.rs` + `migrations/005_figures.sql` + `import.rs`: Figure storage, chunked `GetFigure` streaming, and standalone folder import.
- `crates/aegis-client/src/report/{doc,html,pdf,xlsx}.rs`: Emits `Block::Image` for project and element figures and renders real images in PDF via `krilla::image::Image` (`from_png`, `from_jpeg`, and static SVG `from_rgba8`).
- `crates/aegis-shell/ui/app.slint` & `controller.rs`: Figures management panel on Overview and per-element inspector attachments.

## Verification
- `cargo fmt --all -- --check`: PASS
- `cargo clippy --all-targets -- -D warnings`: PASS
- `cargo test --workspace`: PASS
- `./scripts/deny.sh`: PASS
