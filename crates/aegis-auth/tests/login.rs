//! Login end to end against the mock identity provider: the real client
//! code (discovery, PKCE, loopback, DPoP, refresh) and the real server-side
//! validation, over real HTTP on loopback ports.

use std::sync::Arc;
use std::time::Duration;

use aegis_auth::dpop::DpopVerifier;
use aegis_auth::http::HyperClient;
use aegis_auth::jwks::JwksCache;
use aegis_auth::oidc::{OidcClient, OidcConfig};
use aegis_auth::session::Session;
use aegis_auth::store::{MemorySecretStore, Vault};
use aegis_auth::testing::{AUDIENCE, CLIENT_ID, MockIdp};
use aegis_auth::validate::{
    AdminRule, Authenticator, DpopPolicy, RequestCredentials, ValidatorConfig,
};
use aegis_auth::{AuthError, system_clock};

const SERVER_URL: &str = "http://127.0.0.1:50051/aegis.v1.Aegis/SubmitOps";

type TestSession = Session<HyperClient, Arc<MemorySecretStore>>;

async fn session(idp: &MockIdp, store: &Arc<MemorySecretStore>) -> TestSession {
    let oidc = OidcClient::connect(
        Arc::new(HyperClient::new()),
        OidcConfig::new(idp.issuer(), CLIENT_ID),
        system_clock,
    )
    .await
    .expect("discovery");
    Session::new(
        oidc,
        Vault::new(Arc::clone(store), "alice@aegis"),
        system_clock,
    )
    .expect("session")
}

fn authenticator(
    idp: &MockIdp,
    policy: DpopPolicy,
    min_refetch: u64,
) -> Authenticator<HyperClient> {
    authenticator_at(idp, policy, min_refetch, system_clock)
}

fn authenticator_at(
    idp: &MockIdp,
    policy: DpopPolicy,
    min_refetch: u64,
    clock: aegis_auth::Clock,
) -> Authenticator<HyperClient> {
    let jwks = JwksCache::with_timing(
        Arc::new(HyperClient::new()),
        &format!("{}/jwks", idp.issuer()),
        system_clock,
        Duration::from_secs(600),
        Duration::from_secs(min_refetch),
    );
    Authenticator::new(
        ValidatorConfig {
            issuer: idp.issuer().to_owned(),
            audience: AUDIENCE.to_owned(),
            dpop: policy,
            admin: Some(AdminRule {
                claim: "roles".into(),
                value: "aegis-admin".into(),
            }),
        },
        jwks,
        DpopVerifier::standard(),
        clock,
    )
}

fn request<'a>(auth: &'a aegis_auth::Credentials) -> RequestCredentials<'a> {
    RequestCredentials {
        authorization: Some(&auth.authorization),
        dpop: auth.dpop.as_deref(),
        method: "POST",
        url: SERVER_URL,
    }
}

#[tokio::test]
async fn a_user_logs_in_and_the_server_recognises_them() {
    let idp = MockIdp::start().await;
    idp.user("alice", &["aegis-admin"]);
    let store = Arc::new(MemorySecretStore::new());
    let s = session(&idp, &store).await;
    assert!(!s.is_logged_in().await);

    s.login(&idp.browser(), Duration::from_secs(10))
        .await
        .expect("login");
    assert!(s.is_logged_in().await);

    let creds = s
        .credentials("POST", SERVER_URL)
        .await
        .expect("credentials");
    assert!(creds.authorization.starts_with("DPoP "));
    assert!(creds.dpop.is_some());

    let server = authenticator(&idp, DpopPolicy::Required, 30);
    let claims = server
        .authenticate(request(&creds))
        .await
        .expect("accepted");
    assert_eq!(claims.subject, "alice");
    assert!(claims.admin);
    assert_eq!(claims.jkt.as_deref(), Some(s.key_thumbprint()));
}

