GATE: PASS
Slice: 100c  Base: 67ce712230b70a53acd7dcbf7eacb71c7a3d37c7


Diff: 7 counted lines, 5 files
ok   scripts/ci.sh
ok   env POSTGRES_PASSWORD=x AEGIS_OIDC_ISSUER=https://login.example.com AEGIS_OIDC_AUDIENCE=a AEGIS_PUBLIC_URL=https://aegis.example.com docker compose -f deploy/compose.yaml config -q
ok   grep -q 'AEGIS_SERVER_IMAGE' deploy/compose.yaml
ok   grep -q 'Offline install (air-gapped sites)' docs/deployment.md
ok   grep -q 'offline-bundle.sh' docs/release-checklist.md
ok   test -f docs/adr/0067-offline-bundle.md
ok   scripts/offline-bundle-test.sh
