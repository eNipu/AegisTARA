//! Login for AegisTARA: OIDC with PKCE, DPoP, token validation.
//!
//! No part of this crate talks to a particular identity provider or to the
//! gRPC layer. See `docs/adr/0028-login.md`.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod b64;
pub mod clock;
pub mod dpop;
pub mod error;
pub mod http;
pub mod jose;
pub mod jwks;
pub mod layer;
pub mod loopback;
pub mod oidc;
pub mod pkce;
pub mod session;
pub mod store;
#[cfg(any(test, feature = "test-support"))]
pub mod testing;
pub mod validate;

pub use clock::{Clock, system_clock};
pub use dpop::{DeviceKey, DpopVerifier};
pub use error::AuthError;
pub use session::{Credentials, Session};
