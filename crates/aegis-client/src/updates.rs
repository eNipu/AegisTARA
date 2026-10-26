//! Update checks against the signed manifest (job 036, ADR 0034).
//!
//! This module decides, from the text of a manifest, whether an update exists
//! for this computer and whether a downloaded installer is the one the vendor
//! signed. [`UpdateChecker::check`] does no network I/O, so a manifest can
//! also come from a file or a share, which suits air gapped sites.
//! [`fetch_manifest`] and [`UpdateChecker::check_url`] fetch it over HTTPS
//! with the `aegis-auth` client (rustls, the OS trust store, no redirects,
//! 10 s, 1 MiB; job 048, ADR 0042). [`UpdateOffer::download`] streams the
//! installer to a file and checks its signed size and SHA-256 before the file
//! gets its final name (job 054, ADR 0048).
//!
//! Safety rules, all enforced by `aegis-license`:
//!
//! - the manifest must be signed by a key compiled into this build's update
//!   key list (not the license key list);
//! - only a strictly newer release is offered, so an old manifest cannot
//!   downgrade the app;
//! - an installer is accepted only if its size and SHA-256 match.

use std::io::Write as _;
use std::path::{Path, PathBuf};

use aegis_auth::error::AuthError;
use aegis_auth::http::{HttpClient, HttpDownload, HttpRequest, is_loopback_url};
use aegis_license::MAX_MANIFEST_BYTES;
use aegis_license::{Artifact, Manifest, ManifestError, Offer, TrustedKeys, Version};

/// Why an update check failed.
#[derive(Debug, thiserror::Error)]
pub enum UpdateError {
    /// The address is not `https://` (a loopback `http://` address is allowed
    /// for tests and lab setups).
    #[error("the update address must start with https://")]
    NotHttps,
    /// The request failed: no network, bad certificate, time out.
    #[error("the update server could not be reached: {0}")]
    Fetch(String),
    /// The server answered with a status other than 200.
    #[error("the update server answered {0}")]
    Status(u16),
    /// The answer is not a manifest: too large or not text.
    #[error("the update server did not send a manifest")]
    NotAManifest,
    /// The manifest is not valid or not signed by a trusted update key.
    #[error("the update manifest was refused: {0}")]
    Manifest(#[from] ManifestError),
    /// The installer's size or checksum differs from the signed manifest.
    /// The file was deleted.
    #[error("the downloaded file does not match the signed size and checksum, and was deleted")]
    DownloadMismatch,
    /// The installer could not be fetched.
    #[error("the installer could not be downloaded: {0}")]
    Download(String),
    /// The installer could not be written to disk.
    #[error("the installer could not be saved: {0}")]
    Disk(String),
}

/// Fetches the manifest text from `url`. The text is not trusted yet: pass it
/// to [`UpdateChecker::check`]. One request, no redirects.
///
/// # Errors
///
/// [`UpdateError`] for a non-HTTPS address, a failed request, a status other
/// than 200, or a body that is too large or not text.
pub async fn fetch_manifest<H: HttpClient>(http: &H, url: &str) -> Result<String, UpdateError> {
    if !url.starts_with("https://") && !(url.starts_with("http://") && is_loopback_url(url)) {
        return Err(UpdateError::NotHttps);
    }
    let request = HttpRequest {
        method: "GET",
        url: url.to_owned(),
        headers: vec![("accept", "text/plain".to_owned())],
        body: Vec::new(),
    };
    let response = http
        .send(request)
        .await
        .map_err(|e| UpdateError::Fetch(e.to_string()))?;
    if response.status != 200 {
        return Err(UpdateError::Status(response.status));
    }
    if response.body.len() > MAX_MANIFEST_BYTES {
        return Err(UpdateError::NotAManifest);
    }
    String::from_utf8(response.body)
        .map(|t| t.trim().to_owned())
        .map_err(|_| UpdateError::NotAManifest)
}

/// A newer release for this computer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateOffer {
    /// The release number, for example `0.4.0`.
    pub release: String,
    /// What is new, plain text.
    pub notes: String,
    artifact: Artifact,
}

