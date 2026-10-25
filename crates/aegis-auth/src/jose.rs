//! The small part of JOSE (RFC 7515 to 7519, 7638) that login needs.
//!
//! - Compact JWS only. Algorithms: `ES256` and `RS256`, nothing else
//!   (`none` and the HMAC family are refused by name, so an attacker cannot
//!   turn a public key into an HMAC secret).
//! - Keys are public JWKs: EC P-256 and RSA 2048 to 8192 bits. A JWK with a
//!   private member is refused.
//! - Every size is capped before parsing (R3).
//!
//! Signature math is `ring`'s; nothing here implements a primitive.

use ring::signature::{
    ECDSA_P256_SHA256_FIXED, RSA_PKCS1_2048_8192_SHA256, RsaPublicKeyComponents, UnparsedPublicKey,
};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::b64;
use crate::error::AuthError;

/// Longest compact token accepted, in bytes. Real ID and access tokens are
/// 1 to 4 KiB.
pub const MAX_TOKEN_BYTES: usize = 16 * 1024;
/// Longest decoded protected header, in bytes.
const MAX_HEADER_BYTES: usize = 4 * 1024;
/// Smallest and largest RSA modulus accepted, in bytes (2048 to 8192 bits).
const RSA_MODULUS_BYTES: std::ops::RangeInclusive<usize> = 256..=1024;

/// A signature algorithm this crate accepts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Alg {
    /// ECDSA over P-256 with SHA-256.
    Es256,
    /// RSASSA-PKCS1-v1_5 with SHA-256.
    Rs256,
}

impl Alg {
    /// Maps the `alg` header text.
    #[must_use]
    pub fn parse(text: &str) -> Option<Alg> {
        match text {
            "ES256" => Some(Alg::Es256),
            "RS256" => Some(Alg::Rs256),
            _ => None,
        }
    }

    /// The `alg` header text.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Alg::Es256 => "ES256",
            Alg::Rs256 => "RS256",
        }
    }
}

/// A JSON Web Key as published by an identity provider or sent in a DPoP header.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct Jwk {
    /// Key type: `EC` or `RSA`.
    pub kty: String,
    /// Curve (EC).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crv: Option<String>,
    /// Public point x (EC), base64url.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x: Option<String>,
    /// Public point y (EC), base64url.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y: Option<String>,
    /// Modulus (RSA), base64url.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub n: Option<String>,
    /// Public exponent (RSA), base64url.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub e: Option<String>,
    /// Key ID.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kid: Option<String>,
    /// Intended algorithm.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alg: Option<String>,
    /// Intended use: `sig` or `enc`.
    #[serde(default, rename = "use", skip_serializing_if = "Option::is_none")]
    pub use_: Option<String>,
    /// Private exponent or scalar. Present only in a key that must be refused.
    #[serde(default, skip_serializing)]
    pub d: Option<String>,
}

/// A public key ready for `ring`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PublicKey {
    /// Uncompressed SEC1 point `04 || x || y`.
    Ec(Vec<u8>),
    /// Big-endian modulus and exponent.
    Rsa {
        /// Modulus.
        n: Vec<u8>,
        /// Exponent.
        e: Vec<u8>,
    },
}

fn strip_leading_zeros(mut bytes: Vec<u8>) -> Vec<u8> {
    let zeros = bytes.iter().take_while(|b| **b == 0).count();
    bytes.drain(..zeros);
    bytes
}

impl Jwk {
    /// Validates the key and converts it. Time O(key size).
    ///
    /// # Errors
    ///
    /// [`AuthError::BadKey`] for a private key, an unsupported type or
    /// curve, a wrong coordinate length, a modulus outside 2048 to 8192
    /// bits, or a key meant for encryption.
    pub fn public_key(&self) -> Result<PublicKey, AuthError> {
        if self.d.is_some() || self.use_.as_deref().is_some_and(|u| u != "sig") {
            return Err(AuthError::BadKey);
        }
        let field = |f: &Option<String>| -> Result<Vec<u8>, AuthError> {
            b64::decode(f.as_deref().ok_or(AuthError::BadKey)?).map_err(|_| AuthError::BadKey)
        };
        match self.kty.as_str() {
            "EC" => {
                if self.crv.as_deref() != Some("P-256") {
                    return Err(AuthError::BadKey);
                }
                let (x, y) = (field(&self.x)?, field(&self.y)?);
                if x.len() != 32 || y.len() != 32 {
                    return Err(AuthError::BadKey);
                }
                let mut point = Vec::with_capacity(65);
                point.push(0x04);
                point.extend_from_slice(&x);
                point.extend_from_slice(&y);
                Ok(PublicKey::Ec(point))
            }
            "RSA" => {
                let n = strip_leading_zeros(field(&self.n)?);
                let e = strip_leading_zeros(field(&self.e)?);
                if !RSA_MODULUS_BYTES.contains(&n.len()) || e.is_empty() || e.len() > 8 {
                    return Err(AuthError::BadKey);
                }
                Ok(PublicKey::Rsa { n, e })
            }
            _ => Err(AuthError::BadKey),
        }
    }

