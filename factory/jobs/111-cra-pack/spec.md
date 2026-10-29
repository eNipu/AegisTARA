# Spec — 111-cra-pack

## Title
EU Cyber Resilience Act (Annex I) methodology pack and sample project (`ADR 0053`, `DOM-03`)

## Design Reference
- Reference: `docs/adr/0053-pack-names-and-cra-pack.md`
- Affected crates/paths: `crates/aegis-model/packs/eu-cra.json`, `crates/aegis-model/src/methodology/cra.rs`, `crates/aegis-server/src/methodologies.rs`, `crates/aegis-client/src/sample.rs`, `crates/aegis-shell/tests/ui.rs`

## Problem & Goal
Ship a built-in EU Cyber Resilience Act (Regulation (EU) 2024/2847 Annex I) methodology pack (`eu-cra`) written from public EU legislative text, using `"Harm"` / `"Harms"`, `"Security requirement"` / `"Security requirements"`, and a 4-level plain `"Likelihood"` scale (`attack_potential: None`), hosted on the server and accompanied by an EU CRA sample project (`Industrial IoT Edge Gateway (EU CRA)`).

## Acceptance Criteria
1. `crates/aegis-model/packs/eu-cra.json` and `Methodology::eu_cra()` / `Methodology::eu_cra_pack()` compile and pass SHA-256 pack verification with zero copyrighted ISO text.
2. `Methodologies::builtin()` in `aegis-server` hosts `eu-cra` alongside `iso21434-r155` and `med-device-demo`.
3. `sample_project_for(m)` builds an EU CRA sample project (`Industrial IoT Edge Gateway (EU CRA)`) using `CRA-1.1` / `CRA-2.1` catalog references and plain likelihood ratings when `m.key() == "eu-cra"`.
4. Integration tests in `aegis-client` and `aegis-shell` verify EU CRA project creation, terminology, plain likelihood rating, and report exports.
