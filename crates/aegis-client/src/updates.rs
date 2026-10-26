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

    /// Downloads the installer into `dir` and checks it.
    ///
    /// The bytes go to `<name>.part` while they arrive, are counted and
    /// hashed on the way (never held in memory), and are refused as soon as
    /// they pass the signed size. Only when the size and the SHA-256 both
    /// match is the file renamed to its final name, so a file with that name
    /// is always a checked one. On any failure the `.part` file is removed.
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
        dir: &Path,
        mut progress: impl FnMut(u64) + Send,
    ) -> Result<PathBuf, UpdateError> {
        let url = self.url();
        if !url.starts_with("https://") && !(url.starts_with("http://") && is_loopback_url(url)) {
            return Err(UpdateError::NotHttps);
        }
        std::fs::create_dir_all(dir).map_err(|e| UpdateError::Disk(e.to_string()))?;
        let name = installer_file_name(&self.release, &self.artifact.platform, url);
        let final_path = dir.join(&name);
        let part_path = dir.join(format!("{name}.part"));
        let mut file =
            std::fs::File::create(&part_path).map_err(|e| UpdateError::Disk(e.to_string()))?;

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
        match outcome {
            Ok(()) => std::fs::rename(&part_path, &final_path)
                .map(|()| final_path)
                .map_err(|e| {
                    let _ = std::fs::remove_file(&part_path);
                    UpdateError::Disk(e.to_string())
                }),
            Err(e) => {
                let _ = std::fs::remove_file(&part_path);
                Err(e)
            }
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

/// A folder for downloads: the user's `Downloads` folder if there is one,
/// else the system temporary folder.
#[must_use]
pub fn default_download_dir() -> PathBuf {
    ["HOME", "USERPROFILE"]
        .iter()
        .filter_map(std::env::var_os)
        .map(|home| PathBuf::from(home).join("Downloads"))
        .find(|p| p.is_dir())
        .unwrap_or_else(std::env::temp_dir)
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
