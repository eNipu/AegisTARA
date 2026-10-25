# ADR 0036: HTTPS clients and gRPC-Web

- Status: accepted (jobs 037, 038, 040)
- Date: 2026-10-25
- Scope: `crates/aegis-auth/src/tls.rs`, `crates/aegis-client/src/transport.rs`, `crates/aegis-server/src/{server,health}.rs`, `store/postgres.rs`
- Rules: R6, D5, D11 in `docs/engineering-standards.md`; `direction-v1.md` section 8

## Context

The app must reach a server behind HTTPS proxies. Railway's edge and many
corporate proxies forward HTTP/1.1 only, and plain gRPC needs HTTP/2 end to
end. The OIDC client and the database connection also need TLS.

## Decisions

### 1. rustls with ring and the OS trust store

One TLS setup for every client in the product (`aegis_auth::tls`): rustls,
the ring provider, roots from `rustls-native-certs`. Company proxies that
re-sign TLS work when their CA is in the OS store. All crates are Apache-2.0,
MIT or ISC (D5); `cargo-deny` passes. No OpenSSL.

### 2. gRPC-Web next to gRPC

The server accepts HTTP/1.1 and wraps the services in `tonic_web::GrpcWebLayer`.
The client picks `grpc-web` for `https://` addresses and `grpc` for others,
and the user can override it. Server streams (`OpenProject`, `Watch`) work
over gRPC-Web. Client streams are not used, so nothing is lost.

### 3. Heartbeats on `Watch`

Proxies close idle streams, some after less than a minute. The server sends
a `Heartbeat` event every 20 s. The client treats a stream that is silent for
60 s (`SessionConfig::watch_silence`) as broken and restarts it from the last
confirmed op, so a connection that a proxy dropped without closing does not
freeze live updates.

### 4. Health probe outside gRPC

`GET /healthz` answers `200` before any other layer, without a login, so
platforms can probe it. It says nothing about the data.

### 5. Database TLS modes

`AEGIS_DB_TLS` is `off`, `encrypt` (any certificate, like libpq `require`)
or `verify` (checked against OS roots, like libpq `verify-full`). `encrypt`
exists for hosted databases with self-signed certificates; it stops passive
reading but not an active attacker, and the docs say so.

## Consequences

- Two transport code paths in the client (`Transport` enum, D11). Both run in
  the test suite.
- gRPC-Web adds a little framing overhead. It does not matter at TARA sizes.
