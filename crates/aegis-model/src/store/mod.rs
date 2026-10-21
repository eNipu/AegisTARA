//! Saving and loading: a project snapshot and an append-only op log.
//!
//! Two JSON formats, both versioned and both written with stable vocabulary
//! keys, never with in-memory indices (see
//! `docs/adr/0009-save-load-format.md`):
//!
//! - **Snapshot**, format `aegis-project` version 1: one compact JSON
//!   object on one line, ending in `\n`. It holds the methodology key and
//!   version, then the live assets, damage scenarios, and threat scenarios
//!   in public-ID order. Written by [`Project::save`](crate::Project::save),
//!   read by [`Project::load`](crate::Project::load).
//! - **Op log**, format `aegis-oplog` version 1: JSON Lines. One header
//!   line with the methodology key and version, then one record
//!   `{"seq":N,"op":{...}}` per line, each ending in `\n`, with `seq`
//!   counting 1, 2, 3, ... Written by [`OpLogWriter`], read by
//!   [`OpLogReader`].
//!
//! Rules both formats share:
//!
//! - The only JSON numbers are `version` and `seq`. IDs are hyphenated
//!   lowercase `UUIDv7` text. Vocabulary items are their keys. A catalog
//!   ref is `{"scheme": <key>, "id": <text in the scheme's syntax>}`.
//! - Output is canonical and does not depend on vocabulary order: property
//!   sets are sorted by key; snapshot impacts are an object sorted by
//!   category key with unrated categories left out; snapshot catalog refs
//!   are sorted by scheme key, then by segments; damage links are sorted by
//!   ID. In an op, lists keep the op's own order, so decoding an encoded
//!   op gives the same op.
//! - Decoding is strict: unknown, missing, and repeated fields, repeated
//!   keys in a set or map, and trailing data are errors.
//! - Decoding is capped (R3, R6): the input size, each record, each
//!   string, and each list are checked while decoding, before any model
//!   validation, and a broken cap is a typed
//!   [`StoreError::LimitExceeded`](crate::StoreError::LimitExceeded). Each
//!   cap is at least the model's own limit, so every valid project
//!   round-trips.

mod op_log;
mod snapshot;
mod wire;

use std::fmt;

pub use op_log::{LoggedOp, OpLogReader, OpLogWriter};

use crate::methodology::{MAX_IMPACT_CATEGORIES, MAX_KEY_LEN, MAX_PROPERTIES};
use crate::threat::{MAX_CATALOG_REFS, MAX_DAMAGE_LINKS};

/// Largest snapshot [`Project::load`](crate::Project::load) reads, in
/// bytes: 64 MiB. A typical project (300 assets, 2,000 threats) is about
/// 1 MB.
///
/// ```
/// assert_eq!(aegis_model::MAX_PROJECT_FILE_BYTES, 64 * 1024 * 1024);
/// ```
pub const MAX_PROJECT_FILE_BYTES: u64 = 64 * 1024 * 1024;

/// Longest op log line, header included and the `\n` not counted, in
/// bytes: 64 KiB. The largest valid op is about 13 KiB (`tests/store_limits.rs`).
///
/// ```
/// assert_eq!(aegis_model::MAX_OP_RECORD_BYTES, 65_536);
/// ```
pub const MAX_OP_RECORD_BYTES: usize = 64 * 1024;

/// Longest decoded entity name, in bytes: 1,024. A name has at most 256
/// chars of up to 4 bytes each; the model then checks the char count.
///
/// ```
/// assert_eq!(aegis_model::MAX_NAME_BYTES, 4 * aegis_model::MAX_NAME_CHARS);
/// ```
pub const MAX_NAME_BYTES: usize = 1024;

/// Longest decoded vocabulary key, in bytes: 64, the longest key a
/// methodology may declare. Also caps the methodology key in a header.
///
/// ```
/// assert_eq!(aegis_model::MAX_KEY_BYTES, 64);
/// ```
pub const MAX_KEY_BYTES: usize = MAX_KEY_LEN;