impl UpdateOffer {
    /// Where to download the installer.
    #[must_use]
    pub fn url(&self) -> &str {
        &self.artifact.url
    }

    /// The installer's size in bytes.
    #[must_use]
    pub fn bytes(&self) -> u64 {
        self.artifact.bytes
    }

    /// Downloads the installer into `target` and checks it.
    ///
    /// The bytes go to a new `<name>.<unique>.part` file while they arrive,
    /// are counted and hashed on the way (never held in memory), and are
    /// refused as soon as they pass the signed size. Only when the size and
    /// the SHA-256 both match is the file renamed to its final name, so a file
    /// with that name is always a checked one. On any failure the `.part` file
    /// is removed.
    ///
    /// Safe in a folder other users can write to (ADR 0048, section 7): the
    /// part file is made with `create_new` (the operating system refuses it if
    /// anything is there already, a symlink included, and never follows one),
    /// and [`DownloadDir::PrivateIn`] makes a fresh folder that only this user
    /// can enter for each download.
    ///
    /// The name is built from the signed release and platform, never from
    /// the address, so a server cannot choose where the file goes. Nothing
    /// is run or opened: the user starts the installer.
    ///
    /// `progress` gets the bytes saved so far, after each piece.
    ///
    /// # Errors
    ///
    /// [`UpdateError`]: a non-HTTPS address, a failed or stalled transfer,
    /// a disk error, or [`UpdateError::DownloadMismatch`].
    pub async fn download<H: HttpDownload>(
        &self,
        http: &H,
        target: &DownloadDir,
        mut progress: impl FnMut(u64) + Send,
    ) -> Result<PathBuf, UpdateError> {
        let url = self.url();
        if !url.starts_with("https://") && !(url.starts_with("http://") && is_loopback_url(url)) {
            return Err(UpdateError::NotHttps);
        }
        let (dir, fresh) = target
            .prepare()
            .map_err(|e| UpdateError::Disk(e.to_string()))?;
        let name = installer_file_name(&self.release, &self.artifact.platform, url);
        let final_path = dir.join(&name);
        let (mut file, part_path) = match create_part(&dir, &name, &mut unique_names("")) {
            Ok(opened) => opened,
            Err(e) => {
                if fresh {
                    let _ = std::fs::remove_dir(&dir);
                }
                return Err(UpdateError::Disk(e.to_string()));
            }
        };

        let mut check = self.artifact.start_check();
        let mut mismatch = false;
        let result = http
            .download(url, self.artifact.bytes, |piece| {
                if check.update(piece).is_err() {
                    mismatch = true;
                    return Err(AuthError::TooLarge);
                }
                file.write_all(piece)
                    .map_err(|e| AuthError::Http(format!("disk: {e}")))?;
                progress(check.seen());
                Ok(())
            })
            .await;
        let outcome = match result {
            Ok(_) if mismatch => Err(UpdateError::DownloadMismatch),
            Ok(_) => check
                .finish()
                .map_err(|_| UpdateError::DownloadMismatch)
                .and_then(|()| {
                    file.sync_all()
                        .map_err(|e| UpdateError::Disk(e.to_string()))
                }),
            Err(_) if mismatch => Err(UpdateError::DownloadMismatch),
            // The signed size is the cap, so an oversized answer is a mismatch too.
            Err(AuthError::TooLarge) => Err(UpdateError::DownloadMismatch),
            Err(AuthError::Http(m)) if m.starts_with("disk: ") => {
                Err(UpdateError::Disk(m.trim_start_matches("disk: ").to_owned()))
            }
            Err(e) => Err(UpdateError::Download(e.to_string())),
        };
        drop(file);
        let failed = |e: UpdateError| {
            let _ = std::fs::remove_file(&part_path);
            // A folder made for this download goes with it (only if empty).
            if fresh {
                let _ = std::fs::remove_dir(&dir);
            }
            Err(e)
        };
        match outcome {
            Ok(()) => match std::fs::rename(&part_path, &final_path) {
                Ok(()) => Ok(final_path),
                Err(e) => failed(UpdateError::Disk(e.to_string())),
            },
            Err(e) => failed(e),
        }
    }

