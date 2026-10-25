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

use aegis_model::{MAX_PACK_FILE_BYTES, Methodology, PackError};

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

/// All hosted methodologies, by key. Ordered, so listings are stable (P4).
#[derive(Clone, Debug, Default)]
pub struct Methodologies {
    by_key: BTreeMap<String, HostedMethodology>,
}

impl Methodologies {
    /// Only the built-in ISO/SAE 21434 + UN R155 pack.
    ///
    /// # Errors
    ///
    /// [`PackLoadError::Invalid`] if the embedded pack is broken (a bug).
    pub fn builtin() -> Result<Self, PackLoadError> {
        let mut me = Methodologies::default();
        let pack = Methodology::iso21434_r155_pack().as_bytes();
        me.add_bytes("built-in", pack)?;
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
        self.by_key.insert(
            key,
            HostedMethodology {
                methodology: Arc::new(methodology),
                pack: pack.into(),
            },
        );
        Ok(())
    }

    /// Looks a methodology up by key. Time O(log n).
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&HostedMethodology> {
        self.by_key.get(key)
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
        assert!(me.get("nope").is_none());
    }

    #[test]
    fn duplicate_and_invalid_packs_are_refused() {
        let mut me = Methodologies::builtin().expect("builtin");
        let dup = me.add_bytes("again", Methodology::iso21434_r155_pack().as_bytes());
        assert!(matches!(dup, Err(PackLoadError::Duplicate(k)) if k == "iso21434-r155"));
        let bad = me.add_bytes("junk", b"{}");
        assert!(matches!(bad, Err(PackLoadError::Invalid { .. })));
        assert_eq!(me.iter().count(), 1);
    }
}
