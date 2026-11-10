//! Saving and loading: a project snapshot and an append-only op log.
//!
//! Two JSON formats, both versioned and both written with stable vocabulary
//! keys, never with in-memory indices (see
//! `docs/adr/0009-save-load-format.md`):
//!
//! - **Snapshot**, format `aegis-project` version 1: one compact JSON
//!   object on one line, ending in `\n`. It holds the methodology key and
//!   version, then the live assets, damage scenarios, threat scenarios,
//!   and attack trees in public-ID order. Written by
//!   [`Project::save`](crate::Project::save), read by
//!   [`Project::load`](crate::Project::load).
//! - **Op log**, format `aegis-oplog` version 1: JSON Lines. One header
//!   line with the methodology key and version, then one record
//!   `{"seq":N,"op":{...}}` per line, each ending in `\n`, with `seq`
//!   counting 1, 2, 3, ... Written by [`OpLogWriter`], read by
//!   [`OpLogReader`].
//!
//! Rules both formats share:
//!
//! - The only JSON numbers are `version`, `seq`, and provenance
//!   `timestamp_ms`. IDs are hyphenated lowercase `UUIDv7` text. Vocabulary
//!   items are their keys. A catalog ref is `{"scheme": <key>, "id": <text
//!   in the scheme's syntax>}`.
//! - Output is canonical and does not depend on vocabulary order: property
//!   sets are sorted by key; snapshot impacts are an object sorted by
//!   category key with unrated categories left out; snapshot catalog refs
//!   are sorted by scheme key, then by segments; damage links are sorted by
//!   ID; attack tree nodes and step feasibility ratings are in arena order;
//!   default provenance (`("", 0, Human)`) is omitted. In an op, lists keep
//!   the op's own order, so decoding an encoded op gives the same op.
//! - Decoding is strict: unknown, missing, and repeated fields, repeated
//!   keys in a set or map, and trailing data are errors.
//! - Decoding is capped (R3, R6): the input size, each record, each
//!   string, and each list are checked while decoding, before any model
//!   validation, and a broken cap is a typed
//!   [`StoreError::LimitExceeded`](crate::StoreError::LimitExceeded). Each
//!   cap is at least the model's own limit, so every valid project
//!   round-trips.
//! - Keys are resolved while decoding: each vocabulary key becomes its
//!   1-byte index as it is read, so decoded data never holds key strings
//!   and an unknown key fails inside the entity that holds it. Within one
//!   list or impact map, a cap or a repeated key is reported at once and
//!   the first unknown key when the list ends.

mod op_log;
mod snapshot;
pub(crate) mod wire;
mod workspace;

use std::fmt;

pub use op_log::{LoggedOp, OpCodec, OpLogReader, OpLogWriter};
pub use workspace::WorkspaceFile;

use crate::control::{
    MAX_CONTROL_RULES, MAX_CONTROL_TARGETS, MAX_GOAL_CONTROLS, MAX_STEP_MITIGATIONS,
};
use crate::item::MAX_TRUST_BOUNDARY_MEMBERS;
use crate::methodology::{
    MAX_ATTACK_TYPES, MAX_CATALOG_SCHEMES, MAX_FEASIBILITY_FACTORS, MAX_IMPACT_CATEGORIES,
    MAX_INTERFACE_KINDS, MAX_KEY_LEN, MAX_LABEL_CHARS, MAX_LEVELS, MAX_PACK_TRANSLATIONS,
    MAX_PREFIX_LEN, MAX_PROPERTIES, MAX_SEGMENTS, MAX_THREAT_CATEGORIES,
};
use crate::threat::{MAX_CATALOG_REFS, MAX_DAMAGE_LINKS};
use crate::tree::MAX_ATTACK_TREE_NODES;

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

