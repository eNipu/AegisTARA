//! AegisTARA license files (direction section 12, ADR 0032).
//!
//! A license is a small signed record: customer, seats, validity, grace
//! period, features. The vendor signs it with an Ed25519 key. The server
//! checks it offline against public keys compiled into the build, so air
//! gapped sites work and nothing phones home.
//!
//! - [`TrustedKeys`] holds the public keys a build accepts.
//! - [`License::verify`] parses and checks a license file. It is strict,
//!   bounded (R3), and takes the clock as an argument, so it is a pure
//!   function that tests can drive to any date.
//! - [`License::state`] says whether the license is active, in its grace
//!   period, or over.
//! - [`SeatLedger`] counts active named users against the seat limit.
//! - `Issuer` (feature `issuer`) signs. It is not in server builds.

mod error;
mod keys;
mod license;
mod seats;

#[cfg(feature = "issuer")]
mod issuer;

pub use error::LicenseError;
pub use keys::{MAX_TRUSTED_KEYS, TrustedKeys};
pub use license::{
    DAY, FILE_PREFIX, License, LicenseState, MAX_FEATURES, MAX_FILE_BYTES, MAX_GRACE_DAYS,
    MAX_SEATS, SKEW,
};
pub use seats::{Admission, SeatLedger};

#[cfg(feature = "issuer")]
pub use issuer::Issuer;