#[tokio::test]
async fn every_request_needs_its_own_proof() {
    let idp = MockIdp::start().await;
    let s = session(&idp, &Arc::new(MemorySecretStore::new())).await;
    s.login(&idp.browser(), Duration::from_secs(10))
        .await
        .expect("login");
    let server = authenticator(&idp, DpopPolicy::Required, 30);

    let a = s
        .credentials("POST", SERVER_URL)
        .await
        .expect("credentials");
    assert!(server.authenticate(request(&a)).await.is_ok());
    // The same headers again: replayed proof.
    assert_eq!(
        server.authenticate(request(&a)).await.err(),
        Some(AuthError::Replay)
    );
    // A fresh proof for the same token is fine.
    let b = s
        .credentials("POST", SERVER_URL)
        .await
        .expect("credentials");
    assert!(server.authenticate(request(&b)).await.is_ok());
    // A proof made for another URL does not work here.
    let c = s
        .credentials("POST", "http://127.0.0.1:50051/aegis.v1.Aegis/OpenProject")
        .await
        .expect("credentials");
    assert!(matches!(
        server.authenticate(request(&c)).await,
        Err(AuthError::Dpop(_))
    ));
}

#[tokio::test]
async fn a_stolen_token_is_useless_without_the_device_key() {
    let idp = MockIdp::start().await;
    let s = session(&idp, &Arc::new(MemorySecretStore::new())).await;
    s.login(&idp.browser(), Duration::from_secs(10))
        .await
        .expect("login");
    let creds = s
        .credentials("POST", SERVER_URL)
        .await
        .expect("credentials");
    let server = authenticator(&idp, DpopPolicy::Required, 30);

    // Token without any proof.
    let no_proof = RequestCredentials {
        dpop: None,
        ..request(&creds)
    };
    assert!(matches!(
        server.authenticate(no_proof).await,
        Err(AuthError::Dpop(_))
    ));
    // Token sent as plain bearer.
    let token = creds.authorization.strip_prefix("DPoP ").expect("scheme");
    let bearer = format!("Bearer {token}");
    let as_bearer = RequestCredentials {
        authorization: Some(&bearer),
        dpop: None,
        ..request(&creds)
    };
    assert!(matches!(
        server.authenticate(as_bearer).await,
        Err(AuthError::Dpop(_))
    ));
    // Token with a proof from the thief's own key.
    let thief = aegis_auth::DeviceKey::generate().expect("key");
    let forged = thief
        .proof("POST", SERVER_URL, Some(token), None, system_clock())
        .expect("proof");
    let with_thief_key = RequestCredentials {
        dpop: Some(&forged),
        ..request(&creds)
    };
    assert!(matches!(
        server.authenticate(with_thief_key).await,
        Err(AuthError::Dpop("key is not the one the token is bound to"))
    ));
}

