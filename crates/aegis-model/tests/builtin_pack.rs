//! The built-in ISO/SAE 21434 + UN R155 methodology, now a pack file
//! (`packs/iso21434-r155.json`, job 012).
//!
//! `expected()` is the hand-written definition that `src/methodology/iso21434.rs`
//! built before the vocabulary moved into the pack. The tests check that
//! the shipped pack decodes to exactly it, that the pack is canonical
//! `write_pack` output, and the vocabulary itself (moved here from the
//! unit tests of `iso21434.rs`).

mod common;

use std::sync::Arc;

use aegis_model::{
    CatalogSchemeDef, CatalogSyntax, ImpactCategoryDef, LevelDef, Methodology, MethodologyDef,
    ModelError, PropertyDef, SegmentDef, ThreatCategoryDef,
};

/// The shipped pack bytes.
const PACK: &str = include_str!("../packs/iso21434-r155.json");

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

/// The attack feasibility scale, lowest first.
const FEASIBILITY_LEVELS: [(&str, &str); 4] = [
    ("very-low", "Very low"),
    ("low", "Low"),
    ("medium", "Medium"),
    ("high", "High"),
];

/// The ISO/SAE 21434 clause 15.8 / Table H.8 risk scale, lowest first.
const RISK_LEVELS: [(&str, &str); 5] = [
    ("r1", "Risk 1"),
    ("r2", "Risk 2"),
    ("r3", "Risk 3"),
    ("r4", "Risk 4"),
    ("r5", "Risk 5"),
];

/// ISO/SAE 21434 Table H.8 4x4 risk matrix (`LEVELS` rows x `FEASIBILITY_LEVELS` columns).
const RISK_MATRIX: [[&str; 4]; 4] = [
    ["r1", "r1", "r1", "r1"],
    ["r1", "r1", "r2", "r3"],
    ["r1", "r2", "r3", "r4"],
    ["r2", "r3", "r4", "r5"],
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

fn s(text: &str) -> Arc<str> {
    Arc::from(text)
}

/// The old hand-written definition (job 009), unchanged.
fn expected() -> MethodologyDef {
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
                risk_matrix: RISK_MATRIX
                    .iter()
                    .map(|row| row.iter().map(|&k| s(k)).collect())
                    .collect(),
            })
            .collect(),
        feasibility_levels: FEASIBILITY_LEVELS
            .iter()
            .map(|&(key, label)| LevelDef {
                key: s(key),
                label: s(label),
            })
            .collect(),
        risk_levels: RISK_LEVELS
            .iter()
            .map(|&(key, label)| LevelDef {
                key: s(key),
                label: s(label),
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

fn m() -> Methodology {
    Methodology::iso21434_r155().expect("built-in methodology is valid")
}

#[test]
fn builtin_pack_equals_the_old_definition() {
    let def = MethodologyDef::read_pack(PACK.as_bytes()).expect("pack decodes");
    assert_eq!(def, expected());
    assert_eq!(Methodology::new(expected()).ok(), Some(m()));
}

#[test]
fn builtin_pack_is_canonical() {
    let def = MethodologyDef::read_pack(PACK.as_bytes()).expect("pack decodes");
    let mut out = Vec::new();
    def.write_pack(&mut out).expect("write");
    assert_eq!(String::from_utf8(out).expect("utf-8"), PACK);
    // Writing the old definition gives the same bytes.
    assert_eq!(common::pack_bytes(&expected()), PACK.as_bytes());
}

#[test]
fn iso21434_vocabulary_identity() {
    let m = m();
    assert_eq!(m.key(), "iso21434-r155");
    assert_eq!(m.version(), "0.1.0");
    assert_eq!(m.label(), "ISO/SAE 21434 + UN R155");
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
fn iso21434_vocabulary_feasibility_levels() {
    let m = m();
    let keys: Vec<_> = m
        .feasibility_levels()
        .map(|(l, key, _)| {
            assert_eq!(m.feasibility_level_key(l), Some(key));
            key
        })
        .collect();
    assert_eq!(keys, ["very-low", "low", "medium", "high"]);
    let labels: Vec<_> = m
        .feasibility_levels()
        .map(|(l, _, label)| {
            assert_eq!(m.feasibility_level_label(l), Some(label));
            label
        })
        .collect();
    assert_eq!(labels, ["Very low", "Low", "Medium", "High"]);
    let very_low = m.feasibility_level("very-low").expect("very-low");
    let low = m.feasibility_level("low").expect("low");
    let medium = m.feasibility_level("medium").expect("medium");
    let high = m.feasibility_level("high").expect("high");
    assert!(very_low < low && low < medium && medium < high);
    assert_eq!(very_low.index(), 0);
}

#[test]
fn iso21434_vocabulary_risk_levels_and_matrix() {
    let m = m();
    let keys: Vec<_> = m
        .risk_levels()
        .map(|(r, key, _)| {
            assert_eq!(m.risk_level_key(r), Some(key));
            key
        })
        .collect();
    assert_eq!(keys, ["r1", "r2", "r3", "r4", "r5"]);
    let labels: Vec<_> = m
        .risk_levels()
        .map(|(r, _, label)| {
            assert_eq!(m.risk_level_label(r), Some(label));
            label
        })
        .collect();
    assert_eq!(labels, ["Risk 1", "Risk 2", "Risk 3", "Risk 4", "Risk 5"]);
    for c in m.impact_categories() {
        for (i_idx, (i_key, _)) in LEVELS.iter().enumerate() {
            let impact = m.level(c, i_key).expect(i_key);
            for (f_idx, (f_key, _)) in FEASIBILITY_LEVELS.iter().enumerate() {
                let feas = m.feasibility_level(f_key).expect(f_key);
                let want = m.risk_level(RISK_MATRIX[i_idx][f_idx]).expect("risk");
                assert_eq!(m.risk_for(c, impact, feas), Some(want));
            }
        }
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
    let syntax = m.catalog_syntax(s).expect("syntax").clone();
    assert_eq!(syntax.to_string(), "<1..=32>[.<1..=99>]");
    for ok in ["1", "32", "4.1", "32.99", "4", "4.2", "5", "10.10"] {
        let r = m.parse_catalog_ref(s, ok).expect(ok);
        let shown = m.display_catalog_ref(&r).to_string();
        assert_eq!(shown, ok);
        assert_eq!(r.to_string(), ok);
        assert_eq!(m.parse_catalog_ref(s, &shown), Ok(r));
    }
    for bad in [
        "0", "33", "04", "4.0", "4.", ".1", " 4", "+4", "4.1.2", "", "4.100", "-4", "4 ", "4..1",
        "4.01",
    ] {
        assert_eq!(
            m.parse_catalog_ref(s, bad),
            Err(ModelError::InvalidCatalogId {
                scheme: s,
                syntax: syntax.clone(),
            }),
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
