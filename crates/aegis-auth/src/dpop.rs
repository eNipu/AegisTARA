//! DPoP, demonstrating proof of possession (RFC 9449).
//!
//! A device holds one P-256 key. The identity provider binds its access
//! tokens to the key's thumbprint (`cnf.jkt`). With every request the client
//! sends a short-lived JWT, signed with the key, that names the HTTP method,
//! the URL, and the hash of the access token. A stolen access token is
//! useless without the key, and a stolen proof is useless for any other
//! request or after a minute.
//!
//! [`DeviceKey`] is the client half; [`DpopVerifier`] is the server half.
//!
//! # What the verifier checks
//!
//! type `dpop+jwt`; algorithm ES256; an embedded public EC key; signature;
//! `htm` equals the request method; `htu` equals the request URL without
//! query and fragment; `iat` inside the window; a `jti` never seen before in
//! the window (replay cache, bounded); `ath` equals the hash of the access
//! token when there is one; and, if the caller knows the token's `cnf.jkt`,
//! that the proof key's thumbprint equals it.

use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;

use ring::digest::{SHA256, digest};
use ring::rand::{SecureRandom, SystemRandom};
use ring::signature::{ECDSA_P256_SHA256_FIXED_SIGNING, EcdsaKeyPair, KeyPair};
use serde::{Deserialize, Serialize};

use crate::b64;
use crate::error::AuthError;
use crate::jose::{Alg, Jwk, Token};

const TYP: &str = "dpop+jwt";
/// Longest `jti` accepted, in bytes.
const MAX_JTI_BYTES: usize = 128;

/// The device's proof-of-possession key.
pub struct DeviceKey {
    pair: EcdsaKeyPair,
    pkcs8: Vec<u8>,
    jwk: Jwk,
    thumbprint: String,
}

impl std::fmt::Debug for DeviceKey {
    // Never print key material.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeviceKey")
            .field("thumbprint", &self.thumbprint)
            .finish_non_exhaustive()
    }
}

impl DeviceKey {
    /// Generates a new key with the OS random generator.
    ///
    /// # Errors
    ///
    /// [`AuthError::BadKey`] if the OS generator fails.
    pub fn generate() -> Result<Self, AuthError> {
        let pkcs8 =
            EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &SystemRandom::new())
                .map_err(|_| AuthError::BadKey)?;
        Self::from_pkcs8(pkcs8.as_ref())
    }

    /// Loads a key saved with [`to_pkcs8`](Self::to_pkcs8).
    ///
    /// # Errors
    ///
    /// [`AuthError::BadKey`] if the bytes are not a P-256 PKCS#8 key.
    pub fn from_pkcs8(pkcs8: &[u8]) -> Result<Self, AuthError> {
        let pair = EcdsaKeyPair::from_pkcs8(
            &ECDSA_P256_SHA256_FIXED_SIGNING,
            pkcs8,
            &SystemRandom::new(),
        )
        .map_err(|_| AuthError::BadKey)?;
        let public = pair.public_key().as_ref();
        let jwk = Jwk {
            kty: "EC".into(),
            crv: Some("P-256".into()),
            x: Some(b64::encode(&public[1..33])),
            y: Some(b64::encode(&public[33..65])),
            ..Jwk::default()
        };
        let thumbprint = jwk.thumbprint()?;
        Ok(DeviceKey {
            pair,
            pkcs8: pkcs8.to_vec(),
            jwk,
            thumbprint,
        })
    }

    /// The private key in PKCS#8 form, for the OS keychain. Treat as a secret.
    #[must_use]
    pub fn to_pkcs8(&self) -> &[u8] {
        &self.pkcs8
    }

    /// The public key.
    #[must_use]
    pub fn jwk(&self) -> &Jwk {
        &self.jwk
    }

    /// The key's RFC 7638 thumbprint: what `cnf.jkt` carries.
    #[must_use]
    pub fn thumbprint(&self) -> &str {
        &self.thumbprint
    }

    /// Signs a proof for one request.
    ///
    /// `url` is the full request URL; its query and fragment are dropped.
    /// `access_token` is the token the request carries, if any. `nonce` is
    /// the server-provided nonce (from a `DPoP-Nonce` header), if it asked
    /// for one.
    ///
    /// # Errors
    ///
    /// [`AuthError::Dpop`] for an unusable URL, [`AuthError::BadKey`] if
    /// signing fails.
    pub fn proof(
        &self,
        method: &str,
        url: &str,
        access_token: Option<&str>,
        nonce: Option<&str>,
        now: u64,
    ) -> Result<String, AuthError> {
        let rng = SystemRandom::new();
        let mut jti = [0u8; 16];
        rng.fill(&mut jti).map_err(|_| AuthError::BadKey)?;
        let header = ProofHeader {
            typ: TYP,
            alg: Alg::Es256.name(),
            jwk: &self.jwk,
        };
        let claims = ProofClaims {
            jti: b64::encode(jti),
            htm: method.to_owned(),
            htu: normalize_htu(url)?,
            iat: now,
            ath: access_token.map(ath_of),
            nonce: nonce.map(str::to_owned),
        };
        let input = format!(
            "{}.{}",
            b64::encode(serde_json::to_vec(&header).map_err(|_| AuthError::BadKey)?),
            b64::encode(serde_json::to_vec(&claims).map_err(|_| AuthError::BadKey)?)
        );
        let sig = self
            .pair
            .sign(&rng, input.as_bytes())
            .map_err(|_| AuthError::BadKey)?;
        Ok(format!("{input}.{}", b64::encode(sig.as_ref())))
    }
}

