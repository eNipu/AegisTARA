# ADR 0040: CI builds and starts the server image

- Status: accepted (job 046-image-ci)
- Date: 2026-10-25
- Scope: `.github/workflows/ci.yml` (job `image`), `deploy/verify-image.sh`
- Rules: Q7 in `docs/engineering-standards.md`; gap G12
- Builds on: `0034-release-scaffolding.md`

## Context

The server image was built by hand and on Railway. Nothing in CI proved
that `deploy/Dockerfile` still builds, that the shipped image has no
`dev-license` feature, or that the binary starts in the distroless base.

## Decisions

### 1. One script, run by CI and by hand

`deploy/verify-image.sh` does the work and the CI job only calls it. A
developer with Docker runs the same file. Plain `docker build` is used, not a
build action: every action in `ci.yml` is pinned to a full commit SHA, and
the script needs no cache or registry.

### 2. Two images, three checks

- The **customer image** (no cargo features) must run as `nonroot` and must
  stop at start with `AEGIS_LICENSE_FILE is required`, even when the test
  variable `AEGIS_DEV_LICENSE=1` is set. That proves the `dev-license`
  feature is not compiled in, which `release-checklist.md` demands.
- The **test image** (`CARGO_FEATURES=dev-license`, as on Railway) must
  answer `GET /healthz` with 200 and answer `GetLoginOptions` over gRPC-Web
  with the server name and `grpc-status:0`. That proves the binary starts in
  the distroless base and serves both transports.

The image has no shell and no curl, so the checks run from the host through a
published loopback port.

### 3. Cost is accepted

Two release builds with LTO take a while (the job has a 60 minute limit) and
run on every push. A layer cache would save time but needs a pinned build
action; add it when the job becomes the slowest one.

## Consequences

- The script was checked against local server binaries (the license
  refusal, `/healthz`, the gRPC-Web reply). The `docker build` and `docker
  run` lines have not been run by the author, because the build machine had
  no Docker daemon. The first CI run is their first test.
- The port is `18051` (`VERIFY_PORT` changes it).
