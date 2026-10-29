//! The OIDC authorization code flow, client side.
//!
//! ```text
//! discover ──► begin ──► (browser, loopback) ──► exchange ──► TokenSet
//!                                                     │
//!                                       refresh ◄─────┘ (silent, later)
//! ```
//!
//! What it enforces:
//!
//! - **PKCE S256** on every authorization request.
//! - **`state`** (random, checked by the loopback receiver) and **`nonce`**
//!   (random, checked in the ID token).
//! - **DPoP** on every token request, with `dpop_jkt` on the authorization
//!   request, so the code and the tokens are bound to the device key. If the
//!   provider asks for a nonce (`use_dpop_nonce`), the request is retried
//!   once with it.
//! - The ID token's signature, `iss`, `aud` (the client ID), `exp`, and
//!   `nonce`.
//! - Discovery's `issuer` must equal the configured issuer, and endpoints
//!   must be `https` (or loopback `http` for development).
//!
//! The provider's error body is never shown: only its short OAuth error
//! code, filtered to safe characters.

use std::sync::Arc;

use serde::Deserialize;

use crate::b64;
use crate::clock::Clock;
use crate::dpop::DeviceKey;
use crate::error::AuthError;
use crate::http::{HttpClient, HttpRequest, form_encode, is_loopback_url};
use crate::jwks::JwksCache;
use crate::loopback::sanitize_code;
use crate::pkce::Pkce;
use crate::validate::verify_jwt;

/// Default access token lifetime if the provider does not say.
const DEFAULT_LIFETIME_SECS: u64 = 300;

/// Settings of this client at the provider.
#[derive(Clone, Debug)]
pub struct OidcConfig {
    /// The provider's issuer URL.
    pub issuer: String,
    /// The public client ID registered for AegisTARA.
    pub client_id: String,
    /// Requested scopes.
    pub scopes: Vec<String>,
    /// The API the access token is for (RFC 8707 `resource`), if the provider
    /// uses it to choose the token audience.
    pub resource: Option<String>,
}

impl OidcConfig {
    /// A config with the scopes AegisTARA needs: `openid`, `profile`,
    /// `offline_access` (for the refresh token).
    #[must_use]
    pub fn new(issuer: &str, client_id: &str) -> Self {
        OidcConfig {
            issuer: issuer.to_owned(),
            client_id: client_id.to_owned(),
            scopes: ["openid", "profile", "offline_access"]
                .map(str::to_owned)
                .to_vec(),
            resource: None,
        }
    }
}

#[derive(Deserialize)]
struct DiscoveryDoc {
    issuer: String,
    authorization_endpoint: String,
    token_endpoint: String,
    jwks_uri: String,
}

/// The provider's endpoints, from its discovery document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Discovery {
    /// Issuer, equal to the configured one.
    pub issuer: String,
    /// Where the browser goes to log in.
    pub authorization_endpoint: String,
    /// Where codes and refresh tokens are exchanged.
    pub token_endpoint: String,
    /// Where signing keys are published.
    pub jwks_uri: String,
}

fn secure_endpoint(url: &str) -> bool {
    url.starts_with("https://") || (url.starts_with("http://") && is_loopback_url(url))
}

/// Fetches and validates `<issuer>/.well-known/openid-configuration`.
///
/// # Errors
///
/// [`AuthError::Protocol`] for a bad document, an issuer mismatch, or an
/// endpoint that is not `https`; network errors otherwise.
pub async fn discover<H: HttpClient>(http: &H, issuer: &str) -> Result<Discovery, AuthError> {
    let url = format!(
        "{}/.well-known/openid-configuration",
        issuer.trim_end_matches('/')
    );
    let response = http.send(HttpRequest::get(&url)).await?;
    if response.status != 200 {
        return Err(AuthError::Http(format!(
            "discovery answered {}",
            response.status
        )));
    }
    let doc: DiscoveryDoc =
        serde_json::from_slice(&response.body).map_err(|_| AuthError::Protocol)?;
    if doc.issuer != issuer {
        return Err(AuthError::Protocol);
    }
    for endpoint in [
        &doc.authorization_endpoint,
        &doc.token_endpoint,
        &doc.jwks_uri,
    ] {
        if !secure_endpoint(endpoint) {
            return Err(AuthError::Protocol);
        }
    }
    Ok(Discovery {
        issuer: doc.issuer,
        authorization_endpoint: doc.authorization_endpoint,
        token_endpoint: doc.token_endpoint,
        jwks_uri: doc.jwks_uri,
    })
}

