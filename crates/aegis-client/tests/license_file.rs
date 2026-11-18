//! The license file check (job 208): signature, dates, file size.

use std::path::PathBuf;

use aegis_client::i18n::{Catalog, Language};
use aegis_client::license_file::{MAX_LICENSE_BYTES, check_license, read_license};
use aegis_license::{Issuer, License, TrustedKeys};

/// A fixed "now" so the dates never depend on the test machine.
const NOW: u64 = 1_800_000_000;
const DAY: u64 = 86_400;

fn issuer() -> (Issuer, TrustedKeys) {
    let (pkcs8, public) = Issuer::generate().expect("key");
    let issuer = Issuer::from_pkcs8(&pkcs8).expect("issuer");
    let keys = TrustedKeys::single("test", public).expect("keys");
    (issuer, keys)
}

fn record(issued_at: u64, expires_at: u64, grace_days: u32) -> License {
    License {
        version: 1,
        id: "license-file-test".into(),
        kid: "test".into(),
        customer: "ACME".into(),
        seats: 1,
        issued_at,
        expires_at,
        grace_days,
        features: vec!["core".into()],
    }
}

fn en() -> Catalog {
    Catalog::load(Language::En)
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "aegis-license-file-{name}-{}-{}",
        std::process::id(),
        aegis_client::license_file::now_secs()
    ));
    std::fs::create_dir_all(&dir).expect("dir");
    dir
}

#[test]
fn an_active_license_allows_editing_and_counts_the_days() {
    let (issuer, keys) = issuer();
    let r = record(NOW - DAY, NOW + 30 * DAY + 3_600, 14);
    let expires_in = r.expires_at - NOW;
    let text = issuer.sign(&r).expect("signed");
    let local = check_license(&text, &keys, NOW, &en()).expect("checked");
    assert_eq!(local.customer, "ACME");
    assert!(local.editing);
    assert!(!local.grace);
    assert_eq!(local.days_left, Some(expires_in / 86_400));
}

#[test]
fn a_license_in_grace_still_allows_editing() {
    let (issuer, keys) = issuer();
    let r = record(NOW - 60 * DAY, NOW - DAY, 14);
    let text = issuer.sign(&r).expect("signed");
    let local = check_license(&text, &keys, NOW, &en()).expect("checked");
    assert!(local.editing);
    assert!(local.grace);
    let grace_end = r.expires_at + u64::from(r.grace_days) * DAY;
    assert_eq!(local.days_left, Some((grace_end - NOW) / 86_400));
}

#[test]
fn an_expired_license_is_read_only() {
    let (issuer, keys) = issuer();
    let text = issuer
        .sign(&record(NOW - 60 * DAY, NOW - 30 * DAY, 14))
        .expect("signed");
    let local = check_license(&text, &keys, NOW, &en()).expect("checked");
    assert!(!local.editing);
    assert!(!local.grace);
    assert_eq!(local.days_left, None);
}

#[test]
fn a_license_signed_by_another_key_is_refused() {
    let (issuer, _) = issuer();
    let (_, other_keys) = self::issuer();
    let text = issuer
        .sign(&record(NOW - DAY, NOW + 30 * DAY, 14))
        .expect("signed");
    let err = check_license(&text, &other_keys, NOW, &en()).expect_err("foreign key");
    assert!(err.starts_with("The license file cannot be used"), "{err}");
}

#[test]
fn a_license_from_the_future_is_not_valid_yet() {
    let (issuer, keys) = issuer();
    let text = issuer
        .sign(&record(NOW + 7 * DAY, NOW + 60 * DAY, 14))
        .expect("signed");
    let err = check_license(&text, &keys, NOW, &en()).expect_err("not yet valid");
    assert_eq!(err, en().tr("license-not-yet-valid"));
}

#[test]
fn empty_text_is_refused() {
    let (_, keys) = issuer();
    let err = check_license("", &keys, NOW, &en()).expect_err("empty");
    assert!(err.starts_with("The license file cannot be used"), "{err}");
}

#[test]
fn a_missing_file_names_its_path() {
    let (_, keys) = issuer();
    let path = scratch("missing").join("nowhere.aegis-license");
    let err = read_license(&path, &keys, NOW, &en()).expect_err("missing");
    assert!(err.contains(&path.display().to_string()), "{err}");
}

#[test]
fn a_file_over_the_cap_is_refused() {
    let (issuer, keys) = issuer();
    let dir = scratch("large");
    let path = dir.join("large.aegis-license");
    // A valid license padded past the cap: only the size check refuses it.
    let mut text = issuer
        .sign(&record(NOW - DAY, NOW + 30 * DAY, 14))
        .expect("signed");
    let len = usize::try_from(MAX_LICENSE_BYTES + 1).expect("fits");
    text.push_str(&"\n".repeat(len - text.len()));
    assert_eq!(text.len() as u64, MAX_LICENSE_BYTES + 1);
    std::fs::write(&path, &text).expect("write");
    let err = read_license(&path, &keys, NOW, &en()).expect_err("too large");
    assert!(err.contains("too large"), "{err}");
    let _ = std::fs::remove_dir_all(&dir);
}
