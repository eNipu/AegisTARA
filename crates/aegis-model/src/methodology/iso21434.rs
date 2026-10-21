//! The built-in ISO/SAE 21434 + UN R155 methodology, written as data.
//!
//! This is the only non-test file that names the 21434 and R155
//! vocabulary. It builds a [`MethodologyDef`] and passes it through
//! [`Methodology::new`], exactly as a pack file will. Labels are short names
//! only, with no text or tables from the standards.

use std::sync::Arc;

use super::{
    CatalogSchemeDef, CatalogSyntax, ImpactCategoryDef, LevelDef, Methodology, MethodologyDef,
    PropertyDef, SegmentDef, ThreatCategoryDef,
};
use crate::error::MethodologyError;

/// Impact categories, in declaration order.
const IMPACT_CATEGORIES: [(&str, &str); 4] = [
    ("safety", "Safety"),
    ("financial", "Financial"),
    ("operational", "Operational"),
    ("privacy", "Privacy"),
];

/// The rating scale shared by every impact category, lowest first.
const LEVELS: [(&str, &str); 4] = [
    ("negligible", "Negligible"),
    ("moderate", "Moderate"),
    ("major", "Major"),
    ("severe", "Severe"),
];

/// Security properties, in declaration order.
const PROPERTIES: [(&str, &str); 6] = [
    ("confidentiality", "Confidentiality"),
    ("integrity", "Integrity"),
    ("availability", "Availability"),
    ("authenticity", "Authenticity"),
    ("authorization", "Authorization"),
    ("non-repudiation", "Non-repudiation"),
];

/// STRIDE threat categories and the one property each violates.
const THREAT_CATEGORIES: [(&str, &str, &str); 6] = [
    ("spoofing", "Spoofing", "authenticity"),
    ("tampering", "Tampering", "integrity"),
    ("repudiation", "Repudiation", "non-repudiation"),
    (
        "information-disclosure",
        "Information disclosure",
        "confidentiality",
    ),
    ("denial-of-service", "Denial of service", "availability"),
    (
        "elevation-of-privilege",
        "Elevation of privilege",
        "authorization",
    ),
];

impl Methodology {
    /// The built-in ISO/SAE 21434 + UN R155 methodology, version `0.1.0`.
    ///
    /// - Impact categories `safety`, `financial`, `operational`, `privacy`,
    ///   each rated `negligible` < `moderate` < `major` < `severe`.
    /// - Properties `confidentiality`, `integrity`, `availability`,
    ///   `authenticity`, `authorization`, `non-repudiation`.
    /// - STRIDE threat categories, each violating one property.
    /// - One catalog scheme, `r155-annex5`, with IDs `N` or `N.M`
    ///   (N in 1..=32, M in 1..=99). No catalog contents ship.
    ///
    /// Built through [`Methodology::new`], so it obeys every rule a pack
    /// does. Time and space O(m), m = 40 vocabulary entries.
    ///
    /// # Errors
    ///
    /// Never in practice: the definition is fixed and covered by tests. The
    /// `Result` is kept because `new` is the only constructor.
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), aegis_model::MethodologyError> {
    /// let m = Methodology::iso21434_r155()?;
    /// assert_eq!(m.key(), "iso21434-r155");
    /// assert_eq!(m.version(), "0.1.0");
    /// # Ok(())
    /// # }
    /// ```
    pub fn iso21434_r155() -> Result<Methodology, MethodologyError> {
        Methodology::new(definition())
    }
}

/// Shares a string.
fn s(text: &str) -> Arc<str> {
    Arc::from(text)
}