/// Longest decoded methodology version, in bytes: 256. A methodology
/// version has at most 64 chars of up to 4 bytes each.
///
/// ```
/// assert_eq!(aegis_model::MAX_METHODOLOGY_VERSION_BYTES, 256);
/// ```
pub const MAX_METHODOLOGY_VERSION_BYTES: usize = 256;

/// Longest decoded catalog ref text, in bytes: 64. The longest valid ref
/// is 39 bytes (a 16-byte prefix and 4 segments of 5 digits).
///
/// ```
/// assert_eq!(aegis_model::MAX_CATALOG_REF_BYTES, 64);
/// ```
pub const MAX_CATALOG_REF_BYTES: usize = 64;

/// A decode cap, carried by
/// [`StoreError::LimitExceeded`](crate::StoreError::LimitExceeded). Each
/// variant holds the cap's `max`.
///
/// ```
/// use std::sync::Arc;
/// use aegis_model::{Limit, Methodology, Project, StoreError};
///
/// # fn main() -> Result<(), aegis_model::MethodologyError> {
/// let m = Arc::new(Methodology::iso21434_r155()?);
/// let long_key = "k".repeat(65);
/// let text = format!(
///     r#"{{"format":"aegis-project","version":1,"methodology":{{"key":"{long_key}","version":"1"}}}}"#
/// );
/// let err = Project::load(text.as_bytes(), m).err();
/// assert!(matches!(err, Some(StoreError::LimitExceeded(Limit::KeyBytes { max: 64 }))));
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Limit {
    /// The snapshot is larger than [`MAX_PROJECT_FILE_BYTES`].
    ProjectFileBytes {
        /// The cap, in bytes.
        max: u64,
    },
    /// An op log line is longer than [`MAX_OP_RECORD_BYTES`].
    OpRecordBytes {
        /// The cap, in bytes.
        max: usize,
    },
    /// A name is longer than [`MAX_NAME_BYTES`].
    NameBytes {
        /// The cap, in bytes.
        max: usize,
    },
    /// A vocabulary or methodology key is longer than [`MAX_KEY_BYTES`].
    KeyBytes {
        /// The cap, in bytes.
        max: usize,
    },
    /// A methodology version is longer than
    /// [`MAX_METHODOLOGY_VERSION_BYTES`].
    MethodologyVersionBytes {
        /// The cap, in bytes.
        max: usize,
    },
    /// A catalog ref text is longer than [`MAX_CATALOG_REF_BYTES`].
    CatalogRefBytes {
        /// The cap, in bytes.
        max: usize,
    },
    /// An op or threat lists more than
    /// [`MAX_CATALOG_REFS`](crate::MAX_CATALOG_REFS) catalog refs.
    CatalogRefs {
        /// The cap, in items.
        max: usize,
    },
    /// An op or threat lists more than
    /// [`MAX_DAMAGE_LINKS`](crate::MAX_DAMAGE_LINKS) damage links.
    DamageLinks {
        /// The cap, in items.
        max: usize,
    },
    /// An op or damage scenario rates more than 32 impact categories (the
    /// most a methodology declares).
    Impacts {
        /// The cap, in items.
        max: usize,
    },
    /// A property set has more than 64 members (the most a methodology
    /// declares).
    Properties {
        /// The cap, in items.
        max: usize,
    },
}

