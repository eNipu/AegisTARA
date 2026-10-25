//! How the client reaches the server (docs/adr/0036-https-and-grpc-web.md).
//!
//! Two transports behind one type, [`Transport`], so the rest of the client
//! never knows which one is in use:
//!
//! - **gRPC** over HTTP/2: the default for `http://` addresses and for a
//!   company load balancer that forwards HTTP/2. `https://` uses rustls
//!   with the OS trust store.
//! - **gRPC-Web** over HTTP/1.1: for proxies that cannot carry gRPC, such as
//!   Railway's edge and many corporate proxies (direction section 8). The
//!   default for `https://` addresses, because it works through every
//!   proxy. Streams (open, watch) work the same; the server sends a
//!   heartbeat so idle proxies keep them open.
//!
//! Limits (R6): connecting takes at most 10 s; a unary call at most 30 s
//! until its answer starts. A stream is not limited once it has started.

use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use http::{Request, Response};
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::TokioExecutor;
use tonic::body::Body;
use tonic::transport::{Channel, ClientTlsConfig, Endpoint};
use tonic_web::{GrpcWebCall, GrpcWebClientService};
use tower::Service;

use crate::error::ClientError;

/// Longest wait to establish a connection (R6).
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// Longest a unary call may take until its answer starts (R6).
pub const CALL_TIMEOUT: Duration = Duration::from_secs(30);

/// Which protocol to speak.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransportKind {
    /// gRPC over HTTP/2.
    Grpc,
    /// gRPC-Web over HTTP/1.1.
    GrpcWeb,
}

impl TransportKind {
    /// The default for a server address: gRPC-Web for `https://` (it passes
    /// every proxy), gRPC otherwise.
    #[must_use]
    pub fn auto(url: &str) -> Self {
        if url.starts_with("https://") {
            TransportKind::GrpcWeb
        } else {
            TransportKind::Grpc
        }
    }

    /// Parses `grpc`, `grpc-web` or `auto` (resolved against `url`).
    ///
    /// # Errors
    ///
    /// A message for anything else.
    pub fn parse(text: &str, url: &str) -> Result<Self, String> {
        match text {
            "grpc" => Ok(TransportKind::Grpc),
            "grpc-web" | "web" => Ok(TransportKind::GrpcWeb),
            "" | "auto" => Ok(Self::auto(url)),
            _ => Err("the transport must be grpc, grpc-web or auto".to_owned()),
        }
    }

    /// The name, for settings and logs.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            TransportKind::Grpc => "grpc",
            TransportKind::GrpcWeb => "grpc-web",
        }
    }
}

type WebClient = Client<hyper_rustls::HttpsConnector<HttpConnector>, GrpcWebCall<Body>>;

/// A connection to the server in one of the two protocols. Cheap to clone
/// (both variants share their connection pool).
#[derive(Clone)]
pub enum Transport {
    /// gRPC over HTTP/2.
    Grpc(Channel),
    /// gRPC-Web over HTTP/1.1, with the server's scheme and authority:
    /// tonic sends only the path, and hyper needs an absolute URI.
    Web {
        /// The client (boxed: it is much larger than a channel).
        service: Box<GrpcWebClientService<WebClient>>,
        /// `scheme://authority` of the server.
        origin: http::Uri,
    },
}

/// Checks that `url` is an absolute `http://` or `https://` address.
///
/// # Errors
///
/// [`ClientError::Transport`] otherwise.
pub fn check_url(url: &str) -> Result<http::Uri, ClientError> {
    let uri: http::Uri = url.parse().map_err(|_| {
        ClientError::Transport("the server address must look like https://aegis.example.com".into())
    })?;
    match (uri.scheme_str(), uri.host()) {
        (Some("http" | "https"), Some(host)) if !host.is_empty() => Ok(uri),
        _ => Err(ClientError::Transport(
            "the server address must look like https://aegis.example.com".into(),
        )),
    }
}

