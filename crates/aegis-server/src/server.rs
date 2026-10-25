//! Transport settings shared by the binary and the tests.

use std::time::Duration;

use tonic::transport::Server;

/// Longest a unary call may run before the transport cancels it (R6).
/// Streaming bodies (`OpenProject`, `Watch`) are not limited by it.
pub const CALL_TIMEOUT: Duration = Duration::from_secs(30);

/// Most requests in flight on one connection.
pub const MAX_CONCURRENT_PER_CONNECTION: usize = 64;

/// A tonic server builder with the standard limits: call timeout,
/// per-connection concurrency, HTTP/2 keep-alive pings that detect dead
/// clients (so a vanished client's `Watch` stream ends), and `TCP_NODELAY`.
///
/// Mount the service with `add_service(service.into_server())` and attach
/// authentication as a layer, for example
/// `.layer(tonic::service::InterceptorLayer::new(auth::dev_interceptor))`.
#[must_use]
pub fn builder() -> Server {
    Server::builder()
        .timeout(CALL_TIMEOUT)
        .concurrency_limit_per_connection(MAX_CONCURRENT_PER_CONNECTION)
        .http2_keepalive_interval(Some(Duration::from_secs(30)))
        .http2_keepalive_timeout(Some(Duration::from_secs(10)))
        .tcp_nodelay(true)
}
