Commit: 5bf3fb2 Add security questionnaire answers to the product security pack (102d)
Files: docs/security-pack/en/06-security-questionnaire.md
Done when:
1. `sh scripts/check-security-pack.sh`: `security pack ok (6 files)`
2. `grep -c '^| Q-'`: 71
3. rows without a status word: 0
4. ID gap check: exit 0
5. `grep -c '^## 6\.'`: 12
Also: `cargo fmt --all` clean; `scripts/ci.sh crates/aegis-license` all steps passed. File is 151 lines.
Notes:
- Rows per status: Yes 28, No 9, Partly 16, Customer 7, Planned 1, Vendor 10 (71 total, above the 45-55 aim, within the 40 minimum).
- Penetration test: Vendor. Internal review left out: ADR 0054 and 0061 do not show that high and medium findings were closed.
- Signed installers: Planned (no code signing of installers in the repo; only the manifest and SHA256SUMS are signed).
- "Pinned toolchain": there is no rust-toolchain.toml; the pin is RUST_TOOLCHAIN in ci.yml, and the answer says so only as "the build pins one Rust version".
- Export on leaving: Partly; docs/architecture.md only shows report and table exports.
- Q-13: no idle timeout exists, so Partly. Q-34 code review is Partly (checklist, not independent review).
- Rows cite non-pack files in the reference column, listed in 6.12.
- No contradiction with chapters 1 to 5 found.
NEXT: reviewer
