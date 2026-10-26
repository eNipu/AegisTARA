# ADR 0042: Fetching the update manifest

- Status: accepted (job 048-update-manifest-fetch)
- Date: 2026-10-26
- Scope: `crates/aegis-client/src/{updates,session,settings,commands}.rs`, `crates/aegis-auth/src/{tls,http}.rs`, `crates/aegis-shell` (Help menu)
- Rules: R3, R6 in `docs/engineering-standards.md`; decision D5
- Builds on: `0034-release-scaffolding.md`, `0036-https-and-grpc-web.md`

## Context

ADR 0034 left the manifest check without network code, waiting for the HTTPS
client. That client exists (rustls, ring, OS trust store).

## Decisions

### 1. Transport is not trust

The fetch uses the shared client and its limits, no redirects, and accepts
`https://` only (a loopback `http://` address is allowed for tests and lab
setups). What the server sends is still checked by `UpdateChecker::check`:
a signature from the update key list, no downgrade. A test serves a valid TLS
connection with a manifest signed by the wrong key and expects a refusal.

### 2. Own limits on the body

The manifest is at most 32 KiB (`MAX_MANIFEST_BYTES`), so a larger answer is
refused before parsing, although the client already caps bodies at 1 MiB.

### 3. A client with chosen roots

`HyperClient::with_tls_config` and `tls::config_with_roots` build a client
that trusts exactly the given roots. The tests use them with a throwaway
certificate authority (`tests/fixtures/tls`, test keys made with `openssl`).
They also fit a customer with a private certificate authority, which a later
job may expose as a setting.

### 4. No address, no check

The address is `update_url` in the settings file, overridden by
`AEGIS_UPDATE_URL`. It is empty by default. An air gapped site leaves it
empty, and the Help menu says so. No vendor address is compiled in: the
release owner decides it with the update key (`release-checklist.md`).

### 5. The answer is a notice

"Check for updates" in the Help menu runs in the background and shows a
notice: up to date, a release with its size and download address, or the
reason it failed. Downloading and installing is not part of this job: an
installer can be tens of megabytes, which needs a streaming download beyond
the 1 MiB client limit (rule R3), and then an installer step per platform.

## Consequences

- A user still downloads the installer by hand. The signed size and hash are
  ready for the later download step (`UpdateOffer::verify_download`).
- The check runs only when the user asks. There is no timer and no
  telemetry.
