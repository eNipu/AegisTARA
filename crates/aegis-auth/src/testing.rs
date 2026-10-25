//! A mock OpenID Connect provider on a loopback port, for tests.
//!
//! It speaks enough of the protocol to exercise the real client and server
//! code over real HTTP: discovery, JWKS, an authorization endpoint that
//! "logs the user in" and redirects, and a token endpoint with PKCE and DPoP
//! checks, refresh token rotation, and optional DPoP nonces. Behaviour
//! switches on [`MockIdp`] let tests ask for the awkward cases (unbound
//! tokens, rotated signing keys, revoked refresh tokens, short lifetimes).
//!
//! Enabled by the `test-support` feature. It is test scaffolding: the
//! signing keys are generated per instance and nothing here is secure.

use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::{Arc, Mutex};

use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_util::rt::TokioIo;
use ring::rand::SystemRandom;
use ring::signature::{ECDSA_P256_SHA256_FIXED_SIGNING, EcdsaKeyPair, KeyPair};
use tokio::net::TcpListener;

use crate::b64;
use crate::clock::system_clock;
use crate::dpop::DpopVerifier;
use crate::error::AuthError;
use crate::jose::Jwk;
use crate::session::BrowserLauncher;

/// The client ID the mock accepts.
pub const CLIENT_ID: &str = "aegis-desktop";
/// The audience of the access tokens it issues.
pub const AUDIENCE: &str = "aegis-server";

struct SigningKey {
    kid: String,
    pair: EcdsaKeyPair,
    jwk: Jwk,
}

fn new_key(kid: &str) -> SigningKey {
    let rng = SystemRandom::new();
    let pkcs8 = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &rng).expect("key");
    let pair = EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, pkcs8.as_ref(), &rng)
        .expect("key");
    let public = pair.public_key().as_ref().to_vec();
    let jwk = Jwk {
        kty: "EC".into(),
        crv: Some("P-256".into()),
        x: Some(b64::encode(&public[1..33])),
        y: Some(b64::encode(&public[33..65])),
        kid: Some(kid.into()),
        alg: Some("ES256".into()),
        use_: Some("sig".into()),
        ..Jwk::default()
    };
    SigningKey {
        kid: kid.into(),
        pair,
        jwk,
    }
}

struct CodeGrant {
    client_id: String,
    redirect_uri: String,
    challenge: String,
    nonce: String,
    dpop_jkt: Option<String>,
}

struct RefreshGrant {
    jkt: Option<String>,
}

struct State {
    issuer: String,
    signing: SigningKey,
    retired: Vec<SigningKey>,
    codes: HashMap<String, CodeGrant>,
    refresh: HashMap<String, RefreshGrant>,
    counter: u64,
    // Behaviour switches.
    bind_tokens: bool,
    require_nonce: bool,
    issued_nonce: String,
    access_ttl: u64,
    subject: String,
    roles: Vec<String>,
    token_requests: u64,
    jwks_requests: u64,
    dpop: Arc<DpopVerifier>,
}

/// A running mock provider.
#[derive(Clone)]
pub struct MockIdp {
    state: Arc<Mutex<State>>,
    issuer: String,
}

impl MockIdp {
    /// Starts a provider on a free loopback port.
    ///
    /// # Panics
    ///
    /// If the port cannot be bound (tests only).
    pub async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let issuer = format!("http://{}", listener.local_addr().expect("addr"));
        let state = Arc::new(Mutex::new(State {
            issuer: issuer.clone(),
            signing: new_key("key-1"),
            retired: Vec::new(),
            codes: HashMap::new(),
            refresh: HashMap::new(),
            counter: 0,
            bind_tokens: true,
            require_nonce: false,
            issued_nonce: "server-nonce-1".into(),
            access_ttl: 600,
            subject: "alice".into(),
            roles: Vec::new(),
            token_requests: 0,
            jwks_requests: 0,
            dpop: Arc::new(DpopVerifier::standard()),
        }));
        let shared = Arc::clone(&state);
        tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    continue;
                };
                let state = Arc::clone(&shared);
                tokio::spawn(async move {
                    let service = service_fn(move |req| handle(Arc::clone(&state), req));
                    let _ = hyper::server::conn::http1::Builder::new()
                        .serve_connection(TokioIo::new(stream), service)
                        .await;
                });
            }
        });
        MockIdp { state, issuer }
    }

    /// The issuer URL (`http://127.0.0.1:<port>`).
    #[must_use]
    pub fn issuer(&self) -> &str {
        &self.issuer
    }

    fn with<R>(&self, f: impl FnOnce(&mut State) -> R) -> R {
        f(&mut self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner))
    }

    /// Whether issued tokens carry `cnf.jkt` (default) or are plain bearer.
    pub fn bind_tokens(&self, yes: bool) {
        self.with(|s| s.bind_tokens = yes);
    }

    /// Makes the token endpoint demand a DPoP nonce (RFC 9449 section 8).
    pub fn require_dpop_nonce(&self, yes: bool) {
        self.with(|s| s.require_nonce = yes);
    }

    /// Access token lifetime in seconds.
    pub fn access_ttl(&self, secs: u64) {
        self.with(|s| s.access_ttl = secs);
    }

    /// Who logs in.
    pub fn user(&self, subject: &str, roles: &[&str]) {
        self.with(|s| {
            s.subject = subject.to_owned();
            s.roles = roles.iter().map(|r| (*r).to_owned()).collect();
        });
    }

    /// Switches to a new signing key (the old one stays published).
    pub fn rotate_keys(&self, kid: &str) {
        self.with(|s| {
            let old = std::mem::replace(&mut s.signing, new_key(kid));
            s.retired.push(old);
        });
    }

    /// Drops every refresh token, as if the user's session were revoked.
    pub fn revoke_refresh_tokens(&self) {
        self.with(|s| s.refresh.clear());
    }

    /// How many requests the token endpoint has served.
    #[must_use]
    pub fn token_requests(&self) -> u64 {
        self.with(|s| s.token_requests)
    }

    /// How many times the JWKS document has been fetched.
    #[must_use]
    pub fn jwks_requests(&self) -> u64 {
        self.with(|s| s.jwks_requests)
    }

    /// Signs an access token as the provider would, for server tests that
    /// skip the login flow. `jkt` binds it to a device key.
    #[must_use]
    pub fn access_token(
        &self,
        subject: &str,
        roles: &[&str],
        jkt: Option<&str>,
        ttl: u64,
    ) -> String {
        self.with(|s| {
            let roles: Vec<String> = roles.iter().map(|r| (*r).to_owned()).collect();
            mint_access(s, subject, &roles, jkt, ttl)
        })
    }

    /// A "browser" that follows the authorization URL and the redirect to
    /// the loopback listener, as a logged-in user would.
    #[must_use]
    pub fn browser(&self) -> MockBrowser {
        MockBrowser
    }
}

