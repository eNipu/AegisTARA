Commit: b3a287c Add bundle commands to the vendor CLI and the offline bundle scripts (100b)
Files: crates/aegis-license/src/bin/aegis-license.rs, crates/aegis-license/Cargo.toml, crates/aegis-license/tests/bundle_cli.rs, scripts/offline-bundle.sh, scripts/offline-bundle-test.sh, .github/workflows/ci.yml
Done when:
1. `cargo test --locked -p aegis-license --features issuer --test bundle_cli` - 3 passed
2. `scripts/offline-bundle-test.sh` - all checks passed (OpenSSL 4.1.0 locally)
3. `scripts/ci.sh crates/aegis-license` - fmt, clippy, test ok
4. Both scripts executable (mode 755) - yes
5. `git diff --numstat HEAD~1` - 471 added in total; non-test lines about 182 (CLI 54, Cargo 5, ci.yml 3, offline-bundle.sh 120); under 400
Notes: `sign-bundle` prints `signed N files` counting lines of SHA256SUMS. offline-bundle.sh requires --out to be an existing folder (exit 2 otherwise). The changed-SHA256SUMS negative check flips the first hash character.
NEXT: reviewer

## Rework 100b
Commit: 67ce712 Reject duplicate installer names and quote the images path in the offline bundle script (100b)
Files: scripts/offline-bundle.sh, scripts/offline-bundle-test.sh
- Duplicate installer base names now exit 2 with "installer file name given twice". Test checks exit status 2.
- Images path is quoted in the file check (empty value skipped). Test builds a bundle with --images in a folder named "img dir".
- Ran: cargo fmt, scripts/ci.sh crates/aegis-license, bundle_cli test, scripts/offline-bundle-test.sh: all pass.
NEXT: reviewer
