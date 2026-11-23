//! The methodologies a server hosts.
//!
//! A methodology is data (a pack file, ADR 0011). The server loads the
//! built-in pack and every `*.json` pack in an optional directory at
//! startup, validates each through `Methodology::read_pack` (so every
//! cap applies, R3), and keeps the pack bytes so clients can fetch the
//! same vocabulary.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::sync::Arc;

use aegis_model::{MAX_PACK_FILE_BYTES, Methodology, MethodologyPin, PackError};

/// Why a pinned methodology could not be resolved for an existing project.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PinError {
    /// The project pins an immutable pack digest that this server does not hold.
    #[error("pinned methodology pack {digest} ({key} {version}) is not available on this server")]
    MissingPinnedPack {
        /// Methodology key recorded in the project header.
        key: String,
        /// Methodology version recorded in the project header.
        version: String,
        /// Immutable pack digest (`sha256:<hex>`).
        digest: String,
    },
    /// The project is a legacy unpinned file with no content digest. Never
    /// guess the original pack; require explicit verification and migration.
    #[error(
        "legacy project ({key} {version}) has no methodology pin and requires explicit migration"
    )]
    UnpinnedLegacy {
        /// Methodology key recorded in the legacy header.
        key: String,
        /// Methodology version recorded in the legacy header.
        version: String,
    },
}

/// A failure while loading packs.
#[derive(Debug, thiserror::Error)]
pub enum PackLoadError {
    /// The directory or a file could not be read.
    #[error("cannot read pack {path}: {source}")]
    Io {
        /// File or directory.
        path: String,
        /// Why.
        source: std::io::Error,
    },
    /// A pack file is invalid.
    #[error("pack {path} is invalid: {source}")]
    Invalid {
        /// File.
        path: String,
        /// Why.
        source: PackError,
    },
    /// Two packs declare the same key.
    #[error("duplicate methodology key {0}")]
    Duplicate(String),
}

/// One hosted methodology.
#[derive(Clone, Debug)]
pub struct HostedMethodology {
    /// The validated methodology.
    pub methodology: Arc<Methodology>,
    /// The pack file it was read from.
    pub pack: Arc<[u8]>,
}

/// All hosted methodologies, indexed by active key (for new projects) and by
/// immutable content digest (for loading pinned projects and baselines).
/// Ordered, so listings are stable (P4).
#[derive(Clone, Debug, Default)]
pub struct Methodologies {
    by_key: BTreeMap<String, HostedMethodology>,
    by_digest: BTreeMap<String, HostedMethodology>,
}

impl Methodologies {
    /// The built-in methodology packs (`"iso21434-r155"`, `"eu-cra"`, and
    /// `"stride-services-cloud"`).
    ///
    /// # Errors
    ///
    /// [`PackLoadError::Invalid`] if an embedded pack is broken (a bug).
    pub fn builtin() -> Result<Self, PackLoadError> {
        let mut me = Methodologies::default();
        let pack = Methodology::iso21434_r155_pack().as_bytes();
        me.add_bytes("built-in:iso21434-r155", pack)?;
        let cra_pack = Methodology::eu_cra_pack();
        me.add_bytes("built-in:eu-cra", cra_pack)?;
        let stride_pack = Methodology::stride_services_cloud_pack();
        me.add_bytes("built-in:stride-services-cloud", stride_pack)?;
        Ok(me)
    }

