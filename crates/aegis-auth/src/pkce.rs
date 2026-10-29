//! PKCE (RFC 7636), method `S256` only.
//!
//! The verifier is 32 random bytes (256 bits) as 43 base64url characters,
//! the shortest length the RFC allows and far above its 128-bit guidance.

use ring::digest::{SHA256, digest};
use ring::rand::{SecureRandom, SystemRandom};

use crate::b64;
use crate::error::AuthError;

/// Overwrites `bytes` with zeroes using [`std::hint::black_box`] so the
/// compiler does not elide the wipe (SEC-08).
pub(crate) fn zeroize_bytes(bytes: &mut [u8]) {
    bytes.fill(0);
    std::hint::black_box(bytes);
}

/// Overwrites the heap buffer of `s` with zeroes in place before dropping it
/// (SEC-08).
pub(crate) fn zeroize_string(s: &mut String) {
    let mut bytes = std::mem::take(s).into_bytes();
    zeroize_bytes(&mut bytes);
}

/// A code verifier and its challenge.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pkce {
    verifier: String,
    challenge: String,
}

impl Drop for Pkce {
    fn drop(&mut self) {
        zeroize_string(&mut self.verifier);
    }
}

impl Pkce {
    /// A fresh pair from the OS random generator.
    ///
    /// # Errors
    ///
    /// [`AuthError::Store`] never; [`AuthError::Protocol`] if the OS
    /// generator fails (it does not, in practice).
    pub fn generate() -> Result<Self, AuthError> {
        let mut raw = [0u8; 32];
        SystemRandom::new()
            .fill(&mut raw)
            .map_err(|_| AuthError::Protocol)?;
        let encoded = b64::encode(raw);
        zeroize_bytes(&mut raw);
        Ok(Self::from_verifier(encoded))
    }

    /// The pair for a given verifier (tests; the RFC's appendix B vector).
    #[must_use]
    pub fn from_verifier(verifier: String) -> Self {
        let challenge = challenge_of(&verifier);
        Pkce {
            verifier,
            challenge,
        }
    }

    /// The secret sent with the token request.
    #[must_use]
    pub fn verifier(&self) -> &str {
        &self.verifier
    }

    /// The hash sent with the authorization request.
    #[must_use]
    pub fn challenge(&self) -> &str {
        &self.challenge
    }
}

/// `BASE64URL(SHA256(ASCII(verifier)))`.
#[must_use]
pub fn challenge_of(verifier: &str) -> String {
    b64::encode(digest(&SHA256, verifier.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_the_rfc_7636_appendix_b_vector() {
        let p = Pkce::from_verifier("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk".into());
        assert_eq!(p.challenge(), "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
    }

    #[test]
    fn generated_verifiers_are_long_unique_and_in_the_allowed_alphabet() {
        let a = Pkce::generate().expect("random");
        let b = Pkce::generate().expect("random");
        assert_ne!(a.verifier(), b.verifier());
        assert_eq!(a.verifier().len(), 43);
        assert!(
            a.verifier()
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
        );
        assert_eq!(a.challenge(), challenge_of(a.verifier()));
    }
}
