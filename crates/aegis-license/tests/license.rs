//! Signing and verifying, end to end, with real Ed25519 keys.

use aegis_license::{
    FILE_PREFIX, Issuer, License, LicenseError, LicenseState, MAX_FILE_BYTES, TrustedKeys,
};
use proptest::prelude::*;

fn base64url(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn record() -> License {
    License {
        version: 1,
        id: "lic-2026-001".into(),
        kid: "test".into(),
        customer: "Example Motors GmbH".into(),
        seats: 25,
        issued_at: 1_800_000_000,
        expires_at: 1_800_000_000 + 365 * 86_400,
        grace_days: 30,
        features: vec!["audit".into(), "export".into()],
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
    let keys = TrustedKeys::single("test", public).expect("keys");
    let file = issuer.sign(&record()).expect("signs");
    Setup { issuer, keys, file }
}

#[test]
fn a_signed_license_verifies_and_round_trips() {
    let s = setup();
    assert!(s.file.starts_with(FILE_PREFIX));
    assert!(s.file.is_ascii() && !s.file.contains('\n'));
    let license = License::verify(&s.file, &s.keys).expect("verifies");
    assert_eq!(license, record());
    // A trailing newline from a text editor is fine.
    License::verify(&format!("{}\n", s.file), &s.keys).expect("trims");
}

#[test]
fn the_wrong_key_or_an_unknown_key_is_refused() {
    let s = setup();
    let (_, other_public) = Issuer::generate().expect("key");
    let wrong = TrustedKeys::single("test", other_public).expect("keys");
    assert_eq!(
        License::verify(&s.file, &wrong),
        Err(LicenseError::BadSignature)
    );
    let renamed = TrustedKeys::single("other", other_public).expect("keys");
    assert_eq!(
        License::verify(&s.file, &renamed),
        Err(LicenseError::UnknownKey)
    );
    assert_eq!(
        License::verify(&s.file, &TrustedKeys::none()),
        Err(LicenseError::UnknownKey)
    );
}

#[test]
fn a_changed_payload_is_refused_even_if_it_stays_valid_json() {
    let s = setup();
    let mut richer = record();
    richer.seats = 5000;
    let forged = serde_json::to_vec(&richer).expect("json");
    let parts: Vec<&str> = s.file.split('.').collect();
    let tampered = format!("{}.{}.{}", parts[0], base64url(&forged), parts[2]);
    assert_eq!(
        License::verify(&tampered, &s.keys),
        Err(LicenseError::BadSignature)
    );
}

#[test]
fn a_signature_made_for_another_purpose_is_not_a_license() {
    // The same key signs the raw payload without the domain prefix.
    let (pkcs8, public) = Issuer::generate().expect("key");
    let pair = ring::signature::Ed25519KeyPair::from_pkcs8(&pkcs8).expect("pair");
    let payload = serde_json::to_vec(&record()).expect("json");
    let raw_signature = pair.sign(&payload);
    let file = format!(
        "{FILE_PREFIX}.{}.{}",
        base64url(&payload),
        base64url(raw_signature.as_ref())
    );
    let keys = TrustedKeys::single("test", public).expect("keys");
    assert_eq!(
        License::verify(&file, &keys),
        Err(LicenseError::BadSignature)
    );
}

#[test]
fn malformed_files_are_refused_without_panicking() {
    let s = setup();
    for bad in [
        "",
        "AEGIS-LICENSE-1",
        "AEGIS-LICENSE-1..",
        "AEGIS-LICENSE-2.AAAA.AAAA",
        "AEGIS-LICENSE-1.AAAA.AAAA.AAAA",
        "AEGIS-LICENSE-1.!!!.!!!",
        &s.file.replace('.', ","),
    ] {
        assert!(License::verify(bad, &s.keys).is_err(), "{bad:?}");
    }
    let huge = "A".repeat(MAX_FILE_BYTES + 1);
    assert_eq!(License::verify(&huge, &s.keys), Err(LicenseError::TooLarge));
}

#[test]
fn unknown_fields_and_other_versions_are_refused() {
    let s = setup();
    let mut value = serde_json::to_value(record()).expect("value");
    value["admin"] = serde_json::json!(true);
    let payload = serde_json::to_vec(&value).expect("json");
    let parts: Vec<&str> = s.file.split('.').collect();
    let file = format!("{}.{}.{}", parts[0], base64url(&payload), parts[2]);
    assert_eq!(
        License::verify(&file, &s.keys),
        Err(LicenseError::Malformed)
    );

    let mut v2 = record();
    v2.version = 2;
    assert_eq!(s.issuer.sign(&v2), Err(LicenseError::UnsupportedVersion(2)));
}

#[test]
fn the_issuer_will_not_sign_a_record_the_verifier_would_refuse() {
    let s = setup();
    let mut bad = record();
    bad.seats = 0;
    assert!(matches!(s.issuer.sign(&bad), Err(LicenseError::Invalid(_))));
}

#[test]
fn the_state_follows_the_clock() {
    let s = setup();
    let license = License::verify(&s.file, &s.keys).expect("verifies");
    assert!(matches!(
        license.state(license.issued_at + 1),
        LicenseState::Active { .. }
    ));
    assert!(matches!(
        license.state(license.expires_at + 1),
        LicenseState::Grace { .. }
    ));
    assert_eq!(
        license.state(license.expires_at + 31 * 86_400),
        LicenseState::Expired
    );
    assert!(license.state(license.issued_at).allows_editing());
    assert!(!license.state(u64::MAX).allows_editing());
}

#[test]
fn the_build_with_no_vendor_key_accepts_no_license() {
    // `keys/vendor.keys` holds no key until the owner adds one.
    let s = setup();
    let vendor = TrustedKeys::vendor();
    if vendor.is_empty() {
        assert_eq!(
            License::verify(&s.file, &vendor),
            Err(LicenseError::UnknownKey)
        );
    }
}

proptest! {
    /// Changing any one byte of a license file never gives a license that
    /// differs from the signed one: it is either refused or identical.
    #[test]
    fn no_single_byte_change_forges_a_license(index in 0usize..2000, replacement in any::<u8>()) {
        let s = setup();
        let mut bytes = s.file.clone().into_bytes();
        let i = index % bytes.len();
        prop_assume!(bytes[i] != replacement);
        bytes[i] = replacement;
        if let Ok(text) = String::from_utf8(bytes) {
            match License::verify(&text, &s.keys) {
                Err(_) => {}
                Ok(license) => prop_assert_eq!(license, record()),
            }
        }
    }

    /// Arbitrary text never panics the parser.
    #[test]
    fn arbitrary_input_never_panics(text in ".{0,400}") {
        let keys = TrustedKeys::none();
        let _ = License::verify(&text, &keys);
    }
}
