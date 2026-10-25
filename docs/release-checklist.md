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

## License keys

- [ ] `crates/aegis-license/keys/vendor.keys` holds the production public key.
      An empty file means the build accepts no license.
- [ ] The private key is offline and not in any repository or CI secret that
      builds pull requests.
- [ ] `crates/aegis-license/keys/update.keys` holds the production **update**
      public key. It is a different key from the license key.
- [ ] The update private key is offline too, and held by fewer people than
      the license key.
- [ ] The server is built without the `dev-license` feature
      (`cargo tree -p aegis-server -e features | grep dev-license` prints nothing).
- [ ] `aegis-license inspect` reads a license made with the production key
      against the shipped `vendor.keys`.

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
- [ ] `docker build -f deploy/Dockerfile .` succeeds from a clean checkout,
      the container runs as a non-root user, and a TCP probe of port 50051
      succeeds. (The image was not built when it was written.)
- [ ] `docker compose -f deploy/compose.yaml config` passes with a filled
      `deploy/.env`, and a fresh `up` creates the schema.

## Signing and publishing

- [ ] The update manifest is signed with the update key
      (`aegis-license sign-update`), and `aegis-license inspect-update` reads
      it back against `update.keys`.
- [ ] A test client on the previous release is offered this release, and a
      test client on this release is not offered it again. The installer
      passes `verify_download`.
- [ ] SHA-256 sums are published next to the artifacts.
- [ ] The release notes list the engine version (`ENGINE_VERSION`) and the
      oplog format version.
