//! Update checks with a real signed manifest.

use aegis_client::updates::{UpdateChecker, UpdateStatus};
use aegis_license::{Artifact, Issuer, Manifest, ManifestError, TrustedKeys, Version};

fn sha256_hex(data: &[u8]) -> String {
    use std::fmt::Write;
    ring::digest::digest(&ring::digest::SHA256, data)
        .as_ref()
        .iter()
        .fold(String::new(), |mut out, b| {
            let _ = write!(out, "{b:02x}");
            out
        })
}

fn signed(release: &str, installer: &[u8]) -> (String, TrustedKeys) {
    let (pkcs8, public) = Issuer::generate().expect("key");
    let issuer = Issuer::from_pkcs8(&pkcs8).expect("issuer");
    let manifest = Manifest {
        version: 1,
        kid: "upd".into(),
        release: release.into(),
        published_at: 1_800_000_000,
        artifacts: vec![Artifact {
            platform: "linux-x86_64".into(),
            url: "https://downloads.example.com/aegis.tar.gz".into(),
            sha256: sha256_hex(installer),
            bytes: installer.len() as u64,
        }],
        notes: "Faster.".into(),
    };
    (
        issuer.sign_manifest(&manifest).expect("signs"),
        TrustedKeys::single("upd", public).expect("keys"),
    )
}

fn checker(keys: TrustedKeys, current: &str) -> UpdateChecker {
    UpdateChecker::new(
        keys,
        Version::parse(current).expect("version"),
        "linux-x86_64",
    )
}

#[test]
fn a_newer_signed_release_is_offered_and_its_download_is_verified() {
    let installer = b"the real installer bytes";
    let (file, keys) = signed("0.5.0", installer);
    let UpdateStatus::Available(offer) = checker(keys, "0.4.2").check(&file).expect("checks")
    else {
        panic!("an update is available");
    };
    assert_eq!(offer.release, "0.5.0");
    assert_eq!(offer.url(), "https://downloads.example.com/aegis.tar.gz");
    assert_eq!(offer.bytes(), installer.len() as u64);
    assert_eq!(offer.notes, "Faster.");
    assert!(offer.verify_download(installer).is_ok());
    assert_eq!(
        offer.verify_download(b"a different file of the same!"),
        Err(ManifestError::DownloadMismatch)
    );
}

#[test]
fn an_old_or_same_release_is_never_offered() {
    let (file, keys) = signed("0.5.0", b"x");
    let same = checker(keys.clone(), "0.5.0").check(&file).expect("checks");
    assert_eq!(same, UpdateStatus::UpToDate);
    let newer_installed = checker(keys, "1.0.0").check(&file).expect("checks");
    assert_eq!(
        newer_installed,
        UpdateStatus::UpToDate,
        "a replayed old manifest cannot downgrade"
    );
}

#[test]
fn another_platform_gets_a_clear_answer() {
    let (file, keys) = signed("0.5.0", b"x");
    let other = UpdateChecker::new(
        keys,
        Version::parse("0.4.0").expect("version"),
        "macos-aarch64",
    );
    assert_eq!(
        other.check(&file).expect("checks"),
        UpdateStatus::NoBuildForThisPlatform {
            release: "0.5.0".into()
        }
    );
}

#[test]
fn a_manifest_from_an_untrusted_signer_is_refused() {
    let (file, _) = signed("0.5.0", b"x");
    let (_, stranger) = Issuer::generate().expect("key");
    let keys = TrustedKeys::single("upd", stranger).expect("keys");
    assert_eq!(
        checker(keys, "0.4.0").check(&file),
        Err(ManifestError::BadSignature)
    );
}
