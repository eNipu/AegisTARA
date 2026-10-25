//! AegisTARA server: a gRPC API over the `aegis-model` engine.
//!
//! The server is the source of truth for projects. A client edits by
//! sending typed ops; the server checks the edit lease, decodes each op with
//! the capped decoder, applies it through `Project::apply` (the same engine
//! the client embeds), stores it, and streams it to observers. Design and
//! numbers: `docs/adr/0026-server-skeleton.md`.
//!
//! Layers, bottom up:
//!
//! - [`store`]: persistence behind the [`ProjectStore`] trait.
//! - [`lease`]: the edit lease state machine (decision D4).
//! - [`actor`]: one task per open project (rule C1).
//! - [`registry`]: loads and finds actors.
//! - [`service`]: the tonic service; maps requests to actors and errors to
//!   status codes.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod actor;
pub mod auth;
pub mod auth_layer;
pub mod config;
pub mod error;
pub mod ids;
pub mod lease;
pub mod license;
pub mod methodologies;
pub mod registry;
pub mod server;
pub mod service;
pub mod store;

pub use auth::Principal;
pub use config::ServerConfig;
pub use error::ServerError;
pub use ids::ProjectId;
pub use license::LicenseGate;
pub use registry::Registry;
pub use service::AegisService;
pub use store::{MemoryStore, PostgresStore, ProjectStore};

/// The generated protobuf messages and gRPC service (the `aegis-proto` crate).
pub use aegis_proto as pb;
