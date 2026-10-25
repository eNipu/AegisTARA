//! Client side: sign every outgoing request.
//!
//! [`SignRequests`] wraps any HTTP service (a tonic `Channel`) and, before
//! each request, asks a [`CredentialProvider`] for the `Authorization` and
//! `DPoP` headers for that exact method and URL. It is a layer, not a tonic
//! interceptor, because the DPoP proof covers the request URL and an
//! interceptor never sees it.
//!
//! If the provider fails (not logged in, refresh refused) the request is not
//! sent and the call fails with an error that wraps the [`AuthError`], so the
//! UI can find it with [`auth_error_of`] and show the login screen.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use http::HeaderValue;
use tower::{Layer, Service};

use crate::error::AuthError;
use crate::http::HttpClient;
use crate::session::{Credentials, Session};
use crate::store::SecretStore;

/// Boxed error type of tonic and tower services.
pub type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// Supplies request credentials.
pub trait CredentialProvider: Send + Sync + 'static {
    /// The headers for a request with this method and URL.
    fn credentials(
        &self,
        method: &str,
        url: &str,
    ) -> impl Future<Output = Result<Credentials, AuthError>> + Send;
}

impl<H: HttpClient, S: SecretStore + 'static> CredentialProvider for Session<H, S> {
    fn credentials(
        &self,
        method: &str,
        url: &str,
    ) -> impl Future<Output = Result<Credentials, AuthError>> + Send {
        Session::credentials(self, method, url)
    }
}

/// A request failed because credentials could not be produced.
#[derive(Debug, thiserror::Error)]
#[error("cannot sign request: {0}")]
pub struct SigningFailed(pub AuthError);

/// Finds the [`AuthError`] inside a call error, if signing caused it.
#[must_use]
pub fn auth_error_of<'a>(error: &'a (dyn std::error::Error + 'static)) -> Option<&'a AuthError> {
    let mut current = Some(error);
    while let Some(e) = current {
        if let Some(signing) = e.downcast_ref::<SigningFailed>() {
            return Some(&signing.0);
        }
        current = e.source();
    }
    None
}

/// The layer. `base_url` is the server's public URL, for example
/// `https://aegis.example.com`; it must be what the server uses to rebuild
/// the URL a proof was made for.
pub struct SignRequests<P> {
    provider: Arc<P>,
    base_url: Arc<str>,
}

impl<P> SignRequests<P> {
    /// A layer over `provider`.
    #[must_use]
    pub fn new(provider: Arc<P>, base_url: &str) -> Self {
        SignRequests {
            provider,
            base_url: base_url.trim_end_matches('/').into(),
        }
    }
}

impl<P> Clone for SignRequests<P> {
    fn clone(&self) -> Self {
        SignRequests {
            provider: Arc::clone(&self.provider),
            base_url: Arc::clone(&self.base_url),
        }
    }
}

impl<P, S> Layer<S> for SignRequests<P> {
    type Service = Signed<P, S>;

    fn layer(&self, inner: S) -> Signed<P, S> {
        Signed {
            inner,
            provider: Arc::clone(&self.provider),
            base_url: Arc::clone(&self.base_url),
        }
    }
}

/// The service [`SignRequests`] builds.
pub struct Signed<P, S> {
    inner: S,
    provider: Arc<P>,
    base_url: Arc<str>,
}

impl<P, S: Clone> Clone for Signed<P, S> {
    fn clone(&self) -> Self {
        Signed {
            inner: self.inner.clone(),
            provider: Arc::clone(&self.provider),
            base_url: Arc::clone(&self.base_url),
        }
    }
}

impl<P, S, B> Service<http::Request<B>> for Signed<P, S>
where
    P: CredentialProvider,
    S: Service<http::Request<B>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    S::Response: Send + 'static,
    S::Error: Into<BoxError>,
    B: Send + 'static,
{
    type Response = S::Response;
    type Error = BoxError;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, BoxError>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), BoxError>> {
        self.inner.poll_ready(cx).map_err(Into::into)
    }

    fn call(&mut self, mut request: http::Request<B>) -> Self::Future {
        // `poll_ready` was called on `self.inner`; hand that instance to the
        // future and keep a fresh clone for the next call.
        let clone = self.inner.clone();
        let mut inner = std::mem::replace(&mut self.inner, clone);
        let provider = Arc::clone(&self.provider);
        let url = format!("{}{}", self.base_url, request.uri().path());
        Box::pin(async move {
            let creds = provider
                .credentials(request.method().as_str(), &url)
                .await
                .map_err(|e| Box::new(SigningFailed(e)) as BoxError)?;
            let headers = request.headers_mut();
            let value = |text: &str| {
                HeaderValue::from_str(text)
                    .map_err(|_| Box::new(SigningFailed(AuthError::Malformed)) as BoxError)
            };
            let mut authorization = value(&creds.authorization)?;
            authorization.set_sensitive(true);
            headers.insert(http::header::AUTHORIZATION, authorization);
            if let Some(proof) = &creds.dpop {
                let mut proof = value(proof)?;
                proof.set_sensitive(true);
                headers.insert("dpop", proof);
            }
            inner.call(request).await.map_err(Into::into)
        })
    }
}

#[cfg(test)]
mod tests {
    use std::convert::Infallible;

    use super::*;

    struct Fixed(Result<Credentials, AuthError>);

    impl CredentialProvider for Fixed {
        async fn credentials(&self, _: &str, _: &str) -> Result<Credentials, AuthError> {
            self.0.clone()
        }
    }

    /// Echoes the request headers back as the response.
    #[derive(Clone)]
    struct Echo;

    impl Service<http::Request<()>> for Echo {
        type Response = http::HeaderMap;
        type Error = Infallible;
        type Future = std::future::Ready<Result<http::HeaderMap, Infallible>>;

        fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Infallible>> {
            Poll::Ready(Ok(()))
        }

        fn call(&mut self, request: http::Request<()>) -> Self::Future {
            std::future::ready(Ok(request.headers().clone()))
        }
    }

    #[tokio::test]
    async fn headers_are_added_and_marked_sensitive() {
        let creds = Credentials {
            authorization: "DPoP tok".into(),
            dpop: Some("proof".into()),
        };
        let layer = SignRequests::new(Arc::new(Fixed(Ok(creds))), "https://h/");
        let mut service = layer.layer(Echo);
        let request = http::Request::post("/aegis.v1.Aegis/Watch")
            .body(())
            .expect("request");
        let headers = service.call(request).await.expect("sent");
        assert_eq!(headers["authorization"], "DPoP tok");
        assert!(headers["authorization"].is_sensitive());
        assert_eq!(headers["dpop"], "proof");
        assert!(headers["dpop"].is_sensitive());
    }

    #[tokio::test]
    async fn bearer_credentials_add_no_proof_header() {
        let creds = Credentials {
            authorization: "Bearer tok".into(),
            dpop: None,
        };
        let mut service = SignRequests::new(Arc::new(Fixed(Ok(creds))), "https://h").layer(Echo);
        let headers = service
            .call(http::Request::post("/x").body(()).expect("request"))
            .await
            .expect("sent");
        assert!(headers.get("dpop").is_none());
    }

    #[tokio::test]
    async fn a_signing_failure_stops_the_request_and_can_be_found_again() {
        let mut service =
            SignRequests::new(Arc::new(Fixed(Err(AuthError::LoginRequired))), "https://h")
                .layer(Echo);
        let error = service
            .call(http::Request::post("/x").body(()).expect("request"))
            .await
            .expect_err("not sent");
        assert_eq!(auth_error_of(&*error), Some(&AuthError::LoginRequired));
    }
}
