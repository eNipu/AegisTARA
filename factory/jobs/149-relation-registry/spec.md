## Title
Relation registry in `aegis-client`: one row per existing link (NFR-12, D30, architecture III.9.2)

## Design Reference
- `docs/architecture.md` III.9.2 (relation registry: `Relation`, `RELATIONS`, reverse lookup by scan, bidirectional test), III.9.3 row 7 (element recipe), III.9.1 (`Links(RelationKey)` cells).
- `docs/decisions.md` D30 (one registry for every link, read by inspector, chips, picker, trace, find, report), D37 (workbench), owner rule 2026-11-11 (no new Slint work; GPUI for all future UI).
- `docs/requirements.md` NFR-12 (new element/link via one recipe + one registry row; a test fails when a model link has no row) and G54.
- `factory/backlog.md` job 149 line; job 129 line (left `RelationKey` as a `&'static str` alias "until 149").
- `docs/design/workbench-breakdown.md` section 3 (relation registry: keep and do early; jobs 188, 189, 198 read it).
- Code: `crates/aegis-client/src/view.rs` (`RelationKey`, `ColumnKind::Links`, column specs, `Selection`), `src/table.rs`, `src/lib.rs`; model link fields in `crates/aegis-model/src/{asset,threat,control,item,project}.rs`.

## Problem & Goal
Links between element types are still read ad hoc by each page, so the GPUI workbench jobs (188 inspector, 189 table chips, 198 trace tabs) would each re-implement link navigation, and a sixth new element type would need link code in five places. This job publishes, in `aegis-client` only, one relation registry with one row per existing model link, the real `RelationKey` type, `ElementKind`, forward reads and a scan-based reverse lookup, plus the data helper the inspector will read. Value: modularity (NFR-12, D30) — a new model link without a registry row fails CI, instead of being silently invisible to every consumer — delivered with no visible change (P7, P1).

## Acceptance Criteria
1. `aegis-client::relations` is registered in `lib.rs` and exports `RelationKey`, `ElementKind`, `Relation { key, from, to, label, many, read, link, unlink }`, `RELATIONS`, `relation(key)`, `related(project, key, from)`, `reverse_related(project, key, to)` and `linked(project, target)`.
2. `RelationKey` is the real key type (a newtype over `&'static str` with one associated constant per registry row), not the job-129 alias. `view.rs` re-exports it and all `ColumnKind::Links(...)` cells use its constants; `table_conventions` still passes.
3. `RELATIONS` has exactly one row per link kind, with these keys: `asset-component`, `threat-asset`, `threat-damages`, `threat-tree`, `threat-goal`, `threat-claim`, `goal-controls`, `control-targets`, `step-via`, `step-mitigations`, `dataflow-from`, `dataflow-to`. Each row has correct `from`/`to` `ElementKind` and `many`, a unique key, a Fluent label key present in en/ja/pseudo, and a `read` that returns the targets (empty for a wrong-kind input; no panics).
4. `reverse_related` is a scan of the source elements of that row's `from` kind calling `read`; no index is added; the O(elements x relations) cost is documented on the function.
5. `linked(project, target)` returns the forward and reverse link groups for a selection, so the future inspector "Linked" sections and link chips read one table.
6. `crates/aegis-client/tests/relations.rs` builds a project with one instance of every row in criterion 3, independently enumerates the same links from the model arenas, asserts the two sets are equal, and asserts each link is reachable through `RELATIONS` in both directions. Removing a row or pointing a key at the wrong `read` makes the test fail.
7. The same test asserts keys are unique, every label resolves in en, ja and pseudo, and `many` matches the fixture's forward cardinality.
8. No `.slint` or `aegis-shell` edit, no new dependency, no snapshot/op-log format change. `cargo test --locked -p aegis-client --test relations` and `scripts/ci.sh` pass.

## Out of scope
- The GPUI inspector "Linked" sections, link chips, multi-link picker and trace graph/matrix (jobs 188, 189, 198, 150). Only the data they read is built here; under the owner rule no Slint or `aegis-shell` file is touched, so there is no visible change.
- New link kinds or model changes (e.g. functions in job 160, conditions, parties, several goals per threat). Their jobs add rows to `RELATIONS`.
- Changing computed-cell text or any other part of `table.rs`; the only table change is the `RelationKey` type migration.
- An index or cache for reverse lookups — the table is intentionally a scan.

## Decisions
- Scope is `aegis-client` data level only, following the owner's 2026-11-11 GPUI ruling and the job-129c rework: no `.slint`/`aegis-shell` edits, no screen work.
- **This job owns the real `RelationKey` type.** Job 129 deliberately left a `&'static str` alias; here it becomes a newtype with constants and `view.rs` re-exports it. Reason: D30 calls the key stable and used in settings and report profiles; a newtype stops unrelated strings being passed as keys.
- The addressed element is `aegis_client::view::Selection`, not `aegis_model::ElementRef`, and `ElementKind` mirrors `Selection`. Reason: the required step links need `(TreeId, NodeId)`, which `ElementRef` cannot name while `Selection::TreeNode` can, and link chips already carry `Selection`. It is distinct from `aegis_model::ElementKind` (generic kinds); keep the path qualified in code.
- `read` returns `Vec<Selection>` instead of architecture's `SmallVec<[ElementRef; 4]>`. Reason: `aegis-client` has no `smallvec` dependency and this is a cold, scan-based path; a new dependency would break P3/P7 for no measured gain.
- `link`/`unlink` are `Option<fn(Selection, Selection) -> Option<Edit>>` rather than the architecture's `-> Edit`. Reason: a library function must not panic on a mismatched selection kind (engineering standards, no panics in library code). They are `Some` only where a two-argument `Edit` exists; structural links (threat→asset, data-flow ends) and step mitigations (which carry a feasibility level) use `None`.
- "Data flow ends" is two rows (`dataflow-from`, `dataflow-to`) so each end is individually addressable and checkable both ways.
- `ElementKind` contains the kinds that currently have links plus `Step`, and is `#[non_exhaustive]`; future jobs add their variants.
- The row label is a `&'static str` Fluent message id; no `MsgKey` type exists yet, so none is invented.
- Nothing here changes a stored format, security, licensing or user data, and every choice is reversible inside one crate.
