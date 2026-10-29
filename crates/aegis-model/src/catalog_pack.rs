//! Catalog metadata packs: original catalog summaries and the source terms
//! that govern any future redistribution of catalog records.
//!
//! These built-ins contain no CAPEC, ATT&CK, or EMB3D dataset records. They
//! carry only catalog-level names, original descriptions, links, and the
//! verified attribution and license terms.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Largest catalog metadata pack accepted by the reader, in bytes.
pub const MAX_CATALOG_PACK_BYTES: usize = 16 * 1024;

const FORMAT: &str = "aegis-catalog-pack";
const VERSION: u32 = 1;
const CAPEC: &str = include_str!("../packs/catalogs/capec.json");
const ATTACK: &str = include_str!("../packs/catalogs/mitre-attack.json");
const EMB3D: &str = include_str!("../packs/catalogs/emb3d.json");

/// Why a catalog metadata pack was not accepted.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum CatalogPackError {
    /// The input is larger than [`MAX_CATALOG_PACK_BYTES`].
    #[error("the catalog pack is too large")]
    TooLarge,
    /// The input is not valid JSON of the expected shape.
    #[error("the catalog pack is not valid: {0}")]
    Decode(String),
    /// The format name or version is unsupported.
    #[error("this is not a supported aegis-catalog-pack")]
    Unsupported,
    /// A field is missing, too long, or violates a content rule.
    #[error("the catalog pack breaks a rule: {0}")]
    Invalid(&'static str),
    /// A pack could not be serialized.
    #[error("the catalog pack could not be serialized: {0}")]
    Encode(String),
}

/// Catalog-level metadata. This is not a record copied from the source
/// catalog; `description` is an original summary written for `AegisTARA`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogEntry {
    key: String,
    name: String,
    description: String,
    source_url: String,
    terms_url: String,
    attribution: String,
    license_terms: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    conditions: String,
}

impl CatalogEntry {
    /// Stable identifier for this catalog entry.
    #[must_use]
    pub fn key(&self) -> &str {
        &self.key
    }

    /// Catalog name, with the source's required trademark treatment.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Original, short description of the catalog.
    #[must_use]
    pub fn description(&self) -> &str {
        &self.description
    }

    /// Official catalog homepage.
    #[must_use]
    pub fn source_url(&self) -> &str {
        &self.source_url
    }

    /// Official page stating the source's license terms.
    #[must_use]
    pub fn terms_url(&self) -> &str {
        &self.terms_url
    }

    /// Attribution statement required by the source's terms.
    #[must_use]
    pub fn attribution(&self) -> &str {
        &self.attribution
    }

    /// The source's license grant, retained verbatim for attribution and review.
    #[must_use]
    pub fn license_terms(&self) -> &str {
        &self.license_terms
    }

    /// Additional source conditions, if any.
    #[must_use]
    pub fn conditions(&self) -> &str {
        &self.conditions
    }
}

/// A bounded, validated pack describing one external catalog.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CatalogPack {
    id: String,
    title: String,
    pack_version: String,
    license: String,
    catalog: CatalogEntry,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CatalogPackFile {
    format: String,
    version: u32,
    id: String,
    title: String,
    pack_version: String,
    license: String,
    metadata_only: bool,
    catalog: CatalogEntry,
}

