//! Update checks against the signed manifest (job 036, ADR 0034).
//!
//! This module decides, from the text of a manifest, whether an update exists
//! for this computer and whether a downloaded installer is the one the vendor
//! signed. [`UpdateChecker::check`] does no network I/O, so a manifest can
//! also come from a file or a share, which suits air gapped sites.
//! [`fetch_manifest`] and [`UpdateChecker::check_url`] fetch it over HTTPS
//! with the `aegis-auth` client (rustls, the OS trust store, no redirects,
//! 10 s, 1 MiB; job 048, ADR 0042). The installer download is not here yet.
//!
//! Safety rules, all enforced by `aegis-license`:
//!
//! - the manifest must be signed by a key compiled into this build's update
//!   key list (not the license key list);
//! - only a strictly newer release is offered, so an old manifest cannot
//!   downgrade the app;
//! - an installer is accepted only if its size and SHA-256 match.

use aegis_auth::http::{HttpClient, HttpRequest, is_loopback_url};
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
