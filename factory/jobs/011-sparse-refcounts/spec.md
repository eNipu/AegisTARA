# 011-sparse-refcounts: Sparse per-asset reference counts, so a loaded project stays within the 3x server-memory budget for every methodology size

Follows job 010's escalation (`factory/jobs/010-save-load/decision.md` and the owner's answer at the end of `factory/jobs/010-save-load/rework.md`). The owner accepted the 3x breach only until "the next factory job" makes the per-asset reference-count rows sparse. This is that job. Background numbers: `docs/adr/0009-save-load-format.md`, section "Numbers".

## Open questions

One big-picture question. It has a default. If the owner says nothing, the builder implements the default.

1. **Does the "<= 3x" budget cover the load peak, or only the open project?** Engineering standards section 2 says "server memory per open project <= 3x the project's serialized size". **Default: it covers the open project, which is the "kept" heap after `Project::load` returns.** The transient peak during `load` is also gated in CI, at the measured value plus headroom, but it is not held to 3x. Under this default, every valid shape keeps at most about 2.65x after this job. One case still peaks above 3x while loading: damage scenarios only, over a 32-category methodology (3.53x peak, 2.65x kept). That comes from the dense impact slots in `DamageScenario`, not from reference counts. The other option is to hold the peak to 3x as well. That needs a second change in this job: make `DamageScenario`'s impact slots sparse too. Its internals change, but its public accessors do not. It roughly doubles the job.

Small choices settled here, which the owner does not need to decide:
- **Layout: one sorted, exact-size boxed slice per asset slot.** Each slot holds only the properties that at least one live threat targets on that asset. See Design notes for why this beats the other options.
- **The per-asset threat count (`RefCounts.assets`) and the per-damage-scenario count (`RefCounts.damages`) stay dense.** They cost 4 bytes per slot and do not grow with the methodology.
- **The ADR is a new ADR 0010, and ADRs 0008 and 0009 get pointers and corrected numbers.** L1 asks for a decision record with options and numbers. ADR 0008 describes the old layout, so its section gets a short "changed by ADR 0010" note.
- **The flat-index overflow check in `check_add_asset` is removed.** Nothing is flat-indexed any more. On 64-bit targets it could never fire (`(u32::MAX + 1) x 64` fits a `usize`), so behaviour does not change there. The slot-limit check stays.
- **No new dependencies.**

## Goal

A project's reference counts cost memory only for what threats actually target. An asset that no threat targets carries no per-property counters. An asset that threats target carries one counter for each property they target. Previously each asset carried one counter for every property in the methodology. So loading a file of minimal assets over a 64-property methodology keeps at most 2.2x the file size, down from 5.53x. Every valid shape in the `store_memory` bench then stays within the 3x server-memory budget for every methodology size, and CI checks this. The public API, every error, every count reported in an error (`PropertyInUse.threats`, `AssetInUse.threats`), the save/load format and the op-log format do not change.

## Scope

In:
- `RefCounts` in `crates/aegis-model/src/project.rs`: a sparse per-asset property-count layout. Also every commit that updates it, `check_set_asset_properties`, `check_add_asset` (drop the flat check), and the test-only `check_invariants` recount.
- New unit tests in `src/project.rs`.
- `crates/aegis-model/benches/store_memory.rs`: add a "kept" gate per case, held to the 3x budget for every valid case; add three new shapes that stress the new layout; tighten the peak gates; update the header table.
- Docs: new `docs/adr/0010-sparse-reference-counts.md`; corrected numbers and budget statement in `docs/adr/0009-save-load-format.md`; a pointer in `docs/adr/0008-methodology-neutral-domain-model.md`; the memory paragraph of the `Project::load` doc comment in `src/store/snapshot.rs`.

Out:
- Any public API change, any new public item, and any change to errors or to the file formats.
- Changes under `crates/aegis-model/tests/` or `crates/aegis-model/fuzz/`. The existing tests must pass unchanged, and that is the proof that behaviour is preserved.
- Sparse damage-scenario impact slots (see open question 1), and arena or ID-map layout.
- Persistent (`imbl`) collections for snapshots (D9). That belongs to a later job.

## Design notes

### Files

Only these files may change:

