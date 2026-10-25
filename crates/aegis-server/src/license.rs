//! The license gate (ADR 0032).
//!
//! The server starts only with a valid license. While it runs, the gate
//! decides one thing: may this user take the edit lease? Reading and
//! watching are never gated, so a lapsed license never locks people out of
//! their own data. The clock is a function pointer, like the audit clock, so
//! tests can move time.

use std::sync::{Arc, Mutex};

use crate::pb;
use aegis_license::{Admission, License, LicenseError, LicenseState, SeatLedger, TrustedKeys};

use crate::error::ServerError;

/// Why the server cannot start on a license file.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum LoadError {
    /// The file is not a valid, correctly signed license.
    #[error("{0}")]
    Invalid(#[from] LicenseError),
    /// The license starts in the future.
    #[error("the license is not valid yet")]
    NotYetValid,
    /// The license and its grace period are over.
    #[error("the license has expired")]
    Expired,
}

/// Seconds since the Unix epoch.
pub type ClockSecs = Arc<dyn Fn() -> u64 + Send + Sync>;

/// The system clock as a [`ClockSecs`].
#[must_use]
pub fn system_clock_arc() -> ClockSecs {
    Arc::new(system_clock_secs)
}

/// Checks the license on the paths that need it.
pub struct LicenseGate {
    license: License,
    seats: Mutex<SeatLedger>,
    clock_secs: ClockSecs,
}

impl LicenseGate {
    /// A gate over a verified license. `clock_secs` is seconds since the
    /// Unix epoch.
    #[must_use]
    pub fn new(license: License, clock_secs: ClockSecs) -> Self {
        let seats = Mutex::new(SeatLedger::new(license.seats));
        LicenseGate {
            license,
            seats,
            clock_secs,
        }
    }

    /// Verifies a license file and builds the gate, for server start.
    ///
    /// A license in its grace period loads (the caller should warn); one
    /// that is over or not yet valid does not.
    ///
    /// # Errors
    ///
    /// [`LoadError`] naming why the server must not start.
    pub fn load(file: &str, keys: &TrustedKeys, clock_secs: ClockSecs) -> Result<Self, LoadError> {
        let license = License::verify(file, keys)?;
        let now = clock_secs();
        match license.state(now) {
            LicenseState::NotYetValid => Err(LoadError::NotYetValid),
            LicenseState::Expired => Err(LoadError::Expired),
            LicenseState::Active { .. } | LicenseState::Grace { .. } => {
                Ok(LicenseGate::new(license, clock_secs))
            }
        }
    }

    /// The license.
    #[must_use]
    pub fn license(&self) -> &License {
        &self.license
    }

    /// Where the license stands now.
    #[must_use]
    pub fn state(&self) -> LicenseState {
        self.license.state((self.clock_secs)())
    }

    /// May `subject` start editing? Takes a seat if they do not have one.
    /// Called before the lease is granted. Time O(1) amortised.
    ///
    /// # Errors
    ///
    /// [`ServerError::LicenseExpired`] when the license is over (or not yet
    /// valid), [`ServerError::LicenseSeats`] when other users hold all seats.
    pub fn admit_editor(&self, subject: &str) -> Result<(), ServerError> {
        let now = (self.clock_secs)();
        if !self.license.state(now).allows_editing() {
            return Err(ServerError::LicenseExpired);
        }
        // A poisoned lock means another thread panicked inside `admit`; the
        // ledger is plain data, so carrying on is safe.
        let mut seats = self
            .seats
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match seats.admit(subject, now) {
            Admission::Admitted => Ok(()),
            Admission::Full => Err(ServerError::LicenseSeats),
        }
    }

    /// The summary users and admins see in `GetServerInfo`.
    #[must_use]
    pub fn info(&self) -> pb::LicenseInfo {
        let now = (self.clock_secs)();
        let state = match self.license.state(now) {
            LicenseState::Active { .. } => pb::license_info::State::Active,
            LicenseState::Grace { .. } => pb::license_info::State::Grace,
            LicenseState::NotYetValid | LicenseState::Expired => pb::license_info::State::Expired,
        };
        let used = self
            .seats
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .used(now);
        pb::LicenseInfo {
            state: state.into(),
            customer: self.license.customer.clone(),
            seats: self.license.seats,
            seats_used: u32::try_from(used).unwrap_or(u32::MAX),
            expires_at: self.license.expires_at,
            grace_ends_at: self
                .license
                .expires_at
                .saturating_add(u64::from(self.license.grace_days) * aegis_license::DAY),
            features: self.license.features.clone(),
        }
    }
}

/// The system clock in seconds since the Unix epoch.
#[must_use]
pub fn system_clock_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}