    /// Adds every `*.json` pack in `dir`, in file name order.
    ///
    /// # Errors
    ///
    /// The first unreadable or invalid pack, or a duplicate key.
    pub fn add_dir(&mut self, dir: &Path) -> Result<(), PackLoadError> {
        let io = |path: &Path| {
            let path = path.display().to_string();
            move |source| PackLoadError::Io { path, source }
        };
        let mut files: Vec<_> = std::fs::read_dir(dir)
            .map_err(io(dir))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(io(dir))?
            .into_iter()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "json"))
            .collect();
        files.sort();
        for path in files {
            let mut bytes = Vec::new();
            File::open(&path)
                .map_err(io(&path))?
                .take(MAX_PACK_FILE_BYTES + 1)
                .read_to_end(&mut bytes)
                .map_err(io(&path))?;
            self.add_bytes(&path.display().to_string(), &bytes)?;
        }
        Ok(())
    }

    /// Adds one pack. `origin` names it in errors.
    ///
    /// # Errors
    ///
    /// An invalid pack, or a key that is already hosted.
    pub fn add_bytes(&mut self, origin: &str, pack: &[u8]) -> Result<(), PackLoadError> {
        let methodology =
            Methodology::read_pack(pack).map_err(|source| PackLoadError::Invalid {
                path: origin.to_owned(),
                source,
            })?;
        let key = methodology.key().to_owned();
        if self.by_key.contains_key(&key) {
            return Err(PackLoadError::Duplicate(key));
        }
        let digest = methodology.pack_digest().to_owned();
        let hosted = HostedMethodology {
            methodology: Arc::new(methodology),
            pack: pack.into(),
        };
        self.by_digest.insert(digest, hosted.clone());
        self.by_key.insert(key, hosted);
        Ok(())
    }

    /// Registers a historical/pinned pack by its immutable content digest
    /// without replacing the active `by_key` pack used for new projects.
    /// Returns the immutable digest (`sha256:<hex>`).
    ///
    /// # Errors
    ///
    /// [`PackLoadError::Invalid`] if `pack` fails validation.
    pub fn add_pinned_bytes(&mut self, origin: &str, pack: &[u8]) -> Result<String, PackLoadError> {
        let methodology =
            Methodology::read_pack(pack).map_err(|source| PackLoadError::Invalid {
                path: origin.to_owned(),
                source,
            })?;
        let digest = methodology.pack_digest().to_owned();
        self.by_digest
            .entry(digest.clone())
            .or_insert_with(|| HostedMethodology {
                methodology: Arc::new(methodology),
                pack: pack.into(),
            });
        Ok(digest)
    }

    /// Looks a methodology up by key (for creating new projects). Time O(log n).
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&HostedMethodology> {
        self.by_key.get(key)
    }

    /// Looks a methodology up strictly by its immutable content digest.
    #[must_use]
    pub fn get_by_digest(&self, digest: &str) -> Option<&HostedMethodology> {
        self.by_digest.get(digest)
    }

    /// Resolves a [`MethodologyPin`] strictly: pinned projects load only by
    /// immutable digest (never falling back to `by_key`), and legacy unpinned
    /// projects are reported explicitly instead of guessing.
    ///
    /// # Errors
    ///
    /// [`PinError::MissingPinnedPack`] or [`PinError::UnpinnedLegacy`].
    pub fn resolve_pin(&self, pin: &MethodologyPin) -> Result<&HostedMethodology, PinError> {
        match pin {
            MethodologyPin::Pinned {
                key,
                version,
                digest,
            } => self
                .get_by_digest(digest)
                .ok_or_else(|| PinError::MissingPinnedPack {
                    key: key.clone(),
                    version: version.clone(),
                    digest: digest.clone(),
                }),
            MethodologyPin::LegacyUnpinned { key, version } => Err(PinError::UnpinnedLegacy {
                key: key.clone(),
                version: version.clone(),
            }),
        }
    }

    /// All hosted methodologies in key order.
    pub fn iter(&self) -> impl Iterator<Item = &HostedMethodology> {
        self.by_key.values()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_is_hosted_and_its_pack_round_trips() {
        let me = Methodologies::builtin().expect("builtin");
        let hosted = me.get("iso21434-r155").expect("hosted");
        assert_eq!(hosted.methodology.key(), "iso21434-r155");
        let again = Methodology::read_pack(&*hosted.pack).expect("pack");
        assert_eq!(&again, &*hosted.methodology);
        let cra = me.get("eu-cra").expect("eu-cra hosted");
        assert_eq!(cra.methodology.key(), "eu-cra");
        let cra_again = Methodology::read_pack(&*cra.pack).expect("cra pack");
        assert_eq!(&cra_again, &*cra.methodology);
        let stride = me.get("stride-services-cloud").expect("stride hosted");
        assert_eq!(stride.methodology.key(), "stride-services-cloud");
        let stride_again = Methodology::read_pack(&*stride.pack).expect("stride pack");
        assert_eq!(&stride_again, &*stride.methodology);
        assert!(me.get("nope").is_none());
    }

    #[test]
    fn duplicate_and_invalid_packs_are_refused() {
        let mut me = Methodologies::builtin().expect("builtin");
        let dup = me.add_bytes("again", Methodology::iso21434_r155_pack().as_bytes());
        assert!(matches!(dup, Err(PackLoadError::Duplicate(k)) if k == "iso21434-r155"));
        let bad = me.add_bytes("junk", b"{}");
        assert!(matches!(bad, Err(PackLoadError::Invalid { .. })));
        assert_eq!(me.iter().count(), 3);
    }

    #[test]
    fn pinned_pack_resolution_retains_historical_pack_and_refuses_legacy_guessing() {
        let orig_bytes = Methodology::iso21434_r155_pack().as_bytes().to_vec();
        let mut altered_def =
            aegis_model::MethodologyDef::read_pack(orig_bytes.as_slice()).expect("read");
        let highest = altered_def
            .risk_levels
            .last()
            .expect("risk levels")
            .key
            .clone();
        altered_def.impact_categories[0].risk_matrix[0][0] = highest;
        let mut altered_bytes = Vec::new();
        altered_def
            .write_pack(&mut altered_bytes)
            .expect("write altered");

        let mut hosted = Methodologies::default();
        // Host altered pack as today's active pack for `iso21434-r155`.
        hosted
            .add_bytes("active", &altered_bytes)
            .expect("add active");
        // Retain the historical original pack by immutable digest.
        let orig_digest = hosted
            .add_pinned_bytes("historical", &orig_bytes)
            .expect("add historical");
        let active_digest = hosted
            .get("iso21434-r155")
            .expect("active")
            .methodology
            .pack_digest()
            .to_owned();
        assert_ne!(orig_digest, active_digest);

        let pin = MethodologyPin::Pinned {
            key: "iso21434-r155".into(),
            version: "0.1.0".into(),
            digest: orig_digest.clone(),
        };
        let resolved = hosted.resolve_pin(&pin).expect("resolves historical pack");
        assert_eq!(resolved.methodology.pack_digest(), orig_digest);

        let missing = MethodologyPin::Pinned {
            key: "iso21434-r155".into(),
            version: "0.1.0".into(),
            digest: "sha256:0000000000000000000000000000000000000000000000000000000000000000"
                .into(),
        };
        assert!(matches!(
            hosted.resolve_pin(&missing),
            Err(PinError::MissingPinnedPack { .. })
        ));

        let legacy = MethodologyPin::LegacyUnpinned {
            key: "iso21434-r155".into(),
            version: "0.1.0".into(),
        };
        assert!(matches!(
            hosted.resolve_pin(&legacy),
            Err(PinError::UnpinnedLegacy { .. })
        ));
    }
}
