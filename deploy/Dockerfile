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
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
# Only the server and what it depends on are compiled. The workspace still
# lists the shell, which needs a UI toolkit this image does not carry; cargo
# does not build members that were not asked for.
RUN cargo build --release --locked -p aegis-server --bin aegis-server

FROM gcr.io/distroless/cc-debian12:nonroot
COPY --from=build /src/target/release/aegis-server /usr/local/bin/aegis-server
# Plain HTTP/2 inside the container. TLS ends at the load balancer or reverse
# proxy in front of it (direction section 1).
ENV AEGIS_LISTEN=0.0.0.0:50051
EXPOSE 50051
USER nonroot
# There is no shell, so no in-image HEALTHCHECK. Probe the TCP port instead.
ENTRYPOINT ["/usr/local/bin/aegis-server"]
