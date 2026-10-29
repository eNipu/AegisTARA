# ADR 0053: Methodology element type names, plain likelihood, and EU CRA pack

- Status: accepted (jobs 109-pack-names-and-likelihood, 111-cra-pack)
- Date: 2026-10-28
- Scope: `aegis-model` (`methodology/mod.rs`, `methodology/pack.rs`, `methodology/cra.rs`, `packs/eu-cra.json`), `aegis-server` (`methodologies.rs`), `aegis-client` (`pages.rs`, `report.rs`, `pdf.rs`, `session.rs`, `sample.rs`), `aegis-shell` (`ui/app.slint`, `controller.rs`)
- Rules: R3, R6, D10, P1, P4 in `docs/engineering-standards.md`
- Requirements: PACK-04, PACK-05, PACK-07
- Builds on: `0038-methodology-model.md`, `0046-pack-file-format.md`, `0047-medical-device-pack.md`, `0048-pack-choice-on-create.md`

## Context

ISO/SAE 21434 and UNECE R155 speak of *damage scenarios*, *cybersecurity goals*, and *attack feasibility* derived optionally from five attack-potential factors (`time`, `expertise`, `knowledge`, `window`, `equipment`). Other risk methodologies—such as the EU Cyber Resilience Act (Regulation (EU) 2024/2847 Annex I) and ISO 14971 / AAMI TIR57—use different element terminology (*harm*, *security requirement*, *likelihood*) and rate likelihood directly as a plain scale without the five Common Criteria attack-potential factors (`PACK-04`, `PACK-05`, `PACK-07`).

Previously, table headers, inspector captions, and report/PDF sections hardcoded ISO/SAE 21434 labels in `aegis-client` and `aegis-shell`, and the Attack Trees inspector always showed the five-factor Attack potential editor regardless of whether `Methodology::attack_potential()` was `Some` or `None`.

## Decisions

### 1. `ElementNamesDef` and `ElementNameDef` on `MethodologyDef`

`MethodologyDef` gains `pub element_names: ElementNamesDef` (`#[serde(default, skip_serializing_if = "is_default_element_names")]`), which holds singular/plural (`ElementNameDef { singular, plural }`) pairs for the seven core TARA tables (`asset`, `threat_scenario`, `damage_scenario`, `attack_tree`, `risk_item`, `cybersecurity_goal`, `security_control`) plus a single `feasibility` label.

- `ElementNamesDef::default()` returns the ISO/SAE 21434 terminology (`"Asset"`/`"Assets"`, `"Damage scenario"`/`"Damage scenarios"`, `"Cybersecurity goal"`/`"Cybersecurity goals"`, `"Attack feasibility"`, etc.).
- Because `element_names` defaults on deserialization and is omitted when equal to `ElementNamesDef::default()`, existing pack files (`iso21434-r155.json` and `med-device-demo.json`) remain byte-for-byte unchanged and their SHA-256 digests are preserved.
- `check_element_names` validates all 15 label strings in `Methodology::new` against `MAX_LABEL_CHARS` and non-blank rules (R3).
- `Methodology::element_names(&self) -> &ElementNamesDef` and `Methodology::feasibility_label(&self) -> &str` expose the validated labels, and `Methodology::has_attack_potential(&self) -> bool` reports whether the pack defines an attack-potential factor scheme.

### 2. Plain likelihood mode when `attack_potential` is `None`

When `m.has_attack_potential()` is `false`:
- `aegis-client` and `aegis-shell` hide the five-factor Attack potential editor in the Attack Trees inspector and show only the direct feasibility/likelihood level picker labeled with `m.feasibility_label()`.
- HTML (`report.rs`), Markdown (`report.rs`), and PDF (`pdf.rs`) exports omit the Attack Potential factor breakdown columns and title each section using `m.element_names()` (`damage_scenario.plural`, `cybersecurity_goal.plural`, `feasibility`, etc.).

### 3. Built-in EU Cyber Resilience Act pack (`eu-cra`)

`crates/aegis-model/packs/eu-cra.json` and `crates/aegis-model/src/methodology/cra.rs` (`Methodology::eu_cra()` and `Methodology::eu_cra_pack()`) define a built-in pack for EU CRA Annex I essential cybersecurity requirements:
- Key `"eu-cra"`, version `"0.1.0"`, label `"EU Cyber Resilience Act (Annex I)"`.
- Element names: `"Harm"` / `"Harms"` for damage scenarios, `"Security requirement"` / `"Security requirements"` for cybersecurity goals, and `"Likelihood"` for feasibility.
- Four-level impact scale (`negligible`, `moderate`, `significant`, `critical`), four-level plain likelihood scale (`very-low`, `low`, `medium`, `high`), `attack_potential: None`, and a 4x4 risk matrix mapping to risks 1..=4 (`Low`, `Medium`, `High`, `Critical`).
- Catalog scheme `cra-annex1` (`prefix: "CRA-"`, 2 segments, e.g. `CRA-1.1`, `CRA-2.1`).
- Hosted alongside `iso21434-r155` and `med-device-demo` in `aegis-server` (`Methodologies::builtin()`) and selectable when creating a project (`sample::sample_project_for`) or switching packs.

## Consequences

- `iso21434-r155.json` and `med-device-demo.json` digests do not change.
- Analysts working under EU CRA see CRA-native terminology across the sidebar, table headers, inspector labels, and HTML/Markdown/PDF exports, and rate step/tree likelihood directly on the 4-level scale without irrelevant automotive attack-potential fields.
