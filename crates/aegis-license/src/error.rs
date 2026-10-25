//! Why a license was refused.

/// A license that cannot be used. Messages never echo file contents.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LicenseError {
    /// The file is larger than [`MAX_FILE_BYTES`](crate::MAX_FILE_BYTES).
    #[error("license file is too large")]
    TooLarge,
    /// The file is not in the license format.
    #[error("license file is malformed")]
    Malformed,
    /// A format version this build does not know.
    #[error("license format version {0} is not supported")]
    UnsupportedVersion(u32),
    /// Signed by a key this build does not trust.
    #[error("license is signed by an unknown key")]
    UnknownKey,
    /// The signature does not match the content.
    #[error("license signature is not valid")]
    BadSignature,
    /// The content breaks a rule of the format (named, never quoted).
    #[error("license content is not valid: {0}")]
    Invalid(&'static str),
    /// A key list line that does not parse.
    #[error("trusted key list is not valid: {0}")]
    BadKeyList(&'static str),
    /// Signing failed (issuer only).
    #[error("signing failed")]
    Signing,
}
