VERDICT: PASS

Scope: no GUI or CLI. The user-facing surfaces are the library API (`Project::save/load`, `OpLogWriter`, `OpLogReader`), the `StoreError` and `Limit` messages, and `crates/aegis-model/fuzz/run.sh` with its README section. Nothing touches the UI thread, keyboard access or accessible labels. The 50 ms budget holds: load went 4.31 -> 5.08 ms (build.md), still far inside the budget.

Round 1 UX points, checked against branch `factory/010-save-load` (commit 82da671):
1. `UnsupportedFormat` split: done as `UnsupportedFormat { version: Option<u64> }`. `Some(n)` says "file format version n is not supported: this build reads version 1, a newer application may read it"; `None` says "not a supported AegisTARA file: wrong format name or format version". The user's next step (update the app vs pick another file) is now distinguishable. The number is not decoded text, so the no-echo rule holds. Doctest in `error.rs` covers both.
2. Error location docs: `StoreError` docs now point to `OpLogReader::line()` and state that snapshot errors other than `Decode` carry no position.
3. `UnknownKey` docs give likely causes and the hint an app should show.
4. `fuzz/run.sh`: SECONDS and each target are validated before the build, with "unknown target 'X'; expected snapshot or op_log" and a usage line, exit 2. The README fuzz section is unchanged and still correct.

Memory fix, user-visible effect: a hostile file now fails fast with the same typed error (`UnknownKey`, key capped at 64 bytes and escaped) instead of after a full decode. Errors still never quote entity names.

Non-blocking:
1. crates/aegis-model/src/store/wire.rs (`FormatVersion::visit_u64`) — `"version": 0` gives `Some(0)`, and the message then suggests "a newer application may read it", which is wrong for 0 (and for 2..n it is only likely). Fix when the app layer lands: say "may be from a newer application" only for `v > 1`, or reword to "file format version 0 is not supported; this build reads version 1".
2. crates/aegis-model/src/store/op_log.rs — decoding now comes before the `seq` check, so a record with a bad key and a wrong `seq` reports `UnknownKey`. This is documented in build.md and is the more actionable error; no change needed. Keep `OpLogReader::line()` documented as valid after this error (it is).
