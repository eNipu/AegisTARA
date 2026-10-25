//! The AegisTARA gRPC API (`proto/aegis/v1/aegis.proto`).
//!
//! Shared by the server (which implements `aegis_server::Aegis`) and the
//! client (which calls `aegis_client::AegisClient`), so the two can never
//! drift apart. The `.proto` file is compiled by `protox`, a pure-Rust
//! compiler, so building needs no `protoc`.

#![forbid(unsafe_code)]
// Generated code.
#![allow(missing_docs, clippy::all, clippy::pedantic)]

tonic::include_proto!("aegis.v1");
