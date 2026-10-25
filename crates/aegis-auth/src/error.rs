//! Errors of the login code.
//!
//! Messages are short and fixed. They never include token text, claim
//! values, or response bodies, because they end up in logs and in client
//! error dialogs. Details that help an operator (the HTTP status of the
//! identity provider, the name of the failing claim) are in the variants.

/// Everything that can go wrong while logging in or checking a token.
#[derive(Clone, Debug, thiserror::Error, PartialEq, Eq)]
pub enum AuthError {
    /// Not a JWS compact token, or a part is not valid base64url or JSON.
    #[error("token is malformed")]
    Malformed,
    /// A token, header, or response is larger than its cap (R3).
    #[error("input is too large")]
    TooLarge,
    /// The algorithm is not on the allow list (`none` and HMAC are never accepted).
    #[error("signature algorithm is not allowed")]
    UnsupportedAlgorithm,
    /// The signature does not verify.
    #[error("signature is invalid")]
    BadSignature,
    /// No key with the token's `kid` is published by the issuer.
    #[error("signing key is unknown")]
    UnknownKey,
    /// A key in a JWKS or a DPoP header is not usable.
    #[error("key is not supported")]
    BadKey,
    /// `exp` is in the past.
    #[error("token has expired")]
    Expired,
    /// `nbf` or `iat` is in the future.
    #[error("token is not valid yet")]
    NotYetValid,
    /// A required claim is missing or has the wrong type.
    #[error("claim {0} is missing or invalid")]
    Claim(&'static str),
    /// `iss` is not the configured issuer.
    #[error("issuer does not match")]
    WrongIssuer,
    /// `aud` does not contain the configured audience.
    #[error("audience does not match")]
    WrongAudience,
    /// The token is not bound to a DPoP key but the server requires it.
    #[error("token is not bound to a device key")]
    DpopRequired,
    /// The DPoP proof is missing, invalid, or does not match the request.
    #[error("DPoP proof rejected: {0}")]
    Dpop(&'static str),
    /// The DPoP proof was already used.
    #[error("DPoP proof was already used")]
    Replay,
    /// The OIDC `state` or `nonce` returned does not match the request.
    #[error("login response does not match the request")]
    StateMismatch,
    /// The identity provider answered with an OAuth error code.
    #[error("identity provider refused: {0}")]
    Idp(String),
    /// A network call failed or timed out.
    #[error("network error: {0}")]
    Http(String),
    /// The identity provider's response is not what the protocol says.
    #[error("unexpected response from the identity provider")]
    Protocol,
    /// The user did not finish logging in in time.
    #[error("login timed out")]
    Timeout,
    /// There is no session: log in first.
    #[error("not logged in")]
    LoginRequired,
    /// The OS keychain failed.
    #[error("secret store failed: {0}")]
    Store(String),
}
