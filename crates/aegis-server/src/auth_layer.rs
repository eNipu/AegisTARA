//! Server side login: a tower layer that authenticates every request.
//!
//! It runs before the gRPC service. For each request it rebuilds the URL the
//! client signed (the server's public URL plus the request path), asks the
//! [`Authenticator`] to check the access token and the DPoP proof, and either
//! attaches a [`Principal`] to the request or answers `UNAUTHENTICATED`
//! itself. The service never sees an unauthenticated request.
//!
//! The reply to a rejected request is always the same fixed message, so a
//! caller learns nothing about *why* it failed; the reason is logged. Every
//! rejection is a `warn!` with the error variant only (never a token).
//!
//! One path prefix passes without credentials: the public `Accounts`
//! service (`/aegis.v1.Accounts/`), which only tells a client how to log
//! in and checks passwords. Everything else needs a principal.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use aegis_auth::http::HttpClient;
use aegis_auth::validate::{Authenticator, RequestCredentials};
use tonic::Status;
use tower::{Layer, Service};

use crate::accounts::service::PUBLIC_PATH_PREFIX;
use crate::accounts::{Accounts, bearer};
use crate::auth::{AuthMethod, DEV_ADMIN_HEADER, DEV_USER_HEADER, Principal};

/// What an authentication method may look at: the headers, the method, and
/// the URL the client signed.
#[derive(Debug)]
pub struct RequestInfo {
    /// Request headers.
    pub headers: http::HeaderMap,
    /// HTTP method (always `POST` for gRPC).
    pub method: String,
    /// The public URL of the call: the server's public URL plus the path.
    pub url: String,
}

impl RequestInfo {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).and_then(|v| v.to_str().ok())
    }
}

/// Turns a request into a caller identity.
pub trait Authn: Send + Sync + 'static {
    /// Who is calling? An `Err` means the request is refused; the text goes
    /// to the log, never to the caller.
    fn authenticate(
        &self,
        request: &RequestInfo,
    ) -> impl Future<Output = Result<Principal, String>> + Send;
}

/// OIDC access tokens with DPoP (the production method).
pub struct OidcAuthn<H>(pub Arc<Authenticator<H>>);

impl<H: HttpClient> Authn for OidcAuthn<H> {
    async fn authenticate(&self, request: &RequestInfo) -> Result<Principal, String> {
        let credentials = RequestCredentials {
            authorization: request.header("authorization"),
            dpop: request.header("dpop"),
            method: &request.method,
            url: &request.url,
        };
        let claims = self
            .0
            .authenticate(credentials)
            .await
            .map_err(|e| e.to_string())?;
        Ok(Principal {
            subject: claims.subject.into(),
            admin: claims.admin,
            method: AuthMethod::Oidc,
        })
    }
}

/// Password sessions of this server (`AEGIS_AUTH=password`): the bearer
/// token is looked up in [`Accounts`]. O(1), no I/O.
pub struct PasswordAuthn(pub Arc<Accounts>);

impl Authn for PasswordAuthn {
    async fn authenticate(&self, request: &RequestInfo) -> Result<Principal, String> {
        let token = bearer(&request.headers).ok_or_else(|| "no bearer token".to_owned())?;
        self.0
            .authenticate(token)
            .ok_or_else(|| "no open session for this token".to_owned())
    }
}

/// Company login and password login side by side (`AEGIS_AUTH=both`, job
/// 045). A bearer token with the `ats_` prefix is a password session; anything
/// else goes to OIDC. The prefix cannot appear in a JWT, so one kind of
/// credential is never tried as the other. When an admin turns password login
/// off, [`Accounts::authenticate`] answers `None` and password tokens fail.
pub struct HybridAuthn<H> {
    /// Password sessions.
    pub password: PasswordAuthn,
    /// Company login.
    pub oidc: OidcAuthn<H>,
}