#[derive(Serialize)]
struct ProofHeader<'a> {
    typ: &'a str,
    alg: &'a str,
    jwk: &'a Jwk,
}

#[derive(Serialize, Deserialize)]
struct ProofClaims {
    jti: String,
    htm: String,
    htu: String,
    iat: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ath: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    nonce: Option<String>,
}

/// `base64url(SHA-256(access_token))`, the `ath` claim.
#[must_use]
pub fn ath_of(access_token: &str) -> String {
    b64::encode(digest(&SHA256, access_token.as_bytes()))
}

/// Reduces a URL to what `htu` compares: lowercase scheme and host, default
/// port dropped, path kept, query and fragment removed.
///
/// # Errors
///
/// [`AuthError::Dpop`] if the URL is not absolute `http` or `https`.
pub fn normalize_htu(url: &str) -> Result<String, AuthError> {
    let bad = AuthError::Dpop("url is not absolute");
    let without_fragment = url.split('#').next().unwrap_or_default();
    let without_query = without_fragment.split('?').next().unwrap_or_default();
    let uri: http::Uri = without_query.parse().map_err(|_| bad.clone())?;
    let scheme = uri
        .scheme_str()
        .ok_or_else(|| bad.clone())?
        .to_ascii_lowercase();
    let authority = uri.authority().ok_or_else(|| bad.clone())?;
    let default_port = match scheme.as_str() {
        "https" => 443,
        "http" => 80,
        _ => return Err(bad),
    };
    let host = authority.host().to_ascii_lowercase();
    let port = authority.port_u16().filter(|p| *p != default_port);
    let path = if uri.path().is_empty() {
        "/"
    } else {
        uri.path()
    };
    Ok(match port {
        Some(p) => format!("{scheme}://{host}:{p}{path}"),
        None => format!("{scheme}://{host}{path}"),
    })
}

/// Remembers proof IDs for the length of the acceptance window, so a proof
/// cannot be replayed. Bounded: when full of live entries it refuses new
/// proofs rather than forgetting old ones (fail closed).
pub struct ReplayCache {
    max_entries: usize,
    inner: Mutex<ReplayInner>,
}

#[derive(Default)]
struct ReplayInner {
    seen: HashMap<u128, u64>,
    /// `(key, expiry)` in insertion order; expiries never decrease, because
    /// every entry expires `window` after its own `now`.
    order: VecDeque<(u128, u64)>,
}

impl ReplayCache {
    /// A cache holding at most `max_entries` live proofs (16 bytes each plus
    /// map overhead; 200,000 entries is about 10 MB).
    #[must_use]
    pub fn new(max_entries: usize) -> Self {
        ReplayCache {
            max_entries,
            inner: Mutex::new(ReplayInner::default()),
        }
    }

