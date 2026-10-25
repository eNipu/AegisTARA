# ADR 0034: Release scaffolding

- Status: proposed (job 036-release-scaffolding)
- Date: 2026-10-25
- Scope: `deploy/`, `.dockerignore`, `docs/deployment.md`, `crates/aegis-license` (manifest), `crates/aegis-client` (`updates`)
- Rules: R10, D5 in `docs/engineering-standards.md`; `direction-v1.md` sections 2 and 10
- Builds on: `0032-license-tokens.md`

## Context

A release needs four things that are not features: an optimised build, a way
to run the server, a way for clients to learn that a newer version exists
without trusting the network, and a list of what to check before shipping.

## Decisions

### 1. Release profile

The root `Cargo.toml` already sets `lto = "fat"`, `codegen-units = 1` and
`strip = "symbols"` for shipped binaries (rule R10). Tests and benches use the
dev profile. Nothing else changes.

### 2. Container image

`deploy/Dockerfile` has two stages. The first builds only `aegis-server` with
`--locked` on the pinned toolchain. The second is a distroless image with no
shell or package manager, running as a non-root user. TLS is not in the image:
it ends at a proxy in front (direction section 1). There is no in-image health
check, because the image has no shell. A TCP probe on the port is enough.

### 3. Compose with PostgreSQL

`deploy/compose.yaml` runs the server and PostgreSQL 16. The database sits on
an internal network that only the server joins. The server's port is bound to
the host's loopback. The root file system is read only, all capabilities are
dropped, and `no-new-privileges` is set. No secret has a default: compose
refuses to start if a password or an identity setting is missing.

### 4. Signed update manifest

The manifest has the same shape as a license file (ADR 0032): one ASCII line
`AEGIS-UPDATE-1.<payload>.<signature>`, Ed25519, strict and bounded parsing.
It lists the release number and, per platform, an HTTPS address, a size, and a
SHA-256.

- **Own key list.** Updates are verified against `keys/update.keys`, not
  `keys/vendor.keys`. A leaked license key then cannot ship code. A signature
  also carries its own domain prefix and file tag, so a license can never pass
  as a manifest or the other way round (tested both ways).
- **No downgrade.** Only a release strictly newer than the running one is
  offered, so replaying an old signed manifest does nothing.
- **Mirror proof.** The installer is accepted only if its size and hash match
  the signed manifest, so it may come from any mirror.
- **No network code.** `aegis-client::updates` decides from manifest text.
  Fetching needs the HTTPS client (backlog 038).

### 5. Checklist

`docs/release-checklist.md` is the single list for a release. This job adds the
update key, the manifest signing step, and the image checks.

## Consequences

- The image and compose file are not exercised in CI. The authoring
  environment had no container daemon, so the Dockerfile was not built and the
  compose file was only checked with `docker compose config`. A CI job that
  builds the image is a follow-up for when a runner with a daemon is chosen.
- A real deployment still needs backlog 037 (TLS to PostgreSQL) and 038
  (`https://` identity provider and update fetching). `docs/deployment.md`
  says so.
- Two signing keys now exist (license and update). The checklist says where
  each lives. Neither may be in the repository.