    /// Checks a downloaded installer against the signed size and hash. Run it
    /// before the file is opened or run.
    ///
    /// # Errors
    ///
    /// [`ManifestError::DownloadMismatch`] for any other file.
    pub fn verify_download(&self, data: &[u8]) -> Result<(), ManifestError> {
        self.artifact.verify_download(data)
    }
}

/// The file name of a downloaded installer: `AegisTARA-<release>-<platform>`
/// plus a known installer extension from the address (`.bin` if there is
/// none). Only letters, digits, `.`, `-` and `_` survive, so the name cannot
/// leave the folder whatever the manifest holds.
fn installer_file_name(release: &str, platform: &str, url: &str) -> String {
    const EXTENSIONS: [&str; 10] = [
        ".exe",
        ".msi",
        ".dmg",
        ".pkg",
        ".deb",
        ".rpm",
        ".appimage",
        ".zip",
        ".tar.gz",
        ".msix",
    ];
    let clean = |text: &str| -> String {
        text.chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                    c
                } else {
                    '_'
                }
            })
            .take(40)
            .collect()
    };
    let path = url.split(['?', '#']).next().unwrap_or_default();
    let last = path
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let ext = EXTENSIONS
        .iter()
        .find(|e| last.ends_with(**e))
        .copied()
        .unwrap_or(".bin");
    format!("AegisTARA-{}-{}{ext}", clean(release), clean(platform))
}

/// Where a downloaded installer is saved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DownloadDir {
    /// This folder (made if it is missing): the user's Downloads folder or
    /// one they chose. It is the user's to protect.
    Folder(PathBuf),
    /// A new folder made inside `base` for each download, readable only by
    /// this user (mode 0700 on Unix). For `base` = the system temporary
    /// folder, which other users can write to: nobody can plant a link or a
    /// folder there that the download then trusts.
    PrivateIn(PathBuf),
}

impl DownloadDir {
    /// The user's `Downloads` folder if there is one, else a private folder
    /// in the system temporary folder.
    #[must_use]
    pub fn for_this_user() -> Self {
        ["HOME", "USERPROFILE"]
            .iter()
            .filter_map(std::env::var_os)
            .map(|home| PathBuf::from(home).join("Downloads"))
            .find(|p| p.is_dir())
            .map_or_else(
                || DownloadDir::PrivateIn(std::env::temp_dir()),
                DownloadDir::Folder,
            )
    }

    /// Makes the folder to write into. The flag says it was made for this
    /// download alone (and may be removed again if the download fails).
    fn prepare(&self) -> std::io::Result<(PathBuf, bool)> {
        match self {
            DownloadDir::Folder(dir) => {
                std::fs::create_dir_all(dir)?;
                Ok((dir.clone(), false))
            }
            DownloadDir::PrivateIn(base) => {
                create_private_dir(base, &mut unique_names("aegistara-update")).map(|d| (d, true))
            }
        }
    }
}

/// An endless series of names that no other program can predict well and
/// that never repeat inside this process: the process number, the clock in
/// nanoseconds and a counter. Guessing one only makes a `create_new` fail, and
/// the caller then takes the next name.
fn unique_names(prefix: &'static str) -> impl Iterator<Item = String> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    std::iter::repeat_with(move || {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        format!(
            "{prefix}{}-{nanos:x}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        )
    })
}

/// Most names tried before giving up.
const MAX_NAME_TRIES: usize = 64;

/// Creates a new folder inside `base` that only this user can enter, with the
/// first name from `names` that is free. The folder is made in one step
/// (`mkdir`, mode 0700 on Unix), which fails if the name is taken by anything,
/// a symlink included, so an existing path is never reused.
fn create_private_dir(
    base: &Path,
    names: &mut impl Iterator<Item = String>,
) -> std::io::Result<PathBuf> {
    let mut builder = std::fs::DirBuilder::new();
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    for name in names.take(MAX_NAME_TRIES) {
        let path = base.join(name);
        match builder.create(&path) {
            Ok(()) => return Ok(path),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e),
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "no free name for a private folder",
    ))
}

