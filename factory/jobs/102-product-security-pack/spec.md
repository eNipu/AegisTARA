# Spec: 102-product-security-pack

## Title
Product security pack in English and Japanese (NFR-07, NFR-09; closes the security pack half of gap G39)

## Design Reference
- `docs/requirements.md` line 45 (G39: Japanese buyers ask for a security pack before a PoC), line 324 (NFR-07), line 326 (NFR-09: personal data handling statement for GDPR and APPI: accounts, event log, AI audit log retention), line 301 (AI-05: the AI audit log, job 088, not built).
- `docs/decisions.md` D3 (customer-hosted server, no phone-home), D13 (English and Japanese first), D21 (consultancy first in Japan), D22 (standalone app), D40 (GUI first, files are storage only).
- Facts come from: `docs/architecture.md` II.1 (lines 1211-1248), II.4.1 to II.4.7 (lines 1308-1667), II.6.1 (1815-1841), II.7 (1843-1850); `docs/deployment.md`; `docs/release-checklist.md`; `SECURITY.md`; ADRs 0028, 0032, 0034, 0035, 0039, 0046, 0050, 0054, 0061, 0067; the code under `crates/aegis-server/src/`, `crates/aegis-auth/src/`, `crates/aegis-license/src/`, `crates/aegis-client/src/settings.rs`.
- New files: `docs/security-pack/README.md`, `docs/security-pack/en/*.md` (six chapters), `docs/security-pack/ja/*.md` (the same six), `scripts/check-security-pack.sh`, `scripts/check-security-pack-test.sh`, `docs/adr/0069-product-security-pack.md`.
- Touched: `docs/deployment.md` (one stale limit), `crates/aegis-server/src/store/postgres.rs` (module doc comment only), `README.md`, `docs/release-checklist.md`, `docs/status-and-roadmap.md`, `docs/requirements.md`, `.github/workflows/ci.yml`.

## Problem & Goal
A Japanese OEM or supplier sends a vendor security questionnaire before it lets a tool near its threat models. Today the answers are spread over the architecture reference, ADRs and code, in English only, so every questionnaire costs the owner days and risks an answer the code does not back.

After this job the repository holds one pack of six short chapters, in English and Japanese, that the owner hands to a buyer: architecture and data flow, cryptography, authentication and access, logging and retention, personal data handling (GDPR and APPI), and ready answers to a typical questionnaire. Every statement names the file that proves it, and a script fails when the two languages drift apart or a named file is gone.

Business value (P7): the pack is asked for before a PoC (G39), so it shortens the sales step the consultancy-first plan depends on (D21). The check script earns its place because a security statement that is wrong, or says one thing in English and another in Japanese, is a contract risk; it is about a hundred lines of shell and no dependency.

## Decisions taken in this spec
- **Format and place:** Markdown under `docs/security-pack/{en,ja}/`, one file per chapter, same file names in both languages. Turning the pack into a PDF for hand-over is not part of this job (see Out of scope).
- **Truth rule:** the pack states only what the code or a decision record proves. Each claim that is not built is marked `Planned` (Japanese: `計画中`). Each duty that is the customer's (TLS certificate, disk encryption, backup, identity provider, MFA) is marked `Customer` (`お客様`). No certification, audit or organisational control is claimed.
- **Vendor organisation questions** (certifications, staff screening, insurance, office security) are not product facts and cannot be answered from the repository. The questionnaire lists them with the status `Vendor` and the fixed answer that the vendor answers them in the contract papers for each customer. The owner fills them outside the repository.
- **AI audit log (NFR-09):** not built (AI-05, job 088). The personal data chapter says so, says no AI feature ships in this release, and states what AI-05 will record, as `Planned`.
- **Where the code and a document differ, the code wins and the document is fixed in this job.** Known case: `docs/deployment.md` lines 88-89 and the module doc of `crates/aegis-server/src/store/postgres.rs` lines 22-27 still say `AEGIS_DB_TLS=encrypt` does not check the database certificate. Since job 061 (ADR 0054, SEC-05) both `encrypt` and `verify` check the chain and the host name (`rustls_config`, `postgres.rs` lines 125-141).
- **Shape that the script checks:** chapter `N` has a `# ` title on line 1, `## N.M Title` headings numbered from `N.1` without gaps, and a last section `## N.M Sources` (Japanese: `## N.M 出典`) of bullets `` - `repo/path` ``. Table rows that carry an ID (`DF-01`, `CR-01`, `LG-01`, `PD-01`, `Q-01`) keep the same IDs in the same order in both languages.
- **Not legal advice:** the personal data chapter describes what the product stores and does; it says in its first section that it is not legal advice and that the customer is the controller (GDPR) and the business operator handling personal information (APPI).

