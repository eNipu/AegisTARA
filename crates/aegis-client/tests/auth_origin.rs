//! Credentials belong to an API origin, not an issuer, URL prefix, or later server.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use aegis_auth::AuthError;
use aegis_auth::layer::CredentialProvider;
use aegis_auth::store::{MemorySecretStore, SecretStore};
use aegis_client::connection::PasswordLogin;
use aegis_client::origin::{Origin, OriginError};
use aegis_client::provider::{AppProvider, OidcProvider, OidcSettings, PasswordProvider};

const SERVER: &str = "https://api.example.test";
const TOKEN: &str = "ats_server_a";
const OTHER: &str = "https://other.example.test";

const WRONG_ORIGINS: &[&str] = &[
    "https://other.example.test/rpc",
    "https://api.example.test.attacker.test/rpc",
    "https://api.example.test-attacker.test/rpc",
    "http://api.example.test/rpc",
    "http://api.example.test:443/rpc",
    "https://api.example.test:80/rpc",
    "https://api.example.test:8443/rpc",
];

const INVALID_URLS: &[&str] = &[
    "",
    "/rpc",
    "//api.example.test/rpc",
    "api.example.test:443",
    "https:api.example.test",
    "https:///rpc",
    "https://",
    "ftp://api.example.test/rpc",
    "https://user@api.example.test/rpc",
    "https://user:password@api.example.test/rpc",
    "https://api.example.test@attacker.test/rpc",
    "https://@api.example.test/rpc",
    "https://api.example.test:/rpc",
    "https://api.example.test:abc/rpc",
    "https://api.example.test:65536/rpc",
    "https://api.example.test:443:80/rpc",
    "https://[not-an-ip]/rpc",
    "https://[::1/rpc",
    "https://::1/rpc",
    "https://api..example.test/rpc",
    "https://.example.test/rpc",
    "https://-api.example.test/rpc",
    "https://api.example.test\\@attacker.test/rpc",
    "https://api.example.test/rpc#fragment",
    "https://api.example.test/rpc%",
    "https://api.example.test/rpc%xx",
    "https://%61pi.example.test/rpc",
    " https://api.example.test",
    "https://api.example.test/rpc\n",
    "https://api.example.test/rpc with spaces",
];

#[test]
fn origins_normalize_case_and_default_ports_without_including_rpc_paths() {
    let origin = Origin::parse(SERVER).expect("origin");
    for url in [
        SERVER,
        "HTTPS://API.EXAMPLE.TEST:443/",
        "https://Api.Example.Test/service/Method?query=value",
        "https://api.example.test/path%20segment?next=https://other.example.test",
    ] {
        assert_eq!(Origin::parse(url).expect(url), origin, "{url}");
        origin.check(url).expect(url);
    }
    assert_eq!(
        Origin::parse("HTTP://LOCALHOST:80/a?b").unwrap(),
        Origin::parse("http://localhost").unwrap()
    );
    assert_eq!(
        Origin::parse("https://[2001:DB8::1]:443/a").unwrap(),
        Origin::parse("https://[2001:db8:0:0:0:0:0:1]/b").unwrap()
    );
    assert_eq!(
        Origin::parse("HTTPS://API.EXAMPLE.TEST:443/rpc?q=1")
            .unwrap()
            .key(),
        SERVER
    );
    assert_eq!(
        Origin::parse("https://[2001:DB8::1]:8443/rpc")
            .unwrap()
            .key(),
        "https://[2001:db8::1]:8443"
    );
    assert_ne!(
        Origin::parse("http://localhost:8080").unwrap(),
        Origin::parse("http://localhost").unwrap()
    );
}

#[test]
fn origins_compare_scheme_host_and_effective_port_exactly() {
    let origin = Origin::parse(SERVER).unwrap();
    for url in WRONG_ORIGINS {
        assert_eq!(origin.check(url), Err(OriginError::Mismatch), "{url}");
    }
    assert_eq!(
        Origin::parse("http://api.example.test")
            .unwrap()
            .check(SERVER),
        Err(OriginError::Mismatch),
        "upgrades are a different origin too"
    );
}

