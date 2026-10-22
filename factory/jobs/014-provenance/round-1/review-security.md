VERDICT: PASS

Scope: `git diff master...factory/014-provenance` (commit feb1aa1), checked against `spec.md`, `rework.md`, `build.md`, Q2–Q4, R3/R4/R6, D5, D6, and D7.

No blocking findings.

Checked and OK:
- `src/provenance.rs` & `src/project.rs`: `check_provenance` bounds non-default actor strings to `1..=MAX_ACTOR_CHARS` (256 Unicode scalar values, non-blank after `.trim()`) before any state mutation occurs (R3, R4, Q4).
- `src/error.rs`: `ModelError::EmptyActor` and `ModelError::ActorTooLong { max }` never store or display the rejected actor string, preventing untrusted input reflection in logs or error displays (R3). Verified by `errors_never_contain_names` in `tests/project.rs`.
- `src/provenance.rs`: `Provenance(Option<Arc<ProvenanceInner>>)` uses the `Arc` niche so `size_of::<Provenance>() == 8` and `Provenance::default()` allocates 0 heap bytes. `benches/store_memory.rs` confirms `damages_only_max` stays at 2.78x kept heap (under the 3.0x `SERVER_BUDGET`, R6).
- `src/store/snapshot.rs` & `src/store/op_log.rs`: `Project::save` and `OpLogWriter::append` reject non-default provenance with `StoreError::UnencodableOp` before writing any bytes, so provenance is never silently stripped on disk.
- Q2: `#![forbid(unsafe_code)]` remains intact; no `unsafe` added.
- Q3: No `unwrap()`, `expect()`, `panic!`, `todo!`, or `unimplemented!` in library code outside `#[cfg(test)]`.