/// Creates the part file `<name>.<unique>.part` in `dir`. `create_new` makes
/// the open fail if the path exists in any form, and it never follows a
/// symlink, so nothing planted in `dir` is written to.
fn create_part(
    dir: &Path,
    name: &str,
    names: &mut impl Iterator<Item = String>,
) -> std::io::Result<(std::fs::File, PathBuf)> {
    for unique in names.take(MAX_NAME_TRIES) {
        let path = dir.join(format!("{name}.{unique}.part"));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(file) => return Ok((file, path)),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e),
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "no free name for the download",
    ))
}

/// What a manifest means for this computer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpdateStatus {
    /// This release is the newest, or newer than the manifest.
    UpToDate,
    /// A newer release exists, but there is no installer for this platform.
    NoBuildForThisPlatform {
        /// The newer release.
        release: String,
    },
    /// A newer release is ready.
    Available(UpdateOffer),
}

/// Checks manifests for one running release on one platform.
#[derive(Debug)]
pub struct UpdateChecker {
    keys: TrustedKeys,
    current: Version,
    platform: String,
}

impl UpdateChecker {
    /// A checker with explicit parts (tests, tools).
    #[must_use]
    pub fn new(keys: TrustedKeys, current: Version, platform: impl Into<String>) -> Self {
        UpdateChecker {
            keys,
            current,
            platform: platform.into(),
        }
    }

    /// The checker for this build: its own release number and platform, and
    /// the update keys compiled in. With no key in `keys/update.keys`, every
    /// manifest is refused.
    ///
    /// # Panics
    ///
    /// If this crate's own version is not `major.minor.patch`, which a test
    /// guards.
    #[must_use]
    pub fn for_this_build() -> Self {
        Self::new(
            TrustedKeys::updates(),
            Version::parse(env!("CARGO_PKG_VERSION"))
                .expect("the crate version is major.minor.patch"),
            platform_name(),
        )
    }

    /// Fetches the manifest from `url` and checks it.
    ///
    /// # Errors
    ///
    /// [`UpdateError`], see [`fetch_manifest`] and [`UpdateChecker::check`].
    pub async fn check_url<H: HttpClient>(
        &self,
        http: &H,
        url: &str,
    ) -> Result<UpdateStatus, UpdateError> {
        let text = fetch_manifest(http, url).await?;
        Ok(self.check(&text)?)
    }

    /// Verifies the manifest and says what to do. Time O(n) in the manifest
    /// size, which is capped.
    ///
    /// # Errors
    ///
    /// [`ManifestError`] if the manifest is not valid and signed by a trusted
    /// update key.
    pub fn check(&self, manifest_text: &str) -> Result<UpdateStatus, ManifestError> {
        let manifest = Manifest::verify(manifest_text, &self.keys)?;
        Ok(match manifest.offer(self.current, &self.platform) {
            Offer::UpToDate => UpdateStatus::UpToDate,
            Offer::NoBuildForPlatform { release } => UpdateStatus::NoBuildForThisPlatform {
                release: release.to_string(),
            },
            Offer::Available { release, artifact } => UpdateStatus::Available(UpdateOffer {
                release: release.to_string(),
                notes: manifest.notes.clone(),
                artifact: artifact.clone(),
            }),
        })
    }
}

