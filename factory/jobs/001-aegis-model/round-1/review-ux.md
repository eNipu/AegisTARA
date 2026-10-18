VERDICT: PASS

No user-facing change: library crate only, with no UI, CLI or API.

Error messages in `crates/aegis-model/src/error.rs` are the only text a user could eventually see. They name the problem and are fine for a library. Two notes for the caller layers, not blockers:
- error.rs:21 `NotVersion7` and `InvalidUuid` (error.rs:24) — the messages don't say what to do next, e.g. "expected a UUIDv7". The client should wrap them with that guidance.
- error.rs:33 `ChildrenNotEmpty` and `CapacityExceeded` — these are internal-facing. The UI should not show them raw.