/// Longest decoded provenance actor, in bytes: 1,024. An actor has at most
/// 256 chars of up to 4 bytes each; the model then checks the char count.
///
/// ```
/// assert_eq!(aegis_model::MAX_ACTOR_BYTES, 4 * aegis_model::MAX_NAME_CHARS);
/// ```
pub const MAX_ACTOR_BYTES: usize = 1024;

/// Longest description or notes text accepted while decoding, in bytes: four
/// bytes for each of [`MAX_NOTES_CHARS`](crate::MAX_NOTES_CHARS).
///
/// ```
/// assert_eq!(aegis_model::MAX_DESCRIPTION_BYTES, 4 * aegis_model::MAX_NOTES_CHARS);
/// ```
pub const MAX_DESCRIPTION_BYTES: usize = 4 * crate::description::MAX_NOTES_CHARS;

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

/// Largest methodology pack file
/// [`MethodologyDef::read_pack`](crate::MethodologyDef::read_pack) reads, in
/// bytes: 4 MiB. The largest pack that
/// [`Methodology::new`](crate::Methodology::new) accepts is about 1.4 MB
/// (`docs/adr/0011-methodology-pack-files.md`); the built-in pack is about
/// 3 KB.
///
/// ```
/// assert_eq!(aegis_model::MAX_PACK_FILE_BYTES, 4 * 1024 * 1024);
/// ```
pub const MAX_PACK_FILE_BYTES: u64 = 4 * 1024 * 1024;

/// Longest decoded methodology label, in bytes: 1,024. A label has at most
/// 256 chars of up to 4 bytes each; [`Methodology::new`](crate::Methodology::new)
/// then checks the char count.
///
/// ```
/// assert_eq!(aegis_model::MAX_LABEL_BYTES, 1024);
/// ```
pub const MAX_LABEL_BYTES: usize = 4 * MAX_LABEL_CHARS;

