//! Downloading the installer (job 054): a real TLS server on loopback with
//! the test certificate authority, the real `aegis-auth` client and a real
//! signed manifest. The downloaded file is checked against the signed size
//! and SHA-256 as it arrives.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use aegis_auth::http::HyperClient;
use aegis_auth::tls::config_with_roots;
use aegis_client::updates::{DownloadDir, UpdateChecker, UpdateError, UpdateOffer, UpdateStatus};
use aegis_license::{Artifact, Issuer, Manifest, TrustedKeys, Version};
use rustls_pki_types::pem::PemObject;
use rustls_pki_types::{CertificateDer, PrivateKeyDer};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;

const CA: &[u8] = include_bytes!("fixtures/tls/ca.pem");
const CERT: &[u8] = include_bytes!("fixtures/tls/server.pem");
const KEY: &[u8] = include_bytes!("fixtures/tls/server-pkcs8.pem");

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

/// What the server does with every request.
#[derive(Clone)]
struct Reply {
    status: &'static str,
    body: Vec<u8>,
}

/// Starts a TLS server and returns its base address (`https://127.0.0.1:N`).
async fn serve(reply: Reply) -> String {
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
    let base = format!("https://{}", listener.local_addr().expect("addr"));
    tokio::spawn(async move {
        loop {
            let Ok((tcp, _)) = listener.accept().await else {
                return;
            };
            let acceptor = acceptor.clone();
            let reply = reply.clone();
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
                    "HTTP/1.1 {}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                    reply.status,
                    reply.body.len()
                );
                let _ = tls.write_all(head.as_bytes()).await;
                let _ = tls.write_all(&reply.body).await;
                let _ = tls.shutdown().await;
            });
        }
    });
    base
}

fn trusting_client() -> HyperClient {
    let mut roots = rustls::RootCertStore::empty();
    for cert in CertificateDer::pem_slice_iter(CA) {
        roots.add(cert.expect("ca")).expect("root");
    }
    HyperClient::with_tls_config(&config_with_roots(roots).expect("config"))
}

/// A signed manifest for `installer` served from `base`, and the offer it
/// gives to a client on release 0.4.0.
fn offer_for(base: &str, installer: &[u8]) -> UpdateOffer {
    let (pkcs8, public) = Issuer::generate().expect("key");
    let issuer = Issuer::from_pkcs8(&pkcs8).expect("issuer");
    let manifest = Manifest {
        version: 1,
        kid: "upd".into(),
        release: "0.5.0".into(),
        published_at: 1_800_000_000,
        artifacts: vec![Artifact {
            platform: "linux-x86_64".into(),
            url: format!("{base}/aegis.AppImage"),
            sha256: sha256_hex(installer),
            bytes: installer.len() as u64,
        }],
        notes: "Faster.".into(),
    };
    let file = issuer.sign_manifest(&manifest).expect("signs");
    let keys = TrustedKeys::single("upd", public).expect("keys");
    let checker = UpdateChecker::new(keys, Version::parse("0.4.0").expect("v"), "linux-x86_64");
    match checker.check(&file).expect("checks") {
        UpdateStatus::Available(offer) => offer,
        other => panic!("expected an update, got {other:?}"),
    }
}

