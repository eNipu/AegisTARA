//! The bundle subcommands of the `aegis-license` binary.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

use aegis_license::{Artifact, Issuer};
use base64::Engine;
use ring::digest::{SHA256, digest};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// A fresh folder, removed when dropped.
struct Site(PathBuf);

impl Drop for Site {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

impl Site {
    fn path(&self, rel: &str) -> PathBuf {
        self.0.join(rel)
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_aegis-license"))
            .args(args)
            .output()
            .expect("run aegis-license")
    }

    fn run_in(&self, args: &[&Path]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_aegis-license"))
            .args(args)
            .output()
            .expect("run aegis-license")
    }
}

/// A bundle folder with an installer and a signed manifest, not yet signed.
fn site() -> Site {
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    let root = std::env::temp_dir().join(format!("aegis-bundle-cli-{}-{n}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let site = Site(root);
    fs::create_dir_all(site.path("bundle/installers")).expect("dirs");

    let (pkcs8, public) = Issuer::generate().expect("key");
    fs::write(site.path("key.pk8"), &pkcs8).expect("key file");
    let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(public);
    fs::write(site.path("keys"), format!("upd {encoded}\n")).expect("keys file");

    let installer = b"pretend this is an installer archive";
    fs::write(site.path("bundle/installers/aegis-0.4.0.tar.gz"), installer).expect("installer");
    let sha256: String = digest(&SHA256, installer)
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let manifest = aegis_license::Manifest {
        version: 1,
        kid: "upd".into(),
        release: "0.4.0".into(),
        published_at: 1_800_000_000,
        artifacts: vec![Artifact {
            platform: "linux-x86_64".into(),
            url: "https://downloads.example.com/aegis-0.4.0.tar.gz".into(),
            sha256,
            bytes: installer.len() as u64,
        }],
        notes: "Test.".into(),
    };
    let issuer = Issuer::from_pkcs8(&pkcs8).expect("issuer");
    let signed = issuer.sign_manifest(&manifest).expect("sign manifest");
    fs::write(site.path("bundle/update.aegis"), signed).expect("manifest file");
    site
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn sign(site: &Site) -> Output {
    site.run_in(&[
        Path::new("sign-bundle"),
        &site.path("bundle"),
        Path::new("--key"),
        &site.path("key.pk8"),
    ])
}

fn verify(site: &Site) -> Output {
    site.run_in(&[
        Path::new("verify-bundle"),
        &site.path("bundle"),
        Path::new("--keys"),
        &site.path("keys"),
    ])
}

#[test]
fn signed_bundle_verifies() {
    let site = site();
    let signed = sign(&site);
    assert!(signed.status.success(), "{}", text(&signed.stderr));
    assert!(text(&signed.stdout).contains("signed 2 files"));
    assert!(site.path("bundle/SHA256SUMS").is_file());
    assert_eq!(
        fs::read(site.path("bundle/SHA256SUMS.sig"))
            .expect("sig")
            .len(),
        64
    );
    let checked = verify(&site);
    assert!(checked.status.success(), "{}", text(&checked.stderr));
    let out = text(&checked.stdout);
    assert!(out.contains("bundle ok"));
    assert!(out.contains("release:   0.4.0"));
}

#[test]
fn changed_installer_is_refused() {
    let site = site();
    assert!(sign(&site).status.success());
    let file = site.path("bundle/installers/aegis-0.4.0.tar.gz");
    let mut bytes = fs::read(&file).expect("read");
    bytes[0] ^= 1;
    fs::write(&file, bytes).expect("write");
    let checked = verify(&site);
    assert!(!checked.status.success());
    assert!(text(&checked.stderr).starts_with("error:"));
}

#[test]
fn public_pem_prints_a_key() {
    let site = site();
    let keys = site.path("keys");
    let keys = keys.to_str().expect("utf-8 path");
    let pem = site.run(&["public-pem", "--keys", keys, "--key-id", "upd"]);
    assert!(pem.status.success());
    assert!(text(&pem.stdout).starts_with("-----BEGIN PUBLIC KEY-----"));
    let unknown = site.run(&["public-pem", "--keys", keys, "--key-id", "nope"]);
    assert!(!unknown.status.success());
    assert!(text(&unknown.stderr).contains("no key with that ID"));
}
