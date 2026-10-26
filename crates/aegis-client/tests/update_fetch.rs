//! Fetching the signed update manifest over HTTPS (job 048): a real TLS
//! server on loopback with a test certificate authority, the real
//! `aegis-auth` client, and real signed manifests.
//!
//! The certificates in `fixtures/tls` are test keys made with `openssl`
//! (valid for 100 years, only for 127.0.0.1). They protect nothing.

use std::sync::Arc;

use aegis_auth::http::HyperClient;
use aegis_auth::tls::config_with_roots;
use aegis_client::updates::{UpdateChecker, UpdateError, UpdateStatus, fetch_manifest};
use aegis_license::{Artifact, Issuer, Manifest, TrustedKeys, Version};
use rustls_pki_types::pem::PemObject;
use rustls_pki_types::{CertificateDer, PrivateKeyDer};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;

const CA: &[u8] = include_bytes!("fixtures/tls/ca.pem");
const CERT: &[u8] = include_bytes!("fixtures/tls/server.pem");
const KEY: &[u8] = include_bytes!("fixtures/tls/server-pkcs8.pem");

fn signed(release: &str) -> (String, TrustedKeys) {
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
            sha256: "0".repeat(64),
            bytes: 10,
        }],
        notes: "Faster.".into(),
    };
    (
        issuer.sign_manifest(&manifest).expect("signs"),
        TrustedKeys::single("upd", public).expect("keys"),
    )
}

/// Serves `status` and `body` to every request over TLS. Returns the URL.
async fn serve(status: &'static str, body: Vec<u8>) -> String {
    let certs: Vec<_> = CertificateDer::pem_slice_iter(CERT)
        .collect::<Result<_, _>>()
        .expect("cert");
    let key = PrivateKeyDer::from_pem_slice(KEY).expect("key");
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = rustls::ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .expect("versions")
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .expect("server config");
    let acceptor = TlsAcceptor::from(Arc::new(config));
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let url = format!(
        "https://{}/update.manifest",
        listener.local_addr().expect("addr")
    );
    tokio::spawn(async move {
        loop {
            let Ok((tcp, _)) = listener.accept().await else {
                return;
            };
            let acceptor = acceptor.clone();
            let body = body.clone();
            tokio::spawn(async move {
                let Ok(mut tls) = acceptor.accept(tcp).await else {
                    return;
                };
                let mut seen = Vec::new();
                let mut buf = [0u8; 1024];
                while !seen.windows(4).any(|w| w == b"\r\n\r\n") {
                    match tls.read(&mut buf).await {
                        Ok(0) | Err(_) => return,
                        Ok(n) => seen.extend_from_slice(&buf[..n]),
                    }
                }
                let head = format!(
                    "HTTP/1.1 {status}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                    body.len()
                );
                let _ = tls.write_all(head.as_bytes()).await;
                let _ = tls.write_all(&body).await;
                let _ = tls.shutdown().await;
            });
        }
    });
    url
}

/// A client that trusts the test authority only.
fn trusting_client() -> HyperClient {
    let mut roots = rustls::RootCertStore::empty();
    for cert in CertificateDer::pem_slice_iter(CA) {
        roots.add(cert.expect("ca")).expect("root");
    }
    HyperClient::with_tls_config(&config_with_roots(roots).expect("config"))
}

fn checker(keys: TrustedKeys) -> UpdateChecker {
    UpdateChecker::new(keys, Version::parse("0.4.0").expect("v"), "linux-x86_64")
}

#[tokio::test]
async fn a_signed_manifest_is_fetched_over_https_and_checked() {
    let (file, keys) = signed("0.5.0");
    let url = serve("200 OK", format!("{file}\n").into_bytes()).await;
    let status = checker(keys)
        .check_url(&trusting_client(), &url)
        .await
        .expect("checks");
    let UpdateStatus::Available(offer) = status else {
        panic!("an update is available");
    };
    assert_eq!(offer.release, "0.5.0");
}

#[tokio::test]
async fn a_server_the_client_does_not_trust_is_refused() {
    let (file, _) = signed("0.5.0");
    let url = serve("200 OK", file.into_bytes()).await;
    // The default client trusts the operating system's roots, not the test CA.
    let err = fetch_manifest(&HyperClient::new(), &url)
        .await
        .expect_err("untrusted certificate");
    assert!(matches!(err, UpdateError::Fetch(_)), "{err}");
}

#[tokio::test]
async fn a_manifest_from_a_trusted_server_still_needs_a_trusted_signature() {
    // Transport security is not the signature: a valid TLS server that serves
    // a manifest signed by some other key gets nowhere.
    let (file, _) = signed("0.9.0");
    let (_, other_keys) = signed("0.1.0");
    let url = serve("200 OK", file.into_bytes()).await;
    let err = checker(other_keys)
        .check_url(&trusting_client(), &url)
        .await
        .expect_err("wrong key");
    assert!(matches!(err, UpdateError::Manifest(_)), "{err}");
}

#[tokio::test]
async fn errors_and_garbage_are_named() {
    let url = serve("404 Not Found", b"nope".to_vec()).await;
    let err = fetch_manifest(&trusting_client(), &url)
        .await
        .expect_err("404");
    assert!(matches!(err, UpdateError::Status(404)), "{err}");

    let url = serve("200 OK", vec![0xff, 0xfe, 0x00]).await;
    let err = fetch_manifest(&trusting_client(), &url)
        .await
        .expect_err("binary");
    assert!(matches!(err, UpdateError::NotAManifest), "{err}");

    let url = serve("200 OK", vec![b'a'; 40 * 1024]).await;
    let err = fetch_manifest(&trusting_client(), &url)
        .await
        .expect_err("large");
    assert!(matches!(err, UpdateError::NotAManifest), "{err}");
}

#[tokio::test]
async fn cleartext_to_a_remote_host_is_never_tried() {
    let err = fetch_manifest(&HyperClient::new(), "http://updates.example.com/m")
        .await
        .expect_err("refused");
    assert!(matches!(err, UpdateError::NotHttps), "{err}");
    let err = fetch_manifest(&HyperClient::new(), "ftp://x/m")
        .await
        .expect_err("refused");
    assert!(matches!(err, UpdateError::NotHttps), "{err}");
}
