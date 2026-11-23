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

/// Metadata key carrying a machine-readable error code on failed calls
/// (for example `lock_invalid`, `stale_base`). Clients branch on it.
pub const ERROR_CODE_HEADER: &str = "x-aegis-error";
/// Metadata key carrying the project's current sequence number on
/// `stale_base` errors.
pub const CURRENT_SEQ_HEADER: &str = "x-aegis-current-seq";
/// Request metadata key carrying the client's `ENGINE_SEMANTICS_VERSION` (ADR 0075).
pub const ENGINE_SEMANTICS_HEADER: &str = "x-aegis-engine-semantics";
/// Request metadata key carrying the client's `PROTOCOL_VERSION` (ADR 0075).
pub const PROTOCOL_VERSION_HEADER: &str = "x-aegis-protocol-version";