/// Builds the definition. Time O(m).
fn definition() -> MethodologyDef {
    MethodologyDef {
        key: s("iso21434-r155"),
        version: s("0.1.0"),
        label: s("ISO/SAE 21434 + UN R155"),
        impact_categories: IMPACT_CATEGORIES
            .iter()
            .map(|&(key, label)| ImpactCategoryDef {
                key: s(key),
                label: s(label),
                levels: LEVELS
                    .iter()
                    .map(|&(key, label)| LevelDef {
                        key: s(key),
                        label: s(label),
                    })
                    .collect(),
            })
            .collect(),
        properties: PROPERTIES
            .iter()
            .map(|&(key, label)| PropertyDef {
                key: s(key),
                label: s(label),
            })
            .collect(),
        threat_categories: THREAT_CATEGORIES
            .iter()
            .map(|&(key, label, violates)| ThreatCategoryDef {
                key: s(key),
                label: s(label),
                violates: vec![s(violates)],
            })
            .collect(),
        catalog_schemes: vec![CatalogSchemeDef {
            key: s("r155-annex5"),
            label: s("UN R155 Annex 5"),
            syntax: CatalogSyntax {
                prefix: s(""),
                segments: vec![
                    SegmentDef {
                        min: 1,
                        max: 32,
                        digits: None,
                    },
                    SegmentDef {
                        min: 1,
                        max: 99,
                        digits: None,
                    },
                ],
                required: 1,
            },
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ModelError;

    fn m() -> Methodology {
        Methodology::iso21434_r155().expect("built-in methodology is valid")
    }

    #[test]
    fn iso21434_vocabulary_identity() {
        let m = m();
        assert_eq!(m.key(), "iso21434-r155");
        assert_eq!(m.version(), "0.1.0");
        assert_eq!(m.label(), "ISO/SAE 21434 + UN R155");
        assert_eq!(Methodology::new(definition()), Ok(m));
    }

    #[test]
    fn iso21434_vocabulary_impacts_and_levels() {
        let m = m();
        let keys: Vec<_> = m
            .impact_categories()
            .map(|c| m.impact_category_key(c).expect("key"))
            .collect();
        assert_eq!(keys, ["safety", "financial", "operational", "privacy"]);
        for c in m.impact_categories() {
            let levels: Vec<_> = m
                .levels(c)
                .map(|l| m.level_key(c, l).expect("level"))
                .collect();
            assert_eq!(levels, ["negligible", "moderate", "major", "severe"]);
            let low = m.level(c, "negligible").expect("low");
            let high = m.level(c, "severe").expect("high");
            assert!(low < high);
            assert_eq!(low.index(), 0);
        }
    }

    #[test]
    fn iso21434_vocabulary_properties() {
        let m = m();
        let keys: Vec<_> = m
            .properties()
            .map(|p| m.property_key(p).expect("key"))
            .collect();
        assert_eq!(
            keys,
            [
                "confidentiality",
                "integrity",
                "availability",
                "authenticity",
                "authorization",
                "non-repudiation"
            ]
        );
    }

    #[test]
    fn iso21434_vocabulary_stride_mapping() {
        let m = m();
        let pairs: Vec<_> = m
            .threat_categories()
            .map(|t| {
                let v: Vec<_> = m
                    .violates(t)
                    .iter()
                    .map(|p| m.property_key(p).expect("p"))
                    .collect();
                (m.threat_category_key(t).expect("t"), v)
            })
            .collect();
        let want = [
            ("spoofing", vec!["authenticity"]),
            ("tampering", vec!["integrity"]),
            ("repudiation", vec!["non-repudiation"]),
            ("information-disclosure", vec!["confidentiality"]),
            ("denial-of-service", vec!["availability"]),
            ("elevation-of-privilege", vec!["authorization"]),
        ];
        assert_eq!(pairs, want);
    }

    #[test]
    fn iso21434_vocabulary_catalog_scheme() {
        let m = m();
        let keys: Vec<_> = m
            .catalog_schemes()
            .map(|s| m.catalog_scheme_key(s).expect("k"))
            .collect();
        assert_eq!(keys, ["r155-annex5"]);
    }

    #[test]
    fn catalog_syntax_r155() {
        let m = m();
        let s = m.catalog_scheme("r155-annex5").expect("scheme");
        for ok in ["1", "32", "4.1", "32.99", "4", "4.2", "5", "10.10"] {
            let r = m.parse_catalog_ref(s, ok).expect(ok);
            let shown = m.display_catalog_ref(&r).to_string();
            assert_eq!(shown, ok);
            assert_eq!(m.parse_catalog_ref(s, &shown), Ok(r));
        }
        for bad in [
            "0", "33", "04", "4.0", "4.", ".1", " 4", "+4", "4.1.2", "", "4.100", "-4", "4 ",
            "4..1", "4.01",
        ] {
            assert_eq!(
                m.parse_catalog_ref(s, bad),
                Err(ModelError::InvalidCatalogId { scheme: s }),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn catalog_syntax_r155_order() {
        let m = m();
        let s = m.catalog_scheme("r155-annex5").expect("scheme");
        let mut refs: Vec<_> = ["5", "4.2", "4", "4.1"]
            .iter()
            .map(|t| m.parse_catalog_ref(s, t).expect(t))
            .collect();
        refs.sort();
        let shown: Vec<_> = refs
            .iter()
            .map(|r| m.display_catalog_ref(r).to_string())
            .collect();
        assert_eq!(shown, ["4", "4.1", "4.2", "5"]);
    }
}
