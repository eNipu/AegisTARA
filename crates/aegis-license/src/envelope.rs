//! The signed one-line file shared by licenses and update manifests:
//!
//! ```text
//! <TAG>.<payload>.<signature>
//! ```
//!
//! `payload` is bytes (JSON), `signature` is Ed25519 over `domain + payload`,
//! both in base64url without padding. Each file kind has its own tag and its
//! own domain prefix, so a signature made for one kind never verifies as
//! another.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ring::signature::{ED25519, UnparsedPublicKey};

use crate::error::LicenseError;

pub(crate) fn b64_encode(bytes: impl AsRef<[u8]>) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

pub(crate) fn b64_decode(text: &str) -> Result<Vec<u8>, LicenseError> {
    URL_SAFE_NO_PAD
        .decode(text)
        .map_err(|_| LicenseError::Malformed)
}

/// Splits a file into payload and signature bytes. Nothing is trusted yet.
pub(crate) fn split(
    file: &str,
    tag: &str,
    max_bytes: usize,
) -> Result<(Vec<u8>, Vec<u8>), LicenseError> {
    if file.len() > max_bytes {
        return Err(LicenseError::TooLarge);
    }
    let mut parts = file.trim().split('.');
    let (Some(prefix), Some(payload), Some(signature), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(LicenseError::Malformed);
    };
    if prefix != tag {
        return Err(LicenseError::Malformed);
    }
    Ok((b64_decode(payload)?, b64_decode(signature)?))
}

/// Checks a signature over `domain + payload`.
pub(crate) fn verify(
    key: &[u8; 32],
    domain: &[u8],
    payload: &[u8],
    signature: &[u8],
) -> Result<(), LicenseError> {
    let mut signed = Vec::with_capacity(domain.len() + payload.len());
    signed.extend_from_slice(domain);
    signed.extend_from_slice(payload);
    UnparsedPublicKey::new(&ED25519, key)
        .verify(&signed, signature)
        .map_err(|_| LicenseError::BadSignature)
}

/// Signs `payload` and builds the file text.
#[cfg(feature = "issuer")]
pub(crate) fn seal(
    pair: &ring::signature::Ed25519KeyPair,
    tag: &str,
    domain: &[u8],
    payload: &[u8],
) -> String {
    let mut signed = Vec::with_capacity(domain.len() + payload.len());
    signed.extend_from_slice(domain);
    signed.extend_from_slice(payload);
    let signature = pair.sign(&signed);
    format!(
        "{tag}.{}.{}",
        b64_encode(payload),
        b64_encode(signature.as_ref())
    )
}
