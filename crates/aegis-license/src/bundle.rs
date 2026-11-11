//! Offline bundles for air-gapped sites (job 100).
//!
//! A bundle is one folder that a customer copies to a site with no network:
//!
//! ```text
//! SHA256SUMS          "<64 lowercase hex>  <path>\n" per file, sorted by path bytes
//! SHA256SUMS.sig      raw 64-byte Ed25519 signature over SHA256SUMS
//! update.aegis        the release's signed update manifest (AEGIS-UPDATE-1)
//! installers/<file>   desktop installers; each file's SHA-256 must be an artifact sha256 in update.aegis
//! images.tar.gz       optional, any other top-level file is allowed if listed
//! ```
//!
//! The signature is raw Ed25519 by the **update** key over the exact bytes of
//! `SHA256SUMS`, with no domain prefix, so a customer can check a bundle with
//! OpenSSL 3 and `sha256sum -c` alone. That is safe: the signed text must
//! parse as sums lines, and the other signed messages start with
//! `aegis-update-v1\0` or `aegis-license-v1\0`, which never parse as one.

use std::fs::{self, File};
use std::io::{ErrorKind, Read};
use std::path::Path;

use base64::Engine;
use ring::digest::{Context, SHA256};
use ring::signature::{ED25519, UnparsedPublicKey};

use crate::keys::TrustedKeys;
use crate::manifest::{MAX_MANIFEST_BYTES, Manifest, ManifestError, Version};

/// The list of files and their SHA-256.
pub const SUMS_FILE: &str = "SHA256SUMS";
/// The signature over [`SUMS_FILE`].
pub const SIG_FILE: &str = "SHA256SUMS.sig";
/// The signed update manifest in the bundle.
pub const BUNDLE_MANIFEST: &str = "update.aegis";
/// The folder that holds the desktop installers.
pub const INSTALLERS_DIR: &str = "installers";
/// Largest `SHA256SUMS` accepted.
pub const MAX_SUMS_BYTES: usize = 32 * 1024;
/// Most files listed in one bundle.
pub const MAX_BUNDLE_FILES: usize = 64;
/// Longest path in a bundle.
pub const MAX_PATH_BYTES: usize = 200;
/// A path is `name` or `dir/name`.
const MAX_DEPTH: usize = 2;
const HASH_HEX: usize = 64;
const SIG_BYTES: usize = 64;
const READ_CHUNK: usize = 64 * 1024;
/// The SPKI DER header of an Ed25519 public key.
const SPKI_PREFIX: &[u8; 12] = b"\x30\x2a\x30\x05\x06\x03\x2b\x65\x70\x03\x21\x00";

/// One line of `SHA256SUMS`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SumsEntry {
    /// Bundle-relative path, `name` or `dir/name`.
    pub path: String,
    /// SHA-256 of the file, 64 lowercase hex digits.
    pub sha256: String,
}

/// Why a bundle was refused. Paths are bundle-relative; messages never echo
/// file contents.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BundleError {
    /// A file is over its size cap, or the folder has too many entries.
    #[error("bundle file is too large or the bundle has too many files")]
    TooLarge,
    /// A file or name is not in the expected format.
    #[error("bundle is malformed: {0}")]
    Malformed(&'static str),
    /// `SHA256SUMS.sig` does not verify with any trusted update key.
    #[error("bundle signature is not valid")]
    BadSignature,
    /// A required or listed file is not there.
    #[error("bundle file is missing: {0}")]
    Missing(String),
    /// A file is in the folder but not in `SHA256SUMS`.
    #[error("bundle file is not listed in SHA256SUMS: {0}")]
    Unlisted(String),
    /// A symlink or another thing that is not a regular file.
    #[error("bundle entry is not a regular file: {0}")]
    NotAFile(String),
    /// A file's SHA-256 differs from its line.
    #[error("bundle file does not match SHA256SUMS: {0}")]
    Mismatch(String),
    /// An installer is not an artifact of the signed manifest.
    #[error("installer is not in the update manifest: {0}")]
    NotInManifest(String),
    /// The update manifest was refused.
    #[error(transparent)]
    Manifest(#[from] ManifestError),
    /// A file could not be read.
    #[error("could not read bundle file: {0}")]
    Io(String),
    /// Signing failed (issuer only).
    #[error("signing failed")]
    Signing,
}

/// What a verified bundle holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BundleReport {
    /// The update key that signed `SHA256SUMS`.
    pub key_id: String,
    /// The release in `update.aegis`.
    pub release: Version,
    /// Files listed in `SHA256SUMS`.
    pub files: usize,
}

/// `name` or `dir/name`; each part starts with a letter or digit and holds
/// only ASCII letters, digits, `.`, `_` and `-`.
fn valid_path(path: &str) -> bool {
    let part_ok = |p: &str| {
        p.bytes().next().is_some_and(|b| b.is_ascii_alphanumeric())
            && p.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b".-_".contains(&b))
    };
    (1..=MAX_PATH_BYTES).contains(&path.len())
        && path.split('/').count() <= MAX_DEPTH
        && path.split('/').all(part_ok)
}