    /// The RFC 7638 thumbprint: SHA-256 over the required members in
    /// lexicographic order, base64url. Identifies a key independent of its
    /// `kid` and member order.
    ///
    /// # Errors
    ///
    /// [`AuthError::BadKey`] as for [`public_key`](Self::public_key).
    pub fn thumbprint(&self) -> Result<String, AuthError> {
        // Validate first, so the canonical members below are well formed.
        self.public_key()?;
        let canonical = match self.kty.as_str() {
            "EC" => format!(
                "{{\"crv\":\"P-256\",\"kty\":\"EC\",\"x\":\"{}\",\"y\":\"{}\"}}",
                self.x.as_deref().unwrap_or_default(),
                self.y.as_deref().unwrap_or_default()
            ),
            _ => format!(
                "{{\"e\":\"{}\",\"kty\":\"RSA\",\"n\":\"{}\"}}",
                self.e.as_deref().unwrap_or_default(),
                self.n.as_deref().unwrap_or_default()
            ),
        };
        Ok(b64::encode(ring::digest::digest(
            &ring::digest::SHA256,
            canonical.as_bytes(),
        )))
    }
}

/// The protected header of a token.
#[derive(Clone, Debug, Deserialize)]
pub struct Header {
    /// Signature algorithm name.
    pub alg: String,
    /// Media type, for example `dpop+jwt`.
    #[serde(default)]
    pub typ: Option<String>,
    /// Key ID.
    #[serde(default)]
    pub kid: Option<String>,
    /// Embedded public key (DPoP).
    #[serde(default)]
    pub jwk: Option<Jwk>,
    /// Critical extensions. Present means refused: this crate understands none.
    #[serde(default)]
    pub crit: Option<serde_json::Value>,
}

/// A parsed, not yet verified, compact JWS.
#[derive(Debug)]
pub struct Token<'a> {
    /// The protected header.
    pub header: Header,
    signing_input: &'a str,
    payload: &'a str,
    signature: Vec<u8>,
}

impl<'a> Token<'a> {
    /// Splits and decodes a compact token. Nothing is trusted yet.
    ///
    /// # Errors
    ///
    /// [`AuthError::TooLarge`], [`AuthError::Malformed`], or
    /// [`AuthError::UnsupportedAlgorithm`] for `crit` and unknown `alg`.
    pub fn parse(text: &'a str) -> Result<Self, AuthError> {
        if text.len() > MAX_TOKEN_BYTES {
            return Err(AuthError::TooLarge);
        }
        let mut parts = text.split('.');
        let (Some(h), Some(p), Some(s), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return Err(AuthError::Malformed);
        };
        let header_bytes = b64::decode(h)?;
        if header_bytes.len() > MAX_HEADER_BYTES {
            return Err(AuthError::TooLarge);
        }
        let header: Header =
            serde_json::from_slice(&header_bytes).map_err(|_| AuthError::Malformed)?;
        if header.crit.is_some() || Alg::parse(&header.alg).is_none() {
            return Err(AuthError::UnsupportedAlgorithm);
        }
        // The signing input is `header '.' payload` exactly as received.
        let signing_input = &text[..h.len() + 1 + p.len()];
        Ok(Token {
            header,
            signing_input,
            payload: p,
            signature: b64::decode(s)?,
        })
    }

    /// The algorithm named in the header (checked by [`parse`](Self::parse)).
    ///
    /// # Errors
    ///
    /// [`AuthError::UnsupportedAlgorithm`] cannot happen after `parse`.
    pub fn alg(&self) -> Result<Alg, AuthError> {
        Alg::parse(&self.header.alg).ok_or(AuthError::UnsupportedAlgorithm)
    }