    /// Records `jti`. O(1) amortized.
    ///
    /// # Errors
    ///
    /// [`AuthError::Replay`] if it was seen and is still live;
    /// [`AuthError::Dpop`] if the cache is full of live entries.
    pub fn insert(&self, jti: &str, expires: u64, now: u64) -> Result<(), AuthError> {
        let hash = digest(&SHA256, jti.as_bytes());
        let mut key = [0u8; 16];
        key.copy_from_slice(&hash.as_ref()[..16]);
        let key = u128::from_be_bytes(key);

        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        while let Some(&(k, exp)) = inner.order.front() {
            if exp > now {
                break;
            }
            inner.order.pop_front();
            if inner.seen.get(&k) == Some(&exp) {
                inner.seen.remove(&k);
            }
        }
        if inner.seen.get(&key).is_some_and(|exp| *exp > now) {
            return Err(AuthError::Replay);
        }
        if inner.seen.len() >= self.max_entries {
            return Err(AuthError::Dpop("replay cache is full"));
        }
        inner.seen.insert(key, expires);
        inner.order.push_back((key, expires));
        Ok(())
    }

    /// Live entries, for tests and metrics.
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .seen
            .len()
    }

    /// Whether the cache holds nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// The server half: checks proofs.
pub struct DpopVerifier {
    window: u64,
    skew: u64,
    replay: ReplayCache,
}

impl DpopVerifier {
    /// A verifier that accepts proofs issued up to `window` seconds ago and
    /// up to `skew` seconds in the future, and remembers up to `max_proofs`.
    #[must_use]
    pub fn new(window: u64, skew: u64, max_proofs: usize) -> Self {
        DpopVerifier {
            window,
            skew,
            replay: ReplayCache::new(max_proofs),
        }
    }

    /// The defaults: 60 s window, 5 s skew, 200,000 remembered proofs.
    #[must_use]
    pub fn standard() -> Self {
        Self::new(60, 5, 200_000)
    }

    /// Verifies a proof for one request and returns the thumbprint of the
    /// key that signed it.
    ///
    /// * `method` and `url` are those of the request as the client saw
    ///   them (the server's public URL, not an internal one).
    /// * `access_token` is the token the request carries; the proof must
    ///   hash it.
    /// * `expected_jkt` is the token's `cnf.jkt`; if given, the proof key
    ///   must match it.
    ///
    /// # Errors
    ///
    /// [`AuthError::Dpop`] naming the failed check, [`AuthError::Replay`],
    /// or the JOSE errors for a bad signature or key.
    pub fn verify(
        &self,
        proof: &str,
        method: &str,
        url: &str,
        access_token: Option<&str>,
        expected_jkt: Option<&str>,
        now: u64,
    ) -> Result<String, AuthError> {
        self.verify_with_nonce(proof, method, url, access_token, expected_jkt, None, now)
    }

