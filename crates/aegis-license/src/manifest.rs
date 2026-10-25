//! The signed update manifest (job 036, ADR 0034).
//!
//! The vendor publishes a manifest next to each release: the release number
//! and, per platform, where the installer is, how big it is, and its SHA-256.
//! The client verifies the manifest's signature with a key compiled into the
//! build, then checks any download against the manifest. The manifest is the
//! only thing that has to be trusted: the download itself can come from any
//! mirror, because a wrong file fails the hash.
//!
//! Rules that keep an update channel safe:
//!
//! - It is signed with an **update key**, which is separate from the license
//!   key (`keys/update.keys`), so a leaked license key cannot ship code.
//! - It never offers a release that is not newer than the running one, so a
//!   replayed old manifest cannot downgrade a client.
//! - Parsing is strict and bounded, like licenses.

use std::fmt;

use ring::digest::{SHA256, digest};
use serde::{Deserialize, Serialize};

use crate::envelope;
use crate::error::LicenseError;
use crate::keys::TrustedKeys;

/// The start of every manifest file.
pub const MANIFEST_PREFIX: &str = "AEGIS-UPDATE-1";
/// Largest manifest accepted.
pub const MAX_MANIFEST_BYTES: usize = 32 * 1024;
/// Most artifacts in one manifest.
pub const MAX_ARTIFACTS: usize = 16;
/// Largest installer a manifest may announce (4 GiB).
pub const MAX_ARTIFACT_BYTES: u64 = 4 * 1024 * 1024 * 1024;

pub(crate) const DOMAIN: &[u8] = b"aegis-update-v1\0";
const FORMAT_VERSION: u32 = 1;
const MAX_URL: usize = 512;
const MAX_NOTES: usize = 2000;

/// Why a manifest or a download was refused. Messages never echo content.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ManifestError {
    /// The file is too large.
    #[error("update manifest is too large")]
    TooLarge,
    /// The file is not in the manifest format.
    #[error("update manifest is malformed")]
    Malformed,
    /// A format version this build does not know.
    #[error("update manifest format version {0} is not supported")]
    UnsupportedVersion(u32),
    /// Signed by a key this build does not trust for updates.
    #[error("update manifest is signed by an unknown key")]
    UnknownKey,
    /// The signature does not match.
    #[error("update manifest signature is not valid")]
    BadSignature,
    /// A rule of the format is broken.
    #[error("update manifest content is not valid: {0}")]
    Invalid(&'static str),
    /// A download does not match what the manifest says.
    #[error("the download does not match the manifest")]
    DownloadMismatch,
    /// Signing failed (issuer only).
    #[error("signing failed")]
    Signing,
}

impl From<LicenseError> for ManifestError {
    fn from(e: LicenseError) -> Self {
        match e {
            LicenseError::TooLarge => ManifestError::TooLarge,
            LicenseError::UnknownKey => ManifestError::UnknownKey,
            LicenseError::BadSignature => ManifestError::BadSignature,
            LicenseError::UnsupportedVersion(v) => ManifestError::UnsupportedVersion(v),
            LicenseError::Invalid(why) => ManifestError::Invalid(why),
            LicenseError::Signing => ManifestError::Signing,
            LicenseError::Malformed | LicenseError::BadKeyList(_) => ManifestError::Malformed,
        }
    }
}

/// A release number: `major.minor.patch`, three plain numbers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version {
    /// Incompatible changes.
    pub major: u32,
    /// Compatible additions.
    pub minor: u32,
    /// Fixes.
    pub patch: u32,
}

impl Version {
    /// Parses `1.2.3`. Pre-release and build suffixes are not accepted, so
    /// two spellings never mean the same release.
    ///
    /// # Errors
    ///
    /// [`ManifestError::Invalid`] for anything else.
    pub fn parse(text: &str) -> Result<Version, ManifestError> {
        let bad = || ManifestError::Invalid("a release number is not major.minor.patch");
        let mut parts = text.split('.');
        let mut next = || -> Result<u32, ManifestError> {
            let part = parts.next().ok_or_else(bad)?;
            let plain = !part.is_empty()
                && part.len() <= 9
                && part.bytes().all(|b| b.is_ascii_digit())
                && (part == "0" || !part.starts_with('0'));
            if !plain {
                return Err(bad());
            }
            part.parse().map_err(|_| bad())
        };
        let version = Version {
            major: next()?,
            minor: next()?,
            patch: next()?,
        };
        if parts.next().is_some() {
            return Err(bad());
        }
        Ok(version)
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// One installer in a release.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Artifact {
    /// For example `windows-x86_64`, `macos-aarch64`, `linux-x86_64`.
    pub platform: String,
    /// Where to download it. Always `https://`.
    pub url: String,
    /// SHA-256 of the file, 64 lowercase hex digits.
    pub sha256: String,
    /// Size of the file in bytes.
    pub bytes: u64,
}

impl Artifact {
    /// Checks downloaded bytes against the size and hash. Time O(n) in the
    /// download size.
    ///
    /// # Errors
    ///
    /// [`ManifestError::DownloadMismatch`].
    pub fn verify_download(&self, data: &[u8]) -> Result<(), ManifestError> {
        if data.len() as u64 != self.bytes {
            return Err(ManifestError::DownloadMismatch);
        }
        // The hash is public, so an ordinary comparison leaks nothing.
        if hex(digest(&SHA256, data).as_ref()) == self.sha256 {
            Ok(())
        } else {
            Err(ManifestError::DownloadMismatch)
        }
    }
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut out, b| {
            let _ = write!(out, "{b:02x}");
            out
        })
}

