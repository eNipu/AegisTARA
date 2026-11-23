//! Server side: is this request from a logged-in user?
//!
//! [`Authenticator::authenticate`] takes what an HTTP request carries (the
//! `Authorization` and `DPoP` headers, the method, and the URL) and returns
//! who is calling, or an error that means "401".
//!
//! # Access token (JWT) checks
//!
//! Signature against the issuer's JWKS (ES256 or RS256 only); `iss` equals
//! the configured issuer; `aud` contains the configured audience; `exp` is in
//! the future and `nbf`/`iat` are not, each with 60 s skew; `sub` is present
//! and short. The admin role comes from one configured claim.
//!
//! # DPoP policy
//!
//! - A token with `cnf.jkt` is a *bound* token. It is accepted only with the
//!   `DPoP` scheme and a proof signed by the bound key (RFC 9449 section 7.1).
//!   Sending it as `Bearer` is refused.
//! - [`DpopPolicy::Required`]: unbound tokens are refused too.
//! - [`DpopPolicy::Optional`]: an unbound token is accepted as `Bearer`.
//!   For identity providers that cannot bind tokens (some cloud IdPs). It
//!   weakens the guarantee to "whoever holds the token"; log it loudly.

use serde::Deserialize;

use crate::clock::Clock;
use crate::dpop::DpopVerifier;
use crate::error::AuthError;
use crate::http::HttpClient;
use crate::jose::Token;
use crate::jwks::JwksCache;

/// Clock skew allowed on `exp`, `nbf`, and `iat`, in seconds.
pub const SKEW_SECS: u64 = 60;
/// Longest subject accepted, in bytes.
const MAX_SUBJECT_BYTES: usize = 256;

/// Whether tokens must be bound to a device key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DpopPolicy {
    /// Every token must be DPoP-bound.
    Required,
    /// Unbound tokens are accepted as `Bearer`; bound tokens still need a proof.
    Optional,
}

/// A claim whose value makes the user an admin, for example
/// `roles` containing `aegis-admin`.
#[derive(Clone, Debug)]
pub struct AdminRule {
    /// Claim name. A string or an array of strings.
    pub claim: String,
    /// The value that grants admin.
    pub value: String,
}

/// Validator settings.
#[derive(Clone, Debug)]
pub struct ValidatorConfig {
    /// Expected `iss`.
    pub issuer: String,
    /// Expected `aud` (this server's identifier at the provider).
    pub audience: String,
    /// DPoP policy.
    pub dpop: DpopPolicy,
    /// How admins are recognised.
    pub admin: Option<AdminRule>,
}

/// What the server learns about the caller.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccessClaims {
    /// Verified token issuer (`iss`).
    pub issuer: String,
    /// Stable identity: the `sub` claim.
    pub subject: String,
    /// Whether the admin rule matched.
    pub admin: bool,
    /// Thumbprint the token is bound to, if any.
    pub jkt: Option<String>,
    /// Expiry, seconds since the epoch.
    pub expires: u64,
}

#[derive(Deserialize)]
struct RawClaims {
    iss: Option<String>,
    sub: Option<String>,
    aud: Option<serde_json::Value>,
    exp: Option<u64>,
    nbf: Option<u64>,
    iat: Option<u64>,
    cnf: Option<Cnf>,
    #[serde(flatten)]
    rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Deserialize)]
struct Cnf {
    jkt: Option<String>,
}

/// Checks signed claims shared by access tokens and ID tokens: signature,
/// `iss`, `aud`, and time. Returns the raw claim object.
pub(crate) async fn verify_jwt<H: HttpClient>(
    jwks: &JwksCache<H>,
    text: &str,
    issuer: &str,
    audience: &str,
    now: u64,
) -> Result<serde_json::Value, AuthError> {
    let token = Token::parse(text)?;
    let key = jwks
        .key_for(token.header.kid.as_deref(), token.alg()?)
        .await?;
    token.verify(&key)?;
    let value: serde_json::Value = token.claims()?;
    let claims: RawClaims =
        serde_json::from_value(value.clone()).map_err(|_| AuthError::Malformed)?;
    check_registered(&claims, issuer, audience, now)?;
    Ok(value)
}

fn check_registered(
    c: &RawClaims,
    issuer: &str,
    audience: &str,
    now: u64,
) -> Result<(), AuthError> {
    if c.iss.as_deref().ok_or(AuthError::Claim("iss"))? != issuer {
        return Err(AuthError::WrongIssuer);
    }
    let aud_ok = match c.aud.as_ref().ok_or(AuthError::Claim("aud"))? {
        serde_json::Value::String(a) => a == audience,
        serde_json::Value::Array(list) => list.iter().any(|a| a.as_str() == Some(audience)),
        _ => return Err(AuthError::Claim("aud")),
    };
    if !aud_ok {
        return Err(AuthError::WrongAudience);
    }
    let exp = c.exp.ok_or(AuthError::Claim("exp"))?;
    if exp.saturating_add(SKEW_SECS) <= now {
        return Err(AuthError::Expired);
    }
    if c.nbf.is_some_and(|nbf| nbf > now.saturating_add(SKEW_SECS))
        || c.iat.is_some_and(|iat| iat > now.saturating_add(SKEW_SECS))
    {
        return Err(AuthError::NotYetValid);
    }
    Ok(())
}

