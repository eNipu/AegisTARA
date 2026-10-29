# Build Report — 111-cra-pack

## Summary
Created the built-in EU Cyber Resilience Act (`eu-cra`) methodology pack (`crates/aegis-model/packs/eu-cra.json` and `crates/aegis-model/src/methodology/cra.rs`), registered it in `aegis-server`, and added an EU CRA sample project in `aegis-client` (`ADR 0053`).

## Key Changes
- `crates/aegis-model/packs/eu-cra.json` & `src/methodology/cra.rs`: Built-in `"eu-cra"` pack (`0.1.0`) with `"Harm"`/`"Harms"`, `"Security requirement"`/`"Security requirements"`, `"Likelihood"`, 4-level impact/likelihood scales, `attack_potential: None`, and `cra-annex1` (`CRA-`) catalog scheme.
- `crates/aegis-server/src/methodologies.rs`: Added `Methodology::eu_cra_pack()` to `Methodologies::builtin()`.
- `crates/aegis-client/src/sample.rs`: Added `eu_cra_sample` (`Industrial IoT Edge Gateway (EU CRA)`) dispatched from `sample_project_for`.
- `crates/aegis-client/tests/pages.rs` & `crates/aegis-shell/tests/ui.rs`: Added end-to-end tests verifying EU CRA terminology, plain likelihood, and report exports.

## Verification
- `cargo fmt --check`: PASS
- `cargo clippy --all-targets -- -D warnings`: PASS
- `cargo test`: PASS
- `scripts/deny.sh`: PASS