| File | Change |
|---|---|
| `crates/aegis-model/src/project.rs` | new `RefCounts` layout, its updates, checks, the invariant recount, unit tests |
| `crates/aegis-model/src/asset.rs` | only if a `pub(crate)` helper on `PropertySet` is needed; no `pub` changes |
| `crates/aegis-model/src/store/snapshot.rs` | the memory numbers in the `Project::load` doc comment, nothing else |
| `crates/aegis-model/benches/store_memory.rs` | kept gates, new cases, tightened peak gates, header |
| `docs/adr/0010-sparse-reference-counts.md` | new |
| `docs/adr/0009-save-load-format.md`, `docs/adr/0008-methodology-neutral-domain-model.md` | numbers and pointers |

### Data structure

```rust
/// One counted property on one asset: how many live threats on that asset
/// violate it. Never stored with `threats == 0`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PropertyCount { property: Property, threats: u32 }   // 8 bytes

struct RefCounts {
    /// Per asset slot: the properties some live threat on that asset
    /// targets, sorted by property index, no zero counts. Empty (no heap)
    /// when no threat targets the asset, including tombstoned slots.
    asset_properties: Vec<Box<[PropertyCount]>>,             // 16 bytes per slot
    /// Per asset slot: live threats on it (unchanged).
    assets: Vec<u32>,
    /// Per damage scenario slot: live threats linking it (unchanged).
    damages: Vec<u32>,
}
```

Rules:
- **Canonical form.** Each row is sorted by `Property`, has no duplicates, and has no zero counts. An empty row is `Box::default()`, which does not allocate. So the derived `PartialEq` against a fresh recount in `check_invariants` stays an exact check, and a tombstoned asset slot costs 16 bytes and no heap. (An asset can only be removed when no threat targets it.)
- **Lookup** (`asset_property_refs`, used by `check_set_asset_properties`): binary search in the asset's row, O(log p). A missing entry means 0.
- **Update** (count a threat in or out over `violates`): if every property is already in the row (count in) or no count reaches 0 (count out), adjust the counts in place with no allocation. Otherwise, merge the row and `violates` into one new exact-size boxed slice. That is one allocation, or none if the row becomes empty. It costs O(r + p), with r the row length and p = |violates|, and both are at most the asset's property count (at most 64).
- **`SetThreatCategory` counts the new category in before it counts the old one out.** Properties that both categories violate then never drop to 0, so they cause no allocation churn.
- **`commit_add_asset` pushes an empty row**, not a zeroed row of `property_count` counters.
- **No panics** (Q3). Use `get` and `get_mut`, not indexing, as the current `bump` does. A missing slot is ignored, as now.

Why this layout. ADR 0010 records this comparison with the measured numbers:

| Option | Per asset slot | Per targeted (asset, property) | Lookup | Notes |
|---|---|---|---|---|
| dense row (today) | 4 x methodology properties (256 B at 64), plus `Vec` doubling slack | 0 | O(1) | the 5.53x breach |
| **sorted boxed slice per slot (chosen)** | 16 B | 8 B | O(log p), p <= 64 | one allocation per targeted asset; no new map type or dependency |
| `Option<Box<{PropertySet, Box<[u32]>}>>` per slot | 8 B | 4 B (+24 B header per targeted asset) | O(1) by rank | two allocations per targeted asset, rank arithmetic on the bitset |
| `BTreeMap<(AssetIdx, Property), u32>` | 0 | about 13-20 B | O(log E) | worse than the dense row when one category violates many properties: 64 pairs cost about 1 KiB per asset |
| map `AssetIdx -> row` | 0 | 8 B (+ about 30 B entry) | O(log a), or a new hasher dependency (D7) | saves 16 B per untargeted asset; not needed for the budget |

### Complexity (A8)

State these in the `RefCounts` doc comment. Space: O(asset slots + damage slots + targeted (asset, property) pairs). Before, it was O(asset slots x methodology properties). Op costs stay in the classes ADR 0008 lists. Every count update is O(p) with p <= 64. `Clone` and `==` stay O(n + L + m), but `Clone` now makes one allocation per targeted asset instead of one large copy. Say so in the ADR.

### Expected numbers (to check, not to copy)

Assets only over the maximum-vocabulary methodology: the dense row plus its doubling slack is about 4.0x of the file (16 MiB of `u32` for 54,472 assets in a 4 MiB file). Without it, the rest of the project keeps about 1.5x, plus 16 B per slot (about 0.25x with slack). So expect about 1.8x kept and about 2.4x peak. The gate is the brief's 2.2x kept.