impl<H: HttpClient> Authn for HybridAuthn<H> {
    async fn authenticate(&self, request: &RequestInfo) -> Result<Principal, String> {
        let is_password =
            bearer(&request.headers).is_some_and(|t| t.starts_with(crate::accounts::TOKEN_PREFIX));
        if is_password {
            self.password.authenticate(request).await
        } else {
            self.oidc.authenticate(request).await
        }
    }
}

/// Trusts the `x-aegis-dev-user` header. **Development only**: it checks
/// nothing. The binary enables it only with `AEGIS_DEV_AUTH=1`.
pub struct DevAuthn;

impl Authn for DevAuthn {
    async fn authenticate(&self, request: &RequestInfo) -> Result<Principal, String> {
        let user = request
            .header(DEV_USER_HEADER)
            .filter(|u| !u.is_empty() && u.len() <= 128)
            .ok_or_else(|| "missing development user".to_owned())?;
        Ok(Principal {
            subject: user.into(),
            admin: request.header(DEV_ADMIN_HEADER) == Some("1"),
            method: AuthMethod::Development,
        })
    }
}

/// Wraps a service with authentication.
pub struct AuthLayer<A> {
    authn: Arc<A>,
    public_url: Arc<str>,
}

impl<A> AuthLayer<A> {
    /// `public_url` is the URL clients use to reach this server, for example
    /// `https://aegis.example.com`. It must match the client's, or no DPoP
    /// proof will verify.
    #[must_use]
    pub fn new(authn: Arc<A>, public_url: &str) -> Self {
        AuthLayer {
            authn,
            public_url: public_url.trim_end_matches('/').into(),
        }
    }
}

impl<A> Clone for AuthLayer<A> {
    fn clone(&self) -> Self {
        AuthLayer {
            authn: Arc::clone(&self.authn),
            public_url: Arc::clone(&self.public_url),
        }
    }
}

impl<A, S> Layer<S> for AuthLayer<A> {
    type Service = Authenticated<A, S>;

    fn layer(&self, inner: S) -> Authenticated<A, S> {
        Authenticated {
            inner,
            authn: Arc::clone(&self.authn),
            public_url: Arc::clone(&self.public_url),
        }
    }
}

/// The service [`AuthLayer`] builds.
pub struct Authenticated<A, S> {
    inner: S,
    authn: Arc<A>,
    public_url: Arc<str>,
}

impl<A, S: Clone> Clone for Authenticated<A, S> {
    fn clone(&self) -> Self {
        Authenticated {
            inner: self.inner.clone(),
            authn: Arc::clone(&self.authn),
            public_url: Arc::clone(&self.public_url),
        }
    }
}

impl<A, S, B, ResBody> Service<http::Request<B>> for Authenticated<A, S>
where
    A: Authn,
    S: Service<http::Request<B>, Response = http::Response<ResBody>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Send + 'static,
    B: Send + 'static,
    ResBody: Default + Send + 'static,
{
    type Response = http::Response<ResBody>;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, S::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), S::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut request: http::Request<B>) -> Self::Future {
        let clone = self.inner.clone();
        let mut inner = std::mem::replace(&mut self.inner, clone);
        if request.uri().path().starts_with(PUBLIC_PATH_PREFIX) {
            return Box::pin(async move { inner.call(request).await });
        }
        let authn = Arc::clone(&self.authn);
        // Copy what is needed out of the request: the body type is `Send`
        // but not `Sync`, so the request itself cannot be borrowed across
        // the await below.
        let info = RequestInfo {
            headers: request.headers().clone(),
            method: request.method().as_str().to_owned(),
            url: format!("{}{}", self.public_url, request.uri().path()),
        };
        Box::pin(async move {
            match authn.authenticate(&info).await {
                Ok(principal) => {
                    request.extensions_mut().insert(principal);
                    inner.call(request).await
                }
                Err(error) => {
                    tracing::warn!(%error, "request refused");
                    Ok(Status::unauthenticated("authentication failed").into_http())
                }
            }
        })
    }
}
