//! Signed update manifests, end to end with real Ed25519 keys.

use aegis_license::{
    Artifact, Issuer, License, Manifest, ManifestError, Offer, TrustedKeys, Version,
};
use proptest::prelude::*;

fn manifest() -> Manifest {
    Manifest {
        version: 1,
        kid: "upd".into(),
        release: "0.4.0".into(),
        published_at: 1_800_000_000,
        artifacts: vec![Artifact {
            platform: "linux-x86_64".into(),
            url: "https://downloads.example.com/aegis-0.4.0-linux-x86_64.tar.gz".into(),
            sha256: "0".repeat(64),
            bytes: 1234,
        }],
        notes: "What is new.".into(),
    }
}

struct Setup {
    issuer: Issuer,
    keys: TrustedKeys,
    file: String,
}

fn setup() -> Setup {
    let (pkcs8, public) = Issuer::generate().expect("key");
    let issuer = Issuer::from_pkcs8(&pkcs8).expect("loads");
    let keys = TrustedKeys::single("upd", public).expect("keys");
    let file = issuer.sign_manifest(&manifest()).expect("signs");
    Setup { issuer, keys, file }
}

#[test]
fn a_signed_manifest_verifies_and_offers_a_newer_release() {
    let s = setup();
    assert!(s.file.starts_with("AEGIS-UPDATE-1."));
    let m = Manifest::verify(&s.file, &s.keys).expect("verifies");
    assert_eq!(m, manifest());
    let current = Version::parse("0.3.9").expect("version");
    assert!(matches!(
        m.offer(current, "linux-x86_64"),
        Offer::Available { release, .. } if release.to_string() == "0.4.0"
    ));
    assert_eq!(
        m.offer(Version::parse("0.4.0").expect("version"), "linux-x86_64"),
        Offer::UpToDate
    );
}

#[test]
fn the_wrong_key_unknown_key_and_tampering_are_refused() {
    let s = setup();
    let (_, other) = Issuer::generate().expect("key");
    assert_eq!(
        Manifest::verify(&s.file, &TrustedKeys::single("upd", other).expect("keys")),
        Err(ManifestError::BadSignature)
    );
    assert_eq!(
        Manifest::verify(&s.file, &TrustedKeys::single("else", other).expect("keys")),
        Err(ManifestError::UnknownKey)
    );
    assert_eq!(
        Manifest::verify(&s.file, &TrustedKeys::none()),
        Err(ManifestError::UnknownKey)
    );

    // A new URL under the old signature.
    let mut evil = manifest();
    evil.artifacts[0].url = "https://evil.example.com/x.tar.gz".into();
    let payload = serde_json::to_vec(&evil).expect("json");
    let parts: Vec<&str> = s.file.split('.').collect();
    use base64::Engine;
    let forged = format!(
        "{}.{}.{}",
        parts[0],
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(payload),
        parts[2]
    );
    assert_eq!(
        Manifest::verify(&forged, &s.keys),
        Err(ManifestError::BadSignature)
    );
}

#[test]
fn a_license_is_not_a_manifest_and_a_manifest_is_not_a_license() {
    // The same key signs both kinds; the file tag and the signing domain keep
    // them apart.
    let (pkcs8, public) = Issuer::generate().expect("key");
    let issuer = Issuer::from_pkcs8(&pkcs8).expect("loads");
    let keys = TrustedKeys::single("k", public).expect("keys");
    let license = issuer
        .sign(&License {
            version: 1,
            id: "l".into(),
            kid: "k".into(),
            customer: "C".into(),
            seats: 1,
            issued_at: 1,
            expires_at: 2,
            grace_days: 0,
            features: vec![],
        })
        .expect("signs");
    let mut m = manifest();
    m.kid = "k".into();
    let update = issuer.sign_manifest(&m).expect("signs");

    assert!(Manifest::verify(&license, &keys).is_err());
    assert!(License::verify(&update, &keys).is_err());

    // Even with the tag renamed, the signature does not carry over.
    let relabelled = update.replacen("AEGIS-UPDATE-1", "AEGIS-LICENSE-1", 1);
    assert!(License::verify(&relabelled, &keys).is_err());
    let relabelled = license.replacen("AEGIS-LICENSE-1", "AEGIS-UPDATE-1", 1);
    assert!(Manifest::verify(&relabelled, &keys).is_err());
}

#[test]
fn the_issuer_will_not_sign_a_manifest_the_client_would_refuse() {
    let s = setup();
    let mut bad = manifest();
    bad.artifacts[0].url = "http://insecure.example.com/x".into();
    assert!(matches!(
        s.issuer.sign_manifest(&bad),
        Err(ManifestError::Invalid(_))
    ));
}

#[test]
fn the_shipped_update_key_list_is_separate_and_empty_until_the_owner_adds_one() {
    let s = setup();
    let shipped = TrustedKeys::updates();
    if shipped.is_empty() {
        assert_eq!(
            Manifest::verify(&s.file, &shipped),
            Err(ManifestError::UnknownKey)
        );
    }
}

proptest! {
    #[test]
    fn no_single_byte_change_forges_a_manifest(index in 0usize..2000, replacement in any::<u8>()) {
        let s = setup();
        let mut bytes = s.file.clone().into_bytes();
        let i = index % bytes.len();
        prop_assume!(bytes[i] != replacement);
        bytes[i] = replacement;
        if let Ok(text) = String::from_utf8(bytes) {
            match Manifest::verify(&text, &s.keys) {
                Err(_) => {}
                Ok(found) => prop_assert_eq!(found, manifest()),
            }
        }
    }

    #[test]
    fn arbitrary_input_never_panics(text in ".{0,400}") {
        let _ = Manifest::verify(&text, &TrustedKeys::none());
    }
}