/// Parses `SHA256SUMS` strictly. Time O(n) in the text length.
///
/// # Errors
///
/// [`BundleError::TooLarge`] over [`MAX_SUMS_BYTES`], else
/// [`BundleError::Malformed`] naming the first rule broken.
pub fn parse_sums(text: &str) -> Result<Vec<SumsEntry>, BundleError> {
    if text.len() > MAX_SUMS_BYTES {
        return Err(BundleError::TooLarge);
    }
    let bad = BundleError::Malformed;
    let Some(body) = text.strip_suffix('\n').filter(|b| !b.is_empty()) else {
        return Err(bad("the sums are empty or lack a final newline"));
    };
    if text.contains('\r') {
        return Err(bad("the sums contain a carriage return"));
    }
    let mut entries: Vec<SumsEntry> = Vec::new();
    for line in body.split('\n') {
        if entries.len() == MAX_BUNDLE_FILES {
            return Err(bad("too many files"));
        }
        let (Some(sha256), Some("  "), Some(path)) = (
            line.get(..HASH_HEX),
            line.get(HASH_HEX..HASH_HEX + 2),
            line.get(HASH_HEX + 2..),
        ) else {
            return Err(bad("a line is not `hash  path`"));
        };
        let hex_ok = sha256
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
        if !hex_ok || !valid_path(path) || path == SUMS_FILE || path == SIG_FILE {
            return Err(bad("a hash or a path is not allowed"));
        }
        if entries.last().is_some_and(|e| e.path.as_str() >= path) {
            return Err(bad("the paths are not sorted and unique"));
        }
        entries.push(SumsEntry {
            path: path.to_owned(),
            sha256: sha256.to_owned(),
        });
    }
    Ok(entries)
}

/// Renders `SHA256SUMS` text, one line per entry, in the given order.
#[must_use]
pub fn render_sums(entries: &[SumsEntry]) -> String {
    entries
        .iter()
        .map(|e| format!("{}  {}\n", e.sha256, e.path))
        .collect()
}

/// Checks a raw Ed25519 signature over `sums` and returns the ID of the key
/// that made it. At most [`crate::MAX_TRUSTED_KEYS`] tries.
///
/// # Errors
///
/// [`BundleError::BadSignature`] if no trusted key verifies it.
pub fn verify_sums_signature(
    sums: &[u8],
    signature: &[u8],
    keys: &TrustedKeys,
) -> Result<String, BundleError> {
    if signature.len() != SIG_BYTES {
        return Err(BundleError::BadSignature);
    }
    let verifies = |key: &[u8; 32]| UnparsedPublicKey::new(&ED25519, key).verify(sums, signature);
    keys.iter()
        .find(|(_, key)| verifies(key).is_ok())
        .map(|(id, _)| id.to_owned())
        .ok_or(BundleError::BadSignature)
}

/// The regular files under `dir`, bundle-relative and sorted, without the sums
/// and signature files. Never follows a symlink. Time O(capped entries).
fn list_files(dir: &Path) -> Result<Vec<String>, BundleError> {
    let (mut out, mut seen) = (Vec::new(), 0);
    walk(dir, "", &mut seen, &mut out)?;
    out.sort();
    Ok(out)
}

/// Walks one folder; `pre` is `""` at the top or `dir/` one level down.
fn walk(dir: &Path, pre: &str, n: &mut usize, out: &mut Vec<String>) -> Result<(), BundleError> {
    let io = || BundleError::Io(format!("./{pre}"));
    for entry in fs::read_dir(dir).map_err(|_| io())? {
        let entry = entry.map_err(|_| io())?;
        *n += 1;
        if *n > MAX_BUNDLE_FILES + 2 {
            return Err(BundleError::TooLarge);
        }
        let rel = match entry.file_name().to_str() {
            Some(name) if valid_path(&format!("{pre}{name}")) => format!("{pre}{name}"),
            _ => return Err(BundleError::Malformed("a file name")),
        };
        if pre.is_empty() && (rel == SUMS_FILE || rel == SIG_FILE) {
            continue;
        }
        let meta = fs::symlink_metadata(entry.path()).map_err(|_| BundleError::Io(rel.clone()))?;
        if meta.is_file() {
            out.push(rel);
        } else if meta.is_dir() && pre.is_empty() {
            walk(&entry.path(), &format!("{rel}/"), n, out)?;
        } else {
            return Err(BundleError::NotAFile(rel));
        }
    }
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Streams a file through SHA-256. Time O(n), memory O(1).
fn sha256_file(path: &Path, rel: &str) -> Result<String, BundleError> {
    let io = || BundleError::Io(rel.to_owned());
    let mut file = File::open(path).map_err(|_| io())?;
    let mut hash = Context::new(&SHA256);
    let mut buf = vec![0u8; READ_CHUNK];
    loop {
        let n = match file.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == ErrorKind::Interrupted => continue,
            Err(_) => return Err(io()),
        };
        hash.update(buf.get(..n).ok_or_else(io)?);
    }
    Ok(hex(hash.finish().as_ref()))
}

