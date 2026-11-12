Commit: fc3feac Add the security pack check script and the architecture chapter (102a)
Files: scripts/check-security-pack.sh, scripts/check-security-pack-test.sh, docs/security-pack/README.md, docs/security-pack/en/01-architecture-and-data-flow.md
Done when:
1. `sh scripts/check-security-pack.sh` -> `security pack ok (1 files)`, exit 0.
2. `sh scripts/check-security-pack-test.sh` -> 9 cases ok, `all cases passed`.
3. `dash scripts/check-security-pack-test.sh` -> `all cases passed` (dash installed).
4. `grep -c '^## 1\.'` -> 9; `grep -c '^| DF-'` -> 7.
5. `wc -l` chapter -> 125 lines (script is 141 lines).
Before commit: `cargo fmt --all`, `scripts/ci.sh crates/aegis-license` (all ok), both pack scripts ok.
Notes:
- Telemetry grep (`grep -rn -i 'telemetry\|analytics\|sentry' crates/*/src crates/*/Cargo.toml Cargo.toml`): only false hits, the substring "sentry" inside `SumsEntry` in `crates/aegis-license/src/bundle.rs`. No telemetry, analytics or error-reporting code.
- Code vs prompt: the server also fetches the identity provider's discovery document at start (`main.rs` `oidc::discover`) before the signing keys (`jwks.rs`); DF-04 says so. I did not find a local project cache on the laptop in `aegis-client` (only an in-memory evaluation cache), so none is claimed.
- The keychain also holds a device key and refresh token with "keep me logged in" (provider.rs); the chapter says "the session token" as the prompt asked. Reviewer may want to widen this wording.
- Chapter has 7 DF rows (DF-07 is the export folder). It mentions "OpenID Connect" and the container name `aegis-server` only; no crate or type names. "Customer" and "Planned" markers used.
- The `1.5` claim "per-project permission check by role" rests on `crates/aegis-server/src/access.rs`, which I did not open; it is not listed in sources. The "host's loopback by default" claim is from `docs/deployment.md`.
- R3 in the script also fails a `## ` heading with no number. Test uses `sed -i.bak` (works on macOS and Linux).
NEXT: reviewer

## Rework 1
Commit: 7457ed5 Shorten the security pack check script to fit the review cap (102a)
Gate: counted lines were 224 (cap 200; the two scripts count, docs do not). Cut `scripts/check-security-pack.sh` from 141 to 113 lines with no change in behaviour: `is_name` became a `case`, the heading counting loop was simplified (an unnumbered heading still fails because its second word is not `N.M`), and the R4/R6/R8 checks became one-line tests. Counted total is now 196 (113 + 83).
Re-run: `sh scripts/check-security-pack.sh` ok (1 files); `sh scripts/check-security-pack-test.sh` all cases passed (also under dash); `cargo fmt --all` and `scripts/ci.sh crates/aegis-license` ok.
NEXT: reviewer

## Rework 2
Commit: see `git log -1` on the branch ("Fix keychain and update-check statements and add sources to the architecture chapter (102a)")
- Keychain row split into password login (session token, only with "keep me logged in") and company login (device key and refresh token, always; in memory if no keychain).
- DF-05 and 1.6 now say `update_url` or `AEGIS_UPDATE_URL` (the variable wins); air-gapped sites leave both empty.
- Opened `crates/aegis-server/src/access.rs`: its header confirms every project call resolves the caller's roles and checks the needed permission. Standalone bind to `127.0.0.1:0` confirmed in `standalone.rs:124`. Added `launcher.rs`, `standalone.rs`, `access.rs` to 1.9.
- Re-run: pack check ok (1 files); test script all cases passed; chapter is 129 lines. Only docs changed, so cargo checks are unaffected.
NEXT: reviewer

## Rework 3
Commit: see `git log -1` on the branch ("Fix the listen-address statement and make two pack test cases prove one rule (102a)")
- Chapter 1.5: server speaks plain HTTP; in the container it listens on all interfaces on port 50051; the compose file publishes it only on the host's loopback; keeping the plain port off other networks is a `Customer` duty. Added `deploy/compose.yaml` to 1.9.
- Test: the good chapter now has three sections (1.1, 1.2, 1.3 Sources). "heading gap" changes 1.2 to 1.3 in both en and ja, so only R3 fails. "ja section removed" deletes the ja 1.2 section and renumbers 1.3 to 1.2, so only R5 heading parity fails.
- Mutation check in a scratch edit: with the R3 numbering test disabled only "heading gap" fails; with the R5 heading-parity line disabled only "ja section removed" fails. Script restored.
- Re-run: pack check ok (1 files); test script all cases passed (also dash). Counted lines 113 + 85 = 198. Docs and shell only, so cargo checks are unaffected.
NEXT: reviewer
