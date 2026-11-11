# ADR 0067: Offline bundle for air-gapped sites

- Status: accepted (job 100-offline-bundle)
- Date: 2026-11-11
- Scope: crates/aegis-license/src/bundle.rs, crates/aegis-license/src/bin/aegis-license.rs, scripts/offline-bundle.sh, scripts/offline-bundle-test.sh, deploy/compose.yaml
- Rules: R3 (bounded parsing) in `docs/engineering-standards.md`, P7
- Requirement: NFR-06
- Builds on: `0034-release-scaffolding.md`, `0048-installer-download.md`

## Context

Air-gapped sites cannot pull server images or fetch the update manifest over
the internet. Their security teams accept only files they can authenticate
with their own tools, so the vendor has to ship everything an update needs in
one bundle that the customer checks without trusting a vendor binary.

## Decisions

1. The bundle is one tar, `aegistara-<release>-offline.tar`, with a
   `.tar.sha256` beside it. Inside is one folder, `aegistara-<release>-offline/`,
   with `SHA256SUMS`, `SHA256SUMS.sig`, `update.aegis`, `installers/<files>` and
   optionally `images.tar.gz`.
2. `SHA256SUMS` is signed as raw Ed25519 over its plain text, not with the
   `AEGIS-*` envelope. OpenSSL 3 checks it with `openssl pkeyutl -verify`.
   Domain separation: the signed text must parse as sums lines (64 hex digits,
   two spaces, a path). Manifest and license signatures cover messages that
   start with `aegis-update-v1\0` and `aegis-license-v1\0`, which never parse as
   sums lines, and the issuer refuses to sign anything that does not parse.
3. The signing key is the update key, the same key that signs update manifests
   (ADR 0034). No new key is bought, stored or rotated.
4. `update.aegis` sits inside the bundle, and each installer is bound to it by
   its SHA-256 hash.
5. `images.tar.gz` is optional. A desktop-only customer gets installers only.
6. The outer tar is a wrapper only. Its bytes are not deterministic; the signed
   contents are.
7. Caps: `SHA256SUMS` at most 32 KiB, at most 64 files, paths at most 200 bytes
   and 2 levels deep.
8. There is no customer verifier binary. The customer uses OpenSSL and
   `sha256sum`. The vendor's release CI runs `aegis-license verify-bundle`.
   There is no cargo-fuzz target in this job. The caps above and the tests
   in `crates/aegis-license/tests/bundle.rs` bound the parser.

## Alternatives

- A new bundle key: one more key to guard, for no gain over the update key.
- Signing the tar itself: the customer could not check single files after
  extraction, and the signing input would be multiple gigabytes.
- Our `AEGIS-BUNDLE-1` envelope: OpenSSL cannot check it.
- minisign or GPG: one more tool for the customer to install and trust.

## Consequences

- Customers need OpenSSL 3 or later. LibreSSL, the default on macOS, cannot
  check the signature.
- The public key PEM must be handed to each offline customer out of band.
  The PEM inside a delivery is never the trust anchor.
- The bundle does not build or sign installers. Job 099 does that.

## Tests

- `crates/aegis-license/tests/bundle.rs`
- `crates/aegis-license/tests/bundle_cli.rs`
- `scripts/offline-bundle-test.sh`, which runs in the CI `check` job. It builds,
  verifies and tampers a bundle with a throwaway key.
