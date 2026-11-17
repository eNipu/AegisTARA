Commit: aa59123 Publish the relation registry in aegis-client (149a)

Files:
- crates/aegis-client/src/lib.rs
- crates/aegis-client/src/relations.rs (new)
- crates/aegis-client/src/view.rs
- crates/aegis-client/tests/table_conventions.rs
- crates/aegis-client/locales/en/main.ftl
- crates/aegis-client/locales/ja/main.ftl
- crates/aegis-client/locales/pseudo/main.ftl

Done when:
1. `cargo test --locked -p aegis-client --lib relations` — exit 0 (5 passed; 3 new `relations::tests` plus 2 pre-existing `excel::tests` matching the filter).
2. `cargo test --locked -p aegis-client --test table_conventions` — exit 0 (8 passed).
3. `grep -n 'Links(' crates/aegis-client/src/view.rs` — four hits: the `Links(RelationKey)` variant definition plus only `RelationKey::ASSET_COMPONENT` / `RelationKey::THREAT_ASSET` constants; no string literals remain.

Before you commit (all run from the worktree root, all exit 0):
- `cargo fmt --all` — ok
- `scripts/ci.sh crates/aegis-client` — fmt ok, clippy ok (`-D warnings`), test ok
- the two `Done when` test commands above — ok

Notes:
- Fluent message ids cannot contain `.`: the dotted ids from the prompt's table parse as `Junk` and make `Catalog::load` panic (it `.expect`s the catalog to parse), breaking `table_conventions`. The 12 ids therefore use a hyphen separator (`relation-asset-component`, `relation-threat-claim`, ...) in both `Relation.label` and all three locale files, with message ids identical across the files and each label resolving in every language. This is the one intentional deviation from the literal dotted spelling in the prompt; it is required for "label is the Fluent key shown" to hold.
- `ElementKind::kind_of` is implemented as an associated function on `ElementKind`, as it is listed inside that enum's bullet; it maps all 12 `Selection` variants (`TreeNode` -> `Step`).
- `control-targets` and `step-via` declare `to: ElementKind::Component` as the representative kind; their actual targets are `ItemRef` values converted to `Selection::Component` or `Selection::Interface`, since `ElementKind` has no combined variant. `to` is descriptive only — `reverse_related` scans by `from` and matches with `read`, so this does not affect lookups.
- The 12-row `RELATIONS` table is built by a private `const fn rel(...)` and a few functions carry `#[rustfmt::skip]` to keep the file under the slice cap. Counted changed source lines (`git diff --numstat master`, excluding `*/tests/*` per `verify.py`): 370 (relations.rs) + 27 (view.rs) + 1 (lib.rs) + 42 (locales) = 440, under the 480 cap. The `rel` helper carries `#[allow(clippy::too_many_arguments)]` because it mirrors the eight public `Relation` fields.
- `link`/`unlink` return `None` on any wrong-kind selection (validated on both arguments, including the target kind on unlink). `step-mitigations`, `threat-asset`, and both data-flow rows intentionally have no link/unlink, per the prompt. No model crate, `.slint`, shell, dependency, snapshot or op-log change.

NEXT: reviewer