#[tokio::test]
async fn the_session_resumes_from_the_keychain_and_refresh_tokens_rotate() {
    let idp = MockIdp::start().await;
    let store = Arc::new(MemorySecretStore::new());
    let first = session(&idp, &store).await;
    first
        .login(&idp.browser(), Duration::from_secs(10))
        .await
        .expect("login");
    let saved = Vault::new(Arc::clone(&store), "alice@aegis")
        .load_refresh_token()
        .expect("load")
        .expect("saved");

    // A new run of the app: no tokens in memory, a refresh token on disk.
    let second = session(&idp, &store).await;
    assert!(!second.is_logged_in().await);
    assert_eq!(
        second.key_thumbprint(),
        first.key_thumbprint(),
        "same device key"
    );
    assert!(second.restore().await.expect("restore"));
    assert!(second.is_logged_in().await);
    let rotated = Vault::new(Arc::clone(&store), "alice@aegis")
        .load_refresh_token()
        .expect("load")
        .expect("saved");
    assert_ne!(rotated, saved, "the provider rotated the refresh token");

    // The old refresh token is dead: a third run using it would fail.
    let creds = second
        .credentials("POST", SERVER_URL)
        .await
        .expect("credentials");
    assert!(
        authenticator(&idp, DpopPolicy::Required, 30)
            .authenticate(request(&creds))
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn expiring_tokens_are_refreshed_before_a_request() {
    let idp = MockIdp::start().await;
    // Shorter than the 30 s refresh margin: every call refreshes.
    idp.access_ttl(10);
    let store = Arc::new(MemorySecretStore::new());
    let s = session(&idp, &store).await;
    s.login(&idp.browser(), Duration::from_secs(10))
        .await
        .expect("login");
    assert_eq!(idp.token_requests(), 1);
    let server = authenticator(&idp, DpopPolicy::Required, 30);
    for round in 1..=3 {
        let creds = s
            .credentials("POST", SERVER_URL)
            .await
            .expect("credentials");
        assert!(
            server.authenticate(request(&creds)).await.is_ok(),
            "round {round}"
        );
        assert_eq!(idp.token_requests(), 1 + round);
    }
}

#[tokio::test]
async fn concurrent_requests_share_one_refresh() {
    let idp = MockIdp::start().await;
    idp.access_ttl(10);
    let s = Arc::new(session(&idp, &Arc::new(MemorySecretStore::new())).await);
    s.login(&idp.browser(), Duration::from_secs(10))
        .await
        .expect("login");
    let before = idp.token_requests();
    // With rotation a second concurrent refresh would be rejected; single
    // flight means every caller succeeds. (The first refresh yields a token
    // that also expires within the margin, so each caller refreshes in turn,
    // never in parallel.)
    let calls: Vec<_> = (0..8)
        .map(|_| {
            let s = Arc::clone(&s);
            tokio::spawn(async move { s.credentials("POST", SERVER_URL).await })
        })
        .collect();
    for call in calls {
        call.await.expect("task").expect("credentials");
    }
    assert_eq!(idp.token_requests(), before + 8);
}

#[tokio::test]
async fn a_revoked_session_requires_a_new_login() {
    let idp = MockIdp::start().await;
    let store = Arc::new(MemorySecretStore::new());
    let s = session(&idp, &store).await;
    s.login(&idp.browser(), Duration::from_secs(10))
        .await
        .expect("login");

    idp.revoke_refresh_tokens();
    let fresh_run = session(&idp, &store).await;
    assert!(
        !fresh_run.restore().await.expect("not an error"),
        "must log in again"
    );
    assert_eq!(
        Vault::new(Arc::clone(&store), "alice@aegis")
            .load_refresh_token()
            .expect("load"),
        None,
        "the dead refresh token is forgotten"
    );
    assert_eq!(
        fresh_run.credentials("POST", SERVER_URL).await.err(),
        Some(AuthError::LoginRequired)
    );
}

#[tokio::test]
async fn a_dead_refresh_token_logs_the_running_session_out() {
    let idp = MockIdp::start().await;
    idp.access_ttl(10);
    let s = session(&idp, &Arc::new(MemorySecretStore::new())).await;
    s.login(&idp.browser(), Duration::from_secs(10))
        .await
        .expect("login");
    idp.revoke_refresh_tokens();
    assert_eq!(
        s.credentials("POST", SERVER_URL).await.err(),
        Some(AuthError::LoginRequired)
    );
    assert!(!s.is_logged_in().await);
}

#[tokio::test]
async fn logout_forgets_the_session() {
    let idp = MockIdp::start().await;
    let store = Arc::new(MemorySecretStore::new());
    let s = session(&idp, &store).await;
    s.login(&idp.browser(), Duration::from_secs(10))
        .await
        .expect("login");
    s.logout().await.expect("logout");
    assert!(!s.is_logged_in().await);
    assert!(
        !session(&idp, &store)
            .await
            .restore()
            .await
            .expect("restore")
    );
}

#[tokio::test]
async fn a_provider_that_asks_for_a_dpop_nonce_is_served() {
    let idp = MockIdp::start().await;
    idp.require_dpop_nonce(true);
    let s = session(&idp, &Arc::new(MemorySecretStore::new())).await;
    s.login(&idp.browser(), Duration::from_secs(10))
        .await
        .expect("login");
    // The first attempt was refused with a nonce, the retry succeeded.
    assert_eq!(idp.token_requests(), 2);
}

#[tokio::test]
async fn login_times_out_if_the_user_never_finishes() {
    struct Never;
    impl aegis_auth::session::BrowserLauncher for Never {
        fn open(&self, _: &str) -> Result<(), AuthError> {
            Ok(())
        }
    }
    let idp = MockIdp::start().await;
    let s = session(&idp, &Arc::new(MemorySecretStore::new())).await;
    assert_eq!(
        s.login(&Never, Duration::from_millis(100)).await.err(),
        Some(AuthError::Timeout)
    );
    assert!(!s.is_logged_in().await);
}

#[tokio::test]
async fn discovery_rejects_an_issuer_mismatch() {
    let idp = MockIdp::start().await;
    let wrong = format!("{}/", idp.issuer()); // trailing slash: a different string
    let result = OidcClient::connect(
        Arc::new(HyperClient::new()),
        OidcConfig::new(&wrong, CLIENT_ID),
        system_clock,
    )
    .await;
    assert!(matches!(result, Err(AuthError::Protocol)));
}

// ---------------------------------------------------------------------------
// Server-side policy.
// ---------------------------------------------------------------------------

fn bearer(token: &str) -> String {
    format!("Bearer {token}")
}

fn plain<'a>(authorization: &'a str) -> RequestCredentials<'a> {
    RequestCredentials {
        authorization: Some(authorization),
        dpop: None,
        method: "POST",
        url: SERVER_URL,
    }
}