#[test]
fn malformed_and_userinfo_urls_are_never_origins() {
    let origin = Origin::parse(SERVER).unwrap();
    for url in INVALID_URLS {
        assert_eq!(Origin::parse(url), Err(OriginError::InvalidUrl), "{url}");
        assert_eq!(origin.check(url), Err(OriginError::InvalidUrl), "{url}");
    }
}

#[tokio::test]
async fn password_credentials_allow_only_same_origin_rpc_requests() {
    let login = PasswordLogin::new();
    login.set(SERVER, Some(TOKEN.into())).unwrap();
    for url in [
        SERVER,
        "https://api.example.test/aegis.v1.Aegis/GetServerInfo?x=1",
        "HTTPS://API.EXAMPLE.TEST:443/aegis.v1.Aegis/ListProjects",
    ] {
        let credentials = login.credentials("POST", url).await.expect(url);
        assert_eq!(credentials.authorization, format!("Bearer {TOKEN}"));
        assert!(credentials.dpop.is_none());
        assert!(credentials.extra.is_empty());
    }
    for url in WRONG_ORIGINS.iter().chain(INVALID_URLS) {
        assert!(
            matches!(
                login.credentials("POST", url).await,
                Err(AuthError::Http(_))
            ),
            "{url}"
        );
    }
}

#[tokio::test]
async fn unbound_password_login_never_supplies_credentials() {
    let login = PasswordLogin::new();
    assert!(!login.has_token());
    assert!(matches!(
        login.credentials("POST", SERVER).await,
        Err(AuthError::LoginRequired)
    ));
    for url in INVALID_URLS {
        assert!(login.set(url, Some(TOKEN.into())).is_err(), "{url}");
        assert!(!login.has_token());
    }
    login.set(SERVER, Some(TOKEN.into())).unwrap();
    assert!(login.credentials("POST", SERVER).await.is_ok());
}

#[tokio::test]
async fn password_login_cannot_be_retargeted_even_after_clearing() {
    let login = PasswordLogin::new();
    login.set(SERVER, Some(TOKEN.into())).unwrap();
    assert!(login.set(OTHER, Some("ats_other".into())).is_err());
    assert!(login.set(OTHER, None).is_err());
    assert_eq!(
        login
            .credentials("POST", SERVER)
            .await
            .unwrap()
            .authorization,
        format!("Bearer {TOKEN}")
    );
    login.set("HTTPS://API.EXAMPLE.TEST:443/rpc", None).unwrap();
    assert!(!login.has_token());
    assert!(login.set(OTHER, Some("ats_other".into())).is_err());
    login.set(SERVER, Some(TOKEN.into())).unwrap();
    login.clear();
    assert!(!login.has_token());
    assert!(login.set(OTHER, Some("ats_other".into())).is_err());
}

#[tokio::test]
async fn concurrent_first_tokens_bind_to_exactly_one_origin() {
    let login = Arc::new(PasswordLogin::new());
    let barrier = Arc::new(std::sync::Barrier::new(3));
    let workers: Vec<_> = [(SERVER, TOKEN), (OTHER, "ats_other")]
        .into_iter()
        .map(|(server, token)| {
            let login = Arc::clone(&login);
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                (server, token, login.set(server, Some(token.into())).is_ok())
            })
        })
        .collect();
    barrier.wait();
    let results: Vec<_> = workers.into_iter().map(|w| w.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|(_, _, ok)| *ok).count(), 1);
    for (server, token, accepted) in results {
        let result = login.credentials("POST", server).await;
        if accepted {
            assert_eq!(result.unwrap().authorization, format!("Bearer {token}"));
        } else {
            assert!(result.is_err());
        }
    }
}

#[derive(Default)]
struct CountingStore {
    memory: MemorySecretStore,
    accesses: AtomicUsize,
}

impl SecretStore for CountingStore {
    fn get(&self, name: &str) -> Result<Option<Vec<u8>>, AuthError> {
        self.accesses.fetch_add(1, Ordering::SeqCst);
        self.memory.get(name)
    }

    fn set(&self, name: &str, value: &[u8]) -> Result<(), AuthError> {
        self.accesses.fetch_add(1, Ordering::SeqCst);
        self.memory.set(name, value)
    }

    fn delete(&self, name: &str) -> Result<(), AuthError> {
        self.accesses.fetch_add(1, Ordering::SeqCst);
        self.memory.delete(name)
    }
}

