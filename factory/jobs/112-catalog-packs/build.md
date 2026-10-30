# Build — 112-catalog-packs

## What was built
- `crates/aegis-model/src/catalog_pack.rs` and three metadata-only packs.
- `crates/aegis-model/packs/stride-services-cloud.json` (canonical) and
  `crates/aegis-model/src/methodology/stride.rs`.
- Tests `tests/catalog_pack.rs` and `tests/stride_pack.rs`.

## Verified
- `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`.
- `cargo test -p aegis-model --test catalog_pack --test stride_pack`.
- `./scripts/deny.sh`.

## Not built (remaining)
- None. The library-panel catalog section and the STRIDE methodology picker
  are built in this round (round 2).

## What was built in round 2 (UI)
- `aegis-server`: `Methodologies::builtin()` now hosts the
  `stride-services-cloud` pack, so it can be picked when creating a project.
- `aegis-client`: `CatalogView`/`builtin_catalogs()` and `LibraryView` catalog
  rows, pick and preview; the library search matches catalog metadata (name,
  key, description, attribution).
- `aegis-shell`: a **Catalogs (reference only)** section inside the attack
  tree library panel showing each catalog's name, description, links and
  attribution. Nothing is inserted from a catalog (the Insert flow stays
  attack-tree only).
- The new-project methodology picker offers `STRIDE (Services & Cloud)` in
  addition to the two existing packs; the client maps it to
  `stride-services-cloud` and its open-project fallback includes the pack.

## Verified
- `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`.
- `cargo test --workspace` (ran unsandboxed; loopback bind and OS trust store are blocked by the sandbox).
- `./scripts/deny.sh`.
