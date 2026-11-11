## Title

100-offline-bundle: one signed archive for air-gapped sites (NFR-06)

## Design Reference

- `docs/requirements.md` NFR-06 (signed installers, production signing keys, an offline bundle; jobs 099 and 100).
- `docs/design/workbench-breakdown.md` section 3: server-only jobs wait for a customer order. This job is planned so it is useful without a server customer too (the server images are optional in the bundle).
- `docs/decisions.md` D3 (customer-hosted server, license checked offline, no phone-home), D22 (standalone app with no server).
- `docs/adr/0034-release-scaffolding.md` and `docs/adr/0048-installer-download.md`: the signed update manifest (`AEGIS-UPDATE-1`, update key separate from the license key).
- Code: `crates/aegis-license/src/manifest.rs` (`Manifest`, `Artifact`, `MAX_MANIFEST_BYTES`), `src/envelope.rs`, `src/keys.rs` (`TrustedKeys`, `TrustedKeys::updates()`), `src/issuer.rs` (`Issuer`), `src/bin/aegis-license.rs` (vendor CLI, feature `issuer`).
- Deploy: `deploy/compose.yaml` (server uses `build:` today, PostgreSQL `postgres:16`), `deploy/.env.example`, `docs/deployment.md` ("Updates" says an offline site hands out installers and manifests itself), `docs/release-checklist.md`.

## Problem & Goal

A site without internet (a test lab, a defence or OEM network) cannot pull container images, cannot fetch the update manifest, and its security team will not import files it cannot authenticate. Today the vendor has no single deliverable for such a site: installers, server image and manifest travel separately, and nothing proves they belong together.

Goal: the vendor runs one script and gets `aegistara-<release>-offline.tar` (plus a `.sha256` next to it). Inside is one folder:

```
aegistara-<release>-offline/
  SHA256SUMS          sha256sum format, one line per file below, sorted by path
  SHA256SUMS.sig      raw 64-byte Ed25519 signature over SHA256SUMS, update key
  update.aegis        the release's signed update manifest (AEGIS-UPDATE-1)
  installers/<file>   desktop installers; each hash is listed in update.aegis
  images.tar.gz       optional: `docker save` of the server and postgres:16 images
```

The site verifies it with tools it already has and trusts (OpenSSL 3 and `sha256sum`), using the vendor's update public key handed over out of band as a PEM file. The vendor's release CI verifies it with `aegis-license verify-bundle`. Install steps are in `docs/deployment.md`.

Business value, per moving part (P7):
- The signature over `SHA256SUMS` lets a customer security team accept the delivery without running a vendor binary; this is what makes AegisTARA sellable into air-gapped automotive sites (NFR-06).
- Signing with the existing **update** key, not a new key, keeps one key that may "ship code" (ADR 0034); no new key to buy, store or rotate.
- Raw Ed25519 over the plain `SHA256SUMS` text (not our `AEGIS-*` envelope) is what OpenSSL can check. Domain separation holds because the signed text must parse as sums lines (64 hex digits, two spaces, a path), and the other two signed messages start with `aegis-update-v1\0` and `aegis-license-v1\0`, which never parse as sums lines.
- `update.aegis` in the bundle ties the installers to the signed release: `verify-bundle` refuses an installer whose hash is not in the manifest.
- Optional images: a consultancy or standalone customer (D22) gets an offline bundle with installers only, so the job has value before the first server customer.
- The outer tar is a wrapper only; its bytes need not be deterministic, because everything inside is covered by the signature.

## Acceptance Criteria

1. `aegis_license::bundle` (re-exported from `lib.rs`) has `parse_sums`, `render_sums`, `verify_sums_signature`, `sums_for_dir`, `verify_bundle_dir`, `public_key_pem`, the caps `MAX_SUMS_BYTES = 32 KiB`, `MAX_BUNDLE_FILES = 64`, `MAX_PATH_BYTES = 200`, and `Issuer::sign_bundle_sums`. Check: `cargo test --locked -p aegis-license --test bundle`.
2. `parse_sums(render_sums(x)) == x` for any valid entry list (proptest), and `parse_sums` never panics on arbitrary text (proptest). Check: same command.
3. `verify_bundle_dir` refuses, each with its own error variant and a test that only that guard refuses: a changed byte in any file, a missing listed file, an unlisted extra file, a symlink, a bad or wrong-key signature, an installer whose hash is not in `update.aegis`, an `update.aegis` signed by an untrusted key, an oversized `SHA256SUMS`. Check: same command.
4. `Issuer::sign_bundle_sums` refuses text that does not parse as sums, including text starting with `aegis-update-v1\0` and `aegis-license-v1\0`. Check: same command.
5. The vendor CLI has `sign-bundle DIR --key KEYFILE`, `verify-bundle DIR --keys KEYS_FILE` and `public-pem --keys KEYS_FILE --key-id ID`. Check: `cargo test --locked -p aegis-license --features issuer --test bundle_cli`.
6. `scripts/offline-bundle.sh` builds the tar and its `.sha256` from a signed manifest, installers and optional images. `scripts/offline-bundle-test.sh` builds one with a throwaway key, extracts it, verifies it with `openssl pkeyutl -verify -rawin`, `sha256sum -c --strict` (or `shasum -a 256 -c --strict`) and `verify-bundle`, and shows that a changed installer and an extra file are refused. Check: `scripts/offline-bundle-test.sh`. The CI `check` job runs it.
7. `deploy/compose.yaml` names the server image (`image: ${AEGIS_SERVER_IMAGE:-aegistara-server:local}`) so a loaded image runs with `up -d --no-build`. Check: `docker compose -f deploy/compose.yaml config -q` with the required variables set.
8. `docs/deployment.md` has an "Offline install (air-gapped sites)" section: verify, `docker load`, compose with `AEGIS_SERVER_IMAGE`, client installers, later updates. `docs/release-checklist.md` has the bundle steps. `docs/adr/0067-offline-bundle.md` records the format and the choices above. Check: `grep` in the last slice.

## Out of scope

- Building or signing the installers themselves (job 099, waiting for certificates).
- A customer-side verifier binary: the customer uses OpenSSL 3 and `sha256sum`.
- A cargo-fuzz target: `SHA256SUMS` is the GNU text format, parsed only by vendor tooling and bounded by `MAX_SUMS_BYTES`; proptest covers arbitrary input, as for the manifest format.
- Byte-deterministic outer tar, compression of the outer tar, pinning `postgres:16` by digest, an internal update mirror.
- Any change to the manifest, license or envelope formats.