#[tokio::test]
async fn remembered_tokens_keep_the_existing_keychain_scope_and_bind_on_restore() {
    let store = Arc::new(CountingStore::default());
    let provider = PasswordProvider::new(store.clone());
    provider
        .set(&format!("{SERVER}/"), " Alice ", TOKEN.into(), true)
        .unwrap();
    assert_eq!(
        store
            .memory
            .get(&format!("password-session|{SERVER}|alice"))
            .unwrap(),
        Some(TOKEN.as_bytes().to_vec())
    );

    let restored = PasswordProvider::new(store.clone());
    assert!(restored.restore(SERVER, "Alice"));
    let restored = AppProvider::Password(restored);
    assert_eq!(
        restored
            .credentials("POST", &format!("{SERVER}/rpc?q=1"))
            .await
            .unwrap()
            .authorization,
        format!("Bearer {TOKEN}")
    );
    for url in WRONG_ORIGINS {
        assert!(restored.credentials("POST", url).await.is_err(), "{url}");
    }

    let other = PasswordProvider::new(store.clone());
    assert!(!other.restore(OTHER, "Alice"));
    assert!(!other.login.has_token());
    assert!(other.login.credentials("POST", OTHER).await.is_err());
    assert!(
        !other.restore(SERVER, "Alice"),
        "even a missing restore binds the provider"
    );
    assert!(!other.login.has_token());
}

#[tokio::test]
async fn retargeting_password_provider_never_reads_writes_or_deletes_another_servers_token() {
    let store = Arc::new(CountingStore::default());
    let provider = PasswordProvider::new(store.clone());
    provider.set(SERVER, "alice", TOKEN.into(), true).unwrap();
    let other_entry = format!("password-session|{OTHER}|alice");
    store.memory.set(&other_entry, b"ats_other").unwrap();
    let accesses = store.accesses.load(Ordering::SeqCst);
    for remember in [true, false] {
        assert!(
            provider
                .set(OTHER, "alice", "ats_retargeted".into(), remember)
                .is_err()
        );
    }
    assert!(!provider.restore(OTHER, "alice"));
    assert_eq!(store.accesses.load(Ordering::SeqCst), accesses);
    assert_eq!(
        provider
            .login
            .credentials("POST", SERVER)
            .await
            .unwrap()
            .authorization,
        format!("Bearer {TOKEN}")
    );
    provider.forget();
    assert!(!provider.login.has_token());
    assert!(
        store
            .memory
            .get(&format!("password-session|{SERVER}|alice"))
            .unwrap()
            .is_none()
    );
    assert_eq!(
        store.memory.get(&other_entry).unwrap(),
        Some(b"ats_other".to_vec())
    );
    let accesses = store.accesses.load(Ordering::SeqCst);
    assert!(!provider.restore(OTHER, "alice"));
    assert!(
        provider
            .set(OTHER, "alice", "ats_retargeted".into(), true)
            .is_err()
    );
    assert_eq!(store.accesses.load(Ordering::SeqCst), accesses);
}

#[test]
fn invalid_servers_cannot_touch_the_keychain_or_install_tokens() {
    let store = Arc::new(CountingStore::default());
    let provider = PasswordProvider::new(store.clone());
    for server in INVALID_URLS {
        assert!(!provider.restore(server, "alice"), "{server}");
        assert!(
            provider.set(server, "alice", TOKEN.into(), true).is_err(),
            "{server}"
        );
        assert!(!provider.login.has_token());
    }
    assert_eq!(store.accesses.load(Ordering::SeqCst), 0);
    provider.set(SERVER, "alice", TOKEN.into(), false).unwrap();
}

#[tokio::test]
async fn nonremembered_password_tokens_are_bound_and_clear_the_old_saved_entry() {
    let store = Arc::new(CountingStore::default());
    let provider = PasswordProvider::new(store.clone());
    provider.set(SERVER, "alice", TOKEN.into(), true).unwrap();
    provider
        .set(SERVER, "alice", "ats_new".into(), false)
        .unwrap();
    assert!(
        store
            .memory
            .get(&format!("password-session|{SERVER}|alice"))
            .unwrap()
            .is_none()
    );
    assert_eq!(
        provider
            .login
            .credentials("POST", SERVER)
            .await
            .unwrap()
            .authorization,
        "Bearer ats_new"
    );
    assert!(provider.login.credentials("POST", OTHER).await.is_err());
}