/// Plays the user's browser against [`MockIdp`].
pub struct MockBrowser;

impl BrowserLauncher for MockBrowser {
    fn open(&self, url: &str) -> Result<(), AuthError> {
        let url = url.to_owned();
        tokio::spawn(async move {
            use crate::http::{HttpClient, HttpRequest, HyperClient};
            let client = HyperClient::new();
            let Ok(first) = client.send(HttpRequest::get(&url)).await else {
                return;
            };
            if let Some(location) = first.header("location") {
                let _ = client.send(HttpRequest::get(location)).await;
            }
        });
        Ok(())
    }
}

fn sign(key: &SigningKey, claims: &serde_json::Value) -> String {
    let header = serde_json::json!({"alg": "ES256", "typ": "JWT", "kid": key.kid});
    let input = format!(
        "{}.{}",
        b64::encode(header.to_string()),
        b64::encode(claims.to_string())
    );
    let sig = key
        .pair
        .sign(&SystemRandom::new(), input.as_bytes())
        .expect("sign");
    format!("{input}.{}", b64::encode(sig.as_ref()))
}

fn mint_access(s: &State, subject: &str, roles: &[String], jkt: Option<&str>, ttl: u64) -> String {
    let now = system_clock();
    let mut claims = serde_json::json!({
        "iss": s.issuer,
        "sub": subject,
        "aud": AUDIENCE,
        "iat": now,
        "exp": now + ttl,
        "roles": roles,
    });
    if let Some(jkt) = jkt {
        claims["cnf"] = serde_json::json!({ "jkt": jkt });
    }
    sign(&s.signing, &claims)
}

type Resp = Response<Full<Bytes>>;

fn json(status: u16, body: &serde_json::Value) -> Resp {
    Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(Full::new(Bytes::from(body.to_string())))
        .expect("response")
}

fn query(uri: &hyper::Uri) -> HashMap<String, String> {
    parse_form(uri.query().unwrap_or(""))
}

fn parse_form(text: &str) -> HashMap<String, String> {
    text.split('&')
        .filter(|p| !p.is_empty())
        .filter_map(|p| {
            let (k, v) = p.split_once('=')?;
            let dec = |s: &str| {
                percent_encoding::percent_decode_str(&s.replace('+', " "))
                    .decode_utf8()
                    .map(|c| c.into_owned())
                    .ok()
            };
            Some((dec(k)?, dec(v)?))
        })
        .collect()
}

async fn handle(state: Arc<Mutex<State>>, req: Request<Incoming>) -> Result<Resp, Infallible> {
    let (parts, body) = req.into_parts();
    let body = body
        .collect()
        .await
        .map(|b| b.to_bytes())
        .unwrap_or_default();
    let mut s = state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let issuer = s.issuer.clone();
    Ok(match (parts.method.as_str(), parts.uri.path()) {
        ("GET", "/.well-known/openid-configuration") => json(
            200,
            &serde_json::json!({
                "issuer": issuer,
                "authorization_endpoint": format!("{issuer}/authorize"),
                "token_endpoint": format!("{issuer}/token"),
                "jwks_uri": format!("{issuer}/jwks"),
            }),
        ),
        ("GET", "/jwks") => {
            s.jwks_requests += 1;
            let keys: Vec<&Jwk> = std::iter::once(&s.signing)
                .chain(&s.retired)
                .map(|k| &k.jwk)
                .collect();
            json(200, &serde_json::json!({ "keys": keys }))
        }
        ("GET", "/authorize") => authorize(&mut s, &parts.uri),
        ("POST", "/token") => token(
            &mut s,
            &parts.headers,
            &parse_form(&String::from_utf8_lossy(&body)),
        ),
        _ => json(404, &serde_json::json!({"error": "not_found"})),
    })
}

