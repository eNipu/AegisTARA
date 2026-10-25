# Release checklist

Run this list for every release build. Tick each line in the release notes.

## Licenses

- [ ] `scripts/deny.sh` passes (licenses, bans, advisories, sources).
- [ ] `tools/deny-fixtures/run.sh` passes (the rules still reject what they should).
- [ ] The About window shows the Slint notice (`AboutSlint`). It is the
      Royalty-free License 2.0 condition (D7, D9).
- [ ] The download page shows the "Made with Slint" badge with a link to
      https://slint.dev. This is the second condition. It cannot be checked
      from the repo, so check the live page.
- [ ] The owner has confirmed D4 and D7. If D7 was rejected, `aegis-shell`
      and its `deny.toml` exceptions are gone.

## Build

- [ ] Built from a clean checkout of a tag, with the pinned toolchain
      (`rust-toolchain.toml`) and `--locked`.
- [ ] `cargo build --release --workspace --locked` uses the workspace release
      profile (LTO, one codegen unit, stripped).
- [ ] `scripts/ci.sh` passes with `AEGIS_TEST_DATABASE_URL` set, so the
      PostgreSQL tests run and are not skipped.
- [ ] `cargo bench -p aegis-canvas` shows no budget regression.

## Server

- [ ] Migrations apply to an empty database and to the previous release's
      database.
- [ ] `VerifyAudit` returns a valid chain on a database used by the previous
      release.
- [ ] The server starts only with TLS terminated in front of it, or on
      loopback (backlog 037 tracks TLS to PostgreSQL).
- [ ] The container image runs as a non-root user and has a health check.

## Signing and publishing

- [ ] Artifacts are signed. The update manifest is signed with the release
      key, and the client refuses an unsigned or older manifest.
- [ ] SHA-256 sums are published next to the artifacts.
- [ ] The release notes list the engine version (`ENGINE_VERSION`) and the
      oplog format version.