### Bench changes (`benches/store_memory.rs`)

- Add `max_kept: f64` to `Case`. After each load, fail if `kept > max_kept`. Add `const SERVER_BUDGET: f64 = 3.0;`. For every `loads: true` case, also fail if `case.max_kept > SERVER_BUDGET`. This check is on the configuration itself, so a later edit cannot quietly raise a kept gate past the budget. For `loads: false` cases, set `max_kept` to 0.1.
- Set each case's `max_kept` and `max_factor` (peak) to its new measured value plus about 35%. Cap `max_kept` at `SERVER_BUDGET`. Never raise a gate above its job 010 value. `assets_only_max` must end with `max_kept <= 2.2` and `max_factor <= 3.3` (was 8.3).
- Add three valid shapes, built like `single_kind`, each about `HOSTILE_BYTES`:
  - `targeted_narrow_builtin` and `targeted_narrow_max`: minimal assets (the properties of the first threat category), each targeted by exactly one minimal threat of the first category. This is the densest file in which every asset has a non-empty row.
  - `targeted_wide_max`: assets that carry all 64 properties, each targeted by one threat whose category violates all 64. This is the largest row per asset. It needs a maximum-vocabulary methodology in which one threat category violates every property. Add it as a second helper next to `short_keys`, or change `short`'s last category `t63`, which no existing case uses. The builder chooses and documents the choice in the bench header.
- Update the header table: measured, peak gate, kept gate, and what dominates. Remove the "reference-count row of 4 bytes per methodology property" wording.

## Acceptance criteria

All commands run from `crates/aegis-model/` unless marked "repo root". `master` is the job's base branch.

- [ ] AC1 Public API, behaviour and formats are unchanged.
  - (repo root) `git diff --name-only master...HEAD -- crates/aegis-model` lists only files from the Design notes table. It lists nothing under `tests/` or `fuzz/`, and neither `Cargo.toml` nor `Cargo.lock`.
  - (repo root) `git diff master...HEAD -- crates/aegis-model/src | grep -E '^[-+][[:space:]]*pub[[:space:]]'` prints nothing (no `pub` item added, removed or changed).
  - `cargo test --locked` passes, so every existing test passes unchanged, including the golden v1 fixtures, `referential_integrity_counts_threats_per_property`, and `rejects_every_new_variant`.
- [ ] AC2 The counts are sparse. A unit test `refcounts_are_sparse` in `src/project.rs` covers both methodologies (`testkit::builtin()`, `testkit::second()`) and checks each of these:
  - (a) after 1,000 assets with every property and no threats, every row in `refs.asset_properties` is empty;
  - (b) after one threat on an asset, its row is exactly `violates(category)`, sorted, each count 1, and every other row is empty;
  - (c) a second threat of the same category gives counts of 2;
  - (d) `SetThreatCategory` to a category with different `violates` moves the entries, and shared properties keep their counts;
  - (e) removing every threat empties the row again;
  - (f) removing the asset leaves an empty row on the tombstoned slot.

  Test: `cargo test --lib refcounts_are_sparse`.
- [ ] AC3 The recount invariant covers the new layout. `check_invariants` rebuilds the sparse rows from the threats and compares them exactly. A new test `check_invariants_detects_bad_property_counts` corrupts a project three ways: a wrong count, an entry with `threats: 0`, and two entries out of order. It asserts that `check_invariants` fails each time. The existing proptests `project_invariants_first_methodology` and `project_invariants_second_methodology` (256 cases each) still pass. Test: `cargo test --lib check_invariants` and `cargo test --lib project_invariants`.
- [ ] AC4 The memory budget is gated in CI.
  - `benches/store_memory.rs` has `SERVER_BUDGET = 3.0` and a `max_kept` on every case. It fails if any case keeps more than its `max_kept`, or if a `loads: true` case has `max_kept > SERVER_BUDGET`.
  - `assets_only_max` has `max_kept <= 2.2` and `max_factor <= 3.3`.
  - The three new cases exist and pass: `targeted_narrow_builtin`, `targeted_narrow_max`, `targeted_wide_max`.
  - No gate is higher than its job 010 value.

  Check: `cargo bench --bench store_memory` and `cargo test --benches` pass, and `grep -nE 'max_factor: 8\.3|\("assets_only_max", &short, "assets", 8\.3' benches/store_memory.rs` prints nothing. To confirm the kept gate bites, the builder temporarily sets `assets_only_max`'s `max_kept` to 1.0, sees the bench fail, reverts the change, and records this in build.md.
