Commit: 4e796c6 Add SetAssetProperties edit and grid core selection and row helpers (189b1)
Files:
- crates/aegis-client/src/commands.rs
- crates/aegis-client/src/lib.rs
- crates/aegis-client/src/grid/mod.rs
- crates/aegis-client/tests/grid_edit.rs

Done when:
1. `cargo test --locked -p aegis-client --test grid_edit` — PASS (8 passed, 0 failed)
2. `scripts/ci.sh crates/aegis-client` — PASS (`fmt`, `clippy`, `test` all passed)

Notes:
- Changed source lines (excluding `tests/`): 378 insertions, 1 deletion (379 total, under the 540 cap).
- `system_component` takes `sys: aegis_model::ElementId` because systems in `aegis_model` and `Owner::System(ElementId)` use `ElementId` (there is no `SystemId` type in `aegis_model`).
- In `add_row_edit` for `TableId::Threats | TableId::Risks`, `default_threat_category` selects the first methodology threat category whose violated properties are carried by the chosen asset (falling back to `0`). This preserves `category: 0` whenever category `0` fits the asset, while preventing `ModelError::PropertyNotOnAsset` in ISO/SAE 21434 when adding a threat right after adding an asset with `properties: vec![0]` (Confidentiality, whereas category `0` Spoofing requires Authenticity).

NEXT: reviewer
