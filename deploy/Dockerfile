# AegisTARA server image (job 036, docs/adr/0034-release-scaffolding.md).
#
# Build from the repository root:
#   docker build -f deploy/Dockerfile -t aegistara-server .
#
# Two stages: the toolchain stays in the first, the second holds only the
# binary, on a base with no shell and no package manager, running as a
# non-root user. The binary is built with the workspace release profile
# (LTO, one codegen unit, stripped) and `--locked`, so the image contains
# exactly the dependency set in Cargo.lock.
#
# The toolchain version matches `rust-version` in the root Cargo.toml.

FROM rust:1.99.1-slim-bookworm AS build
# Extra cargo features. Empty for a customer image. A TEST server may set
# `dev-license` (a server without a vendor license, see docs/railway.md);
# docs/release-checklist.md forbids it in a shipped image. Railway passes
# a service variable of the same name as this build argument.
ARG CARGO_FEATURES=""
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
# Only the server and what it depends on are compiled. The workspace still
# lists the shell, which needs a UI toolkit this image does not carry; cargo
# does not build members that were not asked for.
RUN cargo build --release --locked -p aegis-server --bin aegis-server --features "$CARGO_FEATURES"

FROM gcr.io/distroless/cc-debian12:nonroot
COPY --from=build /src/target/release/aegis-server /usr/local/bin/aegis-server
# Plain HTTP/2 and HTTP/1.1 (gRPC-Web) inside the container. TLS ends at the
# load balancer or reverse proxy in front of it (direction section 1). The
# server listens on 0.0.0.0:$PORT; hosting platforms such as Railway set
# PORT, and AEGIS_LISTEN overrides it.
ENV PORT=50051
EXPOSE 50051
USER nonroot
# There is no shell, so no in-image HEALTHCHECK. Probe GET /healthz (200)
# from the platform instead.
ENTRYPOINT ["/usr/local/bin/aegis-server"]