fn temp_dir() -> PathBuf {
    static N: AtomicU32 = AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!(
        "aegis-update-test-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn files(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(Result::ok)
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

fn installer(len: usize) -> Vec<u8> {
    (0..len).map(|i| (i % 251) as u8).collect()
}

#[tokio::test]
async fn a_good_installer_is_saved_checked_and_named_from_the_signed_fields() {
    // 3 MiB: more than the 1 MiB an ordinary request may read.
    let data = installer(3 * 1024 * 1024);
    let base = serve(Reply {
        status: "200 OK",
        body: data.clone(),
    })
    .await;
    let offer = offer_for(&base, &data);
    let dir = temp_dir().join("made/on/demand");
    let mut last = 0u64;
    let mut calls = 0u32;
    let path = offer
        .download(&trusting_client(), &DownloadDir::Folder(dir.clone()), |n| {
            assert!(n > last, "progress only grows");
            last = n;
            calls += 1;
        })
        .await
        .expect("downloads");
    assert_eq!(path, dir.join("AegisTARA-0.5.0-linux-x86_64.appimage"));
    assert_eq!(std::fs::read(&path).expect("read"), data);
    assert_eq!(last, data.len() as u64);
    assert!(calls >= 1);
    assert_eq!(
        files(&dir),
        ["AegisTARA-0.5.0-linux-x86_64.appimage"],
        "no .part left"
    );
    let _ = std::fs::remove_dir_all(temp_dir());
}

#[tokio::test]
async fn a_file_with_the_right_size_and_wrong_bytes_is_deleted() {
    let data = installer(10_000);
    let mut evil = data.clone();
    evil[5000] ^= 1;
    let base = serve(Reply {
        status: "200 OK",
        body: evil,
    })
    .await;
    let offer = offer_for(&base, &data);
    let dir = temp_dir();
    let err = offer
        .download(
            &trusting_client(),
            &DownloadDir::Folder(dir.clone()),
            |_| {},
        )
        .await
        .expect_err("refused");
    assert!(matches!(err, UpdateError::DownloadMismatch), "{err:?}");
    assert!(files(&dir).is_empty(), "nothing is left: {:?}", files(&dir));
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn too_many_bytes_and_too_few_are_both_refused_and_removed() {
    let data = installer(10_000);
    for body in [installer(10_001), installer(9_999), Vec::new()] {
        let base = serve(Reply {
            status: "200 OK",
            body,
        })
        .await;
        let offer = offer_for(&base, &data);
        let dir = temp_dir();
        let err = offer
            .download(
                &trusting_client(),
                &DownloadDir::Folder(dir.clone()),
                |_| {},
            )
            .await
            .expect_err("refused");
        assert!(matches!(err, UpdateError::DownloadMismatch), "{err:?}");
        assert!(files(&dir).is_empty(), "{:?}", files(&dir));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[tokio::test]
async fn a_failed_download_leaves_an_earlier_good_file_alone() {
    let data = installer(2_000);
    let good = serve(Reply {
        status: "200 OK",
        body: data.clone(),
    })
    .await;
    let dir = temp_dir();
    let first = offer_for(&good, &data)
        .download(
            &trusting_client(),
            &DownloadDir::Folder(dir.clone()),
            |_| {},
        )
        .await
        .expect("first");
    let bad = serve(Reply {
        status: "200 OK",
        body: installer(2_000).into_iter().rev().collect(),
    })
    .await;
    let err = offer_for(&bad, &data)
        .download(
            &trusting_client(),
            &DownloadDir::Folder(dir.clone()),
            |_| {},
        )
        .await
        .expect_err("refused");
    assert!(matches!(err, UpdateError::DownloadMismatch));
    assert_eq!(std::fs::read(&first).expect("still there"), data);
    assert_eq!(files(&dir).len(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn a_server_error_is_a_download_error_and_leaves_nothing() {
    let base = serve(Reply {
        status: "404 Not Found",
        body: b"gone".to_vec(),
    })
    .await;
    let offer = offer_for(&base, &installer(100));
    let dir = temp_dir();
    let err = offer
        .download(
            &trusting_client(),
            &DownloadDir::Folder(dir.clone()),
            |_| {},
        )
        .await
        .expect_err("fails");
    assert!(
        matches!(&err, UpdateError::Download(m) if m.contains("404")),
        "{err:?}"
    );
    assert!(files(&dir).is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn a_server_the_computer_does_not_trust_is_not_downloaded_from() {
    let data = installer(100);
    let base = serve(Reply {
        status: "200 OK",
        body: data.clone(),
    })
    .await;
    let offer = offer_for(&base, &data);
    let dir = temp_dir();
    // The default client trusts the operating system's roots, not the test CA.
    let err = offer
        .download(
            &HyperClient::new(),
            &DownloadDir::Folder(dir.clone()),
            |_| {},
        )
        .await
        .expect_err("untrusted certificate");
    assert!(matches!(err, UpdateError::Download(_)), "{err:?}");
    assert!(files(&dir).is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn an_unwritable_folder_is_a_disk_error() {
    let data = installer(100);
    let base = serve(Reply {
        status: "200 OK",
        body: data.clone(),
    })
    .await;
    let offer = offer_for(&base, &data);
    // A file where the folder should be.
    let blocker = temp_dir();
    std::fs::write(&blocker, b"x").expect("file");
    let err = offer
        .download(
            &trusting_client(),
            &DownloadDir::Folder(blocker.join("sub")),
            |_| {},
        )
        .await
        .expect_err("cannot create");
    assert!(matches!(err, UpdateError::Disk(_)), "{err:?}");
    let _ = std::fs::remove_file(&blocker);
}

// ---- through the session (the Help menu's commands) ----

/// Waits for a notice that satisfies `want`, up to ten seconds.
async fn notice_with(
    session: &aegis_client::session::Session,
    want: impl Fn(&aegis_client::state::Notice) -> bool,
) -> aegis_client::state::Notice {
    let mut rx = session.subscribe();
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            if let Some(n) = rx.borrow_and_update().notices.iter().find(|n| want(n)) {
                return n.clone();
            }
            rx.changed().await.expect("the session ended");
        }
    })
    .await
    .expect("no such notice in ten seconds")
}

#[tokio::test]
async fn the_help_menu_checks_then_downloads_and_verifies() {
    use aegis_client::commands::Command;
    use aegis_client::connection::DevLogin;
    use aegis_client::session::{Session, SessionConfig};

    let data = installer(50_000);
    let base = serve(Reply {
        status: "200 OK",
        body: data.clone(),
    })
    .await;
    // The manifest comes from a second TLS server.
    let (pkcs8, public) = Issuer::generate().expect("key");
    let issuer = Issuer::from_pkcs8(&pkcs8).expect("issuer");
    let platform = aegis_client::updates::platform_name();
    let manifest = Manifest {
        version: 1,
        kid: "upd".into(),
        release: "9.1.0".into(),
        published_at: 1_800_000_000,
        artifacts: vec![Artifact {
            platform: platform.clone(),
            url: format!("{base}/setup.deb"),
            sha256: sha256_hex(&data),
            bytes: data.len() as u64,
        }],
        notes: String::new(),
    };
    let file = issuer.sign_manifest(&manifest).expect("signs");
    let manifest_url = serve(Reply {
        status: "200 OK",
        body: file.into_bytes(),
    })
    .await;

    let dir = temp_dir();
    let mut config = SessionConfig::new("alice");
    config.update_url = format!("{manifest_url}/m");
    config.update_dir = DownloadDir::Folder(dir.clone());
    config.update_http = Arc::new(trusting_client());
    config.update_checker = Arc::new(UpdateChecker::new(
        TrustedKeys::single("upd", public).expect("keys"),
        Version::parse("0.1.0").expect("v"),
        platform,
    ));
    // No server is needed for updates: the address is not listening.
    let (session, task) = Session::start(
        "http://127.0.0.1:9",
        Arc::new(DevLogin::new("alice", false)),
        config,
    );

    // Downloading before checking says what to do first.
    session.send(Command::DownloadUpdate).expect("sent");
    notice_with(&session, |n| n.text.contains("Check for updates first")).await;

    session.send(Command::CheckForUpdates).expect("sent");
    notice_with(&session, |n| n.text.contains("9.1.0 is available")).await;
    session.send(Command::DownloadUpdate).expect("sent");
    let done = notice_with(&session, |n| n.text.contains("downloaded and checked")).await;
    let expected =
        dir.join("AegisTARA-9.1.0-".to_owned() + &aegis_client::updates::platform_name() + ".deb");
    assert!(
        done.text.contains(&expected.display().to_string()),
        "{}",
        done.text
    );
    assert_eq!(std::fs::read(&expected).expect("file"), data);
    assert_eq!(files(&dir).len(), 1);

    let _ = session.send(Command::Shutdown);
    let _ = task.await;
    let _ = std::fs::remove_dir_all(&dir);
}

/// Stress: a big installer and several downloads at once.
/// `AEGIS_STRESS_DOWNLOAD_MB` sets the size (default 16; a local release run
/// uses 300). Every file must be complete and checked, and no `.part` may
/// stay behind.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn stress_a_big_installer_and_parallel_downloads_all_verify() {
    let mb: usize = std::env::var("AEGIS_STRESS_DOWNLOAD_MB")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(16);
    let data = installer(mb * 1024 * 1024);
    let base = serve(Reply {
        status: "200 OK",
        body: data.clone(),
    })
    .await;
    let started = std::time::Instant::now();
    let mut tasks = Vec::new();
    for _ in 0..4 {
        let offer = offer_for(&base, &data);
        let dir = temp_dir();
        tasks.push(tokio::spawn(async move {
            let path = offer
                .download(
                    &trusting_client(),
                    &DownloadDir::Folder(dir.clone()),
                    |_| {},
                )
                .await
                .expect("downloads");
            (path, dir)
        }));
    }
    let mut total = 0u64;
    for t in tasks {
        let (path, dir) = t.await.expect("task");
        let meta = std::fs::metadata(&path).expect("file");
        assert_eq!(meta.len(), data.len() as u64);
        assert_eq!(files(&dir).len(), 1, "no .part left");
        total += meta.len();
        let _ = std::fs::remove_dir_all(&dir);
    }
    let secs = started.elapsed().as_secs_f64();
    eprintln!(
        "download stress: {} MB in {secs:.2} s ({:.0} MB/s)",
        total / (1024 * 1024),
        total as f64 / (1024.0 * 1024.0) / secs
    );
}

// ---- a folder other users can write to (ADR 0048, section 7) ----

#[cfg(unix)]
#[tokio::test]
async fn links_planted_in_the_download_folder_are_not_followed() {
    let data = installer(5_000);
    let base = serve(Reply {
        status: "200 OK",
        body: data.clone(),
    })
    .await;
    let offer = offer_for(&base, &data);
    let dir = temp_dir();
    std::fs::create_dir_all(&dir).expect("dir");
    let victim = dir.join("victim.txt");
    std::fs::write(&victim, b"precious").expect("victim");
    // Where an older version, or a guess, would have put the files.
    let name = "AegisTARA-0.5.0-linux-x86_64.appimage";
    std::os::unix::fs::symlink(&victim, dir.join(format!("{name}.part"))).expect("part link");
    std::os::unix::fs::symlink(&victim, dir.join(name)).expect("final link");

    let path = offer
        .download(
            &trusting_client(),
            &DownloadDir::Folder(dir.clone()),
            |_| {},
        )
        .await
        .expect("downloads");
    assert_eq!(std::fs::read(&victim).expect("victim"), b"precious");
    assert_eq!(std::fs::read(&path).expect("installer"), data);
    assert!(
        !std::fs::symlink_metadata(&path)
            .expect("meta")
            .file_type()
            .is_symlink(),
        "the final name is the checked file, not the link"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn a_private_folder_is_made_for_each_download_in_a_shared_base() {
    let data = installer(5_000);
    let base_url = serve(Reply {
        status: "200 OK",
        body: data.clone(),
    })
    .await;
    let offer = offer_for(&base_url, &data);
    // The shared folder, with things other users could have planted.
    let shared = temp_dir();
    std::fs::create_dir_all(&shared).expect("shared");
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink("/", shared.join("aegistara-update")).expect("link");
        std::fs::create_dir(shared.join("aegistara-update-planted")).expect("planted");
    }
    let target = DownloadDir::PrivateIn(shared.clone());

    let first = offer
        .download(&trusting_client(), &target, |_| {})
        .await
        .expect("first");
    let second = offer
        .download(&trusting_client(), &target, |_| {})
        .await
        .expect("second");
    for path in [&first, &second] {
        assert_eq!(std::fs::read(path).expect("installer"), data);
        let folder = path.parent().expect("folder");
        assert_ne!(folder, shared, "not the shared folder itself");
        assert_eq!(folder.parent().expect("base"), shared);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(folder)
                .expect("meta")
                .permissions()
                .mode();
            assert_eq!(mode & 0o077, 0, "only the owner may enter: {mode:o}");
        }
    }
    assert_ne!(first.parent(), second.parent(), "a new folder each time");
    let _ = std::fs::remove_dir_all(&shared);
}

#[tokio::test]
async fn a_failed_download_into_a_private_folder_leaves_no_folder_behind() {
    let data = installer(5_000);
    let mut evil = data.clone();
    evil[10] ^= 1;
    let base_url = serve(Reply {
        status: "200 OK",
        body: evil,
    })
    .await;
    let offer = offer_for(&base_url, &data);
    let shared = temp_dir();
    std::fs::create_dir_all(&shared).expect("shared");
    let err = offer
        .download(
            &trusting_client(),
            &DownloadDir::PrivateIn(shared.clone()),
            |_| {},
        )
        .await
        .expect_err("refused");
    assert!(matches!(err, UpdateError::DownloadMismatch), "{err:?}");
    assert!(
        files(&shared).is_empty(),
        "left behind: {:?}",
        files(&shared)
    );
    let _ = std::fs::remove_dir_all(&shared);
}

#[test]
fn the_default_target_is_the_downloads_folder_or_a_private_folder_in_temp() {
    match DownloadDir::for_this_user() {
        DownloadDir::Folder(dir) => assert!(dir.ends_with("Downloads"), "{dir:?}"),
        DownloadDir::PrivateIn(base) => assert_eq!(base, std::env::temp_dir()),
    }
}
