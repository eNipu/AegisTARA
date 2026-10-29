//! Built-in EU Cyber Resilience Act (CRA) product-security methodology pack
//! (DOM-03, Decision D24).
//!
//! Written from public sources: Regulation (EU) 2024/2847 of the European
//! Parliament and of the Council (Cyber Resilience Act, Official Journal of
//! the European Union) Article 13 and Annex I Parts I and II. No copyrighted
//! standard text is included; the impact categories, plain likelihood scale,
//! and risk matrix are editable examples.

use super::Methodology;
use crate::error::PackError;

/// The canonical JSON text of the built-in `"eu-cra"` pack, embedded from
/// `packs/eu-cra.json` at compile time.
const PACK: &[u8] = include_bytes!("../../packs/eu-cra.json");

impl Methodology {
    /// Returns the built-in `"eu-cra"` methodology pack for products with
    /// digital elements under the EU Cyber Resilience Act (DOM-03).
    ///
    /// Parses `packs/eu-cra.json` through [`Methodology::read_pack`], running
    /// every decode cap and validation rule. Uses pack-defined element names
    /// (`"Harm"` / `"Harms"` for damage scenarios, `"Security requirement"` /
    /// `"Security requirements"` for cybersecurity goals, and `"Likelihood"`
    /// for the feasibility scale) and a plain 4-level likelihood scale rated
    /// directly without multi-factor attack potential (`attack_potential: None`).
    ///
    /// # Errors
    ///
    /// Returns [`PackError`] if `packs/eu-cra.json` fails [`Self::read_pack`];
    /// an integration test checks at build/test time that the embedded file is
    /// valid and in canonical [`MethodologyDef::write_pack`](super::MethodologyDef::write_pack) form.
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), aegis_model::PackError> {
    /// let m = Methodology::eu_cra()?;
    /// assert_eq!(m.key(), "eu-cra");
    /// assert_eq!(m.version(), "0.1.0");
    /// assert_eq!(&*m.element_names().damage_scenario.singular, "Harm");
    /// assert_eq!(&*m.element_names().cybersecurity_goal.singular, "Security requirement");
    /// assert_eq!(m.feasibility_label(), "Likelihood");
    /// assert!(!m.has_attack_potential());
    /// assert_eq!(m.impact_category_count(), 4);
    /// assert_eq!(m.feasibility_levels().len(), 4);
    /// assert_eq!(m.risk_level_count(), 4);
    /// # Ok(())
    /// # }
    /// ```
    pub fn eu_cra() -> Result<Self, PackError> {
        Self::read_pack(PACK)
    }

    /// Returns the canonical JSON bytes of the built-in `"eu-cra"` pack,
    /// embedded from `packs/eu-cra.json` at compile time. Time O(1).
    #[must_use]
    pub const fn eu_cra_pack() -> &'static [u8] {
        PACK
    }
}
