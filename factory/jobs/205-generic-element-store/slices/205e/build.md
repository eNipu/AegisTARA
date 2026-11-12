# Build 205e

Commit: 76222cb Save elements in the snapshot and in one workspace file per kind (205e)

Files:
- crates/aegis-model/src/element.rs (`set_element_loaded_visual`, `ElementStore::set_loaded_visual`, doc sentence on `inbound`)
- crates/aegis-model/src/store/mod.rs (`Limit::Elements`, `ElementAttributes`, `ElementLinks`, consts, Display; `MAX_WORKSPACE_FILES = 10 + KINDS.len() + 10_000`)
- crates/aegis-model/src/store/wire.rs (three caps, `BoundedVec: Default`, `ElementWire`, `ElementsByKind`, `SnapshotWire.elements`)
- crates/aegis-model/src/store/snapshot.rs (`Elements` writer, `ElementList`, `ElementView`, `ElementAttrs`; loader loop and `apply_snapshot_element`)
- crates/aegis-model/src/store/workspace.rs (`ELEMENTS_KEY`, writer, `lookup`, reader, `blame` for element errors)
- crates/aegis-model/tests/elements_store.rs (new, 11 tests)
- crates/aegis-model/tests/workspace.rs (`extra_ops` gains AddElement + SetElementAttribute; `"elements"` in key list)
- crates/aegis-model/tests/common/mod.rs (seed range 0..65, arms 58-63, `element` picker, `attr_key`, `attr_value`, `Seed::is_element_op`)
- crates/aegis-model/tests/ops_properties.rs (kind counts 55 / 52; diff check skips element seeds, see Notes)

Done when:
1. `cargo test --locked -p aegis-model --test elements_store` -> ok, 11 passed
2. `cargo test --locked -p aegis-model --test workspace` -> ok, 16 passed; `--test workspace_limits` -> ok, 13 passed
3. `--test ops_properties` -> ok, 11 passed; `--test store_properties` -> ok, 8 passed; `--test workspace_properties` -> ok, 2 passed
4. `cargo test --locked -p aegis-model --test elements_store loaded_` -> ok, 2 passed
5. `scripts/ci.sh crates/aegis-model` -> fmt, clippy, test all ok; `cargo check --workspace --all-targets` clean
6. `git diff --numstat HEAD~1 -- crates/aegis-model/src` -> 315 added + 8 removed = 323 lines (cap 400)

Notes:
- inbound rebuild: the loader never touches `Element::links` or `inbound`; each stored link is replayed as `apply(Op::LinkElement)` with the element's provenance, and `commit_link_element` adds the one count. `from_workspace_files` still only builds a snapshot and calls `Project::load`. Proven by `loaded_snapshot_refuses_to_remove_a_linked_threat` and `loaded_workspace_refuses_to_remove_a_linked_threat` (exact `elements: 2`, then 1, then removal allowed; `th(1)` removable).
- Deviation (tests only): with element ops in the generator, `diff_soundness_*` in ops_properties.rs failed because `Project::diff` does not see elements yet (`fwd.is_empty()` true while `base != curr`). The diff is slice 205f and `diff.rs` is outside this slice's allow list, so `check_diff_sequence` now drops element seeds (`Seed::is_element_op`) with a comment saying 205f lifts it. 205f should remove that filter.
- Visual numbers: the loader checks the running numbers itself (parse_visual -> InvalidId, repeat -> DuplicateEntry, next = max + 1) instead of `claim_visual`, since `WireVisual` has no constructor and element visual IDs are always written.
- `blame`: for DuplicateElement, UnknownElement, the nine `element: ElementId` variants and UnknownThreatScenario it names the first element file whose bytes hold the ID text; else today's answer.
- Writer element order: kinds in `ElementKind::ALL` order, elements in ID order, attributes in stored (byte) order, links in stored order; `visual_id` always written. Load treats the `elements` value as untrusted: kind keys resolved while decoding, all three caps checked in the decoder before any model check.
- `snapshot_caps` loads `MAX_ELEMENTS_PER_KIND` (10,000) elements generated as JSON text; it runs in well under a second here.

NEXT: reviewer
