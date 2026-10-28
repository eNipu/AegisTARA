# Spec — 057-attack-tree-library

## Title
Attack tree pattern library Phase 0: pack format, demo pack, n-gram search, review marks, and insertion (`ADR 0052`)

## Design Reference
- Reference: `docs/adr/0052-attack-tree-library.md`, `docs/brainstorm-attack-tree-library.md`
- Affected crates/paths: `crates/aegis-model/src/{library,op,project,lint}.rs`, `crates/aegis-model/packs/library-demo.json`, `crates/aegis-client/src/{library,commands,pages,view}.rs`, `crates/aegis-shell/ui/app.slint`, `crates/aegis-shell/src/controller.rs`

## Problem & Goal
Provide an offline, deterministic attack-tree pattern library (`aegis-library` v1 JSON pack format with SHA-256 content hash, license metadata, and decode caps), a built-in demo pack written from public sources (`UN R155 Annex 5` and `MITRE CAPEC` titles with Japanese aliases), fast character n-gram + token fuzzy search (`LibraryIndex`) with facets, and one-step insertion (as a new tree, under a gate, or replacing a step) with optional starting ratings flagged by `Op::SetReviewMark` and `LintCode::LibraryRatingNotReviewed`.

## Acceptance Criteria
1. `LibraryPack` validates format/version, license metadata, node/depth/text limits, and SHA-256 `items` hash (`LibraryPack::rehash` recomputes stale hashes for pack authors).
2. `crates/aegis-model/packs/library-demo.json` ships 12 items (`leaf`, `fragment`, `tree`) under `CC0-1.0` with English and Japanese aliases and zero copyrighted ISO text.
3. `LibraryIndex` indexes items using an inverted character 2/3-gram and word-prefix posting map (`postings: HashMap<String, Vec<(u32, u8)>>`) and precomputed unfiltered facet counts.
4. `insert_ops` inserts a library item as a new tree, under a gate, or in place of a step in a single atomic undo batch; when `include_ratings` is enabled, copied ratings are marked with `Op::SetReviewMark` (`LintCode::LibraryRatingNotReviewed`) until the analyst rates the step or clicks `Accept rating`.
5. All unit and integration tests pass (`cargo test`), `cargo clippy --all-targets -- -D warnings` is clean, `cargo fmt --check` is clean, and `scripts/deny.sh` passes.