/// A signed release description.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    /// Format version. Always 1 today.
    pub version: u32,
    /// The signing key's ID.
    pub kid: String,
    /// The release number, `major.minor.patch`.
    pub release: String,
    /// When it was published, seconds since the Unix epoch.
    pub published_at: u64,
    /// One installer per platform.
    pub artifacts: Vec<Artifact>,
    /// A few lines for the user ("what is new"). Plain text.
    pub notes: String,
}

/// What a client should do about a manifest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Offer<'a> {
    /// The running release is this one or newer.
    UpToDate,
    /// A newer release exists, but not for this platform.
    NoBuildForPlatform {
        /// The newer release.
        release: Version,
    },
    /// A newer release with an installer for this platform.
    Available {
        /// The newer release.
        release: Version,
        /// The installer.
        artifact: &'a Artifact,
    },
}

impl Manifest {
    /// Parses a manifest file and checks its signature against `keys`.
    /// Time O(n) in the file size, which is capped.
    ///
    /// # Errors
    ///
    /// A [`ManifestError`] naming the first thing wrong.
    pub fn verify(file: &str, keys: &TrustedKeys) -> Result<Manifest, ManifestError> {
        let (payload, signature) = envelope::split(file, MANIFEST_PREFIX, MAX_MANIFEST_BYTES)?;
        let manifest: Manifest =
            serde_json::from_slice(&payload).map_err(|_| ManifestError::Malformed)?;
        if manifest.version != FORMAT_VERSION {
            return Err(ManifestError::UnsupportedVersion(manifest.version));
        }
        let key = keys.get(&manifest.kid).ok_or(ManifestError::UnknownKey)?;
        envelope::verify(key, DOMAIN, &payload, &signature)?;
        manifest.check()?;
        Ok(manifest)
    }

    /// Checks the rules of the format. Run after the signature, and by the
    /// issuer before signing.
    ///
    /// # Errors
    ///
    /// [`ManifestError::Invalid`] naming the rule.
    pub fn check(&self) -> Result<(), ManifestError> {
        let invalid = ManifestError::Invalid;
        if self.version != FORMAT_VERSION {
            return Err(ManifestError::UnsupportedVersion(self.version));
        }
        if self.kid.is_empty() || self.kid.len() > 32 || !self.kid.bytes().all(token_byte) {
            return Err(invalid("key id"));
        }
        Version::parse(&self.release)?;
        if self.artifacts.is_empty() || self.artifacts.len() > MAX_ARTIFACTS {
            return Err(invalid("the number of artifacts"));
        }
        for (i, a) in self.artifacts.iter().enumerate() {
            if a.platform.is_empty() || a.platform.len() > 32 || !a.platform.bytes().all(token_byte)
            {
                return Err(invalid("a platform name"));
            }
            if self.artifacts[..i].iter().any(|b| b.platform == a.platform) {
                return Err(invalid("a platform appears twice"));
            }
            if !download_url(&a.url) {
                return Err(invalid("a download address must be a plain https URL"));
            }
            if a.sha256.len() != 64
                || !a
                    .sha256
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                return Err(invalid("a hash must be 64 lowercase hex digits"));
            }
            if a.bytes == 0 || a.bytes > MAX_ARTIFACT_BYTES {
                return Err(invalid("an installer size"));
            }
        }
        if self.notes.chars().count() > MAX_NOTES
            || self.notes.chars().any(|c| c.is_control() && c != '\n')
        {
            return Err(invalid("notes"));
        }
        Ok(())
    }

    /// The release number.
    ///
    /// # Panics
    ///
    /// Never for a manifest that came from [`verify`](Self::verify).
    #[must_use]
    pub fn release(&self) -> Version {
        Version::parse(&self.release).expect("checked when the manifest was verified")
    }

    /// What to do on a computer that runs `current` on `platform`.
    /// Only a strictly newer release is ever offered. Time O(artifacts).
    #[must_use]
    pub fn offer(&self, current: Version, platform: &str) -> Offer<'_> {
        let release = self.release();
        if release <= current {
            return Offer::UpToDate;
        }
        match self.artifacts.iter().find(|a| a.platform == platform) {
            Some(artifact) => Offer::Available { release, artifact },
            None => Offer::NoBuildForPlatform { release },
        }
    }

    #[cfg(feature = "issuer")]
    pub(crate) fn payload(&self) -> Result<Vec<u8>, ManifestError> {
        serde_json::to_vec(self).map_err(|_| ManifestError::Signing)
    }
}