/// A login that has started and is waiting for the browser.
#[derive(Clone, Debug)]
pub struct PendingLogin {
    authorization_url: String,
    state: String,
    nonce: String,
    pkce: Pkce,
    redirect_uri: String,
}

impl PendingLogin {
    /// The URL to open in the browser.
    #[must_use]
    pub fn authorization_url(&self) -> &str {
        &self.authorization_url
    }

    /// The `state` the redirect must carry.
    #[must_use]
    pub fn state(&self) -> &str {
        &self.state
    }
}

/// Tokens from the provider.
#[derive(Clone, PartialEq, Eq)]
pub struct TokenSet {
    /// The access token.
    pub access_token: String,
    /// `DPoP` or `Bearer`: the scheme to send it with.
    pub token_type: String,
    /// The refresh token, if the provider issued one.
    pub refresh_token: Option<String>,
    /// Absolute expiry of the access token, seconds since the epoch.
    pub expires_at: u64,
}

impl std::fmt::Debug for TokenSet {
    // Tokens are secrets; logs must not carry them.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TokenSet")
            .field("token_type", &self.token_type)
            .field("expires_at", &self.expires_at)
            .field("has_refresh_token", &self.refresh_token.is_some())
            .finish_non_exhaustive()
    }
}

impl Drop for TokenSet {
    fn drop(&mut self) {
        crate::pkce::zeroize_string(&mut self.access_token);
        if let Some(refresh) = &mut self.refresh_token {
            crate::pkce::zeroize_string(refresh);
        }
    }
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    token_type: String,
    #[serde(default)]
    expires_in: Option<u64>,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    id_token: Option<String>,
}

impl Drop for TokenResponse {
    fn drop(&mut self) {
        crate::pkce::zeroize_string(&mut self.access_token);
        if let Some(refresh) = &mut self.refresh_token {
            crate::pkce::zeroize_string(refresh);
        }
        if let Some(id) = &mut self.id_token {
            crate::pkce::zeroize_string(id);
        }
    }
}

#[derive(Deserialize)]
struct ErrorResponse {
    error: String,
}

/// The OIDC client of one provider.
pub struct OidcClient<H> {
    http: Arc<H>,
    config: OidcConfig,
    discovery: Discovery,
    jwks: JwksCache<H>,
    clock: Clock,
}

impl<H: HttpClient> OidcClient<H> {
    /// Discovers the provider and builds the client.
    ///
    /// # Errors
    ///
    /// As [`discover`].
    pub async fn connect(
        http: Arc<H>,
        config: OidcConfig,
        clock: Clock,
    ) -> Result<Self, AuthError> {
        let discovery = discover(&*http, &config.issuer).await?;
        let jwks = JwksCache::new(Arc::clone(&http), &discovery.jwks_uri, clock);
        Ok(OidcClient {
            http,
            config,
            discovery,
            jwks,
            clock,
        })
    }

    /// The discovered endpoints.
    #[must_use]
    pub fn discovery(&self) -> &Discovery {
        &self.discovery
    }

    /// Starts a login: new PKCE pair, `state`, and `nonce`, and the URL to
    /// open. `key` is the device key the tokens will be bound to.
    ///
    /// # Errors
    ///
    /// [`AuthError::Protocol`] if the OS random generator fails.
    pub fn begin(&self, redirect_uri: &str, key: &DeviceKey) -> Result<PendingLogin, AuthError> {
        let pkce = Pkce::generate()?;
        let state = random_token()?;
        let nonce = random_token()?;
        let scope = self.config.scopes.join(" ");
        let mut fields = vec![
            ("response_type", "code"),
            ("client_id", self.config.client_id.as_str()),
            ("redirect_uri", redirect_uri),
            ("scope", scope.as_str()),
            ("state", state.as_str()),
            ("nonce", nonce.as_str()),
            ("code_challenge", pkce.challenge()),
            ("code_challenge_method", "S256"),
            ("dpop_jkt", key.thumbprint()),
        ];
        if let Some(resource) = &self.config.resource {
            fields.push(("resource", resource.as_str()));
        }
        let endpoint = &self.discovery.authorization_endpoint;
        let sep = if endpoint.contains('?') { '&' } else { '?' };
        Ok(PendingLogin {
            authorization_url: format!("{endpoint}{sep}{}", form_encode(&fields)),
            state,
            nonce,
            pkce,
            redirect_uri: redirect_uri.to_owned(),
        })
    }