fn oidc(server: &str) -> OidcProvider {
    OidcProvider::new(OidcSettings {
        server_url: server.into(),
        issuer: "https://identity.example.test".into(),
        client_id: "desktop".into(),
        resource: Some("https://resource.example.test".into()),
        account: "test-account".into(),
    })
}

#[tokio::test]
async fn oidc_checks_api_origin_before_looking_for_a_session() {
    let provider = AppProvider::Oidc(oidc(SERVER));
    for url in WRONG_ORIGINS.iter().chain(INVALID_URLS).copied().chain([
        "https://identity.example.test/rpc",
        "https://resource.example.test/rpc",
    ]) {
        assert!(
            matches!(
                provider.credentials("POST", url).await,
                Err(AuthError::Http(_))
            ),
            "{url}"
        );
    }
    for url in [SERVER, "HTTPS://API.EXAMPLE.TEST:443/service/Method?x=1"] {
        assert!(
            matches!(
                provider.credentials("POST", url).await,
                Err(AuthError::LoginRequired)
            ),
            "{url}"
        );
    }
}

#[test]
fn equivalent_server_spellings_cannot_orphan_remembered_passwords() {
    let store = Arc::new(CountingStore::default());
    let provider = PasswordProvider::new(store.clone());
    provider.set(SERVER, "alice", TOKEN.into(), true).unwrap();
    provider
        .set(
            "HTTPS://API.EXAMPLE.TEST:443/rpc",
            "alice",
            "ats_new".into(),
            false,
        )
        .unwrap();
    provider.forget();

    let original_spelling = PasswordProvider::new(store.clone());
    assert!(!original_spelling.restore(SERVER, "alice"));
    assert!(!original_spelling.login.has_token());
    assert!(
        store
            .memory
            .get(&format!("password-session|{SERVER}|alice"))
            .unwrap()
            .is_none()
    );
}

#[test]
fn legacy_keychain_alias_migrates_to_the_canonical_origin() {
    let store = Arc::new(CountingStore::default());
    let legacy = "password-session|HTTPS://API.EXAMPLE.TEST:443/rpc|alice";
    store.memory.set(legacy, TOKEN.as_bytes()).unwrap();
    let provider = PasswordProvider::new(store.clone());
    assert!(provider.restore("HTTPS://API.EXAMPLE.TEST:443/rpc", "alice"));
    assert!(store.memory.get(legacy).unwrap().is_none());
    assert_eq!(
        store
            .memory
            .get(&format!("password-session|{SERVER}|alice"))
            .unwrap(),
        Some(TOKEN.as_bytes().to_vec())
    );
    let canonical = PasswordProvider::new(store);
    assert!(canonical.restore(SERVER, "alice"));
}

#[tokio::test]
async fn invalidating_a_provider_clears_only_its_live_password_token() {
    let store = Arc::new(CountingStore::default());
    let password = PasswordProvider::new(store.clone());
    password.set(SERVER, "alice", TOKEN.into(), true).unwrap();
    let provider = AppProvider::Password(password);
    provider.invalidate();
    assert!(matches!(
        provider.credentials("POST", SERVER).await,
        Err(AuthError::LoginRequired)
    ));
    assert_eq!(
        store
            .memory
            .get(&format!("password-session|{SERVER}|alice"))
            .unwrap(),
        Some(TOKEN.as_bytes().to_vec()),
        "retiring an old provider must not delete a newer remembered login"
    );
}

#[tokio::test]
async fn invalid_oidc_api_origin_is_rejected_before_session_initialization() {
    for server in INVALID_URLS {
        let oidc = oidc(server);
        assert_eq!(
            oidc.restore().await,
            Err(AuthError::from(OriginError::InvalidUrl)),
            "{server}"
        );
        assert_eq!(
            oidc.log_in().await,
            Err(AuthError::from(OriginError::InvalidUrl)),
            "{server}"
        );
        let provider = AppProvider::Oidc(oidc);
        assert!(
            matches!(
                provider.credentials("POST", SERVER).await,
                Err(AuthError::Http(_))
            ),
            "{server}"
        );
    }
}
