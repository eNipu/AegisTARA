# Build Report — 107-standalone-mode

## Summary
Implemented standalone local folder mode (`FolderStore`, in-process loopback server in `aegis-shell`, launcher dialog, CLI flags `--open` / `--new`, and `aegis-server import <folder>`), documented in ADR 0050 (`docs/adr/0050-standalone-mode.md`).

## Key Changes
- `crates/aegis-server/src/store/folder.rs`: `FolderStore` implementing `ProjectStore` over `project.json`, `ops.jsonl`, `ops.chain` (`AEGISOP1` sidecar), and `snapshots/NNNNNNNN.json` with exclusive `.lock`, atomic temp-file renames, directory fsyncs, and torn-tail truncation recovery.
- `crates/aegis-server/src/import.rs`: `Registry::import_folder` and `aegis-server import <folder>` verifying the SHA-256 hash chain and replaying every operation against the server's methodology before persisting.
- `crates/aegis-shell/src/standalone.rs`, `launcher.rs`, `cli.rs`: In-process loopback server bound to `127.0.0.1:0` with `Community` license and single local user, plus Slint launcher window and CLI argument parser.
- Cleaned up `HistoryReader::next` in `crates/aegis-server/src/store/folder.rs` to use `read_exact` directly without `.expect("fixed slice")`.

## Verification
- `cargo fmt --check`: PASS
- `cargo clippy --all-targets -- -D warnings`: PASS
- `cargo test`: PASS
- `scripts/deny.sh`: PASS