    /// Exchanges the authorization code for tokens and checks the ID token.
    ///
    /// # Errors
    ///
    /// [`AuthError::Idp`] with the provider's error code,
    /// [`AuthError::StateMismatch`] for a wrong `nonce`, the JWT errors for a
    /// bad ID token, or network errors.
    pub async fn exchange(
        &self,
        pending: &PendingLogin,
        code: &str,
        key: &DeviceKey,
    ) -> Result<TokenSet, AuthError> {
        let fields = [
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", pending.redirect_uri.as_str()),
            ("client_id", self.config.client_id.as_str()),
            ("code_verifier", pending.pkce.verifier()),
        ];
        let response = self.token_request(&fields, key).await?;
        let now = (self.clock)();
        // An ID token is optional in the response but, when present, must be
        // for this login.
        if let Some(id_token) = &response.id_token {
            let claims = verify_jwt(
                &self.jwks,
                id_token,
                &self.discovery.issuer,
                &self.config.client_id,
                now,
            )
            .await?;
            if claims.get("nonce").and_then(|n| n.as_str()) != Some(pending.nonce.as_str()) {
                return Err(AuthError::StateMismatch);
            }
        }
        Ok(token_set(response, now))
    }

    /// Gets a new access token with a refresh token. The provider may rotate
    /// the refresh token; the returned set carries the one to keep.
    ///
    /// # Errors
    ///
    /// [`AuthError::Idp`] (`invalid_grant`) when the refresh token is no
    /// longer valid and the user must log in again.
    pub async fn refresh(
        &self,
        refresh_token: &str,
        key: &DeviceKey,
    ) -> Result<TokenSet, AuthError> {
        let fields = [
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", self.config.client_id.as_str()),
        ];
        let response = self.token_request(&fields, key).await?;
        let mut set = token_set(response, (self.clock)());
        // Providers without rotation omit the field; keep using the old one.
        set.refresh_token
            .get_or_insert_with(|| refresh_token.to_owned());
        Ok(set)
    }

    async fn token_request(
        &self,
        fields: &[(&str, &str)],
        key: &DeviceKey,
    ) -> Result<TokenResponse, AuthError> {
        let url = &self.discovery.token_endpoint;
        let mut nonce: Option<String> = None;
        for attempt in 0..2 {
            let proof = key.proof("POST", url, None, nonce.as_deref(), (self.clock)())?;
            let request = HttpRequest::post_form(url, fields).with_header("dpop", proof);
            let response = self.http.send(request).await?;
            if response.status == 200 {
                return serde_json::from_slice(&response.body).map_err(|_| AuthError::Protocol);
            }
            let error = serde_json::from_slice::<ErrorResponse>(&response.body)
                .map(|e| sanitize_code(&e.error))
                .unwrap_or_default();
            // RFC 9449 section 8: retry once with the nonce the server chose.
            if attempt == 0
                && error == "use_dpop_nonce"
                && let Some(n) = response.header("dpop-nonce")
            {
                nonce = Some(n.to_owned());
                continue;
            }
            return Err(if error.is_empty() {
                AuthError::Http(format!("token endpoint answered {}", response.status))
            } else {
                AuthError::Idp(error)
            });
        }
        Err(AuthError::Protocol)
    }
}

fn token_set(mut r: TokenResponse, now: u64) -> TokenSet {
    TokenSet {
        access_token: std::mem::take(&mut r.access_token),
        token_type: if r.token_type.eq_ignore_ascii_case("dpop") {
            "DPoP".to_owned()
        } else {
            "Bearer".to_owned()
        },
        refresh_token: r.refresh_token.take(),
        expires_at: now.saturating_add(r.expires_in.unwrap_or(DEFAULT_LIFETIME_SECS)),
    }
}

/// 256 random bits as base64url, for `state` and `nonce`.
fn random_token() -> Result<String, AuthError> {
    use ring::rand::{SecureRandom, SystemRandom};
    let mut raw = [0u8; 32];
    SystemRandom::new()
        .fill(&mut raw)
        .map_err(|_| AuthError::Protocol)?;
    let encoded = b64::encode(raw);
    crate::pkce::zeroize_bytes(&mut raw);
    Ok(encoded)
}
