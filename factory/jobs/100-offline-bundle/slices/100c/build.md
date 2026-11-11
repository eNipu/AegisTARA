# Build 100c

Commit: bb39c3b Name the server image and document offline install with ADR 0067 (100c)

Files:
- deploy/compose.yaml
- deploy/.env.example
- docs/deployment.md
- docs/release-checklist.md
- docs/adr/0067-offline-bundle.md

Done when:
1. `env POSTGRES_PASSWORD=x ... docker compose -f deploy/compose.yaml config -q` exit 0; `grep -q AEGIS_SERVER_IMAGE deploy/compose.yaml` ok.
2. `grep` for the section heading, `pkeyutl -verify` and `no-build` in docs/deployment.md: ok.
3. `grep -q offline-bundle.sh docs/release-checklist.md`: ok.
4. `test -f docs/adr/0067-offline-bundle.md` and NFR-06 in it: ok.
5. `scripts/offline-bundle-test.sh`: exit 0 ("all offline bundle checks passed").
6. `scripts/ci.sh`: exit 0 (workspace fmt, clippy, test ok; "all steps passed in 8 crate(s)").

Before you commit: `cargo fmt --all` ran clean; ci.sh and the compose config check and offline-bundle-test.sh all passed.

Notes:
- Rules label: the prompt says "R3 (bounded parsing)". In docs/engineering-standards.md, R3 is "stream large inputs and outputs". The ADR uses the prompt's label unchanged; the standards file was not touched.
- The prompt gives no reason for "no cargo-fuzz target". The ADR says the caps and crates/aegis-license/tests/bundle.rs bound the parser. Reviewer please check that wording.
- The bundle test runs in the CI `check` job (.github/workflows/ci.yml, around line 126), as the ADR says.
- Nothing was pushed. Diff is docs and deploy files only, 67 changed lines, no source code.

NEXT: reviewer
