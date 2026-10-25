//! The license record, its file format, and the date rules.
//!
//! File format (one line, ASCII):
//!
//! ```text
//! AEGIS-LICENSE-1.<payload>.<signature>
//! ```
//!
//! `payload` is the JSON of [`License`], `signature` is the Ed25519
//! signature over `"aegis-license-v1\0" + payload bytes`, both in base64url
//! without padding. The signature covers the exact bytes, so there is no
//! canonical-JSON problem, and the prefix stops a signature made for another
//! purpose from being replayed as a license.

use serde::{Deserialize, Serialize};

use crate::envelope;
use crate::error::LicenseError;
use crate::keys::TrustedKeys;

/// One day in seconds.
pub const DAY: u64 = 86_400;
/// A license may not be used before its start by more than this, so a clock
/// a few hours off does not lock out a new customer.
pub const SKEW: u64 = DAY;
/// Largest license file accepted (R3).
pub const MAX_FILE_BYTES: usize = 16 * 1024;
/// Most feature flags in one license.
pub const MAX_FEATURES: usize = 64;
/// Longest grace period a license may grant.
pub const MAX_GRACE_DAYS: u32 = 90;
/// Most seats a license may grant.
pub const MAX_SEATS: u32 = 1_000_000;
/// The start of every license file.
pub const FILE_PREFIX: &str = "AEGIS-LICENSE-1";

/// What is put in front of the payload before signing.
pub(crate) const DOMAIN: &[u8] = b"aegis-license-v1\0";
const FORMAT_VERSION: u32 = 1;
const MAX_CUSTOMER_CHARS: usize = 200;
const MAX_ID: usize = 64;
const MAX_KID: usize = 32;
const MAX_FEATURE: usize = 64;

#[cfg(any(feature = "issuer", test))]
/// What a license grants. Times are seconds since the Unix epoch.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct License {
    /// Format version. Always 1 today.
    pub version: u32,
    /// The license's own ID, for support and audit. Letters, digits, `-`, `_`, `.`; up to 64.
    pub id: String,
    /// The signing key's ID.
    pub kid: String,
    /// The licensed organisation.
    pub customer: String,
    /// Named users who may edit within the seat window.
    pub seats: u32,
    /// When the license starts.
    pub issued_at: u64,
    /// When the paid period ends.
    pub expires_at: u64,
    /// Days after `expires_at` in which everything still works, with a
    /// warning, so engineering work never stops suddenly.
    pub grace_days: u32,
    /// Enabled feature flags, sorted, without duplicates.
    pub features: Vec<String>,
}

/// Where a license stands on a given day.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LicenseState {
    /// Before the start (more than the allowed skew).
    NotYetValid,
    /// In the paid period.
    Active {
        /// Seconds until the paid period ends.
        expires_in: u64,
    },
    /// Past the paid period, inside the grace period.
    Grace {
        /// Seconds until the grace period ends.
        ends_in: u64,
    },
    /// Over. Reading stays possible; editing and starting do not.
    Expired,
}

impl LicenseState {
    /// Whether editing is allowed (active or in grace).
    #[must_use]
    pub fn allows_editing(self) -> bool {
        matches!(
            self,
            LicenseState::Active { .. } | LicenseState::Grace { .. }
        )
    }
}

impl License {
    /// Parses a license file and checks its signature against `keys`.
    /// Time O(n) in the file size, which is capped.
    ///
    /// This does not look at the date. Use [`state`](Self::state) for that,
    /// so the date rule is one function with one clock.
    ///
    /// # Errors
    ///
    /// A [`LicenseError`] naming the first thing wrong.
    pub fn verify(file: &str, keys: &TrustedKeys) -> Result<License, LicenseError> {
        let (payload, signature) = envelope::split(file, FILE_PREFIX, MAX_FILE_BYTES)?;
        // Read the record first only to learn which key signed it. Nothing
        // from it is trusted until the signature checks out.
        let license: License =
            serde_json::from_slice(&payload).map_err(|_| LicenseError::Malformed)?;
        if license.version != FORMAT_VERSION {
            return Err(LicenseError::UnsupportedVersion(license.version));
        }
        let key = keys.get(&license.kid).ok_or(LicenseError::UnknownKey)?;
        envelope::verify(key, DOMAIN, &payload, &signature)?;
        license.check()?;
        Ok(license)
    }