impl Transport {
    /// Connects to `url` with `kind`.
    ///
    /// gRPC connects at once (and fails here if the server is down);
    /// gRPC-Web connects on the first call.
    ///
    /// # Errors
    ///
    /// [`ClientError::Transport`] for a bad address, a missing trust store,
    /// or an unreachable server.
    pub async fn connect(url: &str, kind: TransportKind) -> Result<Self, ClientError> {
        let uri = check_url(url)?;
        let tls = uri.scheme_str() == Some("https");
        match kind {
            TransportKind::Grpc => {
                let mut endpoint = Endpoint::from_shared(url.to_owned())
                    .map_err(|e| ClientError::Transport(e.to_string()))?
                    .connect_timeout(CONNECT_TIMEOUT)
                    .timeout(CALL_TIMEOUT)
                    .tcp_nodelay(true)
                    .http2_keep_alive_interval(Duration::from_secs(30))
                    .keep_alive_while_idle(true);
                if tls {
                    endpoint = endpoint
                        .tls_config(ClientTlsConfig::new().with_native_roots())
                        .map_err(|e| ClientError::Transport(e.to_string()))?;
                }
                Ok(Transport::Grpc(endpoint.connect().await?))
            }
            TransportKind::GrpcWeb => {
                let mut http = HttpConnector::new();
                http.set_connect_timeout(Some(CONNECT_TIMEOUT));
                http.set_nodelay(true);
                let connector = aegis_auth::tls::https_connector(http, true, false)
                    .map_err(|e| ClientError::Transport(e.to_string()))?;
                let client = Client::builder(TokioExecutor::new())
                    .pool_idle_timeout(Duration::from_secs(60))
                    .build(connector);
                Ok(Transport::Web {
                    service: Box::new(GrpcWebClientService::new(client)),
                    origin: uri,
                })
            }
        }
    }

    /// Which protocol this is.
    #[must_use]
    pub fn kind(&self) -> TransportKind {
        match self {
            Transport::Grpc(_) => TransportKind::Grpc,
            Transport::Web { .. } => TransportKind::GrpcWeb,
        }
    }
}

type BoxError = Box<dyn std::error::Error + Send + Sync>;
type BoxFuture<T> = Pin<Box<dyn std::future::Future<Output = Result<T, BoxError>> + Send>>;

impl Service<Request<Body>> for Transport {
    type Response = Response<Body>;
    type Error = BoxError;
    type Future = BoxFuture<Response<Body>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), BoxError>> {
        match self {
            Transport::Grpc(c) => c.poll_ready(cx).map_err(Into::into),
            Transport::Web { service, .. } => service.poll_ready(cx).map_err(Into::into),
        }
    }

    fn call(&mut self, request: Request<Body>) -> Self::Future {
        match self {
            Transport::Grpc(c) => {
                let fut = c.call(request);
                Box::pin(async move { fut.await.map_err(Into::into) })
            }
            Transport::Web { service, origin } => {
                let mut request = request;
                let mut parts = http::uri::Parts::default();
                parts.scheme = origin.scheme().cloned();
                parts.authority = origin.authority().cloned();
                parts.path_and_query = request.uri().path_and_query().cloned();
                match http::Uri::from_parts(parts) {
                    Ok(uri) => *request.uri_mut() = uri,
                    Err(e) => return Box::pin(async move { Err(e.into()) }),
                }
                let fut = service.call(request);
                Box::pin(async move {
                    match tokio::time::timeout(CALL_TIMEOUT, fut).await {
                        Ok(Ok(response)) => Ok(response.map(Body::new)),
                        Ok(Err(e)) => Err(e.into()),
                        Err(_) => Err("the server did not answer in time".into()),
                    }
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_picks_web_for_https() {
        assert_eq!(
            TransportKind::auto("https://x.example"),
            TransportKind::GrpcWeb
        );
        assert_eq!(
            TransportKind::auto("http://127.0.0.1:1"),
            TransportKind::Grpc
        );
        assert_eq!(
            TransportKind::parse("grpc", "https://x"),
            Ok(TransportKind::Grpc)
        );
        assert!(TransportKind::parse("smoke", "https://x").is_err());
    }

    #[test]
    fn urls_are_checked() {
        assert!(check_url("https://aegis.example.com").is_ok());
        assert!(check_url("http://127.0.0.1:50051").is_ok());
        for bad in ["", "aegis.example.com", "ftp://x", "https://"] {
            assert!(check_url(bad).is_err(), "{bad}");
        }
    }
}
