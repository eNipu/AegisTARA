# Spec — 109-pack-names-and-likelihood

## Title
Pack-defined element names and plain likelihood method (`ADR 0053`, `DOM-01`, `DOM-02`)

## Design Reference
- Reference: `docs/adr/0053-pack-names-and-cra-pack.md`
- Affected crates/paths: `crates/aegis-model/src/methodology/{mod,pack}.rs`, `crates/aegis-client/src/{pages,report,pdf}.rs`, `crates/aegis-shell/ui/app.slint`, `crates/aegis-shell/src/controller.rs`

## Problem & Goal
Allow methodology packs to define custom singular/plural names for each element type (`ElementNamesDef`, e.g., `"Harm"` / `"Harms"` for damage scenarios and `"Security requirement"` / `"Security requirements"` for cybersecurity goals, plus `"Likelihood"` for feasibility) and support a plain likelihood method (`attack_potential: None`) rated directly without Common Criteria 5-factor attack potential, while preserving byte-for-byte compatibility with existing packs (`iso21434-r155.json` and `med-device-demo.json`).

## Acceptance Criteria
1. `MethodologyDef` includes `element_names: ElementNamesDef` (`Default` = ISO/SAE 21434 labels), validated by `Methodology::new` against `MAX_LABEL_CHARS` and non-blank rules (R3).
2. `iso21434-r155.json` and `med-device-demo.json` round-trip with identical SHA-256 digests when `element_names` equals `ElementNamesDef::default()`.
3. `Methodology::element_names()`, `Methodology::feasibility_label()`, and `Methodology::has_attack_potential()` drive `Vocabulary`, HTML/Markdown/PDF exports, and the Slint UI (`has-attack-potential` hides the 5-factor attack-potential controls when `false`).
4. All unit and integration tests pass (`cargo test`), `cargo clippy --all-targets -- -D warnings` is clean, and `cargo fmt --check` is clean.