    /// Checks the rules of the format. Called after the signature, and by the
    /// issuer before signing, so a bad record is never produced.
    ///
    /// # Errors
    ///
    /// [`LicenseError::Invalid`] naming the rule.
    pub fn check(&self) -> Result<(), LicenseError> {
        let invalid = LicenseError::Invalid;
        if self.version != FORMAT_VERSION {
            return Err(LicenseError::UnsupportedVersion(self.version));
        }
        if !simple_token(&self.id, MAX_ID) {
            return Err(invalid("id"));
        }
        if !simple_token(&self.kid, MAX_KID) {
            return Err(invalid("key id"));
        }
        let chars = self.customer.chars().count();
        if chars == 0
            || chars > MAX_CUSTOMER_CHARS
            || self.customer.chars().any(char::is_control)
            || self.customer.trim() != self.customer
        {
            return Err(invalid("customer"));
        }
        if self.seats == 0 || self.seats > MAX_SEATS {
            return Err(invalid("seats"));
        }
        if self.expires_at <= self.issued_at {
            return Err(invalid("the license must end after it starts"));
        }
        if self.grace_days > MAX_GRACE_DAYS {
            return Err(invalid("grace days"));
        }
        if self.features.len() > MAX_FEATURES {
            return Err(invalid("too many features"));
        }
        if self.features.iter().any(|f| !simple_token(f, MAX_FEATURE)) {
            return Err(invalid("a feature name"));
        }
        if self.features.windows(2).any(|w| w[0] >= w[1]) {
            return Err(invalid("features must be sorted and unique"));
        }
        Ok(())
    }

    /// Where the license stands at `now` (seconds since the Unix epoch).
    /// Time O(1).
    #[must_use]
    pub fn state(&self, now: u64) -> LicenseState {
        if now.saturating_add(SKEW) < self.issued_at {
            return LicenseState::NotYetValid;
        }
        if now < self.expires_at {
            return LicenseState::Active {
                expires_in: self.expires_at - now,
            };
        }
        let grace_end = self
            .expires_at
            .saturating_add(u64::from(self.grace_days) * DAY);
        if now < grace_end {
            return LicenseState::Grace {
                ends_in: grace_end - now,
            };
        }
        LicenseState::Expired
    }

    /// Whether a feature flag is enabled.
    #[must_use]
    pub fn has_feature(&self, name: &str) -> bool {
        self.features
            .binary_search_by(|f| f.as_str().cmp(name))
            .is_ok()
    }
}

/// Letters, digits, `-`, `_` and `.`; non-empty and bounded. Used for IDs and
/// feature names, so they are safe to log and to put in a file name.
fn simple_token(text: &str, max: usize) -> bool {
    !text.is_empty()
        && text.len() <= max
        && text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

impl License {
    /// The JSON that is signed.
    #[cfg(feature = "issuer")]
    pub(crate) fn payload(&self) -> Result<Vec<u8>, LicenseError> {
        serde_json::to_vec(self).map_err(|_| LicenseError::Signing)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> License {
        License {
            version: 1,
            id: "lic-1".into(),
            kid: "k1".into(),
            customer: "Example Motors".into(),
            seats: 10,
            issued_at: 1_000_000,
            expires_at: 2_000_000,
            grace_days: 30,
            features: vec!["audit".into(), "export.pdf".into()],
        }
    }

    #[test]
    fn the_date_rule_has_exact_edges() {
        let l = sample();
        assert_eq!(l.state(l.issued_at - SKEW - 1), LicenseState::NotYetValid);
        assert!(matches!(
            l.state(l.issued_at - SKEW),
            LicenseState::Active { .. }
        ));
        assert_eq!(
            l.state(l.expires_at - 1),
            LicenseState::Active { expires_in: 1 }
        );
        assert_eq!(
            l.state(l.expires_at),
            LicenseState::Grace { ends_in: 30 * DAY }
        );
        let end = l.expires_at + 30 * DAY;
        assert_eq!(l.state(end - 1), LicenseState::Grace { ends_in: 1 });
        assert_eq!(l.state(end), LicenseState::Expired);
        assert_eq!(l.state(u64::MAX), LicenseState::Expired);
    }

    #[test]
    fn a_license_with_no_grace_ends_at_expiry() {
        let mut l = sample();
        l.grace_days = 0;
        assert_eq!(l.state(l.expires_at), LicenseState::Expired);
    }

    #[test]
    fn the_rules_reject_bad_records() {
        let ok = sample();
        assert!(ok.check().is_ok());
        let mut l = sample();
        l.seats = 0;
        assert!(l.check().is_err());
        let mut l = sample();
        l.seats = MAX_SEATS + 1;
        assert!(l.check().is_err());
        let mut l = sample();
        l.expires_at = l.issued_at;
        assert!(l.check().is_err());
        let mut l = sample();
        l.grace_days = MAX_GRACE_DAYS + 1;
        assert!(l.check().is_err());
        let mut l = sample();
        l.customer = " padded".into();
        assert!(l.check().is_err());
        let mut l = sample();
        l.customer = "line\nbreak".into();
        assert!(l.check().is_err());
        let mut l = sample();
        l.features = vec!["b".into(), "a".into()];
        assert!(l.check().is_err());
        let mut l = sample();
        l.features = vec!["a".into(), "a".into()];
        assert!(l.check().is_err());
        let mut l = sample();
        l.features = (0..=MAX_FEATURES).map(|i| format!("f{i:03}")).collect();
        assert!(l.check().is_err());
        let mut l = sample();
        l.id = "has space".into();
        assert!(l.check().is_err());
    }

    #[test]
    fn features_are_found_by_name() {
        let l = sample();
        assert!(l.has_feature("audit"));
        assert!(l.has_feature("export.pdf"));
        assert!(!l.has_feature("export"));
    }
}
