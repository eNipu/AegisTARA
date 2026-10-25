//! A cached view of an identity provider's signing keys (its JWKS).
//!
//! Keys are fetched on first use and again when they are older than the
//! TTL. A token signed with an unknown `kid` triggers one refetch, but never
//! more than one per `min_refetch`, so an attacker sending random `kid`s
//! cannot turn the server into a request amplifier against the provider.
//! Concurrent callers share one fetch.

use std::sync::Arc;
use std::time::Duration;

use serde::Deserialize;
use tokio::sync::{Mutex, RwLock};

use crate::clock::Clock;
use crate::error::AuthError;
use crate::http::{HttpClient, HttpRequest};
use crate::jose::{Alg, Jwk, PublicKey};

/// Most keys read from one JWKS document; extras are ignored.
const MAX_KEYS: usize = 64;

#[derive(Deserialize)]
struct JwkSet {
    keys: Vec<serde_json::Value>,
}

#[derive(Clone)]
struct Entry {
    kid: Option<String>,
    alg: Option<Alg>,
    key: PublicKey,
}

#[derive(Default)]
struct State {
    keys: Vec<Entry>,
    fetched_at: u64,
    /// Whether a fetch ever succeeded (`fetched_at == 0` is a valid time in tests).
    loaded: bool,
    last_attempt: Option<u64>,
}

/// Fetches and caches the keys at one `jwks_uri`.
pub struct JwksCache<H> {
    http: Arc<H>,
    uri: String,
    ttl: u64,
    min_refetch: u64,
    clock: Clock,
    state: RwLock<State>,
    fetch_gate: Mutex<()>,
}

impl<H: HttpClient> JwksCache<H> {
    /// A cache for `uri` with a 10 minute TTL and at most one forced
    /// refetch every 30 seconds.
    #[must_use]
    pub fn new(http: Arc<H>, uri: &str, clock: Clock) -> Self {
        Self::with_timing(
            http,
            uri,
            clock,
            Duration::from_secs(600),
            Duration::from_secs(30),
        )
    }

    /// Like [`new`](Self::new) with explicit timing.
    #[must_use]
    pub fn with_timing(
        http: Arc<H>,
        uri: &str,
        clock: Clock,
        ttl: Duration,
        min_refetch: Duration,
    ) -> Self {
        JwksCache {
            http,
            uri: uri.to_owned(),
            ttl: ttl.as_secs(),
            min_refetch: min_refetch.as_secs(),
            clock,
            state: RwLock::new(State::default()),
            fetch_gate: Mutex::new(()),
        }
    }

    /// The key to verify a token with.
    ///
    /// With a `kid` the key must carry it. Without one, the cache accepts
    /// a single usable key of the right type, never a guess among several.
    ///
    /// # Errors
    ///
    /// [`AuthError::UnknownKey`] if no key fits (after one refetch if that
    /// is allowed), or the fetch errors.
    pub async fn key_for(&self, kid: Option<&str>, alg: Alg) -> Result<PublicKey, AuthError> {
        let now = (self.clock)();
        {
            let state = self.state.read().await;
            if state.loaded
                && now < state.fetched_at.saturating_add(self.ttl)
                && let Some(key) = find(&state.keys, kid, alg)
            {
                return Ok(key);
            }
        }
        // Slow path: one fetcher at a time; the others reuse its result.
        let _gate = self.fetch_gate.lock().await;
        {
            let state = self.state.read().await;
            let fresh = state.loaded && now < state.fetched_at.saturating_add(self.ttl);
            if let Some(key) = find(&state.keys, kid, alg).filter(|_| fresh) {
                return Ok(key);
            }
            let throttled = state
                .last_attempt
                .is_some_and(|t| now < t.saturating_add(self.min_refetch));
            // A fresh cache that lacks the kid may refetch at most every
            // `min_refetch`; an expired or empty cache always may.
            if fresh && throttled {
                return Err(AuthError::UnknownKey);
            }
        }
        self.refresh(now).await?;
        let state = self.state.read().await;
        find(&state.keys, kid, alg).ok_or(AuthError::UnknownKey)
    }

    async fn refresh(&self, now: u64) -> Result<(), AuthError> {
        self.state.write().await.last_attempt = Some(now);
        let response = self.http.send(HttpRequest::get(&self.uri)).await?;
        if response.status != 200 {
            return Err(AuthError::Http(format!(
                "jwks answered {}",
                response.status
            )));
        }
        let set: JwkSet =
            serde_json::from_slice(&response.body).map_err(|_| AuthError::Protocol)?;
        let keys: Vec<Entry> = set
            .keys
            .into_iter()
            .take(MAX_KEYS)
            .filter_map(|v| {
                let jwk: Jwk = serde_json::from_value(v).ok()?;
                let key = jwk.public_key().ok()?;
                Some(Entry {
                    alg: jwk.alg.as_deref().and_then(Alg::parse),
                    kid: jwk.kid,
                    key,
                })
            })
            .collect();
        if keys.is_empty() {
            return Err(AuthError::Protocol);
        }
        let mut state = self.state.write().await;
        state.keys = keys;
        state.fetched_at = now;
        state.loaded = true;
        Ok(())
    }
}

fn find(keys: &[Entry], kid: Option<&str>, alg: Alg) -> Option<PublicKey> {
    let fits = |e: &&Entry| {
        e.alg.is_none_or(|a| a == alg)
            && matches!(
                (&e.key, alg),
                (PublicKey::Ec(_), Alg::Es256) | (PublicKey::Rsa { .. }, Alg::Rs256)
            )
    };
    match kid {
        Some(kid) => keys
            .iter()
            .filter(fits)
            .find(|e| e.kid.as_deref() == Some(kid))
            .map(|e| e.key.clone()),
        None => {
            let mut usable = keys.iter().filter(fits);
            match (usable.next(), usable.next()) {
                (Some(only), None) => Some(only.key.clone()),
                _ => None,
            }
        }
    }
}
