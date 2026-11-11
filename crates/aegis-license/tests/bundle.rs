//! Offline bundle folders, end to end with real Ed25519 keys and files.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use aegis_license::bundle::{
    BUNDLE_MANIFEST, MAX_SUMS_BYTES, SIG_FILE, SUMS_FILE, SumsEntry, parse_sums, public_key_pem,
    render_sums, sums_for_dir, verify_bundle_dir,
};
use aegis_license::{Artifact, BundleError, Issuer, Manifest, ManifestError, TrustedKeys};
use base64::Engine;
use proptest::prelude::*;
use ring::digest::{SHA256, digest};

const INSTALLER: &str = "installers/aegis-0.4.0-linux-x86_64.tar.gz";
const INSTALLER_BYTES: &[u8] = b"pretend this is an installer archive";

static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// A fresh folder, removed when dropped.
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let path =
            std::env::temp_dir().join(format!("aegis-bundle-test-{}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("temp dir");
        TempDir(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn manifest(kid: &str) -> Manifest {
    Manifest {
        version: 1,
        kid: kid.into(),
        release: "0.4.0".into(),
        published_at: 1_800_000_000,
        artifacts: vec![Artifact {
            platform: "linux-x86_64".into(),
            url: "https://downloads.example.com/aegis-0.4.0-linux-x86_64.tar.gz".into(),
            sha256: hex(digest(&SHA256, INSTALLER_BYTES).as_ref()),
            bytes: INSTALLER_BYTES.len() as u64,
        }],
        notes: "What is new.".into(),
    }
}

struct Bundle {
    dir: TempDir,
    issuer: Issuer,
    keys: TrustedKeys,
}

impl Bundle {
    fn path(&self) -> &Path {
        &self.dir.0
    }

    /// Rewrites `SHA256SUMS` from the folder and signs it again.
    fn reseal(&self) {
        let sums = sums_for_dir(self.path()).expect("sums");
        fs::write(self.path().join(SUMS_FILE), &sums).expect("write sums");
        let sig = self.issuer.sign_bundle_sums(&sums).expect("signs");
        fs::write(self.path().join(SIG_FILE), sig).expect("write sig");
    }

    fn verify(&self) -> Result<aegis_license::bundle::BundleReport, BundleError> {
        verify_bundle_dir(self.path(), &self.keys)
    }
}

fn bundle() -> Bundle {
    let (pkcs8, public) = Issuer::generate().expect("key");
    let issuer = Issuer::from_pkcs8(&pkcs8).expect("loads");
    let keys = TrustedKeys::single("upd", public).expect("keys");
    let dir = TempDir::new();
    fs::create_dir(dir.0.join("installers")).expect("mkdir");
    fs::write(dir.0.join(INSTALLER), INSTALLER_BYTES).expect("installer");
    fs::write(dir.0.join("images.tar.gz"), b"pretend images").expect("images");
    let update = issuer.sign_manifest(&manifest("upd")).expect("manifest");
    fs::write(dir.0.join(BUNDLE_MANIFEST), update).expect("update");
    let b = Bundle { dir, issuer, keys };
    b.reseal();
    b
}

#[test]
fn a_valid_bundle_verifies() {
    let b = bundle();
    let report = b.verify().expect("verifies");
    assert_eq!(report.key_id, "upd");
    assert_eq!(report.release.to_string(), "0.4.0");
    assert_eq!(report.files, 3);
}

#[test]
fn a_changed_installer_byte_is_a_mismatch() {
    let b = bundle();
    let mut bytes = INSTALLER_BYTES.to_vec();
    bytes[0] ^= 1;
    fs::write(b.path().join(INSTALLER), bytes).expect("write");
    assert_eq!(b.verify(), Err(BundleError::Mismatch(INSTALLER.into())));
}

#[test]
fn a_deleted_file_is_missing_and_an_extra_file_is_unlisted() {
    let b = bundle();
    fs::write(b.path().join("notes.txt"), b"hello").expect("write");
    assert_eq!(b.verify(), Err(BundleError::Unlisted("notes.txt".into())));
    fs::remove_file(b.path().join("notes.txt")).expect("rm");
    fs::remove_file(b.path().join("images.tar.gz")).expect("rm");
    assert_eq!(
        b.verify(),
        Err(BundleError::Missing("images.tar.gz".into()))
    );
}

#[cfg(unix)]
#[test]
fn a_symlink_is_not_a_file() {
    let b = bundle();
    std::os::unix::fs::symlink(b.path().join("images.tar.gz"), b.path().join("link"))
        .expect("symlink");
    assert_eq!(b.verify(), Err(BundleError::NotAFile("link".into())));
}

#[test]
fn a_flipped_signature_bit_and_another_key_are_refused() {
    let b = bundle();
    let (_, other) = Issuer::generate().expect("key");
    let other = TrustedKeys::single("upd", other).expect("keys");
    assert_eq!(
        verify_bundle_dir(b.path(), &other),
        Err(BundleError::BadSignature)
    );

    let sig_path = b.path().join(SIG_FILE);
    let mut sig = fs::read(&sig_path).expect("read");
    sig[10] ^= 0x04;
    fs::write(&sig_path, sig).expect("write");
    assert_eq!(b.verify(), Err(BundleError::BadSignature));
}

#[test]
fn an_installer_not_in_the_manifest_is_refused() {
    let b = bundle();
    let other = "installers/other.tar.gz";
    fs::write(b.path().join(other), b"something else").expect("write");
    b.reseal();
    assert_eq!(b.verify(), Err(BundleError::NotInManifest(other.into())));
}

#[test]
fn a_manifest_by_an_untrusted_key_is_refused() {
    let b = bundle();
    let (pkcs8, _) = Issuer::generate().expect("key");
    let stranger = Issuer::from_pkcs8(&pkcs8).expect("loads");
    let update = stranger
        .sign_manifest(&manifest("stranger"))
        .expect("signs");
    fs::write(b.path().join(BUNDLE_MANIFEST), update).expect("write");
    b.reseal();
    assert_eq!(
        b.verify(),
        Err(BundleError::Manifest(ManifestError::UnknownKey))
    );
}

#[test]
fn an_oversized_sums_file_is_too_large() {
    let b = bundle();
    fs::write(b.path().join(SUMS_FILE), vec![b'a'; MAX_SUMS_BYTES + 1]).expect("write");
    assert_eq!(b.verify(), Err(BundleError::TooLarge));
}

#[test]
fn the_issuer_signs_only_valid_sums() {
    let b = bundle();
    let h = "a".repeat(64);
    let refused = [
        "aegis-update-v1\0{\"version\":1}".to_owned(),
        "aegis-license-v1\0{\"version\":1}".to_owned(),
        String::new(),
        format!("{h}  a\r\n"),
        format!("{h}  b\n{h}  a\n"),
        format!("{h}  ../x\n"),
        format!("{h}  .hidden\n"),
        format!("{h}  a/b/c\n"),
    ];
    for text in &refused {
        assert!(
            matches!(
                b.issuer.sign_bundle_sums(text),
                Err(BundleError::Malformed(_))
            ),
            "signed {text:?}"
        );
    }
    assert!(b.issuer.sign_bundle_sums(&format!("{h}  a/b\n")).is_ok());
}

#[test]
fn the_public_key_pem_is_spki() {
    let (_, public) = Issuer::generate().expect("key");
    let keys = TrustedKeys::single("upd", public).expect("keys");
    let pem = public_key_pem(&keys, "upd").expect("pem");
    let body = pem
        .strip_prefix("-----BEGIN PUBLIC KEY-----\n")
        .and_then(|r| r.strip_suffix("\n-----END PUBLIC KEY-----\n"))
        .expect("markers");
    let der = base64::engine::general_purpose::STANDARD
        .decode(body)
        .expect("base64");
    let mut want = vec![
        0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00,
    ];
    want.extend_from_slice(&public);
    assert_eq!(der, want);
    assert_eq!(public_key_pem(&keys, "other"), None);
}

fn path_strategy() -> impl Strategy<Value = String> {
    "[a-z0-9][a-z0-9._-]{0,20}(/[a-z0-9][a-zA-Z0-9._-]{0,20})?"
}

proptest! {
    #[test]
    fn rendered_sums_parse_back(
        map in proptest::collection::btree_map(path_strategy(), "[0-9a-f]{64}", 1..=20)
    ) {
        let entries: Vec<SumsEntry> = map
            .into_iter()
            .map(|(path, sha256)| SumsEntry { path, sha256 })
            .collect();
        prop_assert_eq!(parse_sums(&render_sums(&entries)), Ok(entries));
    }

    #[test]
    fn parse_sums_never_panics_on_text(text in ".{0,600}") {
        let _ = parse_sums(&text);
    }

    #[test]
    fn parse_sums_never_panics_on_bytes(bytes in proptest::collection::vec(any::<u8>(), 0..600)) {
        let _ = parse_sums(&String::from_utf8_lossy(&bytes));
    }
}