#[tokio::test]
async fn unbound_tokens_follow_the_dpop_policy() {
    let idp = MockIdp::start().await;
    let token = idp.access_token("bob", &[], None, 600);
    let header = bearer(&token);

    let required = authenticator(&idp, DpopPolicy::Required, 30);
    assert_eq!(
        required.authenticate(plain(&header)).await.err(),
        Some(AuthError::DpopRequired)
    );
    let optional = authenticator(&idp, DpopPolicy::Optional, 30);
    let claims = optional
        .authenticate(plain(&header))
        .await
        .expect("accepted");
    assert_eq!(
        (claims.subject.as_str(), claims.admin, claims.jkt),
        ("bob", false, None)
    );
    // Claiming DPoP for a token that is not bound is a mismatch either way.
    let dpop_header = format!("DPoP {token}");
    assert!(matches!(
        optional.authenticate(plain(&dpop_header)).await,
        Err(AuthError::Dpop(_))
    ));
}

#[tokio::test]
async fn token_checks_name_the_failing_claim() {
    let idp = MockIdp::start().await;
    let other = MockIdp::start().await;
    let auth = authenticator(&idp, DpopPolicy::Optional, 0);
    let check = |token: String| {
        let auth = &auth;
        async move { auth.authenticate(plain(&bearer(&token))).await.err() }
    };

    // A valid token, then the same token judged by a clock far in the future
    // (expired beyond the 60 s skew) and far in the past (not valid yet).
    assert_eq!(check(idp.access_token("a", &[], None, 600)).await, None);
    fn later() -> u64 {
        system_clock() + 1000
    }
    fn earlier() -> u64 {
        system_clock() - 1000
    }
    let token = bearer(&idp.access_token("a", &[], None, 600));
    let expired = authenticator_at(&idp, DpopPolicy::Optional, 0, later);
    assert_eq!(
        expired.authenticate(plain(&token)).await.err(),
        Some(AuthError::Expired)
    );
    let early = authenticator_at(&idp, DpopPolicy::Optional, 0, earlier);
    assert_eq!(
        early.authenticate(plain(&token)).await.err(),
        Some(AuthError::NotYetValid)
    );
    // A token from another issuer, signed by another key set.
    let foreign = other.access_token("a", &[], None, 600);
    assert!(matches!(
        check(foreign).await,
        Some(AuthError::UnknownKey | AuthError::BadSignature | AuthError::WrongIssuer)
    ));
    // Garbage and none-algorithm tokens.
    assert_eq!(check("abc".into()).await, Some(AuthError::Malformed));
    let none = format!(
        "{}.{}.",
        aegis_auth::b64::encode(r#"{"alg":"none"}"#),
        aegis_auth::b64::encode(r#"{"sub":"root"}"#)
    );
    assert_eq!(check(none).await, Some(AuthError::UnsupportedAlgorithm));
    // Scheme errors.
    assert_eq!(
        auth.authenticate(RequestCredentials {
            authorization: None,
            ..plain("")
        })
        .await
        .err(),
        Some(AuthError::Claim("authorization"))
    );
    assert_eq!(
        auth.authenticate(plain("Basic dXNlcjpwYXNz")).await.err(),
        Some(AuthError::Malformed)
    );
}

#[tokio::test]
async fn rotated_signing_keys_are_picked_up_but_unknown_keys_cannot_flood_the_provider() {
    let idp = MockIdp::start().await;
    let auth = authenticator(&idp, DpopPolicy::Optional, 3600);
    let header = bearer(&idp.access_token("a", &[], None, 600));
    auth.authenticate(plain(&header)).await.expect("first key");
    assert_eq!(idp.jwks_requests(), 1);
    // Cached: no more fetches.
    for _ in 0..5 {
        auth.authenticate(plain(&header)).await.expect("cached");
    }
    assert_eq!(idp.jwks_requests(), 1);

    // The provider rotates. A token from the new key has an unknown kid, but
    // a refetch is throttled to once per `min_refetch` (here an hour), so it
    // is refused rather than hammering the provider.
    idp.rotate_keys("key-2");
    let rotated = bearer(&idp.access_token("a", &[], None, 600));
    for _ in 0..5 {
        assert_eq!(
            auth.authenticate(plain(&rotated)).await.err(),
            Some(AuthError::UnknownKey)
        );
    }
    assert_eq!(idp.jwks_requests(), 1, "throttled");

    // With a normal throttle (0 s here) the new key is fetched once.
    let eager = authenticator(&idp, DpopPolicy::Optional, 0);
    eager
        .authenticate(plain(&header))
        .await
        .expect("old key still published");
    eager.authenticate(plain(&rotated)).await.expect("new key");
    let before = idp.jwks_requests();
    eager.authenticate(plain(&rotated)).await.expect("cached");
    assert_eq!(idp.jwks_requests(), before);
}

#[tokio::test]
async fn rs256_tokens_from_a_real_rsa_key_verify() {
    // Fixture signed offline with a throwaway 2048-bit RSA key.
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/rs256.json")).expect("fixture");
    let token = fixture["token"].as_str().expect("token");
    let parsed = aegis_auth::jose::Token::parse(token).expect("parses");
    assert_eq!(parsed.header.kid.as_deref(), Some("rsa-fixture"));
    let jwk: aegis_auth::jose::Jwk =
        serde_json::from_value(fixture["jwks"]["keys"][0].clone()).expect("jwk");
    parsed
        .verify(&jwk.public_key().expect("key"))
        .expect("RS256 verifies");
    let claims: serde_json::Value = parsed.claims().expect("claims");
    assert_eq!(claims["sub"], "fixture-user");

    // One flipped bit in the signature fails.
    let mut broken = token.to_owned().into_bytes();
    let last = broken.len() - 3;
    broken[last] = if broken[last] == b'A' { b'B' } else { b'A' };
    let broken = String::from_utf8(broken).expect("utf8");
    let parsed = aegis_auth::jose::Token::parse(&broken).expect("parses");
    assert_eq!(
        parsed.verify(&jwk.public_key().expect("key")),
        Err(AuthError::BadSignature)
    );
}