fn authorize(s: &mut State, uri: &hyper::Uri) -> Resp {
    let q = query(uri);
    let get = |k: &str| q.get(k).cloned().unwrap_or_default();
    if get("response_type") != "code"
        || get("code_challenge_method") != "S256"
        || get("code_challenge").is_empty()
        || get("state").is_empty()
        || !get("redirect_uri").starts_with("http://127.0.0.1:")
    {
        return json(400, &serde_json::json!({"error": "invalid_request"}));
    }
    s.counter += 1;
    let code = format!("code-{}", s.counter);
    s.codes.insert(
        code.clone(),
        CodeGrant {
            client_id: get("client_id"),
            redirect_uri: get("redirect_uri"),
            challenge: get("code_challenge"),
            nonce: get("nonce"),
            dpop_jkt: q.get("dpop_jkt").cloned(),
        },
    );
    let location = format!(
        "{}?code={code}&state={}",
        get("redirect_uri"),
        percent_encoding::utf8_percent_encode(&get("state"), percent_encoding::NON_ALPHANUMERIC)
    );
    Response::builder()
        .status(302)
        .header("location", location)
        .body(Full::new(Bytes::new()))
        .expect("response")
}

fn oauth_error(code: &str) -> Resp {
    json(400, &serde_json::json!({"error": code}))
}

fn token(s: &mut State, headers: &hyper::HeaderMap, form: &HashMap<String, String>) -> Resp {
    s.token_requests += 1;
    let url = format!("{}/token", s.issuer);
    let now = system_clock();

    // DPoP proof on the token request (optional only when binding is off).
    let mut jkt = None;
    if let Some(proof) = headers.get("dpop").and_then(|v| v.to_str().ok()) {
        let nonce = s.require_nonce.then(|| s.issued_nonce.clone());
        match s
            .dpop
            .verify_with_nonce(proof, "POST", &url, None, None, nonce.as_deref(), now)
        {
            Ok(k) => jkt = Some(k),
            Err(AuthError::Dpop("nonce does not match")) => {
                return Response::builder()
                    .status(400)
                    .header("content-type", "application/json")
                    .header("dpop-nonce", s.issued_nonce.clone())
                    .body(Full::new(Bytes::from(r#"{"error":"use_dpop_nonce"}"#)))
                    .expect("response");
            }
            Err(_) => return oauth_error("invalid_dpop_proof"),
        }
    } else if s.bind_tokens {
        return oauth_error("invalid_dpop_proof");
    }

    let field = |k: &str| form.get(k).map_or("", String::as_str);
    let (subject, nonce, refresh_jkt) = match field("grant_type") {
        "authorization_code" => {
            let Some(grant) = s.codes.remove(field("code")) else {
                return oauth_error("invalid_grant");
            };
            let verifier_ok = crate::pkce::challenge_of(field("code_verifier")) == grant.challenge;
            let bound_ok = grant.dpop_jkt.is_none() || grant.dpop_jkt == jkt;
            if !verifier_ok
                || !bound_ok
                || grant.client_id != field("client_id")
                || grant.redirect_uri != field("redirect_uri")
            {
                return oauth_error("invalid_grant");
            }
            (s.subject.clone(), Some(grant.nonce), jkt.clone())
        }
        "refresh_token" => {
            // Rotation: a refresh token works once.
            let Some(grant) = s.refresh.remove(field("refresh_token")) else {
                return oauth_error("invalid_grant");
            };
            if grant.jkt.is_some() && grant.jkt != jkt {
                return oauth_error("invalid_grant");
            }
            (s.subject.clone(), None, grant.jkt)
        }
        _ => return oauth_error("unsupported_grant_type"),
    };

    s.counter += 1;
    let refresh_token = format!("refresh-{}", s.counter);
    s.refresh
        .insert(refresh_token.clone(), RefreshGrant { jkt: refresh_jkt });
    let bound = s.bind_tokens.then(|| jkt.clone()).flatten();
    let roles = s.roles.clone();
    let access = mint_access(s, &subject, &roles, bound.as_deref(), s.access_ttl);
    let mut body = serde_json::json!({
        "access_token": access,
        "token_type": if bound.is_some() { "DPoP" } else { "Bearer" },
        "expires_in": s.access_ttl,
        "refresh_token": refresh_token,
    });
    if let Some(nonce) = nonce {
        body["id_token"] = serde_json::Value::String(sign(
            &s.signing,
            &serde_json::json!({
                "iss": s.issuer, "sub": subject, "aud": CLIENT_ID,
                "iat": now, "exp": now + 300, "nonce": nonce,
            }),
        ));
    }
    json(200, &body)
}
