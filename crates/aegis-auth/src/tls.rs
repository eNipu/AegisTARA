//! TLS for every outgoing connection: login, key sets, the AegisTARA server
//! and PostgreSQL (backlog jobs 037 and 038, docs/adr/0036-https-and-grpc-web.md).
//!
//! One rule set for the whole product:
//!
//! - **rustls** with the **ring** provider. Both are already reviewed under
//!   decision D5 (Apache-2.0, ISC, MIT); no OpenSSL, no aws-lc.
//! - Certificates are checked against the **operating system's trust
//!   store** (`rustls-native-certs`), so a company that adds its own root CA
//!   to its laptops needs no AegisTARA setting. The Mozilla bundle
//!   (`webpki-roots`) is not used: its license (CDLA-Permissive-2.0) is not
//!   on the D5 list.
//! - TLS 1.2 and 1.3 only, as rustls allows.
//!
//! The configuration is built once per process and shared ([`client_config`]).

use std::sync::{Arc, OnceLock};

use hyper_rustls::{HttpsConnector, HttpsConnectorBuilder};
use hyper_util::client::legacy::connect::HttpConnector;

/// Why TLS could not be set up.
#[derive(Clone, Debug, thiserror::Error, PartialEq, Eq)]
pub enum TlsError {
    /// The OS trust store has no usable root certificate.
    #[error("the system has no trusted root certificates")]
    NoRoots,
    /// rustls refused the configuration.
    #[error("TLS setup failed: {0}")]
    Setup(String),
}

static CONFIG: OnceLock<Result<Arc<rustls::ClientConfig>, TlsError>> = OnceLock::new();

/// The root certificates of the operating system. Certificates the OS lists
/// but rustls cannot parse are skipped (and counted in the log).
///
/// # Errors
///
/// [`TlsError::NoRoots`] if none is usable.
pub fn system_roots() -> Result<rustls::RootCertStore, TlsError> {
    let found = rustls_native_certs::load_native_certs();
    for error in &found.errors {
        tracing::warn!(%error, "a system certificate could not be read");
    }
    let mut roots = rustls::RootCertStore::empty();
    let (added, ignored) = roots.add_parsable_certificates(found.certs);
    if ignored > 0 {
        tracing::debug!(ignored, "system certificates rustls cannot use");
    }
    if added == 0 {
        return Err(TlsError::NoRoots);
    }
    Ok(roots)
}

fn build() -> Result<Arc<rustls::ClientConfig>, TlsError> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| TlsError::Setup(e.to_string()))?
        .with_root_certificates(system_roots()?)
        .with_no_client_auth();
    Ok(Arc::new(config))
}

/// A client configuration that trusts exactly `roots`: ring, no client
/// certificate. For a private certificate authority (a company that signs its
/// own server certificates) and for tests; production code that trusts the
/// operating system uses [`client_config`].
///
/// # Errors
///
/// [`TlsError::Setup`] if rustls refuses the configuration.
pub fn config_with_roots(
    roots: rustls::RootCertStore,
) -> Result<Arc<rustls::ClientConfig>, TlsError> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| TlsError::Setup(e.to_string()))?
        .with_root_certificates(roots)
        .with_no_client_auth();
    Ok(Arc::new(config))
}

/// The shared client configuration: ring, the OS roots, no client
/// certificate. Built on first use; later calls are a clone of an `Arc`.
///
/// # Errors
///
/// [`TlsError`] if the trust store is empty or rustls refuses it.
pub fn client_config() -> Result<Arc<rustls::ClientConfig>, TlsError> {
    CONFIG.get_or_init(build).clone()
}

/// A hyper connector that speaks `https://` with [`client_config`] and,
/// when `allow_http` is set, also plain `http://`. ALPN offers HTTP/1.1, and
/// HTTP/2 too when `http2` is set (gRPC-Web wants HTTP/1.1 only, so a proxy
/// that cannot carry gRPC never sees an HTTP/2 stream).
///
/// # Errors
///
/// As [`client_config`].
pub fn https_connector(
    http: HttpConnector,
    allow_http: bool,
    http2: bool,
) -> Result<HttpsConnector<HttpConnector>, TlsError> {
    Ok(https_connector_with(
        &client_config()?,
        http,
        allow_http,
        http2,
    ))
}

/// As [`https_connector`], with the given configuration instead of the
/// shared one.
#[must_use]
pub fn https_connector_with(
    config: &Arc<rustls::ClientConfig>,
    mut http: HttpConnector,
    allow_http: bool,
    http2: bool,
) -> HttpsConnector<HttpConnector> {
    http.enforce_http(false);
    let config = (**config).clone();
    let builder = HttpsConnectorBuilder::new().with_tls_config(config);
    let builder = if allow_http {
        builder.https_or_http()
    } else {
        builder.https_only()
    };
    if http2 {
        builder.enable_http1().enable_http2().wrap_connector(http)
    } else {
        builder.enable_http1().wrap_connector(http)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shared_config_is_built_once() {
        // The CI image and developer machines have a trust store; a machine
        // without one gets the typed error instead of a panic.
        match (client_config(), client_config()) {
            (Ok(a), Ok(b)) => assert!(Arc::ptr_eq(&a, &b)),
            (Err(a), Err(b)) => assert_eq!(a, b),
            _ => panic!("the second call disagrees with the first"),
        }
    }
}
