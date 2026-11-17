Commit: 346c4b58a548766793284af2e25f9e7d7d8e7d9b Add typed party element with ops, guards and diff (145a)

Files:
- crates/aegis-model/src/party.rs (new: PartyKind, Party, MAX_PARTIES)
- crates/aegis-model/src/id.rs (PartyIdx/PartyId, VisualPrefix::Party -> "P")
- crates/aegis-model/src/lib.rs (module + public exports)
- crates/aegis-model/src/op.rs (four ops, inverse table, provenance/removal lists)
- crates/aegis-model/src/error.rs (five error variants with doc tests)
- crates/aegis-model/src/project.rs (arena, accessors, validate/apply/commit, PartialEq, diff wiring, eval-cache no-op arms)
- crates/aegis-model/src/diff.rs (ProjectDiff::parties(), is_empty())
- crates/aegis-model/src/store/wire.rs (OpWire variants)
- crates/aegis-model/src/store/op_log.rs (OpView, within_caps, both conversions)
- crates/aegis-model/tests/parties.rs (new: ids, inverses by save() bytes, guards, cap, 101-party op-log round trip, diff)
- crates/aegis-client/tests/undo_every_op.rs (party walk for all four ops)

Done when:
- `cargo test --locked -p aegis-model --lib id` — PASS: 34 tests, 0 failed; includes P00/P100 assertions.
- `cargo test --locked -p aegis-model --test parties` — PASS: 6 tests, 0 failed; inverse-by-`save()`-bytes, all five guards (plus blank actor), cap 256/257, 101-party round trip and `ProjectDiff::parties()`.
- `cargo test --locked -p aegis-client --test undo_every_op` — PASS: 2 tests, 0 failed; all four party ops exercised and undone/redone exactly.

Before you commit (all run from the worktree root, all passed):
- `cargo fmt --all` — PASS.
- `scripts/ci.sh crates/aegis-model crates/aegis-client` — PASS: fmt, clippy (`-D warnings`) and tests for both crates.
- the three checks above — PASS (re-run after the final format).

Notes:
- `RemoveParty` has no in-use guard in this slice as specified (145b adds it); its inverse already restores name, kind, provenance and visual ID exactly.
- Parties are not persisted: no snapshot field or envelope version change. Because neither `save()` bytes nor the pre-existing `Project ==` covered parties, I added parties to `Project::PartialEq` and the inverse test also asserts the party list state directly (L06).
- `PartyKind` derives `serde::Deserialize` (`rename_all = "snake_case"`) so the wire reader resolves its closed set directly; unknown kinds fail as decode errors. The inherent `PartyKind::from_str` returns `Option` as the prompt requires; clippy's `should_implement_trait` is allowed locally with a reason comment rather than changing the requested API.
- Source diff is 598 counted lines against the slice's 600-line cap (tests excluded by the verifier).
- No new dependency, no risk-number change, no lint/persistence/UI work.

NEXT: reviewer
