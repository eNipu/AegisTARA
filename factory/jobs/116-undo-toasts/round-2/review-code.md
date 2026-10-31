# Code Review — 116-undo-toasts (Round 2)

VERDICT: PASS

## Blocking findings
None.

## Advisory findings
- `undo_label.rs` has two 60-line matches over `Op` (`parts`, `target_of`) with a
  wildcard arm; a new `Op` compiles and falls back to "change project". The
  `undo_every_op` test does not check labels. Acceptable: the label is cosmetic.
- `Workspace::take_cleared` counts steps already dropped plus the live stacks, so
  `resync` after `on_rejected` does not double-announce. This is covered only by
  the session path, not a unit test.

## Standards Check
- Undo/redo description is built in the client core; no element name reaches a log
  (`tracing` calls unchanged, labels only in `Notice` text and `ProjectView`).
- One shortcut table, `@tr` for new strings (ja and pseudo catalogs updated, the
  i18n test passes), existing notice widget reused.
- Tests: `undo_every_op` (all 64 kinds, fails on a new variant), `undo_labels`,
  `session` (toast with Undo, Undone toast with Redo, page opened), `toast`
  (window), `dialogs` (allow list). Not faked: the first run showed the model kit
  reaches only 37 kinds, which is why the second walk exists.
- Checks run and seen: fmt, clippy `-D warnings`, `cargo test --workspace --locked`
  (81 binaries, 0 failed), `scripts/deny.sh` ok.
- `Cargo.lock` changed by one line (`uuid` as a dev-dependency of `aegis-client`).