/// Validates access tokens.
pub struct AccessTokenValidator<H> {
    config: ValidatorConfig,
    jwks: JwksCache<H>,
}

impl<H: HttpClient> AccessTokenValidator<H> {
    /// A validator over the issuer's key cache.
    #[must_use]
    pub fn new(config: ValidatorConfig, jwks: JwksCache<H>) -> Self {
        AccessTokenValidator { config, jwks }
    }

    /// Validates one access token.
    ///
    /// # Errors
    ///
    /// The [`AuthError`] naming the failed check.
    pub async fn validate(&self, text: &str, now: u64) -> Result<AccessClaims, AuthError> {
        let value = verify_jwt(
            &self.jwks,
            text,
            &self.config.issuer,
            &self.config.audience,
            now,
        )
        .await?;
        let claims: RawClaims = serde_json::from_value(value).map_err(|_| AuthError::Malformed)?;
        let subject = claims
            .sub
            .filter(|s| !s.is_empty() && s.len() <= MAX_SUBJECT_BYTES)
            .ok_or(AuthError::Claim("sub"))?;
        let admin =
            self.config
                .admin
                .as_ref()
                .is_some_and(|rule| match claims.rest.get(&rule.claim) {
                    Some(serde_json::Value::String(v)) => v == &rule.value,
                    Some(serde_json::Value::Array(list)) => {
                        list.iter().any(|v| v.as_str() == Some(rule.value.as_str()))
                    }
                    _ => false,
                });
        Ok(AccessClaims {
            issuer: self.config.issuer.clone(),
            subject,
            admin,
            jkt: claims.cnf.and_then(|c| c.jkt),
            expires: claims.exp.ok_or(AuthError::Claim("exp"))?,
        })
    }
}

/// What an HTTP request offers as credentials.
#[derive(Clone, Copy, Debug)]
pub struct RequestCredentials<'a> {
    /// Value of the `Authorization` header.
    pub authorization: Option<&'a str>,
    /// Value of the `DPoP` header.
    pub dpop: Option<&'a str>,
    /// HTTP method of the request.
    pub method: &'a str,
    /// The request URL as the client built it (the server's public URL).
    pub url: &'a str,
}

/// Access token validation plus DPoP verification.
pub struct Authenticator<H> {
    validator: AccessTokenValidator<H>,
    dpop: DpopVerifier,
    policy: DpopPolicy,
    clock: Clock,
}

impl<H: HttpClient> Authenticator<H> {
    /// Combines the pieces. `config.dpop` is the policy.
    #[must_use]
    pub fn new(
        config: ValidatorConfig,
        jwks: JwksCache<H>,
        dpop: DpopVerifier,
        clock: Clock,
    ) -> Self {
        let policy = config.dpop;
        Authenticator {
            validator: AccessTokenValidator::new(config, jwks),
            dpop,
            policy,
            clock,
        }
    }

    /// Who is calling?
    ///
    /// # Errors
    ///
    /// Any [`AuthError`]; callers should answer `UNAUTHENTICATED` with a
    /// generic message and log the variant.
    pub async fn authenticate(
        &self,
        req: RequestCredentials<'_>,
    ) -> Result<AccessClaims, AuthError> {
        let now = (self.clock)();
        let header = req.authorization.ok_or(AuthError::Claim("authorization"))?;
        let (scheme, token) = header.split_once(' ').ok_or(AuthError::Malformed)?;
        let token = token.trim();
        let claims = self.validator.validate(token, now).await?;
        let proof_expected = match scheme {
            s if s.eq_ignore_ascii_case("DPoP") => true,
            s if s.eq_ignore_ascii_case("Bearer") => false,
            _ => return Err(AuthError::Malformed),
        };
        match (&claims.jkt, proof_expected) {
            // A bound token must come with its proof.
            (Some(jkt), true) => {
                let proof = req.dpop.ok_or(AuthError::Dpop("proof is missing"))?;
                self.dpop
                    .verify(proof, req.method, req.url, Some(token), Some(jkt), now)?;
            }
            (Some(_), false) => return Err(AuthError::Dpop("bound token sent as bearer")),
            // The scheme says DPoP but the token is not bound.
            (None, true) => return Err(AuthError::Dpop("token is not bound to a key")),
            (None, false) => {
                if self.policy == DpopPolicy::Required {
                    return Err(AuthError::DpopRequired);
                }
            }
        }
        Ok(claims)
    }
}