- [ ] AC5 Time does not regress. Run `cargo bench --bench store` on `master` and on the job branch, on the same machine. The `load` and `replay_log` medians are each within +10% of the `master` run. Both runs are recorded in build.md and ADR 0010.
- [ ] AC6 Docs.
  - (a) `docs/adr/0010-sparse-reference-counts.md` exists. It has: context (the job 010 escalation); the options table above with measured or computed bytes; the decision; a before/after table of peak and kept for every `store_memory` case, plus the AC5 times; and the complexity statement. Check (repo root): `test -f docs/adr/0010-sparse-reference-counts.md && grep -q 'assets_only_max\|assets only' docs/adr/0010-sparse-reference-counts.md`.
  - (b) ADR 0009 shows the new numbers. Its budget statement says that the kept heap is within 3x for every methodology size measured. Its "Follow-up for the in-memory model" paragraph is replaced by a pointer to ADR 0010. Check (repo root): `grep -q '0010' docs/adr/0009-save-load-format.md && ! grep -q 'does \*\*not\*\* hold' docs/adr/0009-save-load-format.md`.
  - (c) ADR 0008's "Reference counts, not scanning" section notes the change and points to ADR 0010. Check (repo root): `grep -q '0010' docs/adr/0008-methodology-neutral-domain-model.md`.
  - (d) The `Project::load` doc comment states the new worst-case peak and kept factors per methodology size. Check: `! grep -nE '6\.16x|394 MiB|reference-count row' src/store/snapshot.rs`.
  - (e) The `RefCounts` doc comment states its space and time cost (A8).
- [ ] AC7 No panics in library code, and no unsafe code (Q2, Q3). Check (repo root), which must print nothing: `awk '/^#\[cfg\(test\)\]/{exit} {print FILENAME":"FNR": "$0}' crates/aegis-model/src/project.rs | grep -E '\.unwrap\(\)|\.expect\(|panic!|unreachable!|todo!|unimplemented!'`. Also, `grep -q 'forbid(unsafe_code)' crates/aegis-model/src/lib.rs` succeeds.
- [ ] AC8 Neutrality still holds. This is job 010's AC17. Check (repo root), which must print nothing:
  `grep -rniE 'safety|financial|operational|privacy|stride|spoof|tamper|r155|annex|21434|confidential' crates/aegis-model/src --include='*.rs' | grep -v '^crates/aegis-model/src/methodology/iso21434.rs:' | grep -vE '^[^:]+:[0-9]+:\s*//' | grep -vE ':[0-9]+:\s*(pub(\([a-z]+\))? )?mod iso21434;$'`
- [ ] AC9 Nothing else regresses. `cargo test`, `cargo test --benches`, `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` pass. (Repo root) `scripts/ci.sh crates/aegis-model` and `scripts/deny.sh licenses bans sources` pass.

## Checks

From `crates/aegis-model/`:

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo test --locked
cargo test --benches
cargo bench --bench store_memory
cargo bench --bench store
```

From the repo root:

```sh
scripts/ci.sh crates/aegis-model
scripts/deny.sh licenses bans sources
git diff --name-only master...HEAD -- crates/aegis-model
# AC1: must print nothing
git diff master...HEAD -- crates/aegis-model/src | grep -E '^[-+][[:space:]]*pub[[:space:]]'
# AC7: must print nothing
awk '/^#\[cfg\(test\)\]/{exit} {print FILENAME":"FNR": "$0}' crates/aegis-model/src/project.rs | grep -E '\.unwrap\(\)|\.expect\(|panic!|unreachable!|todo!|unimplemented!'
# AC8: must print nothing
grep -rniE 'safety|financial|operational|privacy|stride|spoof|tamper|r155|annex|21434|confidential' crates/aegis-model/src --include='*.rs' | grep -v '^crates/aegis-model/src/methodology/iso21434.rs:' | grep -vE '^[^:]+:[0-9]+:\s*//' | grep -vE ':[0-9]+:\s*(pub(\([a-z]+\))? )?mod iso21434;$'
test -f docs/adr/0010-sparse-reference-counts.md
```

If the 1.85 toolchain is installed, also run `cargo +1.85 check --locked` from `crates/aegis-model/`. CI runs it in any case (ADR 0007).