    /// Like [`verify`](Self::verify), and if `required_nonce` is given the
    /// proof must carry exactly that nonce (RFC 9449 section 8).
    ///
    /// # Errors
    ///
    /// As [`verify`](Self::verify), plus [`AuthError::Dpop`] for a wrong
    /// nonce.
    #[allow(clippy::too_many_arguments)]
    pub fn verify_with_nonce(
        &self,
        proof: &str,
        method: &str,
        url: &str,
        access_token: Option<&str>,
        expected_jkt: Option<&str>,
        required_nonce: Option<&str>,
        now: u64,
    ) -> Result<String, AuthError> {
        let token = Token::parse(proof)?;
        if token.header.typ.as_deref() != Some(TYP) {
            return Err(AuthError::Dpop("wrong type"));
        }
        if token.alg()? != Alg::Es256 {
            return Err(AuthError::Dpop("wrong algorithm"));
        }
        let jwk = token.header.jwk.as_ref().ok_or(AuthError::Dpop("no key"))?;
        let key = jwk.public_key()?;
        token.verify(&key)?;
        let jkt = jwk.thumbprint()?;
        let claims: ProofClaims = token.claims()?;

        if claims.htm != method {
            return Err(AuthError::Dpop("method does not match"));
        }
        if normalize_htu(&claims.htu)? != normalize_htu(url)? {
            return Err(AuthError::Dpop("url does not match"));
        }
        if claims.iat > now.saturating_add(self.skew) {
            return Err(AuthError::Dpop("issued in the future"));
        }
        if claims.iat.saturating_add(self.window) < now {
            return Err(AuthError::Dpop("too old"));
        }
        if let Some(token) = access_token {
            if claims.ath.as_deref() != Some(&ath_of(token)) {
                return Err(AuthError::Dpop("access token hash does not match"));
            }
        } else if claims.ath.is_some() {
            return Err(AuthError::Dpop("unexpected token hash"));
        }
        if let Some(expected) = expected_jkt
            && expected != jkt
        {
            return Err(AuthError::Dpop("key is not the one the token is bound to"));
        }
        if let Some(nonce) = required_nonce
            && claims.nonce.as_deref() != Some(nonce)
        {
            return Err(AuthError::Dpop("nonce does not match"));
        }
        if claims.jti.is_empty() || claims.jti.len() > MAX_JTI_BYTES {
            return Err(AuthError::Dpop("bad proof id"));
        }
        // Last: only a fully valid proof is remembered.
        self.replay
            .insert(&claims.jti, claims.iat.saturating_add(self.window), now)?;
        Ok(jkt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 1_800_000_000;
    const URL: &str = "https://aegis.example.com/aegis.v1.Aegis/SubmitOps";

    fn verifier() -> DpopVerifier {
        DpopVerifier::standard()
    }

    #[test]
    fn a_proof_verifies_and_names_the_key() {
        let key = DeviceKey::generate().expect("key");
        let proof = key
            .proof("POST", URL, Some("token-1"), None, NOW)
            .expect("proof");
        let jkt = verifier()
            .verify(
                &proof,
                "POST",
                URL,
                Some("token-1"),
                Some(key.thumbprint()),
                NOW,
            )
            .expect("verifies");
        assert_eq!(jkt, key.thumbprint());
    }

    #[test]
    fn a_proof_works_once() {
        let key = DeviceKey::generate().expect("key");
        let v = verifier();
        let proof = key.proof("POST", URL, None, None, NOW).expect("proof");
        assert!(v.verify(&proof, "POST", URL, None, None, NOW).is_ok());
        assert_eq!(
            v.verify(&proof, "POST", URL, None, None, NOW + 1),
            Err(AuthError::Replay)
        );
    }

    #[test]
    fn a_proof_is_bound_to_method_url_and_token() {
        let key = DeviceKey::generate().expect("key");
        let v = verifier();
        let mk = |tok: Option<&str>| key.proof("POST", URL, tok, None, NOW).expect("proof");
        let wrong_method = v.verify(&mk(Some("t")), "GET", URL, Some("t"), None, NOW);
        assert_eq!(wrong_method, Err(AuthError::Dpop("method does not match")));
        let other_url = "https://aegis.example.com/aegis.v1.Aegis/OpenProject";
        let wrong_url = v.verify(&mk(Some("t")), "POST", other_url, Some("t"), None, NOW);
        assert_eq!(wrong_url, Err(AuthError::Dpop("url does not match")));
        let wrong_token = v.verify(&mk(Some("t")), "POST", URL, Some("other"), None, NOW);
        assert_eq!(
            wrong_token,
            Err(AuthError::Dpop("access token hash does not match"))
        );
        let missing_token = v.verify(&mk(None), "POST", URL, Some("t"), None, NOW);
        assert_eq!(
            missing_token,
            Err(AuthError::Dpop("access token hash does not match"))
        );
        let extra_token = v.verify(&mk(Some("t")), "POST", URL, None, None, NOW);
        assert_eq!(extra_token, Err(AuthError::Dpop("unexpected token hash")));
    }

    #[test]
    fn a_proof_from_another_key_does_not_match_the_tokens_binding() {
        let owner = DeviceKey::generate().expect("key");
        let thief = DeviceKey::generate().expect("key");
        let proof = thief
            .proof("POST", URL, Some("t"), None, NOW)
            .expect("proof");
        assert_eq!(
            verifier().verify(
                &proof,
                "POST",
                URL,
                Some("t"),
                Some(owner.thumbprint()),
                NOW
            ),
            Err(AuthError::Dpop("key is not the one the token is bound to"))
        );
    }

    #[test]
    fn proofs_expire_and_cannot_come_from_the_future() {
        let key = DeviceKey::generate().expect("key");
        let v = verifier();
        let old = key.proof("POST", URL, None, None, NOW - 61).expect("proof");
        assert_eq!(
            v.verify(&old, "POST", URL, None, None, NOW),
            Err(AuthError::Dpop("too old"))
        );
        let edge = key.proof("POST", URL, None, None, NOW - 60).expect("proof");
        assert!(v.verify(&edge, "POST", URL, None, None, NOW).is_ok());
        let future = key.proof("POST", URL, None, None, NOW + 6).expect("proof");
        assert_eq!(
            v.verify(&future, "POST", URL, None, None, NOW),
            Err(AuthError::Dpop("issued in the future"))
        );
        let near = key.proof("POST", URL, None, None, NOW + 5).expect("proof");
        assert!(v.verify(&near, "POST", URL, None, None, NOW).is_ok());
    }

    #[test]
    fn a_forged_or_tampered_proof_is_rejected() {
        let key = DeviceKey::generate().expect("key");
        let proof = key.proof("POST", URL, None, None, NOW).expect("proof");
        let mut parts: Vec<String> = proof.split('.').map(str::to_owned).collect();
        parts[1] = b64::encode(
            br#"{"jti":"x","htm":"POST","htu":"https://aegis.example.com/aegis.v1.Aegis/SubmitOps","iat":1800000000}"#,
        );
        let forged = parts.join(".");
        assert_eq!(
            verifier().verify(&forged, "POST", URL, None, None, NOW),
            Err(AuthError::BadSignature)
        );
    }

    #[test]
    fn the_wrong_token_type_is_rejected() {
        // A perfectly signed JWT that is not a DPoP proof.
        let key = DeviceKey::generate().expect("key");
        let header = serde_json::json!({"typ":"JWT","alg":"ES256","jwk":key.jwk()});
        let input = format!(
            "{}.{}",
            b64::encode(header.to_string()),
            b64::encode(r#"{"jti":"a","htm":"POST","htu":"https://x/","iat":1}"#)
        );
        let sig = key
            .pair
            .sign(&SystemRandom::new(), input.as_bytes())
            .expect("sign");
        let jwt = format!("{input}.{}", b64::encode(sig.as_ref()));
        assert_eq!(
            verifier().verify(&jwt, "POST", "https://x/", None, None, 1),
            Err(AuthError::Dpop("wrong type"))
        );
    }

    #[test]
    fn htu_normalization_ignores_case_default_ports_query_and_fragment() {
        let a = normalize_htu("HTTPS://Aegis.Example.COM:443/x/y?q=1#frag").expect("url");
        assert_eq!(a, "https://aegis.example.com/x/y");
        assert_eq!(normalize_htu("http://h:80").expect("url"), "http://h/");
        assert_eq!(
            normalize_htu("http://h:8080/p").expect("url"),
            "http://h:8080/p"
        );
        assert!(normalize_htu("/relative").is_err());
        assert!(normalize_htu("ftp://h/").is_err());
        assert!(normalize_htu("").is_err());
    }

    #[test]
    fn key_round_trips_through_pkcs8_with_the_same_thumbprint() {
        let key = DeviceKey::generate().expect("key");
        let again = DeviceKey::from_pkcs8(key.to_pkcs8()).expect("loads");
        assert_eq!(again.thumbprint(), key.thumbprint());
        assert!(DeviceKey::from_pkcs8(b"junk").is_err());
        assert!(!format!("{key:?}").contains(&b64::encode(key.to_pkcs8())));
    }

    #[test]
    fn the_replay_cache_forgets_expired_proofs_and_fails_closed_when_full() {
        let cache = ReplayCache::new(2);
        cache.insert("a", 100, 0).expect("new");
        cache.insert("b", 110, 0).expect("new");
        assert_eq!(cache.insert("a", 100, 50), Err(AuthError::Replay));
        // Full of live entries: refuse rather than forget.
        assert!(matches!(
            cache.insert("c", 120, 50),
            Err(AuthError::Dpop(_))
        ));
        // After "a" expires there is room, and "a" may be used again.
        cache.insert("c", 200, 100).expect("room after expiry");
        assert_eq!(cache.len(), 2);
        cache
            .insert("a", 300, 111)
            .expect("expired entries are gone");
    }
}
