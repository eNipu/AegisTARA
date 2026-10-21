VERDICT: PASS

Scope: `git diff master...factory/012-methodology-packs` (commit 928bbe5), checked against the spec, `rework.md` (owner confirmed open question 1: identity is key + version only, no signature or hash), `build.md`, Q2–Q4, R3/R4/R6, D5 and D7. This is round 1, so there are no earlier security reviews.

No blocking findings.

Checked and OK:
- `src/methodology/pack.rs:443-455`: `read_pack` wraps the reader in `CappedReader(MAX_PACK_FILE_BYTES)`, which pulls at most cap + 1 bytes (`take(cap+1)` in `src/store/wire.rs` `CappedReader::new`). `exceeded()` is checked before the decode result, so an over-cap input always gives `LimitExceeded(PackFileBytes)` and never a misleading I/O or Decode error (R3/R6). The serde_json string scratch buffer is bounded by the same file cap.
- `src/methodology/pack.rs:56-65, 121-192`: every leaf is bounded during decoding. Keys and `violates` entries use `Key` (64 B), version uses `VersionText` (256 B), labels use `LabelCap` (1,024 B), the prefix uses `PrefixCap` (16 B), and every list is a `BoundedVec` with the model's max. `BoundedVec` pre-allocates at most `min(size_hint, MAX)` and stops at MAX + 1 (R4: no unbounded growth). Structs use `deny_unknown_fields`, which rejects an unknown field before its value is parsed. `Object`/`MapOnly` closes serde's array-as-struct form. Numbers decode into exact `u8`/`u16` types, so an out-of-range value is a typed error (Q4).
- `src/store/wire.rs` `decode_pack`: `Decode` keeps only line and column, never the parser's message (which can quote input). `UnsupportedFormat` and `Limit` `Display` text name only the cap and value. The thread-local fault cell is reset before and taken after each decode, so no fault leaks between decodes.
- `src/methodology/mod.rs` `check_key` / `error_key_text`: the length check runs first (O(1) rejection), and an oversized key is cut at a char boundary to at most 61 B plus `...`. The boundary search and `get(..end)` use `unwrap_or` fallbacks, with no panic path. Every other `MethodologyError` variant that carries a key only does so after `add_entry` → `check_key` (or, for `RepeatedViolatedProperty`, after a successful lookup in the property map), so all error key text is at most 64 B. `LabelTooLong` and `InvalidVersion` carry no label or version text.
- Q2: no `unsafe` was added. The fuzz crate adds `#![forbid(unsafe_code)]`.
- Q3: no `unwrap`/`expect`/`panic!` in library code. The `expect` hits are only in `fuzz/fuzz_targets/pack.rs` (tooling, where a panic is the intended fuzz signal) and in `#[cfg(test)]` code.
- D7: no `HashMap`/`HashSet` was added. Vocabulary lookups stay `BTreeMap`.
- D5 / dependencies: no new dependency. `Cargo.toml` adds only `[[bench]] pack`, and `fuzz/Cargo.toml` adds only `[[bin]] pack`.
- Secrets and data: the new packs, fixtures and seeds hold vocabulary keys and labels only. There are no tokens, credentials or customer data. The bench `eprintln!` prints only byte counts.

Accepted risk (not a finding): packs are unsigned, and identity is key + version only. Two different pack files with the same identity could change what a project means. The owner confirmed this in `rework.md`, ADR 0011 records it, and it is deferred to the server pack-registry job. Any future code path that loads packs from untrusted locations (an install path or upload) should not ship until that job adds signing or content hashes.
