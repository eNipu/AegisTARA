//! Password hashing and the password rules.
//!
//! Hashes are **Argon2id** (RFC 9106) in PHC string format
//! (`$argon2id$v=19$m=...,t=...,p=...$salt$hash`), so the parameters travel
//! with each hash and can be raised later without a migration: a login with
//! an older hash re-hashes it ([`needs_rehash`]).
//!
//! Parameters: 19 MiB memory, 2 passes, 1 lane, the OWASP minimum for
//! Argon2id (2025 cheat sheet). One hash costs about 25 ms on the reference
//! laptop. Hashing is CPU-bound, so callers run it on the blocking pool
//! (standards R5), never on an async worker.
//!
//! Rules for a new password (NIST SP 800-63B section 3.1.1.2): at least
//! [`MIN_PASSWORD_CHARS`] characters, at most [`MAX_PASSWORD_BYTES`] bytes,
//! not only whitespace, not equal to the user name. No composition rules
//! (they make passwords weaker, not stronger).

use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::{Algorithm, Argon2, Params, Version};
use ring::rand::{SecureRandom, SystemRandom};

/// Shortest accepted new password, in characters.
pub const MIN_PASSWORD_CHARS: usize = 10;
/// Longest accepted password, in bytes. Bounds the hashing work an
/// unauthenticated caller can cause (R6).
pub const MAX_PASSWORD_BYTES: usize = 256;

const MEMORY_KIB: u32 = 19 * 1024;
const PASSES: u32 = 2;
const LANES: u32 = 1;

/// Why a new password is refused. The text is shown to the user.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PasswordRule {
    /// Shorter than [`MIN_PASSWORD_CHARS`].
    #[error("the password must have at least 10 characters")]
    TooShort,
    /// Longer than [`MAX_PASSWORD_BYTES`].
    #[error("the password must have at most 256 bytes")]
    TooLong,
    /// Only spaces.
    #[error("the password must not be only spaces")]
    Blank,
    /// The same as the user name.
    #[error("the password must not be the user name")]
    SameAsUsername,
}

/// Checks a new password against the rules.
///
/// # Errors
///
/// The first rule it breaks.
pub fn check_new(password: &str, username: &str) -> Result<(), PasswordRule> {
    if password.len() > MAX_PASSWORD_BYTES {
        return Err(PasswordRule::TooLong);
    }
    if password.chars().count() < MIN_PASSWORD_CHARS {
        return Err(PasswordRule::TooShort);
    }
    if password.trim().is_empty() {
        return Err(PasswordRule::Blank);
    }
    if password.eq_ignore_ascii_case(username) {
        return Err(PasswordRule::SameAsUsername);
    }
    Ok(())
}

fn argon2() -> Argon2<'static> {
    // The constants are valid Argon2 parameters (checked by the test below),
    // so the fallback is never taken; it keeps library code panic free (Q3).
    let params = Params::new(MEMORY_KIB, PASSES, LANES, None).unwrap_or_default();
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
}

/// Hashes a password with a fresh 16-byte salt from the OS generator.
/// CPU-bound: call from `spawn_blocking`.
///
/// # Errors
///
/// A message if the generator or the hasher fails (never in practice).
pub fn hash(password: &str) -> Result<String, String> {
    let mut salt = [0u8; 16];
    SystemRandom::new()
        .fill(&mut salt)
        .map_err(|_| "the random generator failed".to_owned())?;
    let salt = SaltString::encode_b64(&salt).map_err(|e| e.to_string())?;
    argon2()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| e.to_string())
}

/// Whether `password` matches the PHC string `stored`. A malformed stored
/// hash never matches. CPU-bound: call from `spawn_blocking`. The
/// comparison inside the hasher is constant time.
#[must_use]
pub fn verify(password: &str, stored: &str) -> bool {
    if password.len() > MAX_PASSWORD_BYTES {
        return false;
    }
    PasswordHash::new(stored).is_ok_and(|parsed| {
        argon2()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok()
    })
}

/// Whether a stored hash uses weaker parameters than this build, so the
/// next successful login should replace it.
#[must_use]
pub fn needs_rehash(stored: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(stored) else {
        return true;
    };
    let Ok(params) = Params::try_from(&parsed) else {
        return true;
    };
    parsed.algorithm.as_str() != "argon2id"
        || params.m_cost() < MEMORY_KIB
        || params.t_cost() < PASSES
}

/// A hash of a random password, verified against when the user name does
/// not exist, so a wrong name costs as much time as a wrong password and the
/// answer time does not reveal which names exist.
///
/// # Errors
///
/// As [`hash`].
pub fn decoy() -> Result<String, String> {
    let mut random = [0u8; 24];
    SystemRandom::new()
        .fill(&mut random)
        .map_err(|_| "the random generator failed".to_owned())?;
    hash(&crate::accounts::b64(&random))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parameters_are_valid() {
        assert!(Params::new(MEMORY_KIB, PASSES, LANES, None).is_ok());
    }

    #[test]
    fn hash_and_verify_round_trip() {
        let h = hash("correct horse battery").expect("hash");
        assert!(h.starts_with("$argon2id$v=19$m=19456,t=2,p=1$"));
        assert!(verify("correct horse battery", &h));
        assert!(!verify("correct horse batterz", &h));
        assert!(!needs_rehash(&h));
    }

    #[test]
    fn two_hashes_of_one_password_differ() {
        assert_ne!(hash("same password!").ok(), hash("same password!").ok());
    }

    #[test]
    fn malformed_and_weak_hashes() {
        assert!(!verify("x", "not a hash"));
        assert!(needs_rehash("not a hash"));
        let weak = Argon2::new(
            Algorithm::Argon2id,
            Version::V0x13,
            Params::new(1024, 1, 1, None).expect("params"),
        )
        .hash_password(
            b"long enough password",
            &SaltString::encode_b64(&[7; 16]).expect("salt"),
        )
        .expect("hash")
        .to_string();
        assert!(verify("long enough password", &weak));
        assert!(needs_rehash(&weak));
    }

    #[test]
    fn rules() {
        assert_eq!(check_new("short", "bob"), Err(PasswordRule::TooShort));
        assert_eq!(check_new("          ", "bob"), Err(PasswordRule::Blank));
        assert_eq!(
            check_new("Administrator", "administrator"),
            Err(PasswordRule::SameAsUsername)
        );
        assert_eq!(
            check_new(&"x".repeat(MAX_PASSWORD_BYTES + 1), "bob"),
            Err(PasswordRule::TooLong)
        );
        assert_eq!(check_new("a fine passphrase", "bob"), Ok(()));
        // Characters, not bytes: ten umlauts are ten characters.
        assert_eq!(check_new("üüüüüüüüüü", "bob"), Ok(()));
        assert!(!verify(&"x".repeat(MAX_PASSWORD_BYTES + 1), "$argon2id$"));
    }

    #[test]
    fn decoy_is_a_valid_hash() {
        let d = decoy().expect("decoy");
        assert!(!needs_rehash(&d));
    }
}
