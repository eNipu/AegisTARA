//! Report profiles (RPT-02 to RPT-04): the user picks sections, filters,
//! language and branding before an export, and the choice is saved per
//! project or company and recorded in the export audit event.
//!
//! Profiles are plain data (serde JSON) so they persist in the same settings
//! store as every other client preference and can travel with a project.

use std::sync::Arc;

use aegis_model::Project;

use crate::i18n::{Catalog, Language};

/// Largest logo kept in a profile, in bytes (R3).
pub const MAX_LOGO_BYTES: usize = 1024 * 1024;

/// The report language: one language or both side by side.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReportLanguage {
    /// English only.
    #[default]
    English,
    /// Japanese only.
    Japanese,
    /// English and Japanese (L10N-08).
    Bilingual,
}

impl ReportLanguage {
    /// The languages this choice writes, in order.
    #[must_use]
    pub fn languages(self) -> Vec<Language> {
        match self {
            ReportLanguage::English => vec![Language::En],
            ReportLanguage::Japanese => vec![Language::Ja],
            ReportLanguage::Bilingual => vec![Language::En, Language::Ja],
        }
    }
}

/// The classification mark printed on every page (RPT-04). The default is
/// `Confidential`: a TARA is security analysis, and the pre-report watermarks
/// already said "Confidential" (job 050).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Classification {
    /// No mark.
    None,
    /// "Internal".
    Internal,
    /// "Confidential".
    #[default]
    Confidential,
    /// "Secret".
    Secret,
}

impl Classification {
    /// The message id of the label.
    #[must_use]
    pub fn label_id(self) -> &'static str {
        match self {
            Classification::None => "classification-none",
            Classification::Internal => "classification-internal",
            Classification::Confidential => "classification-confidential",
            Classification::Secret => "classification-secret",
        }
    }

    /// The localized label.
    #[must_use]
    pub fn label(self, catalog: &Catalog) -> String {
        catalog.tr(self.label_id())
    }
}

/// One row of the revision history on the cover page.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RevisionEntry {
    /// The revision tag, for example `1.0` or `A`.
    pub revision: String,
    /// The date, as the user wrote it.
    pub date: String,
    /// What changed.
    pub description: String,
}

/// One attached figure (PNG, JPEG, or sanitized static SVG) on the project or
/// an element (job 073, ARC-01, RPT-06).
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct FigureAttachment {
    /// Stable UUIDv7 figure identifier.
    pub figure_id: String,
    /// Media type (`image/png`, `image/jpeg`, or `image/svg+xml`).
    pub media_type: String,
    /// Caption text.
    pub caption: String,
    /// Target string (`""` / `"project"` or `<kind>:<uuidv7>`).
    pub target: String,
    /// Width in pixels.
    pub width_px: u32,
    /// Height in pixels.
    pub height_px: u32,
    /// Size in bytes.
    pub size_bytes: u64,
    /// Raw image bytes (empty when only metadata was listed).
    pub data: Vec<u8>,
    /// Author of the last change.
    pub updated_by: String,
    /// Timestamp of the last change (ms since epoch).
    pub updated_at_ms: i64,
}

/// Which sections the report has, in fixed order (RPT-02).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ReportSections {
    /// Cover page: title, document ID, revision history, approvers, logo.
    pub cover: bool,
    /// Project notes.
    pub notes: bool,
    /// Executive summary.
    pub executive_summary: bool,
    /// Item definition: components, interfaces, data flows, trust boundaries (job 074).
    pub item_definition: bool,
    /// Attached figures (job 073).
    pub figures: bool,
    /// Assets.
    pub assets: bool,
    /// Damage scenarios.
    pub damage: bool,
    /// Threat scenarios.
    pub threats: bool,
    /// Attack trees.
    pub attack_trees: bool,
    /// Risk register and treatment.
    pub risks: bool,
    /// Security controls.
    pub controls: bool,
    /// Goals and claims.
    pub goals_claims: bool,
    /// Open problems.
    pub problems: bool,
}

