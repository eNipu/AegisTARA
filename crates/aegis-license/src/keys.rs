//! The public keys a build trusts.

use crate::error::LicenseError;

/// Most keys a build may trust. Enough to rotate, small enough to review.
pub const MAX_TRUSTED_KEYS: usize = 8;
const KEY_ID_MAX: usize = 32;

/// A fixed, small set of Ed25519 public keys, by key ID.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TrustedKeys {
    keys: Vec<(String, [u8; 32])>,
}

impl TrustedKeys {
    /// No keys: every license is refused.
    #[must_use]
    pub fn none() -> Self {
        TrustedKeys::default()
    }

    /// The keys compiled into this build (`keys/vendor.keys`).
    ///
    /// # Panics
    ///
    /// If the shipped key file is malformed. A test parses it, so a bad file
    /// cannot reach a release.
    #[must_use]
    pub fn vendor() -> Self {
        Self::parse(include_str!("../keys/vendor.keys")).expect("keys/vendor.keys is well formed")
    }

    /// The keys that may sign update manifests (`keys/update.keys`). A
    /// different list from [`vendor`](Self::vendor), so a license key cannot
    /// ship code.
    ///
    /// # Panics
    ///
    /// If the shipped key file is malformed; a test parses it.
    #[must_use]
    pub fn updates() -> Self {
        Self::parse(include_str!("../keys/update.keys")).expect("keys/update.keys is well formed")
    }

    /// One key, for tests and tools.
    ///
    /// # Errors
    ///
    /// [`LicenseError::BadKeyList`] for an empty or long key ID.
    pub fn single(key_id: &str, public_key: [u8; 32]) -> Result<Self, LicenseError> {
        check_key_id(key_id)?;
        Ok(TrustedKeys {
            keys: vec![(key_id.to_owned(), public_key)],
        })
    }

    /// Parses `key-id public-key` lines. Blank lines and `#` comments are
    /// skipped. Time O(n) in the text length.
    ///
    /// # Errors
    ///
    /// [`LicenseError::BadKeyList`] for a bad line, a duplicate key ID, or
    /// more than [`MAX_TRUSTED_KEYS`] keys.
    pub fn parse(text: &str) -> Result<Self, LicenseError> {
        let mut keys: Vec<(String, [u8; 32])> = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut parts = line.split_whitespace();
            let (Some(id), Some(key), None) = (parts.next(), parts.next(), parts.next()) else {
                return Err(LicenseError::BadKeyList(
                    "a line is not `key-id public-key`",
                ));
            };
            check_key_id(id)?;
            let bytes = crate::envelope::b64_decode(key)
                .map_err(|_| LicenseError::BadKeyList("a public key is not base64url"))?;
            let key: [u8; 32] = bytes
                .try_into()
                .map_err(|_| LicenseError::BadKeyList("a public key is not 32 bytes"))?;
            if keys.iter().any(|(k, _)| k == id) {
                return Err(LicenseError::BadKeyList("a key ID appears twice"));
            }
            if keys.len() == MAX_TRUSTED_KEYS {
                return Err(LicenseError::BadKeyList("too many keys"));
            }
            keys.push((id.to_owned(), key));
        }
        Ok(TrustedKeys { keys })
    }

    /// Whether no key is trusted.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    pub(crate) fn get(&self, key_id: &str) -> Option<&[u8; 32]> {
        self.keys.iter().find(|(k, _)| k == key_id).map(|(_, v)| v)
    }

    /// Every key with its ID, in list order.
    pub(crate) fn iter(&self) -> impl Iterator<Item = (&str, &[u8; 32])> {
        self.keys.iter().map(|(k, v)| (k.as_str(), v))
    }
}

fn check_key_id(id: &str) -> Result<(), LicenseError> {
    let ok = !id.is_empty()
        && id.len() <= KEY_ID_MAX
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.');
    if ok {
        Ok(())
    } else {
        Err(LicenseError::BadKeyList(
            "a key ID is empty, long, or has odd characters",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shipped_key_files_parse() {
        let _ = TrustedKeys::vendor();
        let _ = TrustedKeys::updates();
    }

    #[test]
    fn parses_comments_and_rejects_bad_lines() {
        let key = crate::envelope::b64_encode([7u8; 32]);
        let ok = TrustedKeys::parse(&format!("# c\n\nk1 {key}\n")).expect("parses");
        assert_eq!(ok.get("k1"), Some(&[7u8; 32]));
        assert!(TrustedKeys::parse("k1").is_err());
        assert!(TrustedKeys::parse(&format!("k1 {key} extra")).is_err());
        assert!(TrustedKeys::parse(&format!("k1 {key}\nk1 {key}")).is_err());
        assert!(TrustedKeys::parse("k1 AAAA").is_err());
        assert!(TrustedKeys::parse(&format!("bad id {key}")).is_err());
    }

    #[test]
    fn the_list_is_bounded() {
        let key = crate::envelope::b64_encode([1u8; 32]);
        let many: String = (0..=MAX_TRUSTED_KEYS)
            .map(|i| format!("k{i} {key}\n"))
            .collect();
        assert!(TrustedKeys::parse(&many).is_err());
    }
}