fn token_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.')
}

/// `https://`, ASCII, no spaces, quotes, angle brackets, or controls, and no
/// credentials in the address.
fn download_url(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else {
        return false;
    };
    let host_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    url.len() <= MAX_URL
        && !rest.is_empty()
        && !rest[..host_end].is_empty()
        && !rest[..host_end].contains('@')
        && url
            .bytes()
            .all(|b| b.is_ascii_graphic() && !matches!(b, b'"' | b'<' | b'>' | b'\\' | b'`'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn artifact(platform: &str) -> Artifact {
        Artifact {
            platform: platform.into(),
            url: format!("https://downloads.example.com/aegis-0.2.0-{platform}.zip"),
            sha256: hex(digest(&SHA256, b"installer").as_ref()),
            bytes: 9,
        }
    }

    fn manifest() -> Manifest {
        Manifest {
            version: 1,
            kid: "upd".into(),
            release: "0.2.0".into(),
            published_at: 1_800_000_000,
            artifacts: vec![artifact("linux-x86_64"), artifact("windows-x86_64")],
            notes: "Faster layout.\nBug fixes.".into(),
        }
    }

    #[test]
    fn versions_parse_strictly_and_order_by_number() {
        assert_eq!(Version::parse("1.20.3").expect("ok").to_string(), "1.20.3");
        for bad in [
            "",
            "1",
            "1.2",
            "1.2.3.4",
            "1.2.x",
            "01.2.3",
            "1.2.3-rc1",
            "+1.2.3",
            "1..3",
            "1.2.3 ",
        ] {
            assert!(Version::parse(bad).is_err(), "{bad:?}");
        }
        let v = |s| Version::parse(s).expect("ok");
        assert!(v("0.10.0") > v("0.9.9"));
        assert!(v("1.0.0") > v("0.99.99"));
        assert!(v("1.2.4") > v("1.2.3"));
    }

    #[test]
    fn only_a_newer_release_is_offered() {
        let m = manifest();
        let v = |s| Version::parse(s).expect("ok");
        assert_eq!(m.offer(v("0.2.0"), "linux-x86_64"), Offer::UpToDate);
        assert_eq!(
            m.offer(v("0.3.0"), "linux-x86_64"),
            Offer::UpToDate,
            "no downgrade"
        );
        assert!(matches!(
            m.offer(v("0.1.9"), "linux-x86_64"),
            Offer::Available { release, artifact } if release == v("0.2.0") && artifact.platform == "linux-x86_64"
        ));
        assert_eq!(
            m.offer(v("0.1.9"), "macos-aarch64"),
            Offer::NoBuildForPlatform {
                release: v("0.2.0")
            }
        );
    }

    #[test]
    fn a_download_must_match_size_and_hash() {
        let a = artifact("linux-x86_64");
        assert!(a.verify_download(b"installer").is_ok());
        assert_eq!(
            a.verify_download(b"installeR"),
            Err(ManifestError::DownloadMismatch)
        );
        assert_eq!(
            a.verify_download(b"installer!"),
            Err(ManifestError::DownloadMismatch)
        );
        assert_eq!(a.verify_download(b""), Err(ManifestError::DownloadMismatch));
    }

    #[test]
    fn the_rules_reject_bad_manifests() {
        assert!(manifest().check().is_ok());
        let edit = |f: fn(&mut Manifest)| {
            let mut m = manifest();
            f(&mut m);
            m.check().is_err()
        };
        assert!(edit(|m| m.release = "latest".into()));
        assert!(edit(|m| m.artifacts.clear()));
        assert!(
            edit(|m| m.artifacts.push(artifact("linux-x86_64"))),
            "duplicate platform"
        );
        assert!(edit(
            |m| m.artifacts[0].url = "http://example.com/a.zip".into()
        ));
        assert!(edit(
            |m| m.artifacts[0].url = "https://user:pw@example.com/a.zip".into()
        ));
        assert!(edit(|m| m.artifacts[0].url = "https:///a.zip".into()));
        assert!(edit(
            |m| m.artifacts[0].url = "https://example.com/a b.zip".into()
        ));
        assert!(edit(|m| m.artifacts[0].sha256 = "ABCD".into()));
        assert!(edit(|m| m.artifacts[0].sha256 = "G".repeat(64)));
        assert!(edit(|m| m.artifacts[0].bytes = 0));
        assert!(edit(|m| m.artifacts[0].bytes = MAX_ARTIFACT_BYTES + 1));
        assert!(edit(|m| m.notes = "bell\u{7}".into()));
        assert!(edit(|m| m.notes = "x".repeat(MAX_NOTES + 1)));
        assert!(edit(|m| m.artifacts = (0..=MAX_ARTIFACTS)
            .map(|i| artifact(&format!("p{i}")))
            .collect()));
    }
}
