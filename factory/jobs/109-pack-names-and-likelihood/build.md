# Build Report — 109-pack-names-and-likelihood

## Summary
Implemented `ElementNameDef` and `ElementNamesDef` in `aegis-model::methodology`, `has_attack_potential()` and `feasibility_label()` on `Methodology`, pack serialization/deserialization that omits default element names, and UI/export threading across `aegis-client` and `aegis-shell` (`ADR 0053`).

## Key Changes
- `crates/aegis-model/src/methodology/{mod,pack}.rs`: Added `ElementNameDef`, `ElementNamesDef`, `check_element_names`, `Methodology::element_names`, `Methodology::feasibility_label`, and `Methodology::has_attack_potential`.
- `crates/aegis-client/src/{pages,report,pdf}.rs`: Threaded `ElementNamesDef` and `has_attack_potential` into `Vocabulary`, HTML/Markdown reports, and PDF exports (omitting the 5-factor Attack Potential breakdown when `has_attack_potential` is `false`).
- `crates/aegis-shell/ui/app.slint` & `src/controller.rs`: Added `damage-singular`, `damage-plural`, `goal-singular`, `goal-plural`, `feasibility-label`, and `has-attack-potential` properties to `AppWindow` and bound them to sidebar, table headers, and inspector panels.

## Verification
- `cargo fmt --check`: PASS
- `cargo clippy --all-targets -- -D warnings`: PASS
- `cargo test`: PASS
- `scripts/deny.sh`: PASS
