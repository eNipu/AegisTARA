# Spec — 108-remove-live-view

## Title
Remove live observer stream and replace with lightweight status check and reload (`D23`, `P7`)

## Design Reference
- Reference: `docs/adr/0033-live-observers.md`, `docs/decisions.md` (D23)
- Affected crates/paths: `crates/aegis-proto/proto/aegis/v1/aegis.proto`, `crates/aegis-server/src/{actor,service,config,error}.rs`, `crates/aegis-client/src/{session,connection,workspace,state,view}.rs`, `crates/aegis-shell/ui/app.slint`, `crates/aegis-shell/src/controller.rs`

## Problem & Goal
Per Decision D23 and rule P7, TARA reviews are asynchronous and nobody needs to watch edits live as they happen. Remove the `Watch` server-streaming RPC, broadcast channels, recent-ops ring buffer, heartbeat tasks, and observer op-application machinery, and replace them with a simple `GetProjectStatus` RPC polled every 30 seconds in read-only mode to offer a clean `Reload` banner when a newer version is committed.

## Acceptance Criteria
1. `Watch`, `WatchRequest`, `WatchEvent`, `AcceptedOp`, `LockEvent`, `RisksEvent`, and `Heartbeat` are removed from `aegis.proto`; `GetProjectStatus` returns `current_seq` and `lock_holder`.
2. Server actor answers `Status` from memory without broadcast channels, ring buffers, or background risk-push tasks.
3. Read-only client sessions poll `GetProjectStatus` every `version_check` (30 s), set `newer_version = true` when `status.current_seq > confirmed_seq`, and reload before acquiring the edit lock if the local copy is stale.
4. Shell displays `"A newer version is available"` with a `Reload` button and menu item.
5. All unit and integration tests pass (`cargo test`), `cargo clippy --all-targets -- -D warnings` is clean, `cargo fmt --check` is clean, and `scripts/deny.sh` passes.