## Acceptance Criteria
1. **Six English chapters exist** under `docs/security-pack/en/`: `01-architecture-and-data-flow.md`, `02-cryptography.md`, `03-authentication-and-access.md`, `04-logging-and-retention.md`, `05-personal-data.md`, `06-security-questionnaire.md`, each at most 220 lines. Check: `sh scripts/check-security-pack.sh --complete`.
2. **Six Japanese chapters exist** under `docs/security-pack/ja/` with the same file names, the same heading numbers and the same row IDs as the English ones. Check: `sh scripts/check-security-pack.sh --complete`.
3. **Every chapter names its sources** and every named path exists in the repository. Check: `sh scripts/check-security-pack.sh --complete`.
4. **The script refuses a broken pack.** `scripts/check-security-pack-test.sh` builds throwaway packs in a temporary folder and proves the script exits 1 for: a Japanese chapter with a missing section, a Japanese chapter with a missing row ID, a source path that does not exist, a gap in the heading numbers, a file name outside the six, a `TODO` marker, a Japanese file identical to the English one, and a missing chapter under `--complete`; and exits 0 for a good pack. Check: `sh scripts/check-security-pack-test.sh`.
5. **Cryptography chapter** has an inventory table (`CR-NN`) with, per row: purpose, algorithm and parameters, library, where the key or secret lives. It covers at least: client TLS, database TLS, password hashing, session tokens, OIDC token validation, DPoP, license files, update manifests, the offline bundle, the project operation hash chain, the event log hash chain, figure digests, the OS keychain. It states that the product does not encrypt data at rest itself and is not FIPS 140 validated. Check: reviewer reads the table against the sources.
6. **Personal data chapter** has an inventory table (`PD-NN`) with, per row: the data, where it is stored, why, how long it is kept, how it is removed. It covers accounts, sessions, the event log, the project operation history (actor and time), baselines and figures (author), SSO subject and groups, network addresses at login, the desktop settings file and keychain, and the AI audit log (`Planned`). It states the erasure limit of the hash-chained operation history and the event log, and maps the facts to GDPR Articles 5, 15 to 17, 25, 30, 32, 44 and to the APPI duties (purpose of use, security control measures, provision to third parties, transfer to a foreign country, requests from the person). Check: reviewer.
7. **Questionnaire chapter** has at least 40 rows (`Q-NN`), each with a question, a status (`Yes`, `No`, `Partly`, `Customer`, `Planned`, `Vendor`), a short answer and a reference to a chapter section. Check: `grep -c '^| Q-' docs/security-pack/en/06-security-questionnaire.md` prints 40 or more, and the same for `ja/`.
8. **No stale statement about database TLS** remains. Check: `grep -n 'does not check the database certificate' docs/deployment.md` prints nothing, and `grep -n 'without checking the certificate' crates/aegis-server/src/store/postgres.rs` prints nothing.
9. **CI runs the check.** The `check` job in `.github/workflows/ci.yml` runs `scripts/check-security-pack.sh --complete` and `scripts/check-security-pack-test.sh`. Check: `grep -n 'check-security-pack' .github/workflows/ci.yml` prints two `run:` lines.
10. **Written down.** `docs/adr/0069-product-security-pack.md` records the format, the truth rule and the script rules; `README.md` links the pack; `docs/release-checklist.md` has a line to re-read the pack for each release; `docs/requirements.md` G39 and `docs/status-and-roadmap.md` say the security pack exists and the user manual (job 103) is still open.
11. **Gate.** `scripts/ci.sh` passes on the whole workspace. No new dependency. No change to any Rust code other than the one doc comment.

## Out of scope
- The user manual (job 103).
- A PDF or Word rendering of the pack, and any in-app view of it.
- Answers to vendor organisation questions, a legal review of the GDPR and APPI text, and a native-speaker review of the Japanese: owner tasks after the merge.
- Building anything the pack finds missing (encryption at rest, MFA for password login, an event log archive, the AI audit log). The pack reports them as they are.
- New Slint or GPUI work.
