VERDICT: PASS

## Check results

Run from `factory/worktrees/012-methodology-packs/` (branch `factory/012-methodology-packs`, commit 928bbe5), not taken from build.md.

| Command | Result |
|---|---|
| `cargo fmt --check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass |
| `cargo test` | pass (513 passed, 0 failed, 1 ignored; doctests included) |
| `cargo test --benches` | pass |
| `scripts/ci.sh crates/aegis-model` | pass (fmt, clippy, test) |
| `scripts/deny.sh licenses bans sources` | pass (aegis-model and aegis-model/fuzz) |
| `crates/aegis-model/fuzz/run.sh 30 pack` | pass (exit 0, 2.0M runs) |
| `crates/aegis-model/fuzz/run.sh 5 snapshot op_log` | pass (exit 0) |
| AC2 grep on `iso21434.rs` | prints nothing |
| AC3 neutrality grep | prints only `src/methodology/mod.rs:12:mod iso21434;` |
| AC4 `git diff --exit-code master -- <golden fixtures, snapshot and op_log seeds>` | exit 0 |
| AC6 `grep -rn 'med-device' src` | prints nothing |
| AC15 unwrap/expect/panic grep on `pack.rs`, `iso21434.rs` | prints nothing |
| AC18 `git diff master -- Cargo.toml` | only the new `[[bench]] pack` |
| AC17 ADR exists and names `MAX_PACK_FILE_BYTES` | pass |

## Acceptance criteria

All met and tested. Tests exist and pass for each of these: `builtin_pack_is_canonical`, the `second_pack_*` tests (including every-op), the `pack_rejects_*` tests, `pack_limits` (at-cap and cap+1 for every row, key cap in every position, maximal pack of 1,481,198 bytes), `pack_file_cap_stops_reading`, `round_trip` and `hostile` in `pack_properties`, and `methodology_error_key_text_*` (1 MiB keys in all six positions plus the `violates` entry, 4-byte char cuts, `Arc::ptr_eq` for short keys).

I also checked the ADR's largest-pack arithmetic: 689 x 256 x 6 = 1,058,304, which matches. The parts add up to the measured size. The owner's answer to open question 1 is recorded in the ADR.

## Code review

- `pack.rs` is sound.
  - Bounded leaves are reused from the store, and no strings are copied: `Arc<str>` values are moved.
  - `Object<T>` and `MapOnly` reject the array form of a struct. This is stricter than the spec asked for and is justified by the spec's "wrong shape gives Decode". It is small and tested.
  - `write_pack` serializes from borrowed views with no copy and skips `digits` when `None`.
  - The capped reader reports `PackFileBytes` correctly, and the `exceeded()` check comes after the decode, so an I/O error from the cap is never mistaken for a plain I/O error.
- `check_key` checks the length first (O(1) reject). `error_key_text` shares short keys and cuts long ones at a char boundary to at most 61 bytes plus `...`. A cut text can never look like a valid key.
- `decode_pack` takes the fault from the same channel as the store and keeps only the position for `Decode`, so no input text leaks into errors.
- `CappedReader` was moved into `wire.rs` and parameterized. The snapshot behavior and test are unchanged apart from the accessor.
- Standards:
  - Cost (A8) is documented on `read_pack`, `write_pack`, `Methodology::read_pack`, `iso21434_r155` and `error_key_text`.
  - Output is deterministic (D7, P4). There are no O(n²) paths.
  - No needless clones (R1): the only clone in the new library code is the intended `Arc::clone`.
  - There are no new panics in library code. `expect` appears only in the fuzz target and in tests.
- No code beyond the spec. The extra pieces are the `Object` wrapper, `bytes_read` (`cfg(test)` only) and the `max_def` and `pack_bytes` test helpers. All are justified.

## Findings (non-blocking)

1. `crates/aegis-model/tests/pack.rs` — Some test names start with `pack_rejects_` but assert that something is accepted: `pack_rejects_nothing_when_fields_are_reordered`, `pack_rejects_nothing_in_the_tiny_pack`, `pack_rejects_neither_missing_nor_null_digits`. This is only a naming wart, and it makes the `cargo test --test pack pack_rejects_` filter also run positive tests. Fix: rename them (for example `pack_accepts_reordered_fields`) if the filter should only select rejections.
2. `crates/aegis-model/src/store/wire.rs` / `FORMAT_VERSION` — One `FormatVersion` is now shared by three formats. build.md and the ADR already note it must become per-format when any format moves to v2. Nothing to do now.
3. Snapshot and op-log wire structs still accept the array form of a struct. This is out of scope and build.md flags it. A later job can apply `Object<T>` there.
