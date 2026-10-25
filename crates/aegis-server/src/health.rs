//! Health check for load balancers and hosting platforms (Railway, a
//! Kubernetes probe): `GET /healthz` answers `200 OK` with an empty body,
//! without credentials, before any gRPC handling.
//!
//! "Healthy" means the process serves requests. Storage problems show up as
//! failing calls, not as a failing probe, so a database hiccup does not get
//! the server restarted in a loop.

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use tower::{Layer, Service};

/// The probe path.
pub const HEALTH_PATH: &str = "/healthz";

/// Answers the probe; passes everything else on.
#[derive(Clone, Copy, Debug, Default)]
pub struct HealthLayer;

impl<S> Layer<S> for HealthLayer {
    type Service = Health<S>;

    fn layer(&self, inner: S) -> Health<S> {
        Health { inner }
    }
}

/// The service [`HealthLayer`] builds.
#[derive(Clone, Debug)]
pub struct Health<S> {
    inner: S,
}

impl<S, B, ResBody> Service<http::Request<B>> for Health<S>
where
    S: Service<http::Request<B>, Response = http::Response<ResBody>>,
    S::Future: Send + 'static,
    ResBody: Default + Send + 'static,
{
    type Response = http::Response<ResBody>;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, S::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), S::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: http::Request<B>) -> Self::Future {
        if request.uri().path() == HEALTH_PATH
            && matches!(*request.method(), http::Method::GET | http::Method::HEAD)
        {
            let mut response = http::Response::new(ResBody::default());
            response.headers_mut().insert(
                http::header::CACHE_CONTROL,
                http::HeaderValue::from_static("no-store"),
            );
            return Box::pin(async move { Ok(response) });
        }
        Box::pin(self.inner.call(request))
    }
}