impl Default for ReportSections {
    fn default() -> Self {
        Self {
            cover: true,
            notes: true,
            executive_summary: true,
            item_definition: true,
            figures: true,
            assets: true,
            damage: true,
            threats: true,
            attack_trees: true,
            risks: true,
            controls: true,
            goals_claims: true,
            problems: true,
        }
    }
}

/// Which content is filtered out (RPT-02). A threat matches a filter when
/// its asset is selected (or the asset list is empty) and its residual risk
/// level is selected (or the risk list is empty).
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ReportFilters {
    /// Residual risk level labels to keep; empty keeps every level.
    pub risk_levels: Vec<String>,
    /// Asset names to keep; empty keeps every asset.
    pub assets: Vec<String>,
    /// A component name. Components do not exist yet (job 074); the value is
    /// kept in the profile so it is ready when they do.
    pub component: Option<String>,
    /// A baseline name to compare against; the report adds a comparison
    /// section when a baseline project is supplied with it.
    pub baseline: Option<String>,
}

/// Branding on the cover and every page (RPT-04).
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ReportBranding {
    /// The document ID, for example `TARA-2026-0042`.
    pub document_id: String,
    /// The classification mark.
    pub classification: Classification,
    /// Approver names, in order.
    pub approvers: Vec<String>,
    /// Revision history, oldest first.
    pub revisions: Vec<RevisionEntry>,
    /// Logo bytes (PNG, JPEG or sanitized SVG), capped at [`MAX_LOGO_BYTES`].
    pub logo: Option<Vec<u8>>,
}

/// A saved report profile.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ReportProfile {
    /// The profile name shown in the list and the audit event.
    pub name: String,
    /// Report language.
    pub language: ReportLanguage,
    /// Sections.
    pub sections: ReportSections,
    /// Filters.
    pub filters: ReportFilters,
    /// Branding.
    pub branding: ReportBranding,
    /// Project and element figures attached to the project when exporting
    /// (job 073, RPT-06).
    #[serde(skip)]
    pub figures: Vec<FigureAttachment>,
}

impl Default for ReportProfile {
    fn default() -> Self {
        Self {
            name: "Default".to_owned(),
            language: ReportLanguage::default(),
            sections: ReportSections::default(),
            filters: ReportFilters::default(),
            branding: ReportBranding::default(),
            figures: Vec::new(),
        }
    }
}

impl ReportProfile {
    /// A short machine-readable summary for the export audit event, for
    /// example `profile=Submission lang=ja`. Bounded and printable only.
    #[must_use]
    pub fn audit_summary(&self) -> String {
        let lang = match self.language {
            ReportLanguage::English => "en",
            ReportLanguage::Japanese => "ja",
            ReportLanguage::Bilingual => "en+ja",
        };
        let name: String = self
            .name
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '-'
                }
            })
            .collect();
        let name = if name.trim_matches('-').is_empty() {
            "default"
        } else {
            name.trim_matches('-')
        };
        let mut parts = format!("profile={name} lang={lang}");
        if !self.filters.risk_levels.is_empty() {
            parts.push_str(" risk=1");
        }
        if !self.filters.assets.is_empty() {
            parts.push_str(" asset=1");
        }
        if self
            .filters
            .component
            .as_ref()
            .is_some_and(|c| !c.trim().is_empty())
        {
            parts.push_str(" component=1");
        }
        if let Some(baseline) = &self.filters.baseline {
            parts.push_str(" baseline=1");
            let _ = baseline;
        }
        parts
    }
}

/// Everything the report builder needs beyond the workspace: the profile and
/// an optional baseline project for the comparison section.
#[derive(Clone, Debug, Default)]
pub struct ReportOptions {
    /// The profile.
    pub profile: ReportProfile,
    /// A baseline snapshot to compare against, when the profile asks for one.
    pub baseline_project: Option<Arc<Project>>,
}

impl ReportOptions {
    /// The report language to write (the first of a bilingual pair; bilingual
    /// reports write both languages in one document, see `report::doc`).
    #[must_use]
    pub fn languages(&self) -> Vec<Language> {
        self.profile.language.languages()
    }
}