/// Writes the `SHA256SUMS` text for every file in `dir`.
///
/// # Errors
///
/// A [`BundleError`] for a bad entry, an I/O failure, or a folder the
/// verifier would refuse.
pub fn sums_for_dir(dir: &Path) -> Result<String, BundleError> {
    let mut entries = Vec::new();
    for path in list_files(dir)? {
        let sha256 = sha256_file(&dir.join(&path), &path)?;
        entries.push(SumsEntry { path, sha256 });
    }
    let text = render_sums(&entries);
    parse_sums(&text)?;
    Ok(text)
}

/// Checks that `rel` in `dir` is a regular file no longer than `cap`, and
/// reads it. A file that grows while it is read is still refused.
fn read_regular(dir: &Path, rel: &str, cap: usize) -> Result<Vec<u8>, BundleError> {
    let path = dir.join(rel);
    let io = || BundleError::Io(rel.to_owned());
    let meta = fs::symlink_metadata(&path).map_err(|e| match e.kind() {
        ErrorKind::NotFound => BundleError::Missing(rel.to_owned()),
        _ => io(),
    })?;
    if !meta.is_file() {
        return Err(BundleError::NotAFile(rel.to_owned()));
    }
    let mut bytes = Vec::new();
    if meta.len() <= cap as u64 {
        let file = File::open(&path).map_err(|_| io())?;
        file.take(cap as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| io())?;
    }
    if meta.len() > cap as u64 || bytes.len() > cap {
        return Err(BundleError::TooLarge);
    }
    Ok(bytes)
}

/// Verifies an offline bundle folder: the signature over `SHA256SUMS`, that
/// the folder holds exactly the listed files with the listed hashes, the
/// update manifest, and that every installer is one of its artifacts.
/// Time O(total file bytes); memory O(1) per file.
///
/// # Errors
///
/// A [`BundleError`] naming the first thing wrong.
pub fn verify_bundle_dir(dir: &Path, keys: &TrustedKeys) -> Result<BundleReport, BundleError> {
    let sums = read_regular(dir, SUMS_FILE, MAX_SUMS_BYTES)?;
    let signature = read_regular(dir, SIG_FILE, SIG_BYTES).map_err(|e| match e {
        BundleError::TooLarge => BundleError::BadSignature,
        other => other,
    })?;
    let key_id = verify_sums_signature(&sums, &signature, keys)?;
    let text =
        std::str::from_utf8(&sums).map_err(|_| BundleError::Malformed("the sums are not UTF-8"))?;
    let entries = parse_sums(text)?;

    // Both lists are short (capped), so the quadratic lookups are cheap.
    let on_disk = list_files(dir)?;
    let listed: Vec<String> = entries.iter().map(|e| e.path.clone()).collect();
    if let Some(p) = listed.iter().find(|p| !on_disk.contains(p)) {
        return Err(BundleError::Missing(p.clone()));
    }
    if let Some(p) = on_disk.iter().find(|p| !listed.contains(p)) {
        return Err(BundleError::Unlisted(p.clone()));
    }
    for e in &entries {
        if sha256_file(&dir.join(&e.path), &e.path)? != e.sha256 {
            return Err(BundleError::Mismatch(e.path.clone()));
        }
    }
    if !listed.iter().any(|p| p == BUNDLE_MANIFEST) {
        return Err(BundleError::Missing(BUNDLE_MANIFEST.to_owned()));
    }
    let manifest_bytes = read_regular(dir, BUNDLE_MANIFEST, MAX_MANIFEST_BYTES)?;
    let manifest_text = std::str::from_utf8(&manifest_bytes)
        .map_err(|_| BundleError::Manifest(ManifestError::Malformed))?;
    let manifest = Manifest::verify(manifest_text, keys)?;
    let dir_prefix = format!("{INSTALLERS_DIR}/");
    let installers: Vec<&SumsEntry> = entries
        .iter()
        .filter(|e| e.path.starts_with(&dir_prefix))
        .collect();
    if installers.is_empty() {
        return Err(BundleError::Malformed("no installer"));
    }
    let known = |e: &&&SumsEntry| manifest.artifacts.iter().any(|a| a.sha256 == e.sha256);
    if let Some(e) = installers.iter().find(|e| !known(e)) {
        return Err(BundleError::NotInManifest(e.path.clone()));
    }
    Ok(BundleReport {
        key_id,
        release: manifest.release(),
        files: entries.len(),
    })
}

/// The PEM `PUBLIC KEY` (SPKI) of a trusted key, for checking a bundle with
/// `openssl pkeyutl -verify -rawin`. `None` for an unknown key ID.
#[must_use]
pub fn public_key_pem(keys: &TrustedKeys, key_id: &str) -> Option<String> {
    let key = keys.get(key_id)?;
    let der = [&SPKI_PREFIX[..], &key[..]].concat();
    let b64 = base64::engine::general_purpose::STANDARD.encode(der);
    Some(format!(
        "-----BEGIN PUBLIC KEY-----\n{b64}\n-----END PUBLIC KEY-----\n"
    ))
}
