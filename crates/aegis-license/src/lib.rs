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
//! - [`Manifest`] is the signed update manifest the client checks before it
//!   offers an update (ADR 0034). It uses its own key list.
//! - `Issuer` (feature `issuer`) signs both. It is not in server or client
//!   builds.

mod envelope;
mod error;
mod keys;
mod license;
mod manifest;
mod seats;

#[cfg(feature = "issuer")]
mod issuer;

pub use error::LicenseError;
pub use keys::{MAX_TRUSTED_KEYS, TrustedKeys};
pub use license::{
    DAY, FILE_PREFIX, License, LicenseState, MAX_FEATURES, MAX_FILE_BYTES, MAX_GRACE_DAYS,
    MAX_SEATS, SKEW,
};
pub use manifest::{
    Artifact, MANIFEST_PREFIX, MAX_ARTIFACT_BYTES, MAX_ARTIFACTS, MAX_MANIFEST_BYTES, Manifest,
    ManifestError, Offer, Version,
};
pub use seats::{Admission, SeatLedger};

/// How long a seat stays taken after its user's last edit session, in
/// seconds (30 days).
pub const SEAT_WINDOW_SECS: u64 = SeatLedger::WINDOW_DAYS * DAY;

#[cfg(feature = "issuer")]
pub use issuer::Issuer;