    /// Checks the signature with `key`. The key type must match the
    /// header's algorithm.
    ///
    /// # Errors
    ///
    /// [`AuthError::BadSignature`] if it does not verify,
    /// [`AuthError::BadKey`] if the key type does not fit the algorithm.
    pub fn verify(&self, key: &PublicKey) -> Result<(), AuthError> {
        let message = self.signing_input.as_bytes();
        match (self.alg()?, key) {
            (Alg::Es256, PublicKey::Ec(point)) => {
                UnparsedPublicKey::new(&ECDSA_P256_SHA256_FIXED, point)
                    .verify(message, &self.signature)
                    .map_err(|_| AuthError::BadSignature)
            }
            (Alg::Rs256, PublicKey::Rsa { n, e }) => RsaPublicKeyComponents { n, e }
                .verify(&RSA_PKCS1_2048_8192_SHA256, message, &self.signature)
                .map_err(|_| AuthError::BadSignature),
            _ => Err(AuthError::BadKey),
        }
    }

    /// Decodes the claims. Call only after [`verify`](Self::verify).
    ///
    /// # Errors
    ///
    /// [`AuthError::Malformed`] if the payload is not the JSON `T`.
    pub fn claims<T: DeserializeOwned>(&self) -> Result<T, AuthError> {
        let bytes = b64::decode(self.payload)?;
        serde_json::from_slice(&bytes).map_err(|_| AuthError::Malformed)
    }
}

#[cfg(test)]
mod tests {
    use ring::rand::SystemRandom;
    use ring::signature::{ECDSA_P256_SHA256_FIXED_SIGNING, EcdsaKeyPair, KeyPair};

    use super::*;

