//! Base64url without padding, the encoding of every JOSE and PKCE value.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;

use crate::error::AuthError;

/// Encodes bytes. Time O(n).
#[must_use]
pub fn encode(bytes: impl AsRef<[u8]>) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

/// Decodes text; padding and the standard alphabet are rejected. Time O(n).
///
/// # Errors
///
/// [`AuthError::Malformed`] for anything that is not canonical base64url.
pub fn decode(text: &str) -> Result<Vec<u8>, AuthError> {
    URL_SAFE_NO_PAD
        .decode(text)
        .map_err(|_| AuthError::Malformed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_is_url_safe() {
        for bytes in [&b""[..], b"a", b"ab", b"abc", &[0xfb, 0xff, 0xfe]] {
            let text = encode(bytes);
            assert!(!text.contains(['+', '/', '=']));
            assert_eq!(decode(&text).expect("decodes"), bytes);
        }
    }

    #[test]
    fn rejects_padding_and_the_standard_alphabet() {
        assert!(decode("YQ==").is_err());
        assert!(decode("+/+/").is_err());
        assert!(decode("a b").is_err());
    }
}