impl Limit {
    pub(crate) const PROJECT_FILE: Limit = Limit::ProjectFileBytes {
        max: MAX_PROJECT_FILE_BYTES,
    };
    pub(crate) const OP_RECORD: Limit = Limit::OpRecordBytes {
        max: MAX_OP_RECORD_BYTES,
    };
    pub(crate) const NAME: Limit = Limit::NameBytes {
        max: MAX_NAME_BYTES,
    };
    pub(crate) const KEY: Limit = Limit::KeyBytes { max: MAX_KEY_BYTES };
    pub(crate) const VERSION: Limit = Limit::MethodologyVersionBytes {
        max: MAX_METHODOLOGY_VERSION_BYTES,
    };
    pub(crate) const CATALOG_REF: Limit = Limit::CatalogRefBytes {
        max: MAX_CATALOG_REF_BYTES,
    };
    pub(crate) const CATALOG_REFS: Limit = Limit::CatalogRefs {
        max: MAX_CATALOG_REFS,
    };
    pub(crate) const DAMAGE_LINKS: Limit = Limit::DamageLinks {
        max: MAX_DAMAGE_LINKS,
    };
    pub(crate) const IMPACTS: Limit = Limit::Impacts {
        max: MAX_IMPACT_CATEGORIES,
    };
    pub(crate) const PROPERTIES: Limit = Limit::Properties {
        max: MAX_PROPERTIES,
    };
}

impl fmt::Display for Limit {
    /// Names the cap and its value, never the offending input.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Limit::ProjectFileBytes { max } => write!(f, "project file is larger than {max} bytes"),
            Limit::OpRecordBytes { max } => write!(f, "op log line is longer than {max} bytes"),
            Limit::NameBytes { max } => write!(f, "name is longer than {max} bytes"),
            Limit::KeyBytes { max } => write!(f, "key is longer than {max} bytes"),
            Limit::MethodologyVersionBytes { max } => {
                write!(f, "methodology version is longer than {max} bytes")
            }
            Limit::CatalogRefBytes { max } => {
                write!(f, "catalog ref text is longer than {max} bytes")
            }
            Limit::CatalogRefs { max } => write!(f, "more than {max} catalog refs"),
            Limit::DamageLinks { max } => write!(f, "more than {max} damage links"),
            Limit::Impacts { max } => write!(f, "more than {max} impact ratings"),
            Limit::Properties { max } => write!(f, "more than {max} properties"),
        }
    }
}

/// The vocabulary a key was looked up in, carried by
/// [`StoreError::UnknownKey`](crate::StoreError::UnknownKey).
///
/// ```
/// use aegis_model::VocabKind;
///
/// assert_eq!(VocabKind::ThreatCategory.to_string(), "threat category");
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum VocabKind {
    /// An impact category.
    ImpactCategory,
    /// A level of an impact category's scale.
    Level,
    /// A security property.
    Property,
    /// A threat category.
    ThreatCategory,
    /// A catalog scheme.
    CatalogScheme,
}

impl fmt::Display for VocabKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            VocabKind::ImpactCategory => "impact category",
            VocabKind::Level => "level",
            VocabKind::Property => "property",
            VocabKind::ThreatCategory => "threat category",
            VocabKind::CatalogScheme => "catalog scheme",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limit_display_names_the_cap() {
        assert_eq!(
            Limit::PROJECT_FILE.to_string(),
            "project file is larger than 67108864 bytes"
        );
        assert_eq!(
            Limit::DAMAGE_LINKS.to_string(),
            "more than 256 damage links"
        );
        for l in [
            Limit::OP_RECORD,
            Limit::NAME,
            Limit::KEY,
            Limit::VERSION,
            Limit::CATALOG_REF,
            Limit::CATALOG_REFS,
            Limit::IMPACTS,
            Limit::PROPERTIES,
        ] {
            assert_ne!(l.to_string(), "");
        }
    }

    #[test]
    fn vocab_kind_display() {
        let all = [
            VocabKind::ImpactCategory,
            VocabKind::Level,
            VocabKind::Property,
            VocabKind::ThreatCategory,
            VocabKind::CatalogScheme,
        ];
        let names: Vec<_> = all.iter().map(ToString::to_string).collect();
        assert_eq!(
            names,
            [
                "impact category",
                "level",
                "property",
                "threat category",
                "catalog scheme"
            ]
        );
    }
}
