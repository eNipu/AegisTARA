VERDICT: PASS
## For the owner
What changed: one new file, chapter 6 of the security pack: 71 ready answers to a buyer's security questionnaire.
Each answer has a status word (Yes, No, Partly, Customer, Planned, Vendor) and points to the chapter section or repository file that backs it.
No code and no other chapter changed.
Look at:
- docs/security-pack/en/06-security-questionnaire.md:101 (Q-53): installer code signing is `Planned`. Correct today, but it is the answer a buyer will press hardest on; update it when jobs 099/100 land.
- docs/security-pack/en/06-security-questionnaire.md:88 (Q-45): penetration test is `Vendor`, and the internal review is left out because ADR 0054/0061 do not show that the findings were closed. You must decide what you will say in the contract papers.
- docs/security-pack/en/06-security-questionnaire.md:78 (Q-40): "the build pins one Rust version" is true (RUST_TOOLCHAIN in ci.yml). But `docs/release-checklist.md:47-48` points to a `rust-toolchain.toml` that does not exist. That is a separate doc defect for the planner, not part of this slice.
Risk: a wrong answer would mislead a buyer. I checked the answers most likely to be wrong against their sources: password and lockout numbers, session length (3.3, 3.4), TLS versions and the database TLS default (2.2), log retention (4.5), the `unsafe` rule (engineering-standards Q2), the fuzz targets (README) and that they are not in CI, the property tests (proptest in 4 crates), when CI runs (fmt/clippy/test on master, cargo-deny on PRs), SBOM and CRA timings (SECURITY.md), license grace and the edit lock (ADR 0032 section 5), one instance per database (1.8) and NFR-06. All of them match. Status words lean conservative: `Partly` wherever a limit exists.
View: git -C factory/worktrees/102-product-security-pack diff 60b18b3ce2b211a0c2bd7b37bc96c2a7b949cb62..HEAD
## Findings
- none blocking.
- Note: there are 71 rows, more than the 45-55 aim but within the rules (at least 40, file under 220 lines). Q-36 says property tests are used, and that is true, but it cites only the standards file, which states the rule and not the use. That is acceptable.
## Lesson
[planner] When a release checklist names a file (here `rust-toolchain.toml`), check that the file exists before a pack chapter relies on the checklist. Job 102d found a reference to a missing file.
NEXT: human
