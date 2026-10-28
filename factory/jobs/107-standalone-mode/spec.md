# Spec — 107-standalone-mode

## Title
Standalone local mode with folder-backed ProjectStore, launcher, and server import (`D22`)

## Design Reference
- Reference: `docs/adr/0050-standalone-mode.md`, `docs/decisions.md` (D22)
- Affected crates/paths: `crates/aegis-server/src/store/folder.rs`, `crates/aegis-server/src/import.rs`, `crates/aegis-shell/src/standalone.rs`, `crates/aegis-shell/src/launcher.rs`, `crates/aegis-shell/src/cli.rs`, `crates/aegis-shell/src/main.rs`

## Problem & Goal
Enable an analyst to open or create a TARA project as a plain folder on disk without running PostgreSQL or configuring a separate server, while preserving one execution path (`aegis-shell` -> `aegis-client` -> `aegis-proto` -> `aegis-server`) via an in-process loopback server backed by `FolderStore`. Also provide `aegis-server import <folder>` to import a standalone folder project into a team server with full hash-chain verification.

## Acceptance Criteria
1. `FolderStore` implements `ProjectStore` over `<folder>/project.json`, `<folder>/ops.jsonl`, `<folder>/ops.chain`, and `<folder>/snapshots/NNNNNNNN.json` with exclusive `.lock`, fsync durability, and torn-tail recovery on startup.
2. `aegis-shell` opens a launcher window (`New local project`, `Open local project`, `Connect to server`) or accepts `--open <folder>` / `--new <folder> --name <title>` on the command line.
3. `Registry::import_folder` (`aegis-server import <folder>`) verifies the hash chain, replays every op through `Project::apply` against a hosted methodology, and imports the history and latest snapshot into the server store.
4. All unit and integration tests pass (`cargo test`), `cargo clippy --all-targets -- -D warnings` is clean, `cargo fmt --check` is clean, and `scripts/deny.sh` passes.
