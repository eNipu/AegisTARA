VERDICT: PASS

Round 2 reviewed as the delta 1b04165..69014b3 (3 files: catalog.rs, methodology/mod.rs, project.rs; 69 insertions, 48 deletions) on top of the round-1 review, which found the rest of the code sound.

## Check results (run from factory/worktrees/009-domain-model-v0-2)

- `cargo fmt --check` (crates/aegis-model) -> pass
- `cargo clippy --all-targets -- -D warnings` -> pass (also inside ci.sh)
- `cargo test` -> pass (119 lib, 3+10+8+10+3+11+12+2+3 other test binaries, 181 doctests, 0 failed)
- `cargo test --benches` -> pass (exit 0)
- `scripts/ci.sh crates/aegis-model` -> pass (fmt, clippy, test)
- `scripts/deny.sh licenses bans sources` -> pass
- Neutrality grep (corrected form from rework.md `## Rework 1`) -> pass, prints nothing
- `test -f docs/adr/0008-methodology-neutral-domain-model.md` + `grep -i 'second methodology'` -> pass
- `git diff --exit-code master -- crates/aegis-model/Cargo.toml Cargo.lock` -> pass (no dependency change)
- AC17 grep for unwrap/expect/panic -> first hit per file is after the last `#[cfg(test)]` line (catalog.rs 376 > 316, iso21434.rs 163 > 157, mod.rs 1423 > 1349, project.rs 1544 > 1517), so hits are test-only.
- `grep 'allow(' src` excluding `cfg(test)` -> no hits. No `#[allow]` remains in non-test code.

## Round-1 findings, re-checked

1. AC15 grep (spec defect) -> resolved by the corrected check in rework.md. It prints nothing. No code change needed.
2. mod.rs `#[allow(dead_code)]` on the macro `new` -> fixed. The macro no longer defines `new`. `ImpactCategory::new` and `Property::new` are `pub(crate)` and used (damage impacts, `PropertySet::iter`). `Level::new` and `CatalogScheme::new` are `#[cfg(test)]`. `ThreatCategory` has none. Clippy with `-D warnings` shows no dead-code warning in either profile, which confirms the used/unused split is right. Vocabulary indices still cannot be built publicly.
3. catalog.rs eager `err.clone()` -> fixed. `let err = || ModelError::InvalidCatalogId { scheme }` with `ok_or_else(err)` (the closure captures only the Copy `scheme`, so nothing is cloned). The behaviour is identical; the catalog_syntax tests pass.
4. `to_vec` in `validate` -> accepted in round 1, unchanged.
5. type_complexity and too_many_arguments allows -> fixed. A private `Vocab<T> { items, by_key }` is the return type of the four `build_*` helpers, and `Methodology::new` destructures it field by field with no extra clones (fields moved). The `too_many_arguments` allow on `check_add_threat` was removed and clippy passes, so it was never needed.
6, 7. Extra public API (`with_slot_limit`, extra getters) and the second methodology in the shared test kit -> accepted in round 1, unchanged.

## Acceptance criteria

AC1-AC20 are met and tested. The round-2 diff touches no behaviour, and every round-1 named test still passes. AC15 passes with the corrected check.

## Findings

None blocking.

- `crates/aegis-model/src/methodology/mod.rs:322-354` — minor, no action — the raw constructors now sit outside the macro in two `pub(crate)` impls and two `#[cfg(test)]` impls. This is a little less uniform than before but is the smallest change that removes the dead-code allow, and the comment explains it.
- Optional UX message items (review-ux.md) were declined in build.md because they would change `ModelError` variant shapes the spec fixed. This is a reasonable reason, and the rework marked them optional. Not a defect.