    /// A fresh P-256 key and its JWK.
    fn ec_key() -> (EcdsaKeyPair, Jwk) {
        let rng = SystemRandom::new();
        let pkcs8 =
            EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &rng).expect("gen");
        let pair = EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, pkcs8.as_ref(), &rng)
            .expect("load");
        let public = pair.public_key().as_ref();
        let jwk = Jwk {
            kty: "EC".into(),
            crv: Some("P-256".into()),
            x: Some(b64::encode(&public[1..33])),
            y: Some(b64::encode(&public[33..65])),
            ..Jwk::default()
        };
        (pair, jwk)
    }

    fn sign(pair: &EcdsaKeyPair, header: &str, payload: &str) -> String {
        let input = format!("{}.{}", b64::encode(header), b64::encode(payload));
        let sig = pair
            .sign(&SystemRandom::new(), input.as_bytes())
            .expect("sign");
        format!("{input}.{}", b64::encode(sig.as_ref()))
    }

    #[test]
    fn a_signed_token_verifies_and_yields_its_claims() {
        let (pair, jwk) = ec_key();
        let token = sign(&pair, r#"{"alg":"ES256","kid":"k1"}"#, r#"{"sub":"alice"}"#);
        let parsed = Token::parse(&token).expect("parses");
        assert_eq!(parsed.header.kid.as_deref(), Some("k1"));
        parsed
            .verify(&jwk.public_key().expect("key"))
            .expect("verifies");
        let claims: serde_json::Value = parsed.claims().expect("claims");
        assert_eq!(claims["sub"], "alice");
    }

    #[test]
    fn any_change_to_the_token_breaks_the_signature() {
        let (pair, jwk) = ec_key();
        let key = jwk.public_key().expect("key");
        let token = sign(&pair, r#"{"alg":"ES256"}"#, r#"{"sub":"alice"}"#);
        let parts: Vec<&str> = token.split('.').collect();
        let forged_payload = format!(
            "{}.{}.{}",
            parts[0],
            b64::encode(r#"{"sub":"root"}"#),
            parts[2]
        );
        assert_eq!(
            Token::parse(&forged_payload).expect("parses").verify(&key),
            Err(AuthError::BadSignature)
        );
        // A different key.
        let (_, other) = ec_key();
        assert_eq!(
            Token::parse(&token)
                .expect("parses")
                .verify(&other.public_key().expect("key")),
            Err(AuthError::BadSignature)
        );
    }

    #[test]
    fn dangerous_and_unknown_algorithms_are_refused_before_any_key_is_used() {
        for alg in ["none", "HS256", "HS512", "PS256", "ES384", "RS512", "es256"] {
            let token = format!(
                "{}.{}.{}",
                b64::encode(format!(r#"{{"alg":"{alg}"}}"#)),
                b64::encode("{}"),
                b64::encode("sig")
            );
            assert_eq!(
                Token::parse(&token).err(),
                Some(AuthError::UnsupportedAlgorithm),
                "{alg}"
            );
        }
        let crit = format!(
            "{}.{}.{}",
            b64::encode(r#"{"alg":"ES256","crit":["exp"]}"#),
            b64::encode("{}"),
            b64::encode("s")
        );
        assert_eq!(
            Token::parse(&crit).err(),
            Some(AuthError::UnsupportedAlgorithm)
        );
    }

    #[test]
    fn malformed_and_oversized_tokens_are_refused() {
        for bad in ["", "a", "a.b", "a.b.c.d", "!.!.!", "e30.e30.", "e30.e30"] {
            assert!(Token::parse(bad).is_err(), "{bad:?}");
        }
        let big = "a".repeat(MAX_TOKEN_BYTES + 1);
        assert_eq!(Token::parse(&big).err(), Some(AuthError::TooLarge));
    }

    #[test]
    fn the_key_type_must_fit_the_algorithm() {
        let (pair, _) = ec_key();
        let token = sign(&pair, r#"{"alg":"ES256"}"#, "{}");
        let rsa = PublicKey::Rsa {
            n: vec![1; 256],
            e: vec![1, 0, 1],
        };
        assert_eq!(
            Token::parse(&token).expect("parses").verify(&rsa),
            Err(AuthError::BadKey)
        );
    }

    #[test]
    fn jwk_validation_rejects_private_wrong_curve_and_weak_keys() {
        let (_, good) = ec_key();
        assert!(good.public_key().is_ok());
        let mut private = good.clone();
        private.d = Some("AA".into());
        assert_eq!(private.public_key(), Err(AuthError::BadKey));
        let mut curve = good.clone();
        curve.crv = Some("P-384".into());
        assert_eq!(curve.public_key(), Err(AuthError::BadKey));
        let mut short = good.clone();
        short.x = Some(b64::encode([1u8; 31]));
        assert_eq!(short.public_key(), Err(AuthError::BadKey));
        let mut enc = good;
        enc.use_ = Some("enc".into());
        assert_eq!(enc.public_key(), Err(AuthError::BadKey));

        let rsa = |n: usize| Jwk {
            kty: "RSA".into(),
            n: Some(b64::encode(vec![0xff; n])),
            e: Some(b64::encode([1u8, 0, 1])),
            ..Jwk::default()
        };
        assert!(rsa(256).public_key().is_ok());
        assert_eq!(rsa(128).public_key(), Err(AuthError::BadKey));
        assert_eq!(rsa(2000).public_key(), Err(AuthError::BadKey));
        assert_eq!(
            Jwk {
                kty: "oct".into(),
                ..Jwk::default()
            }
            .public_key(),
            Err(AuthError::BadKey)
        );
    }

    #[test]
    fn thumbprints_match_rfc_7638_section_3_1() {
        // The RSA example key of RFC 7638 section 3.1.
        let jwk = Jwk {
            kty: "RSA".into(),
            n: Some(
                "0vx7agoebGcQSuuPiLJXZptN9nndrQmbXEps2aiAFbWhM78LhWx4cbbfAAtVT86zwu1RK7aPFFxuhDR1L6tSoc_BJECPebWKRXjBZCiFV4n3oknjhMstn64tZ_2W-5JsGY4Hc5n9yBXArwl93lqt7_RN5w6Cf0h4QyQ5v-65YGjQR0_FDW2QvzqY368QQMicAtaSqzs8KJZgnYb9c7d0zgdAZHzu6qMQvRL5hajrn1n91CbOpbISD08qNLyrdkt-bFTWhAI4vMQFh6WeZu0fM4lFd2NcRwr3XPksINHaQ-G_xBniIqbw0Ls1jF44-csFCur-kEgU8awapJzKnqDKgw".into(),
            ),
            e: Some("AQAB".into()),
            ..Jwk::default()
        };
        assert_eq!(
            jwk.thumbprint().expect("thumbprint"),
            "NzbLsXh8uDCcd-6MNwXF4W_7noWXFZAfHkxZsRGC9Xs"
        );
    }

    #[test]
    fn thumbprint_ignores_kid_and_member_order() {
        let (_, mut jwk) = ec_key();
        let before = jwk.thumbprint().expect("thumbprint");
        jwk.kid = Some("anything".into());
        jwk.alg = Some("ES256".into());
        assert_eq!(jwk.thumbprint().expect("thumbprint"), before);
    }
}