/// A decode cap, carried by both
/// [`StoreError::LimitExceeded`](crate::StoreError::LimitExceeded) (project
/// snapshots and op logs) and
/// [`PackError::LimitExceeded`](crate::PackError::LimitExceeded)
/// (methodology packs). Each variant holds the cap's `max`. Its `Display`
/// names the cap and its value, never the input.
///
/// ```
/// use std::sync::Arc;
/// use aegis_model::{Limit, Methodology, Project, StoreError};
///
/// # fn main() -> Result<(), aegis_model::PackError> {
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
    /// A description or notes text is longer than [`MAX_DESCRIPTION_BYTES`].
    DescriptionBytes {
        /// The cap, in bytes.
        max: usize,
    },
    /// A translation language tag is longer than
    /// [`MAX_LANG_TAG_BYTES`](crate::MAX_LANG_TAG_BYTES).
    LangBytes {
        /// The cap, in bytes.
        max: usize,
    },
    /// A provenance actor is longer than [`MAX_ACTOR_BYTES`].
    ActorBytes {
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
    /// A property set, a pack's property list, or a pack threat category's
    /// `violates` list has more than 64 members (the most a methodology
    /// declares).
    Properties {
        /// The cap, in items.
        max: usize,
    },
    /// An attack tree, gate child list, or shared-node parent list has more
    /// than [`MAX_ATTACK_TREE_NODES`](crate::MAX_ATTACK_TREE_NODES) nodes.
    AttackTreeNodes {
        /// The cap, in items.
        max: usize,
    },
    /// An attack tree or op lists more than
    /// [`MAX_ATTACK_TREE_NODES`](crate::MAX_ATTACK_TREE_NODES) step
    /// feasibility ratings.
    StepFeasibility {
        /// The cap, in items.
        max: usize,
    },
    /// An attack step lists more than
    /// [`MAX_STEP_MITIGATIONS`](crate::MAX_STEP_MITIGATIONS) security
    /// control mitigations.
    StepMitigations {
        /// The cap, in items.
        max: usize,
    },
    /// A cybersecurity goal or op lists more than
    /// [`MAX_GOAL_CONTROLS`](crate::MAX_GOAL_CONTROLS) security controls.
    GoalControls {
        /// The cap, in items.
        max: usize,
    },
    /// A security control lists more than
    /// [`MAX_CONTROL_TARGETS`](crate::MAX_CONTROL_TARGETS) applied targets.
    ControlTargets {
        /// The cap, in items.
        max: usize,
    },
    /// A security control lists more than
    /// [`MAX_CONTROL_RULES`](crate::MAX_CONTROL_RULES) rules.
    ControlRules {
        /// The cap, in items.
        max: usize,
    },
    /// A trust boundary or op lists more than
    /// [`MAX_TRUST_BOUNDARY_MEMBERS`](crate::MAX_TRUST_BOUNDARY_MEMBERS) components.
    TrustBoundaryMembers {
        /// The cap, in items.
        max: usize,
    },
    /// A methodology pack is larger than [`MAX_PACK_FILE_BYTES`].
    PackFileBytes {
        /// The cap, in bytes.
        max: u64,
    },
    /// A pack label is longer than [`MAX_LABEL_BYTES`].
    LabelBytes {
        /// The cap, in bytes.
        max: usize,
    },
    /// A pack catalog prefix is longer than 16 bytes (the longest prefix a
    /// methodology may declare).
    CatalogPrefixBytes {
        /// The cap, in bytes.
        max: usize,
    },
    /// A pack declares more than 32 impact categories.
    ImpactCategories {
        /// The cap, in items.
        max: usize,
    },
    /// A pack impact category, feasibility scale, risk scale, or risk matrix
    /// row or column has more than 16 levels.
    Levels {
        /// The cap, in items.
        max: usize,
    },
    /// A pack declares more than 64 threat categories.
    ThreatCategories {
        /// The cap, in items.
        max: usize,
    },
    /// A pack declares more than 16 catalog schemes.
    CatalogSchemes {
        /// The cap, in items.
        max: usize,
    },
    /// A pack catalog syntax has more than 4 segments.
    CatalogSegments {
        /// The cap, in items.
        max: usize,
    },
    /// A pack `attack_potential.factors` list or a snapshot's per-step
    /// factor map has more than
    /// [`MAX_FEASIBILITY_FACTORS`](crate::MAX_FEASIBILITY_FACTORS) factors.
    FeasibilityFactors {
        /// The cap, in items.
        max: usize,
    },
    /// A pack declares more than [`MAX_INTERFACE_KINDS`](crate::MAX_INTERFACE_KINDS)
    /// interface kinds.
    InterfaceKinds {
        /// The cap, in items.
        max: usize,
    },
    /// A pack declares more than [`MAX_ATTACK_TYPES`](crate::MAX_ATTACK_TYPES)
    /// attack types.
    AttackTypes {
        /// The cap, in items.
        max: usize,
    },
    /// A pack declares more than [`MAX_PACK_TRANSLATIONS`](crate::MAX_PACK_TRANSLATIONS)
    /// language translations.
    PackTranslations {
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
    pub(crate) const DESCRIPTION: Limit = Limit::DescriptionBytes {
        max: MAX_DESCRIPTION_BYTES,
    };
    pub(crate) const LANG: Limit = Limit::LangBytes {
        max: crate::description::MAX_LANG_TAG_BYTES,
    };
    pub(crate) const ACTOR: Limit = Limit::ActorBytes {
        max: MAX_ACTOR_BYTES,
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
    pub(crate) const ATTACK_TREE_NODES: Limit = Limit::AttackTreeNodes {
        max: MAX_ATTACK_TREE_NODES,
    };
    pub(crate) const STEP_FEASIBILITY: Limit = Limit::StepFeasibility {
        max: MAX_ATTACK_TREE_NODES,
    };
    pub(crate) const STEP_MITIGATIONS: Limit = Limit::StepMitigations {
        max: MAX_STEP_MITIGATIONS,
    };
    pub(crate) const GOAL_CONTROLS: Limit = Limit::GoalControls {
        max: MAX_GOAL_CONTROLS,
    };
    pub(crate) const CONTROL_TARGETS: Limit = Limit::ControlTargets {
        max: MAX_CONTROL_TARGETS,
    };
    pub(crate) const CONTROL_RULES: Limit = Limit::ControlRules {
        max: MAX_CONTROL_RULES,
    };
    pub(crate) const TRUST_BOUNDARY_MEMBERS: Limit = Limit::TrustBoundaryMembers {
        max: MAX_TRUST_BOUNDARY_MEMBERS,
    };
    pub(crate) const PACK_FILE: Limit = Limit::PackFileBytes {
        max: MAX_PACK_FILE_BYTES,
    };
    pub(crate) const LABEL: Limit = Limit::LabelBytes {
        max: MAX_LABEL_BYTES,
    };
    pub(crate) const CATALOG_PREFIX: Limit = Limit::CatalogPrefixBytes {
        max: MAX_PREFIX_LEN,
    };
    pub(crate) const IMPACT_CATEGORIES: Limit = Limit::ImpactCategories {
        max: MAX_IMPACT_CATEGORIES,
    };
    pub(crate) const LEVELS: Limit = Limit::Levels { max: MAX_LEVELS };
    pub(crate) const THREAT_CATEGORIES: Limit = Limit::ThreatCategories {
        max: MAX_THREAT_CATEGORIES,
    };
    pub(crate) const CATALOG_SCHEMES: Limit = Limit::CatalogSchemes {
        max: MAX_CATALOG_SCHEMES,
    };
    pub(crate) const CATALOG_SEGMENTS: Limit = Limit::CatalogSegments { max: MAX_SEGMENTS };
    pub(crate) const FEASIBILITY_FACTORS: Limit = Limit::FeasibilityFactors {
        max: MAX_FEASIBILITY_FACTORS,
    };
    pub(crate) const INTERFACE_KINDS: Limit = Limit::InterfaceKinds {
        max: MAX_INTERFACE_KINDS,
    };
    pub(crate) const ATTACK_TYPES: Limit = Limit::AttackTypes {
        max: MAX_ATTACK_TYPES,
    };
    pub(crate) const PACK_TRANSLATIONS: Limit = Limit::PackTranslations {
        max: MAX_PACK_TRANSLATIONS,
    };
}

impl fmt::Display for Limit {
    /// Names the cap and its value, never the offending input.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Limit::ProjectFileBytes { max } => write!(f, "project file is larger than {max} bytes"),
            Limit::OpRecordBytes { max } => write!(f, "op log line is longer than {max} bytes"),
            Limit::NameBytes { max } => write!(f, "name is longer than {max} bytes"),
            Limit::DescriptionBytes { max } => {
                write!(f, "description is longer than {max} bytes")
            }
            Limit::LangBytes { max } => write!(f, "language tag is longer than {max} bytes"),
            Limit::ActorBytes { max } => write!(f, "provenance actor is longer than {max} bytes"),
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
            Limit::AttackTreeNodes { max } => write!(f, "more than {max} attack tree nodes"),
            Limit::StepFeasibility { max } => {
                write!(f, "more than {max} step feasibility ratings")
            }
            Limit::StepMitigations { max } => {
                write!(f, "more than {max} step mitigations")
            }
            Limit::GoalControls { max } => {
                write!(f, "more than {max} goal controls")
            }
            Limit::ControlTargets { max } => {
                write!(f, "more than {max} control targets")
            }
            Limit::ControlRules { max } => {
                write!(f, "more than {max} control rules")
            }
            Limit::TrustBoundaryMembers { max } => {
                write!(f, "more than {max} trust boundary members")
            }
            Limit::PackFileBytes { max } => {
                write!(f, "methodology pack is larger than {max} bytes")
            }
            Limit::LabelBytes { max } => write!(f, "label is longer than {max} bytes"),
            Limit::CatalogPrefixBytes { max } => {
                write!(f, "catalog prefix is longer than {max} bytes")
            }
            Limit::ImpactCategories { max } => write!(f, "more than {max} impact categories"),
            Limit::Levels { max } => write!(f, "more than {max} levels in one impact category"),
            Limit::ThreatCategories { max } => write!(f, "more than {max} threat categories"),
            Limit::CatalogSchemes { max } => write!(f, "more than {max} catalog schemes"),
            Limit::CatalogSegments { max } => write!(f, "more than {max} catalog ID segments"),
            Limit::FeasibilityFactors { max } => {
                write!(f, "more than {max} feasibility factors")
            }
            Limit::InterfaceKinds { max } => {
                write!(f, "more than {max} interface kinds")
            }
            Limit::AttackTypes { max } => {
                write!(f, "more than {max} attack types")
            }
            Limit::PackTranslations { max } => {
                write!(f, "more than {max} pack translations")
            }
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
    /// A level of the methodology's feasibility scale.
    FeasibilityLevel,
    /// An attack potential factor.
    FeasibilityFactor,
    /// A level of an attack potential factor's scale.
    FactorLevel,
    /// A security property.
    Property,
    /// A threat category.
    ThreatCategory,
    /// A catalog scheme.
    CatalogScheme,
    /// An item definition interface kind.
    InterfaceKind,
    /// An attack step attack type.
    AttackType,
}

impl fmt::Display for VocabKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            VocabKind::ImpactCategory => "impact category",
            VocabKind::Level => "level",
            VocabKind::FeasibilityLevel => "feasibility level",
            VocabKind::FeasibilityFactor => "feasibility factor",
            VocabKind::FactorLevel => "factor level",
            VocabKind::Property => "property",
            VocabKind::ThreatCategory => "threat category",
            VocabKind::CatalogScheme => "catalog scheme",
            VocabKind::InterfaceKind => "interface kind",
            VocabKind::AttackType => "attack type",
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
            Limit::ACTOR,
            Limit::KEY,
            Limit::VERSION,
            Limit::CATALOG_REF,
            Limit::CATALOG_REFS,
            Limit::IMPACTS,
            Limit::PROPERTIES,
            Limit::ATTACK_TREE_NODES,
            Limit::STEP_FEASIBILITY,
            Limit::STEP_MITIGATIONS,
            Limit::GOAL_CONTROLS,
            Limit::CONTROL_TARGETS,
            Limit::CONTROL_RULES,
        ] {
            assert_ne!(l.to_string(), "");
        }
    }

    #[test]
    fn pack_limit_display_names_the_cap() {
        let shown: Vec<_> = [
            Limit::PACK_FILE,
            Limit::LABEL,
            Limit::CATALOG_PREFIX,
            Limit::IMPACT_CATEGORIES,
            Limit::LEVELS,
            Limit::THREAT_CATEGORIES,
            Limit::CATALOG_SCHEMES,
            Limit::CATALOG_SEGMENTS,
            Limit::FEASIBILITY_FACTORS,
        ]
        .iter()
        .map(ToString::to_string)
        .collect();
        assert_eq!(
            shown,
            [
                "methodology pack is larger than 4194304 bytes",
                "label is longer than 1024 bytes",
                "catalog prefix is longer than 16 bytes",
                "more than 32 impact categories",
                "more than 16 levels in one impact category",
                "more than 64 threat categories",
                "more than 16 catalog schemes",
                "more than 4 catalog ID segments",
                "more than 16 feasibility factors",
            ]
        );
        assert_eq!(MAX_LABEL_BYTES, 1024);
    }

    #[test]
    fn vocab_kind_display() {
        let all = [
            VocabKind::ImpactCategory,
            VocabKind::Level,
            VocabKind::FeasibilityLevel,
            VocabKind::FeasibilityFactor,
            VocabKind::FactorLevel,
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
                "feasibility level",
                "feasibility factor",
                "factor level",
                "property",
                "threat category",
                "catalog scheme"
            ]
        );
    }
}
