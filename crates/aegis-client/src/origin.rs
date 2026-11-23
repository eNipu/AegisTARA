//! Strict HTTP(S) origins for binding credentials to one API server.

use std::net::Ipv6Addr;

use http::Uri;

/// A normalized scheme, host, and effective port. Paths and queries are not
/// part of an origin; a different scheme is always a different origin.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Origin {
    scheme: String,
    host: String,
    port: u16,
}

/// Origin validation failures. Messages never contain the supplied URL.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum OriginError {
    /// Not an absolute HTTP(S) URL, or contains ambiguous URL syntax.
    #[error("server URL must be a valid absolute HTTP(S) URL without userinfo or a fragment")]
    InvalidUrl,
    /// Scheme, host, or effective port differs from the bound server.
    #[error("request origin does not match the login server")]
    Mismatch,
}

impl From<OriginError> for aegis_auth::AuthError {
    fn from(error: OriginError) -> Self {
        Self::Http(error.to_string())
    }
}

impl Origin {
    /// Parses an absolute HTTP(S) URL, accepting RPC paths and queries.
    /// Host/scheme casing and omitted default ports are normalized. DNS
    /// names must be ASCII (use punycode for internationalized names).
    ///
    /// # Errors
    ///
    /// [`OriginError::InvalidUrl`] for malformed, relative, non-HTTP(S),
    /// userinfo-bearing, or fragment-bearing URLs.
    pub fn parse(url: &str) -> Result<Self, OriginError> {
        // Uri is a request-target parser: it is intentionally more permissive
        // than a credential boundary (including discarding URL fragments).
        if url.bytes().any(|b| !b.is_ascii_graphic()) || url.contains(['#', '\\']) {
            return Err(OriginError::InvalidUrl);
        }
        let mut bytes = url.bytes();
        while let Some(byte) = bytes.next() {
            if byte == b'%'
                && !(bytes.next().is_some_and(|b| b.is_ascii_hexdigit())
                    && bytes.next().is_some_and(|b| b.is_ascii_hexdigit()))
            {
                return Err(OriginError::InvalidUrl);
            }
        }
        let uri: Uri = url.parse().map_err(|_| OriginError::InvalidUrl)?;
        let (scheme, default_port) = match uri.scheme_str() {
            Some(s) if s.eq_ignore_ascii_case("http") => ("http", 80),
            Some(s) if s.eq_ignore_ascii_case("https") => ("https", 443),
            _ => return Err(OriginError::InvalidUrl),
        };
        let authority = uri.authority().ok_or(OriginError::InvalidUrl)?;
        if authority.as_str().contains('@') {
            return Err(OriginError::InvalidUrl);
        }
        let host = authority.host();
        let normalized_host = if host.starts_with('[') {
            let address = host
                .strip_prefix('[')
                .and_then(|s| s.strip_suffix(']'))
                .ok_or(OriginError::InvalidUrl)?;
            let address: Ipv6Addr = address.parse().map_err(|_| OriginError::InvalidUrl)?;
            format!("[{address}]")
        } else {
            let domain = host.strip_suffix('.').unwrap_or(host);
            if domain.len() > 253
                || domain.split('.').any(|label| {
                    label.is_empty()
                        || label.len() > 63
                        || label.starts_with('-')
                        || label.ends_with('-')
                        || !label
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
                })
            {
                return Err(OriginError::InvalidUrl);
            }
            host.to_ascii_lowercase()
        };
        // Do not turn an empty, non-numeric, or overflowing explicit port
        // into a default port merely because Uri::port_u16 returns None.
        let suffix = authority
            .as_str()
            .strip_prefix(host)
            .ok_or(OriginError::InvalidUrl)?;
        let port = if suffix.is_empty() {
            default_port
        } else {
            let digits = suffix.strip_prefix(':').ok_or(OriginError::InvalidUrl)?;
            if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
                return Err(OriginError::InvalidUrl);
            }
            digits.parse().map_err(|_| OriginError::InvalidUrl)?
        };
        Ok(Self {
            scheme: scheme.into(),
            host: normalized_host,
            port,
        })
    }

    /// Canonical origin text for credential storage keys. Equivalent host
    /// casing, default ports, paths and queries therefore share one entry.
    #[must_use]
    pub fn key(&self) -> String {
        let default_port = (self.scheme == "http" && self.port == 80)
            || (self.scheme == "https" && self.port == 443);
        if default_port {
            format!("{}://{}", self.scheme, self.host)
        } else {
            format!("{}://{}:{}", self.scheme, self.host, self.port)
        }
    }

    /// Checks a request URL before credentials or refresh work may begin.
    ///
    /// # Errors
    ///
    /// [`OriginError::InvalidUrl`] for an invalid URL, or
    /// [`OriginError::Mismatch`] for a different origin.
    pub fn check(&self, url: &str) -> Result<(), OriginError> {
        if *self == Self::parse(url)? {
            Ok(())
        } else {
            Err(OriginError::Mismatch)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Origin, OriginError};

    #[test]
    fn normalized_origin_still_distinguishes_nondefault_ports_and_schemes() {
        let origin = Origin::parse("HTTPS://EXAMPLE.TEST:443/base").unwrap();
        assert_eq!(origin.check("https://example.test/rpc?x=1"), Ok(()));
        assert_eq!(
            origin.check("https://example.test:444/rpc"),
            Err(OriginError::Mismatch)
        );
        assert_eq!(
            origin.check("http://example.test:443/rpc"),
            Err(OriginError::Mismatch)
        );
    }

    #[test]
    fn rejects_authority_ambiguities_instead_of_normalizing_them() {
        for url in [
            "https://user@example.test",
            "https://example.test:",
            "https://example.test:65536",
            "https://example.test#ignored",
            "https://%65xample.test",
            "//example.test",
        ] {
            assert_eq!(Origin::parse(url), Err(OriginError::InvalidUrl), "{url}");
        }
    }
}
