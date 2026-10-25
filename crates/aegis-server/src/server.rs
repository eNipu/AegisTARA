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
/// HTTP/1.1 is accepted next to HTTP/2, so gRPC-Web (`tonic_web::GrpcWebLayer`)
/// works through proxies that only forward HTTP/1.1, such as Railway's edge
/// and many corporate proxies (direction section 8).
///
/// Mount the service with `add_service(service.into_server())` and attach
/// authentication as a layer, for example
/// `.layer(tonic::service::InterceptorLayer::new(auth::dev_interceptor))`.
#[must_use]
pub fn builder() -> Server {
    Server::builder()
        .accept_http1(true)
        .timeout(CALL_TIMEOUT)
        .concurrency_limit_per_connection(MAX_CONCURRENT_PER_CONNECTION)
        .http2_keepalive_interval(Some(Duration::from_secs(30)))
        .http2_keepalive_timeout(Some(Duration::from_secs(10)))
        .tcp_nodelay(true)
}

/// The layers every request passes, outermost first: the health probe,
/// gRPC-Web translation, then authentication.
pub type Layers<A> = tower::layer::util::Stack<
    crate::auth_layer::AuthLayer<A>,
    tower::layer::util::Stack<
        tonic_web::GrpcWebLayer,
        tower::layer::util::Stack<crate::health::HealthLayer, tower::layer::util::Identity>,
    >,
>;

/// The complete server as the binary runs it: [`builder`] limits, the
/// [`Layers`], the authenticated `Aegis` service and the public `Accounts`
/// service. Tests use the same function, so they exercise exactly what ships.
#[must_use]
pub fn router<S: crate::ProjectStore, A: crate::auth_layer::Authn>(
    service: crate::AegisService<S>,
    public: crate::accounts::service::AccountsService,
    auth: crate::auth_layer::AuthLayer<A>,
) -> tonic::transport::server::Router<Layers<A>> {
    builder()
        .layer(crate::health::HealthLayer)
        .layer(tonic_web::GrpcWebLayer::new())
        .layer(auth)
        .add_service(service.into_server())
        .add_service(public.into_server())
}