impl CatalogPack {
    /// Reads and validates one catalog metadata pack.
    ///
    /// # Errors
    ///
    /// Returns [`CatalogPackError`] for oversized, malformed, unsupported,
    /// or invalid input.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, CatalogPackError> {
        if bytes.len() > MAX_CATALOG_PACK_BYTES {
            return Err(CatalogPackError::TooLarge);
        }
        let file: CatalogPackFile = serde_json::from_slice(bytes).map_err(|error| {
            CatalogPackError::Decode(format!(
                "{:?} at line {}, column {}",
                error.classify(),
                error.line(),
                error.column()
            ))
        })?;
        if file.format != FORMAT || file.version != VERSION {
            return Err(CatalogPackError::Unsupported);
        }
        if !file.metadata_only {
            return Err(CatalogPackError::Invalid(
                "built-in pack must be metadata-only",
            ));
        }
        check_text(&file.id, 64, "pack id")?;
        check_text(&file.title, 256, "pack title")?;
        check_text(&file.pack_version, 64, "pack version")?;
        check_text(&file.license, 128, "license name")?;
        check_text(&file.catalog.key, 64, "catalog key")?;
        check_text(&file.catalog.name, 256, "catalog name")?;
        check_text(&file.catalog.description, 1024, "catalog description")?;
        check_text(&file.catalog.source_url, 512, "catalog source URL")?;
        check_text(&file.catalog.terms_url, 512, "catalog terms URL")?;
        check_text(&file.catalog.attribution, 1024, "attribution")?;
        check_text(&file.catalog.license_terms, 2048, "license terms")?;
        check_optional_text(&file.catalog.conditions, 2048, "additional conditions")?;
        if !file.catalog.source_url.starts_with("https://")
            || !file.catalog.terms_url.starts_with("https://")
        {
            return Err(CatalogPackError::Invalid("catalog URLs must use HTTPS"));
        }
        Ok(Self {
            id: file.id,
            title: file.title,
            pack_version: file.pack_version,
            license: file.license,
            catalog: file.catalog,
        })
    }

    /// Returns the built-in CAPEC metadata pack.
    ///
    /// # Panics
    ///
    /// Panics if the embedded pack bytes are invalid; the checked-in file is
    /// validated by `built_in_catalog_packs_load_from_the_checked_in_files`.
    #[must_use]
    pub fn builtin_capec() -> Self {
        Self::from_bytes(CAPEC.as_bytes()).expect("the built-in CAPEC metadata pack is valid")
    }

    /// Returns the canonical source text for the built-in CAPEC metadata pack.
    #[must_use]
    pub const fn builtin_capec_pack() -> &'static str {
        CAPEC
    }

    /// Returns the built-in MITRE ATT&CK metadata pack.
    ///
    /// # Panics
    ///
    /// Panics if the embedded pack bytes are invalid; the checked-in file is
    /// validated by `built_in_catalog_packs_load_from_the_checked_in_files`.
    #[must_use]
    pub fn builtin_mitre_attack() -> Self {
        Self::from_bytes(ATTACK.as_bytes()).expect("the built-in ATT&CK metadata pack is valid")
    }

    /// Returns the canonical source text for the built-in MITRE ATT&CK metadata pack.
    #[must_use]
    pub const fn builtin_mitre_attack_pack() -> &'static str {
        ATTACK
    }

    /// Returns the built-in EMB3D metadata pack.
    ///
    /// # Panics
    ///
    /// Panics if the embedded pack bytes are invalid; the checked-in file is
    /// validated by `built_in_catalog_packs_load_from_the_checked_in_files`.
    #[must_use]
    pub fn builtin_emb3d() -> Self {
        Self::from_bytes(EMB3D.as_bytes()).expect("the built-in EMB3D metadata pack is valid")
    }

    /// Returns the canonical source text for the built-in EMB3D metadata pack.
    #[must_use]
    pub const fn builtin_emb3d_pack() -> &'static str {
        EMB3D
    }

    /// The stable pack identifier.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Human-readable pack title.
    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Version of this metadata pack.
    #[must_use]
    pub fn pack_version(&self) -> &str {
        &self.pack_version
    }

    /// Name of the source terms governing the named catalog.
    #[must_use]
    pub fn license(&self) -> &str {
        &self.license
    }

    /// The one catalog-level metadata entry in this pack.
    #[must_use]
    pub fn catalog(&self) -> &CatalogEntry {
        &self.catalog
    }

    /// Whether this pack contains metadata only, not source-catalog records.
    #[must_use]
    pub const fn metadata_only(&self) -> bool {
        true
    }

    /// Serializes this pack in deterministic field order.
    ///
    /// # Errors
    ///
    /// Returns [`CatalogPackError::Encode`] if JSON serialization fails.
    pub fn to_bytes(&self) -> Result<Vec<u8>, CatalogPackError> {
        serde_json::to_vec(&CatalogPackFile {
            format: FORMAT.to_owned(),
            version: VERSION,
            id: self.id.clone(),
            title: self.title.clone(),
            pack_version: self.pack_version.clone(),
            license: self.license.clone(),
            metadata_only: true,
            catalog: self.catalog.clone(),
        })
        .map_err(|error| CatalogPackError::Encode(error.to_string()))
    }
}

fn check_text(value: &str, max: usize, field: &'static str) -> Result<(), CatalogPackError> {
    if value.trim().is_empty() {
        return Err(CatalogPackError::Invalid(field));
    }
    if value.len() > max {
        return Err(CatalogPackError::Invalid(field));
    }
    Ok(())
}

fn check_optional_text(
    value: &str,
    max: usize,
    field: &'static str,
) -> Result<(), CatalogPackError> {
    if value.len() > max {
        return Err(CatalogPackError::Invalid(field));
    }
    Ok(())
}
