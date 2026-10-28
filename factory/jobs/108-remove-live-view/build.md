# Build Report — 108-remove-live-view

## Summary
Removed the `Watch` live-observer streaming RPC and associated server/client complexity (~630 net lines deleted) and replaced it with `GetProjectStatus`, a 30-second read-only version check, and an explicit `Reload` banner/action (`docs/adr/0033-live-observers.md`).

## Key Changes
- `crates/aegis-proto/proto/aegis/v1/aegis.proto`: Replaced `Watch` and its streaming messages with unary `GetProjectStatus`.
- `crates/aegis-server/src/{actor,service,config,error}.rs`: Removed observer broadcast channels, recent-ops ring buffer, heartbeat timers, and `OutOfRange` error; added `Command::Status`.
- `crates/aegis-client/src/{session,workspace,state}.rs`: Removed watch task, silence watchdog, and `apply_remote`; added periodic `GetProjectStatus` check in read-only mode, `Command::Reload`, and automatic reload when taking the lock on a stale copy.
- `crates/aegis-shell/ui/app.slint` & `src/controller.rs`: Added newer-version banner and `Reload` button/menu action.

## Verification
- `cargo fmt --check`: PASS
- `cargo clippy --all-targets -- -D warnings`: PASS
- `cargo test`: PASS
- `scripts/deny.sh`: PASS
