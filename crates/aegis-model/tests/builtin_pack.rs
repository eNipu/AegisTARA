//! The built-in ISO/SAE 21434 + UN R155 methodology, now a pack file
//! (`packs/iso21434-r155.json`, job 012).
//!
//! `expected()` is the hand-written definition that `src/methodology/iso21434.rs`
//! built before the vocabulary moved into the pack. The tests check that
//! the shipped pack decodes to exactly it, that the pack is canonical
//! `write_pack` output, and the vocabulary itself (moved here from the
//! unit tests of `iso21434.rs`).

mod common;

use std::collections::BTreeMap;
use std::sync::Arc;

use aegis_model::{
    AttackPotentialDef, CatalogSchemeDef, CatalogSyntax, ElementNameDef, ElementNamesDef,
    FactorDef, FactorLevelDef, ImpactCategoryDef, LevelDef, Methodology, MethodologyDef,
    ModelError, PackTranslationDef, PotentialBandDef, PropertyDef, SegmentDef, ThreatCategoryDef,
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

const ATTACK_TYPES: [(&str, &str); 10] = [
    ("firmware-modification", "Firmware modification"),
    ("bootloader-exploit", "Bootloader exploit"),
    ("debug-access", "Debug access"),
    ("physical-tampering", "Physical tampering"),
    ("flash-read-out", "Flash read-out"),
    ("diagnostic-access", "Diagnostic access"),
    ("local-bus-injection", "Local bus injection"),
    ("remote-network", "Remote network"),
    ("wireless-proximity", "Wireless proximity"),
    ("side-channel", "Side channel"),
];

fn ja_translation() -> PackTranslationDef {
    PackTranslationDef {
        label: Some(s("ISO/SAE 21434 + UN R155")),
        element_names: Some(ElementNamesDef {
            component: ElementNameDef {
                singular: s("コンポーネント"),
                plural: s("コンポーネント"),
            },
            interface: ElementNameDef {
                singular: s("インターフェース"),
                plural: s("インターフェース"),
            },
            data_flow: ElementNameDef {
                singular: s("データフロー"),
                plural: s("データフロー"),
            },
            trust_boundary: ElementNameDef {
                singular: s("信頼境界"),
                plural: s("信頼境界"),
            },
            asset: ElementNameDef {
                singular: s("資産"),
                plural: s("資産"),
            },
            damage_scenario: ElementNameDef {
                singular: s("ダメージシナリオ"),
                plural: s("ダメージシナリオ"),
            },
            threat_scenario: ElementNameDef {
                singular: s("脅威シナリオ"),
                plural: s("脅威シナリオ"),
            },
            attack_tree: ElementNameDef {
                singular: s("アタックツリー"),
                plural: s("アタックツリー"),
            },
            security_control: ElementNameDef {
                singular: s("セキュリティ対策"),
                plural: s("セキュリティ対策"),
            },
            cybersecurity_goal: ElementNameDef {
                singular: s("サイバーセキュリティ目標"),
                plural: s("サイバーセキュリティ目標"),
            },
            cybersecurity_claim: ElementNameDef {
                singular: s("サイバーセキュリティ主張"),
                plural: s("サイバーセキュリティ主張"),
            },
            feasibility: s("攻撃実現可能性"),
            interface_kinds: Vec::new(),
            attack_types: Vec::new(),
            control_kinds: Vec::new(),
            translations: BTreeMap::new(),
        }),
        impact_categories: BTreeMap::from([
            (s("safety"), s("安全性")),
            (s("financial"), s("財務")),
            (s("operational"), s("運用")),
            (s("privacy"), s("プライバシー")),
        ]),
        impact_levels: BTreeMap::from([
            (s("negligible"), s("無視可能")),
            (s("moderate"), s("中程度")),
            (s("major"), s("重大")),
            (s("severe"), s("深刻")),
        ]),
        feasibility_levels: BTreeMap::from([
            (s("very-low"), s("極めて低い")),
            (s("low"), s("低い")),
            (s("medium"), s("中")),
            (s("high"), s("高い")),
        ]),
        risk_levels: BTreeMap::from([
            (s("r1"), s("リスク 1")),
            (s("r2"), s("リスク 2")),
            (s("r3"), s("リスク 3")),
            (s("r4"), s("リスク 4")),
            (s("r5"), s("リスク 5")),
        ]),
        properties: BTreeMap::from([
            (s("confidentiality"), s("機密性")),
            (s("integrity"), s("完全性")),
            (s("availability"), s("可用性")),
            (s("authenticity"), s("真正性")),
            (s("authorization"), s("認可")),
            (s("non-repudiation"), s("否認防止")),
        ]),
        threat_categories: BTreeMap::from([
            (s("spoofing"), s("なりすまし")),
            (s("tampering"), s("改ざん")),
            (s("repudiation"), s("否認")),
            (s("information-disclosure"), s("情報漏洩")),
            (s("denial-of-service"), s("サービス拒否")),
            (s("elevation-of-privilege"), s("権限昇格")),
        ]),
        catalog_schemes: BTreeMap::from([(s("r155-annex5"), s("UN R155 附属書 5"))]),
        feasibility_factors: BTreeMap::from([
            (s("elapsed-time"), s("経過時間")),
            (s("expertise"), s("専門知識")),
            (s("knowledge"), s("対象に関する知識")),
            (s("window-of-opportunity"), s("攻撃機会")),
            (s("equipment"), s("機材")),
        ]),
        factor_levels: BTreeMap::from([
            (s("le-1d"), s("1日以下")),
            (s("le-1w"), s("1週間以下")),
            (s("le-1m"), s("1か月以下")),
            (s("le-6m"), s("6か月以下")),
            (s("gt-6m"), s("6か月超")),
            (s("layman"), s("一般人")),
            (s("proficient"), s("熟練者")),
            (s("expert"), s("専門家")),
            (s("multiple-experts"), s("複数の専門家")),
            (s("public"), s("公開情報")),
            (s("restricted"), s("制限情報")),
            (s("confidential"), s("機密情報")),
            (s("strictly-confidential"), s("極秘情報")),
            (s("unlimited"), s("無制限")),
            (s("easy"), s("容易")),
            (s("moderate"), s("中程度")),
            (s("difficult"), s("困難")),
            (s("standard"), s("標準")),
            (s("specialized"), s("専門的")),
            (s("bespoke"), s("特注")),
            (s("multiple-bespoke"), s("複数の特注")),
        ]),
        interface_kinds: BTreeMap::from([
            (s("can"), s("CAN")),
            (s("ethernet"), s("Ethernet")),
            (s("wireless"), s("無線")),
            (s("debug"), s("デバッグ")),
            (s("cellular"), s("セルラー")),
            (s("usb"), s("USB")),
        ]),
        attack_types: BTreeMap::from([
            (s("firmware-modification"), s("ファームウェア改ざん")),
            (s("bootloader-exploit"), s("ブートローダの脆弱性悪用")),
            (s("debug-access"), s("デバッグアクセス")),
            (s("physical-tampering"), s("物理的改ざん")),
            (s("flash-read-out"), s("フラッシュ読み出し")),
            (s("diagnostic-access"), s("診断アクセス")),
            (s("local-bus-injection"), s("ローカルバス注入")),
            (s("remote-network"), s("リモートネットワーク")),
            (s("wireless-proximity"), s("近接無線")),
            (s("side-channel"), s("サイドチャネル")),
        ]),
        control_kinds: BTreeMap::new(),
    }
}

/// The hand-written definition matching the shipped pack.
fn expected() -> MethodologyDef {
    MethodologyDef {
        key: s("iso21434-r155"),
        version: s("0.1.0"),
        label: s("ISO/SAE 21434 + UN R155"),
        element_names: ElementNamesDef {
            interface_kinds: vec![
                LevelDef {
                    key: s("can"),
                    label: s("CAN"),
                },
                LevelDef {
                    key: s("ethernet"),
                    label: s("Ethernet"),
                },
                LevelDef {
                    key: s("wireless"),
                    label: s("Wireless"),
                },
                LevelDef {
                    key: s("debug"),
                    label: s("Debug"),
                },
                LevelDef {
                    key: s("cellular"),
                    label: s("Cellular"),
                },
                LevelDef {
                    key: s("usb"),
                    label: s("USB"),
                },
            ],
            attack_types: ATTACK_TYPES
                .iter()
                .map(|&(key, label)| LevelDef {
                    key: s(key),
                    label: s(label),
                })
                .collect(),
            translations: BTreeMap::from([(s("ja"), ja_translation())]),
            ..ElementNamesDef::default()
        },
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
        require_rationale: false,
        attack_potential: Some(AttackPotentialDef {
            factors: vec![
                FactorDef {
                    key: s("elapsed-time"),
                    label: s("Elapsed time"),
                    levels: vec![
                        FactorLevelDef {
                            key: s("le-1d"),
                            label: s("<= 1 day"),
                            points: 0,
                        },
                        FactorLevelDef {
                            key: s("le-1w"),
                            label: s("<= 1 week"),
                            points: 1,
                        },
                        FactorLevelDef {
                            key: s("le-1m"),
                            label: s("<= 1 month"),
                            points: 4,
                        },
                        FactorLevelDef {
                            key: s("le-6m"),
                            label: s("<= 6 months"),
                            points: 17,
                        },
                        FactorLevelDef {
                            key: s("gt-6m"),
                            label: s("> 6 months"),
                            points: 19,
                        },
                    ],
                },
                FactorDef {
                    key: s("expertise"),
                    label: s("Expertise"),
                    levels: vec![
                        FactorLevelDef {
                            key: s("layman"),
                            label: s("Layman"),
                            points: 0,
                        },
                        FactorLevelDef {
                            key: s("proficient"),
                            label: s("Proficient"),
                            points: 3,
                        },
                        FactorLevelDef {
                            key: s("expert"),
                            label: s("Expert"),
                            points: 6,
                        },
                        FactorLevelDef {
                            key: s("multiple-experts"),
                            label: s("Multiple experts"),
                            points: 8,
                        },
                    ],
                },
                FactorDef {
                    key: s("knowledge"),
                    label: s("Knowledge of the item"),
                    levels: vec![
                        FactorLevelDef {
                            key: s("public"),
                            label: s("Public"),
                            points: 0,
                        },
                        FactorLevelDef {
                            key: s("restricted"),
                            label: s("Restricted"),
                            points: 3,
                        },
                        FactorLevelDef {
                            key: s("confidential"),
                            label: s("Confidential"),
                            points: 7,
                        },
                        FactorLevelDef {
                            key: s("strictly-confidential"),
                            label: s("Strictly confidential"),
                            points: 11,
                        },
                    ],
                },
                FactorDef {
                    key: s("window-of-opportunity"),
                    label: s("Window of opportunity"),
                    levels: vec![
                        FactorLevelDef {
                            key: s("unlimited"),
                            label: s("Unlimited"),
                            points: 0,
                        },
                        FactorLevelDef {
                            key: s("easy"),
                            label: s("Easy"),
                            points: 1,
                        },
                        FactorLevelDef {
                            key: s("moderate"),
                            label: s("Moderate"),
                            points: 4,
                        },
                        FactorLevelDef {
                            key: s("difficult"),
                            label: s("Difficult"),
                            points: 10,
                        },
                    ],
                },
                FactorDef {
                    key: s("equipment"),
                    label: s("Equipment"),
                    levels: vec![
                        FactorLevelDef {
                            key: s("standard"),
                            label: s("Standard"),
                            points: 0,
                        },
                        FactorLevelDef {
                            key: s("specialized"),
                            label: s("Specialized"),
                            points: 4,
                        },
                        FactorLevelDef {
                            key: s("bespoke"),
                            label: s("Bespoke"),
                            points: 7,
                        },
                        FactorLevelDef {
                            key: s("multiple-bespoke"),
                            label: s("Multiple bespoke"),
                            points: 9,
                        },
                    ],
                },
            ],
            bands: vec![
                PotentialBandDef {
                    min: 0,
                    max: Some(13),
                    feasibility: s("high"),
                },
                PotentialBandDef {
                    min: 14,
                    max: Some(19),
                    feasibility: s("medium"),
                },
                PotentialBandDef {
                    min: 20,
                    max: Some(24),
                    feasibility: s("low"),
                },
                PotentialBandDef {
                    min: 25,
                    max: None,
                    feasibility: s("very-low"),
                },
            ],
        }),
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

#[test]
fn iso21434_vocabulary_attack_potential() {
    let m = m();
    let fkeys: Vec<_> = m
        .feasibility_factors()
        .map(|f| m.feasibility_factor_key(f).expect("f"))
        .collect();
    assert_eq!(
        fkeys,
        [
            "elapsed-time",
            "expertise",
            "knowledge",
            "window-of-opportunity",
            "equipment"
        ]
    );
    let bands: Vec<_> = m
        .potential_bands()
        .iter()
        .map(|b| {
            (
                b.min(),
                b.max(),
                m.feasibility_level_key(b.feasibility()).expect("feas"),
            )
        })
        .collect();
    assert_eq!(
        bands,
        [
            (0, Some(13), "high"),
            (14, Some(19), "medium"),
            (20, Some(24), "low"),
            (25, None, "very-low"),
        ]
    );
    let min_levels: Vec<_> = m
        .feasibility_factors()
        .map(|f| Some(m.factor_levels(f).next().expect("min")))
        .collect();
    let ap_min = m.resolve_attack_potential(&min_levels).expect("min ap");
    assert_eq!(ap_min.score(), 0);
    assert_eq!(m.feasibility_level_key(ap_min.feasibility()), Some("high"));
    let max_levels: Vec<_> = m
        .feasibility_factors()
        .map(|f| Some(m.factor_levels(f).last().expect("max")))
        .collect();
    let ap_max = m.resolve_attack_potential(&max_levels).expect("max ap");
    assert_eq!(ap_max.score(), 19 + 8 + 11 + 10 + 9);
    assert_eq!(
        m.feasibility_level_key(ap_max.feasibility()),
        Some("very-low")
    );
}

#[test]
fn eu_cra_pack_is_canonical_and_evaluates_plain_likelihood() {
    let raw = Methodology::eu_cra_pack();
    let def = MethodologyDef::read_pack(raw).expect("read eu-cra pack");
    let mut canonical = Vec::new();
    def.write_pack(&mut canonical).expect("write eu-cra pack");
    assert_eq!(
        canonical.as_slice(),
        raw,
        "packs/eu-cra.json must match write_pack output byte-for-byte"
    );

    let m = Methodology::eu_cra().expect("eu-cra methodology");
    assert_eq!(m.key(), "eu-cra");
    assert_eq!(m.version(), "0.1.0");
    assert_eq!(m.label(), "EU Cyber Resilience Act (Annex I)");
    assert!(!m.has_attack_potential());
    assert_eq!(m.feasibility_factor_count(), 0);
    assert_eq!(m.feasibility_label(), "Likelihood");
    assert_eq!(&*m.element_names().damage_scenario.singular, "Harm");
    assert_eq!(&*m.element_names().damage_scenario.plural, "Harms");
    assert_eq!(
        &*m.element_names().cybersecurity_goal.singular,
        "Security requirement"
    );
    assert_eq!(
        &*m.element_names().cybersecurity_goal.plural,
        "Security requirements"
    );
    assert_eq!(&*m.element_names().asset.singular, "Asset");

    let cats: Vec<_> = m
        .impact_categories()
        .filter_map(|c| m.impact_category_key(c))
        .collect();
    assert_eq!(
        cats,
        [
            "safety-health",
            "data-privacy",
            "availability-continuity",
            "financial-material",
        ]
    );
    let feas: Vec<_> = m.feasibility_levels().map(|(_, k, _)| k).collect();
    assert_eq!(feas, ["very-low", "low", "medium", "high"]);
    let risks: Vec<_> = m.risk_levels().map(|(_, k, _)| k).collect();
    assert_eq!(risks, ["low", "medium", "high", "critical"]);

    let scheme = m.catalog_scheme("cra-annex1").expect("cra-annex1 scheme");
    let parsed = m
        .parse_catalog_ref(scheme, "CRA-1.2")
        .expect("parse CRA-1.2");
    assert_eq!(m.display_catalog_ref(&parsed).to_string(), "CRA-1.2");
}

#[test]
fn iso21434_interface_kinds_and_japanese_translations() {
    let m = m();
    let ik_keys: Vec<_> = m
        .interface_kinds()
        .filter_map(|k| m.interface_kind_key(k))
        .collect();
    assert_eq!(
        ik_keys,
        ["can", "ethernet", "wireless", "debug", "cellular", "usb"]
    );
    let wireless = m.interface_kind("wireless").expect("wireless");
    assert_eq!(m.interface_kind_label(wireless), Some("Wireless"));
    assert_eq!(
        m.interface_kind_label_for_lang(wireless, "ja"),
        Some("無線")
    );
    assert_eq!(
        m.interface_kind_label_for_lang(wireless, "en"),
        Some("Wireless")
    );

    let safety = m.impact_category("safety").expect("safety");
    let severe = m.level(safety, "severe").expect("severe");
    assert_eq!(
        m.impact_category_label_for_lang(safety, "ja"),
        Some("安全性")
    );
    assert_eq!(m.level_label_for_lang(safety, severe, "ja"), Some("深刻"));

    let high = m.feasibility_level("high").expect("high");
    assert_eq!(m.feasibility_level_label_for_lang(high, "ja"), Some("高い"));

    let r5 = m.risk_level("r5").expect("r5");
    assert_eq!(m.risk_level_label_for_lang(r5, "ja"), Some("リスク 5"));

    let integrity = m.property("integrity").expect("integrity");
    assert_eq!(m.property_label_for_lang(integrity, "ja"), Some("完全性"));

    let tampering = m.threat_category("tampering").expect("tampering");
    assert_eq!(
        m.threat_category_label_for_lang(tampering, "ja"),
        Some("改ざん")
    );

    let ja_names = m.element_names_for_lang("ja");
    assert_eq!(&*ja_names.asset.singular, "資産");
    assert_eq!(&*ja_names.damage_scenario.singular, "ダメージシナリオ");
    assert_eq!(&*ja_names.threat_scenario.singular, "脅威シナリオ");
    assert_eq!(&*ja_names.component.singular, "コンポーネント");
    assert_eq!(&*ja_names.interface.singular, "インターフェース");
    assert_eq!(&*ja_names.data_flow.singular, "データフロー");
    assert_eq!(&*ja_names.trust_boundary.singular, "信頼境界");
    assert_eq!(m.feasibility_label_for_lang("ja"), "攻撃実現可能性");
}
