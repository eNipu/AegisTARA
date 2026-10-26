//! The HTTP calls login makes: discovery, JWKS, token endpoint.
//!
//! [`HttpClient`] is the seam: production code uses [`HyperClient`], tests
//! use the same client against a mock identity provider on a loopback port.
//!
//! [`HyperClient`] speaks `https://` (rustls with the OS trust store, see
//! [`crate::tls`]) to any host, and cleartext `http://` only to loopback
//! addresses unless told otherwise (`allow_insecure_http`). Tokens never go
//! over cleartext to another host.
//!
//! Limits (R6): 10 s per request, 1 MiB per response body, no redirects.

use std::future::Future;
use std::time::Duration;

use bytes::Bytes;
use http_body_util::{BodyExt, Full, Limited};
use hyper_rustls::HttpsConnector;
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::TokioExecutor;

use crate::error::AuthError;

/// Largest response body read, in bytes.
pub const MAX_RESPONSE_BYTES: usize = 1024 * 1024;

/// A request to an identity provider.
#[derive(Clone, Debug)]
pub struct HttpRequest {
    /// `GET` or `POST`.
    pub method: &'static str,
    /// Absolute URL.
    pub url: String,
    /// Header names (lowercase) and values.
    pub headers: Vec<(&'static str, String)>,
    /// Body, empty for `GET`.
    pub body: Vec<u8>,
}

impl HttpRequest {
    /// A `GET`.
    #[must_use]
    pub fn get(url: &str) -> Self {
        HttpRequest {
            method: "GET",
            url: url.to_owned(),
            headers: vec![("accept", "application/json".to_owned())],
            body: Vec::new(),
        }
    }

    /// A form `POST`, `application/x-www-form-urlencoded`.
    #[must_use]
    pub fn post_form(url: &str, fields: &[(&str, &str)]) -> Self {
        HttpRequest {
            method: "POST",
            url: url.to_owned(),
            headers: vec![
                ("accept", "application/json".to_owned()),
                (
                    "content-type",
                    "application/x-www-form-urlencoded".to_owned(),
                ),
            ],
            body: form_encode(fields).into_bytes(),
        }
    }

    /// Adds a header.
    #[must_use]
    pub fn with_header(mut self, name: &'static str, value: String) -> Self {
        self.headers.push((name, value));
        self
    }
}

/// A response.
#[derive(Clone, Debug)]
pub struct HttpResponse {
    /// Status code.
    pub status: u16,
    /// Header names (lowercase) and values.
    pub headers: Vec<(String, String)>,
    /// Body, at most [`MAX_RESPONSE_BYTES`].
    pub body: Vec<u8>,
}

impl HttpResponse {
    /// The first header with this (lowercase) name.
    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }
}

/// Sends requests. Implementations enforce the limits in the module docs.
pub trait HttpClient: Send + Sync + 'static {
    /// Sends one request.
    fn send(
        &self,
        request: HttpRequest,
    ) -> impl Future<Output = Result<HttpResponse, AuthError>> + Send;
}

/// `application/x-www-form-urlencoded` encoding.
#[must_use]
pub fn form_encode(fields: &[(&str, &str)]) -> String {
    use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};
    // Unreserved characters stay as they are (RFC 3986).
    const KEEP: &AsciiSet = &NON_ALPHANUMERIC
        .remove(b'-')
        .remove(b'.')
        .remove(b'_')
        .remove(b'~');
    fields
        .iter()
        .map(|(k, v)| {
            format!(
                "{}={}",
                utf8_percent_encode(k, KEEP),
                utf8_percent_encode(v, KEEP)
            )
        })
        .collect::<Vec<_>>()
        .join("&")
}

/// Whether the URL's host is a loopback address or `localhost`.
#[must_use]
pub fn is_loopback_url(url: &str) -> bool {
    url.parse::<http::Uri>()
        .ok()
        .and_then(|u| u.host().map(str::to_owned))
        .is_some_and(|h| {
            let h = h.trim_start_matches('[').trim_end_matches(']');
            h == "localhost"
                || h.parse::<std::net::IpAddr>()
                    .is_ok_and(|ip| ip.is_loopback())
        })
}

/// The production [`HttpClient`], on `hyper`.
pub struct HyperClient {
    /// An error when the machine has no usable trust store: `http://` to
    /// loopback still works, `https://` answers with the TLS error.
    https: Result<Client<HttpsConnector<HttpConnector>, Full<Bytes>>, crate::tls::TlsError>,
    plain: Client<HttpConnector, Full<Bytes>>,
    timeout: Duration,
    allow_insecure_http: bool,
}