/// The platform name used in manifests: `os-arch`, for example
/// `linux-x86_64`, `windows-x86_64`, `macos-aarch64`.
#[must_use]
pub fn platform_name() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_file_name_comes_from_signed_fields_and_a_known_extension() {
        assert_eq!(
            installer_file_name(
                "0.4.0",
                "linux-x86_64",
                "https://x.example/a/Aegis.AppImage?sig=1"
            ),
            "AegisTARA-0.4.0-linux-x86_64.appimage"
        );
        assert_eq!(
            installer_file_name("1.0.0", "windows-x86_64", "https://x/setup.EXE"),
            "AegisTARA-1.0.0-windows-x86_64.exe"
        );
        // An unknown extension, or a traversal attempt, becomes `.bin`.
        let n = installer_file_name("1.0.0", "../../etc/x", "https://x/../../passwd");
        assert_eq!(n, "AegisTARA-1.0.0-.._.._etc_x.bin");
        assert!(!n.contains('/') && !n.contains('\\'));
    }

    fn scratch(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("aegis-updates-unit-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch");
        dir
    }

    #[test]
    fn a_private_folder_skips_names_that_are_taken_and_never_follows_a_link() {
        let base = scratch("private");
        let elsewhere = scratch("private-elsewhere");
        // The first two names are taken: a folder and a symlink to another
        // folder (which a planted link would use to lead the download away).
        std::fs::create_dir(base.join("a")).expect("taken");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&elsewhere, base.join("b")).expect("link");
        #[cfg(not(unix))]
        std::fs::create_dir(base.join("b")).expect("taken");
        let mut names = ["a", "b", "c"].into_iter().map(str::to_owned);
        let made = create_private_dir(&base, &mut names).expect("made");
        assert_eq!(made, base.join("c"));
        assert!(
            std::fs::read_dir(&elsewhere)
                .expect("read")
                .next()
                .is_none(),
            "nothing went through the link"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&made).expect("meta").permissions().mode();
            assert_eq!(mode & 0o077, 0, "only the owner may enter: {mode:o}");
        }
        // With every name taken it gives up instead of reusing one.
        let mut taken = ["a", "b", "c"].into_iter().map(str::to_owned);
        let err = create_private_dir(&base, &mut taken).expect_err("all taken");
        assert_eq!(err.kind(), std::io::ErrorKind::AlreadyExists);
        let _ = std::fs::remove_dir_all(&base);
        let _ = std::fs::remove_dir_all(&elsewhere);
    }

    #[cfg(unix)]
    #[test]
    fn a_part_file_is_never_written_through_a_planted_link_or_over_a_planted_file() {
        let dir = scratch("part");
        let victim = dir.join("victim.txt");
        std::fs::write(&victim, b"precious").expect("victim");
        let planted = dir.join("planted.txt");
        std::fs::write(&planted, b"planted").expect("planted");
        // `x.0.part` is a link to the victim, `x.1.part` is a planted file,
        // and a dangling link would be followed too by a plain `File::create`.
        std::os::unix::fs::symlink(&victim, dir.join("x.0.part")).expect("link");
        std::fs::write(dir.join("x.1.part"), b"someone else's").expect("file");
        std::os::unix::fs::symlink(dir.join("not-there"), dir.join("x.2.part")).expect("dangling");
        let mut names = ["0", "1", "2", "3"].into_iter().map(str::to_owned);
        let (mut file, path) = create_part(&dir, "x", &mut names).expect("part");
        assert_eq!(path, dir.join("x.3.part"));
        file.write_all(b"installer").expect("write");
        assert_eq!(std::fs::read(&victim).expect("victim"), b"precious");
        assert_eq!(
            std::fs::read(dir.join("x.1.part")).expect("planted"),
            b"someone else's"
        );
        assert!(
            !dir.join("not-there").exists(),
            "the dangling link was not created through"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn generated_names_do_not_repeat() {
        let names: Vec<String> = unique_names("p").take(1000).collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), names.len());
        assert!(names.iter().all(|n| n.starts_with('p') && !n.contains('/')));
    }

    #[test]
    fn this_crates_version_is_a_release_number() {
        let _ = UpdateChecker::for_this_build();
    }

    #[test]
    fn the_platform_name_has_an_os_and_an_arch() {
        let name = platform_name();
        let (os, arch) = name.split_once('-').expect("os-arch");
        assert!(!os.is_empty() && !arch.is_empty());
    }

    #[test]
    fn a_build_with_no_update_key_refuses_every_manifest() {
        // keys/update.keys holds no key until the owner adds one.
        let checker = UpdateChecker::for_this_build();
        assert!(checker.check("AEGIS-UPDATE-1.AAAA.AAAA").is_err());
    }
}