impl HyperClient {
    /// A client for `https://` to any host and cleartext only to loopback
    /// addresses.
    #[must_use]
    pub fn new() -> Self {
        let mut connector = HttpConnector::new();
        connector.set_connect_timeout(Some(Duration::from_secs(5)));
        let https = crate::tls::https_connector(connector.clone(), false, true)
            .map(|c| Client::builder(TokioExecutor::new()).build(c));
        connector.enforce_http(true);
        HyperClient {
            https,
            plain: Client::builder(TokioExecutor::new()).build(connector),
            timeout: Duration::from_secs(10),
            allow_insecure_http: false,
        }
    }

    /// A client that trusts exactly the roots of `config` for `https://`
    /// instead of the operating system's (see
    /// [`config_with_roots`](crate::tls::config_with_roots)): a company's own
    /// certificate authority, or a test server.
    #[must_use]
    pub fn with_tls_config(config: &std::sync::Arc<rustls::ClientConfig>) -> Self {
        let mut connector = HttpConnector::new();
        connector.set_connect_timeout(Some(Duration::from_secs(5)));
        let https = Ok(Client::builder(TokioExecutor::new()).build(
            crate::tls::https_connector_with(config, connector.clone(), false, true),
        ));
        connector.enforce_http(true);
        HyperClient {
            https,
            plain: Client::builder(TokioExecutor::new()).build(connector),
            timeout: Duration::from_secs(10),
            allow_insecure_http: false,
        }
    }

    /// Also allows cleartext `http://` to non-loopback hosts. For a lab
    /// network only; tokens cross it unprotected.
    #[must_use]
    pub fn allow_insecure_http(mut self) -> Self {
        self.allow_insecure_http = true;
        self
    }
}

impl Default for HyperClient {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpClient for HyperClient {
    async fn send(&self, request: HttpRequest) -> Result<HttpResponse, AuthError> {
        let tls = request.url.starts_with("https://");
        if !tls && !self.allow_insecure_http && !is_loopback_url(&request.url) {
            return Err(AuthError::Http(
                "cleartext http is only allowed to a loopback address".to_owned(),
            ));
        }
        let mut builder = http::Request::builder()
            .method(request.method)
            .uri(&request.url);
        for (name, value) in &request.headers {
            builder = builder.header(*name, value);
        }
        let req = builder
            .body(Full::new(Bytes::from(request.body)))
            .map_err(|_| AuthError::Http("invalid request".to_owned()))?;

        let exchange = async {
            let response = if tls {
                match &self.https {
                    Ok(client) => client.request(req).await,
                    Err(e) => return Err(AuthError::Http(e.to_string())),
                }
            } else {
                self.plain.request(req).await
            }
            .map_err(|e| AuthError::Http(format!("request failed: {e}")))?;
            let (parts, body) = response.into_parts();
            let body = Limited::new(body, MAX_RESPONSE_BYTES)
                .collect()
                .await
                .map_err(|_| AuthError::TooLarge)?
                .to_bytes();
            Ok(HttpResponse {
                status: parts.status.as_u16(),
                headers: parts
                    .headers
                    .iter()
                    .filter_map(|(n, v)| Some((n.as_str().to_owned(), v.to_str().ok()?.to_owned())))
                    .collect(),
                body: body.to_vec(),
            })
        };
        tokio::time::timeout(self.timeout, exchange)
            .await
            .map_err(|_| AuthError::Http("request timed out".to_owned()))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn form_encoding_escapes_reserved_characters() {
        assert_eq!(
            form_encode(&[
                ("a", "b c"),
                ("redirect_uri", "http://127.0.0.1:1/cb?x=1&y=2"),
                ("k-._~", "ü")
            ]),
            "a=b%20c&redirect_uri=http%3A%2F%2F127.0.0.1%3A1%2Fcb%3Fx%3D1%26y%3D2&k-._~=%C3%BC"
        );
    }

    #[test]
    fn loopback_detection() {
        for yes in [
            "http://127.0.0.1:80/x",
            "http://localhost/x",
            "http://[::1]:9/x",
            "http://127.9.9.9/",
        ] {
            assert!(is_loopback_url(yes), "{yes}");
        }
        for no in [
            "http://example.com/",
            "http://10.0.0.1/",
            "http://127.0.0.1.evil.com/",
            "garbage",
        ] {
            assert!(!is_loopback_url(no), "{no}");
        }
    }

    #[tokio::test]
    async fn cleartext_to_a_remote_host_is_refused_before_any_connection() {
        let client = HyperClient::new();
        let remote = client
            .send(HttpRequest::get("http://203.0.113.9/jwks"))
            .await;
        assert!(matches!(remote, Err(AuthError::Http(m)) if m.contains("loopback")));
    }

    #[tokio::test]
    async fn https_to_a_closed_port_fails_as_a_request_error() {
        // TCP connect to a closed loopback port fails at once; the point is
        // that https is attempted, no longer refused by policy.
        let client = HyperClient::new();
        let r = client
            .send(HttpRequest::get("https://127.0.0.1:9/jwks"))
            .await;
        assert!(matches!(r, Err(AuthError::Http(m)) if !m.contains("not available")));
    }
}
