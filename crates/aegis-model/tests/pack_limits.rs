//! Pack decode caps (job 012, AC9 and AC10): for every cap, the value at
//! the cap decodes (or reaches a model error where the model is stricter)
//! and cap + 1 gives `LimitExceeded` with that cap's `Limit`, before
//! `Methodology::new` runs.
//!
//! Over-cap packs are built by breaking a valid definition and writing it
//! with `write_pack`, which does not validate.

mod common;

use std::io::Read;
use std::sync::Arc;

use aegis_model::{
    AttackPotentialDef, CatalogSchemeDef, ControlKindDef, FactorDef, FactorLevelDef,
    ImpactCategoryDef, LevelDef, Limit, MAX_LABEL_BYTES, MAX_PACK_FILE_BYTES, Methodology,
    MethodologyDef, MethodologyError, PackError, PotentialBandDef, PropertyDef, SegmentDef,
    ThreatCategoryDef,
};
use common::{key_of_len, max_def, pack_bytes, second_def};

fn decode(def: &MethodologyDef) -> Result<MethodologyDef, PackError> {
    MethodologyDef::read_pack(pack_bytes(def).as_slice())
}

/// Asserts that `def` decodes back equal and that `Methodology::new`
/// accepts it.
fn at_cap(def: &MethodologyDef, what: &str) {
    let back = decode(def).unwrap_or_else(|e| panic!("{what}: {e}"));
    assert_eq!(&back, def, "{what}");
    if let Err(e) = Methodology::new(back) {
        panic!("{what}: model rejected it: {e}");
    }
}

/// Asserts that `def` fails to decode with exactly `want`.
fn over_cap(def: &MethodologyDef, want: Limit, what: &str) {
    match decode(def) {
        Err(PackError::LimitExceeded(got)) => assert_eq!(got, want, "{what}"),
        other => panic!("{what}: expected {want:?}, got {other:?}"),
    }
}

/// A 64-property, valid variant of the second definition.
fn wide_properties() -> MethodologyDef {
    let mut d = second_def();
    d.properties = (0..64)
        .map(|i| PropertyDef {
            key: key_of_len("p", i, 8),
            label: "P".into(),
        })
        .collect();
    let all: Vec<Arc<str>> = d.properties.iter().map(|p| Arc::clone(&p.key)).collect();
    for t in &mut d.threat_categories {
        t.violates = vec![Arc::clone(&all[0])];
    }
    d
}

#[test]
fn key_cap_in_every_position() {
    const KEY: Limit = Limit::KeyBytes { max: 64 };
    // Each case sets one key position to `key`.
    type Set = fn(&mut MethodologyDef, Arc<str>);
    let positions: [(&str, Set); 13] = [
        ("methodology key", |d, k| d.key = k),
        ("impact category key", |d, k| d.impact_categories[1].key = k),
        ("level key", |d, k| d.impact_categories[0].levels[2].key = k),
        ("property key", |d, k| {
            // Keep `violates` pointing at it so the model accepts.
            d.properties[0].key = Arc::clone(&k);
            d.threat_categories[0].violates = vec![k];
        }),
        ("threat category key", |d, k| d.threat_categories[3].key = k),
        ("violates entry", |d, k| {
            d.properties.push(PropertyDef {
                key: Arc::clone(&k),
                label: "Extra".into(),
            });
            d.threat_categories[2].violates.push(k);
        }),
        ("catalog scheme key", |d, k| d.catalog_schemes[0].key = k),
        ("feasibility level key", |d, k| {
            d.feasibility_levels[2].key = Arc::clone(&k);
            d.attack_potential.as_mut().expect("ap").bands[2].feasibility = k;
        }),
        ("risk level key", |d, k| {
            d.risk_levels.push(LevelDef {
                key: k,
                label: "Extra".into(),
            });
        }),
        ("risk matrix entry", |d, k| {
            d.risk_levels.push(LevelDef {
                key: Arc::clone(&k),
                label: "Extra".into(),
            });
            d.impact_categories[0].risk_matrix[0][0] = k;
        }),
        ("feasibility factor key", |d, k| {
            d.attack_potential.as_mut().expect("ap").factors[1].key = k;
        }),
        ("factor level key", |d, k| {
            d.attack_potential.as_mut().expect("ap").factors[1].levels[2].key = k;
        }),
        ("potential band feasibility key", |d, k| {
            d.feasibility_levels.push(LevelDef {
                key: Arc::clone(&k),
                label: "Extra".into(),
            });
            for c in &mut d.impact_categories {
                for row in &mut c.risk_matrix {
                    row.push("acceptable".into());
                }
            }
            d.attack_potential.as_mut().expect("ap").bands[0].feasibility = k;
        }),
    ];
    for (what, set) in positions {
        let mut d = second_def();
        set(&mut d, key_of_len("k", 7, 64));
        at_cap(&d, what);
        let mut d = second_def();
        set(&mut d, key_of_len("k", 7, 65));
        over_cap(&d, KEY, what);
    }
}

#[test]
fn methodology_version_cap() {
    const VERSION: Limit = Limit::MethodologyVersionBytes { max: 256 };
    // 64 chars of 4 bytes: at both the model's and the decoder's limit.
    let mut d = second_def();
    d.version = "\u{1f600}".repeat(64).into();
    at_cap(&d, "version of 256 bytes");
    // 256 one-byte chars decode, then the model's 64-char rule rejects.
    let mut d = second_def();
    d.version = "v".repeat(256).into();
    assert_eq!(decode(&d).ok().as_ref(), Some(&d));
    assert!(matches!(
        Methodology::read_pack(pack_bytes(&d).as_slice()),
        Err(PackError::Methodology(MethodologyError::InvalidVersion))
    ));
    let mut d = second_def();
    d.version = format!("v{}", "\u{1f600}".repeat(64)).into();
    over_cap(&d, VERSION, "version of 257 bytes");
}

#[test]
fn label_cap_in_every_position() {
    const LABEL: Limit = Limit::LabelBytes { max: 1024 };
    type Set = fn(&mut MethodologyDef, Arc<str>);
    let positions: [(&str, Set); 25] = [
        ("methodology label", |d, l| d.label = l),
        ("element_names asset singular", |d, l| {
            d.element_names.asset.singular = l
        }),
        ("element_names asset plural", |d, l| {
            d.element_names.asset.plural = l
        }),
        ("element_names damage_scenario singular", |d, l| {
            d.element_names.damage_scenario.singular = l
        }),
        ("element_names damage_scenario plural", |d, l| {
            d.element_names.damage_scenario.plural = l
        }),
        ("element_names threat_scenario singular", |d, l| {
            d.element_names.threat_scenario.singular = l
        }),
        ("element_names threat_scenario plural", |d, l| {
            d.element_names.threat_scenario.plural = l
        }),
        ("element_names attack_tree singular", |d, l| {
            d.element_names.attack_tree.singular = l
        }),
        ("element_names attack_tree plural", |d, l| {
            d.element_names.attack_tree.plural = l
        }),
        ("element_names security_control singular", |d, l| {
            d.element_names.security_control.singular = l
        }),
        ("element_names security_control plural", |d, l| {
            d.element_names.security_control.plural = l
        }),
        ("element_names cybersecurity_goal singular", |d, l| {
            d.element_names.cybersecurity_goal.singular = l
        }),
        ("element_names cybersecurity_goal plural", |d, l| {
            d.element_names.cybersecurity_goal.plural = l
        }),
        ("element_names cybersecurity_claim singular", |d, l| {
            d.element_names.cybersecurity_claim.singular = l
        }),
        ("element_names cybersecurity_claim plural", |d, l| {
            d.element_names.cybersecurity_claim.plural = l
        }),
        ("element_names feasibility", |d, l| {
            d.element_names.feasibility = l
        }),
        ("impact category label", |d, l| {
            d.impact_categories[2].label = l
        }),
        ("level label", |d, l| {
            d.impact_categories[1].levels[4].label = l
        }),
        ("property label", |d, l| d.properties[1].label = l),
        ("threat category label", |d, l| {
            d.threat_categories[0].label = l
        }),
        ("catalog scheme label", |d, l| {
            d.catalog_schemes[0].label = l
        }),
        ("feasibility level label", |d, l| {
            d.feasibility_levels[2].label = l
        }),
        ("risk level label", |d, l| d.risk_levels[1].label = l),
        ("feasibility factor label", |d, l| {
            d.attack_potential.as_mut().expect("ap").factors[0].label = l
        }),
        ("factor level label", |d, l| {
            d.attack_potential.as_mut().expect("ap").factors[0].levels[1].label = l
        }),
    ];
    assert_eq!(MAX_LABEL_BYTES, 1024);
    for (what, set) in positions {
        // 256 chars of 4 bytes: at both limits.
        let mut d = second_def();
        set(&mut d, "\u{1f600}".repeat(256).into());
        at_cap(&d, what);
        let mut d = second_def();
        set(&mut d, format!("x{}", "\u{1f600}".repeat(256)).into());
        over_cap(&d, LABEL, what);
    }
    // 1,024 one-byte chars decode, then the model's 256-char rule rejects.
    let mut d = second_def();
    d.label = "x".repeat(1024).into();
    assert!(matches!(
        Methodology::read_pack(pack_bytes(&d).as_slice()),
        Err(PackError::Methodology(MethodologyError::LabelTooLong {
            max: 256,
            ..
        }))
    ));
}

#[test]
fn catalog_prefix_cap() {
    const PREFIX: Limit = Limit::CatalogPrefixBytes { max: 16 };
    let mut d = second_def();
    d.catalog_schemes[0].syntax.prefix = "ABCDEFGHIJKLMNOP".into();
    at_cap(&d, "16-byte prefix");
    d.catalog_schemes[0].syntax.prefix = "ABCDEFGHIJKLMNOPQ".into();
    over_cap(&d, PREFIX, "17-byte prefix");
}

#[test]
fn impact_category_count_cap() {
    let category = |i: usize| ImpactCategoryDef {
        key: key_of_len("c", i, 6),
        label: "C".into(),
        levels: second_def().impact_categories[2].levels.clone(),
        risk_matrix: second_def().impact_categories[2].risk_matrix.clone(),
    };
    let mut d = second_def();
    d.impact_categories = (0..32).map(category).collect();
    at_cap(&d, "32 impact categories");
    d.impact_categories.push(category(32));
    over_cap(&d, Limit::ImpactCategories { max: 32 }, "33");
}

#[test]
fn level_count_cap() {
    let level = |i: usize| LevelDef {
        key: key_of_len("l", i, 6),
        label: "L".into(),
    };
    let mut d = second_def();
    let row = d.impact_categories[1].risk_matrix[0].clone();
    d.impact_categories[1].levels = (0..16).map(level).collect();
    d.impact_categories[1].risk_matrix = vec![row; 16];
    at_cap(&d, "16 levels");
    d.impact_categories[1].levels.push(level(16));
    over_cap(&d, Limit::Levels { max: 16 }, "17 levels");
}

#[test]
fn feasibility_level_count_cap() {
    let level = |i: usize| LevelDef {
        key: key_of_len("f", i, 6),
        label: "F".into(),
    };
    let mut d = second_def();
    d.feasibility_levels = (0..16).map(level).collect();
    for c in &mut d.impact_categories {
        for row in &mut c.risk_matrix {
            row.resize(16, "acceptable".into());
        }
    }
    let f0 = Arc::clone(&d.feasibility_levels[0].key);
    for b in &mut d.attack_potential.as_mut().expect("ap").bands {
        b.feasibility = Arc::clone(&f0);
    }
    at_cap(&d, "16 feasibility levels");
    d.feasibility_levels.push(level(16));
    over_cap(&d, Limit::Levels { max: 16 }, "17 feasibility levels");
}

#[test]
fn feasibility_factor_and_band_count_caps() {
    let factor = |i: usize| FactorDef {
        key: key_of_len("af", i, 6),
        label: "AF".into(),
        levels: second_def().attack_potential.expect("ap").factors[0]
            .levels
            .clone(),
    };
    let mut d = second_def();
    d.attack_potential.as_mut().expect("ap").factors = (0..16).map(factor).collect();
    at_cap(&d, "16 feasibility factors");
    d.attack_potential
        .as_mut()
        .expect("ap")
        .factors
        .push(factor(16));
    over_cap(
        &d,
        Limit::FeasibilityFactors { max: 16 },
        "17 feasibility factors",
    );

    // Factor levels cap.
    let flvl = |i: usize| FactorLevelDef {
        key: key_of_len("al", i, 6),
        label: "AL".into(),
        points: i as u16,
    };
    let mut d = second_def();
    d.attack_potential.as_mut().expect("ap").factors[0].levels = (0..16).map(flvl).collect();
    at_cap(&d, "16 factor levels");
    d.attack_potential.as_mut().expect("ap").factors[0]
        .levels
        .push(flvl(16));
    over_cap(&d, Limit::Levels { max: 16 }, "17 factor levels");

    // Potential bands cap.
    let band = |i: usize, last: bool| PotentialBandDef {
        min: i as u16,
        max: (!last).then_some(i as u16),
        feasibility: "frequent".into(),
    };
    let mut d = second_def();
    d.attack_potential.as_mut().expect("ap").bands = (0..16).map(|i| band(i, i == 15)).collect();
    at_cap(&d, "16 potential bands");
    d.attack_potential.as_mut().expect("ap").bands = (0..17).map(|i| band(i, i == 16)).collect();
    over_cap(&d, Limit::Levels { max: 16 }, "17 potential bands");
}

#[test]
fn risk_level_and_matrix_count_caps() {
    let level = |i: usize| LevelDef {
        key: key_of_len("r", i, 6),
        label: "R".into(),
    };
    let mut d = second_def();
    d.risk_levels = (0..16).map(level).collect();
    let r0 = Arc::clone(&d.risk_levels[0].key);
    for c in &mut d.impact_categories {
        for row in &mut c.risk_matrix {
            for cell in row {
                *cell = Arc::clone(&r0);
            }
        }
    }
    at_cap(&d, "16 risk levels");
    d.risk_levels.push(level(16));
    over_cap(&d, Limit::Levels { max: 16 }, "17 risk levels");

    // Matrix row and column caps.
    let mut d = second_def();
    d.impact_categories[0]
        .risk_matrix
        .resize(17, vec!["acceptable".into(); 5]);
    over_cap(&d, Limit::Levels { max: 16 }, "17 risk matrix rows");

    let mut d = second_def();
    d.impact_categories[0].risk_matrix[0].resize(17, "acceptable".into());
    over_cap(&d, Limit::Levels { max: 16 }, "17 risk matrix cols");
}

#[test]
fn property_count_cap() {
    let mut d = wide_properties();
    at_cap(&d, "64 properties");
    d.properties.push(PropertyDef {
        key: "extra".into(),
        label: "Extra".into(),
    });
    over_cap(&d, Limit::Properties { max: 64 }, "65 properties");
}

#[test]
fn violates_count_cap() {
    let mut d = wide_properties();
    let all: Vec<Arc<str>> = d.properties.iter().map(|p| Arc::clone(&p.key)).collect();
    d.threat_categories[1].violates = all.clone();
    at_cap(&d, "violates 64");
    d.threat_categories[1].violates.push(Arc::clone(&all[0]));
    over_cap(&d, Limit::Properties { max: 64 }, "violates 65");
}

#[test]
fn threat_category_count_cap() {
    let threat = |i: usize| ThreatCategoryDef {
        key: key_of_len("t", i, 6),
        label: "T".into(),
        violates: vec!["c".into()],
    };
    let mut d = second_def();
    d.threat_categories = (0..64).map(threat).collect();
    at_cap(&d, "64 threat categories");
    d.threat_categories.push(threat(64));
    over_cap(&d, Limit::ThreatCategories { max: 64 }, "65");
}

#[test]
fn catalog_scheme_count_cap() {
    let scheme = |i: usize| CatalogSchemeDef {
        key: key_of_len("s", i, 6),
        ..second_def().catalog_schemes[0].clone()
    };
    let mut d = second_def();
    d.catalog_schemes = (0..16).map(scheme).collect();
    at_cap(&d, "16 catalog schemes");
    d.catalog_schemes.push(scheme(16));
    over_cap(&d, Limit::CatalogSchemes { max: 16 }, "17");
}

#[test]
fn catalog_segment_count_cap() {
    let seg = SegmentDef {
        min: 1,
        max: 9,
        digits: None,
    };
    let mut d = second_def();
    d.catalog_schemes[0].syntax.segments = vec![seg; 4];
    at_cap(&d, "4 segments");
    d.catalog_schemes[0].syntax.segments.push(seg);
    over_cap(&d, Limit::CatalogSegments { max: 4 }, "5 segments");
}

#[test]
fn pack_file_cap() {
    let cap = usize::try_from(MAX_PACK_FILE_BYTES).expect("fits");
    let mut bytes = pack_bytes(&second_def());
    // Pad with trailing whitespace to exactly the cap: it loads.
    bytes.resize(cap, b' ');
    let m = Methodology::read_pack(bytes.as_slice()).expect("at the cap");
    assert_eq!(m.key(), "med-device-demo");
    // One more byte breaks it.
    bytes.push(b' ');
    assert!(matches!(
        Methodology::read_pack(bytes.as_slice()),
        Err(PackError::LimitExceeded(Limit::PackFileBytes { max })) if max == MAX_PACK_FILE_BYTES
    ));
}

/// Counts the bytes pulled from the inner reader.
struct Counting<R> {
    inner: R,
    pulled: u64,
}

impl<R: Read> Read for Counting<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.pulled += n as u64;
        Ok(n)
    }
}

#[test]
fn pack_file_cap_stops_reading() {
    for &fill in b" [" {
        let mut source = Counting {
            inner: std::io::repeat(fill),
            pulled: 0,
        };
        let err = MethodologyDef::read_pack(&mut source).err();
        if fill == b' ' {
            assert!(
                matches!(
                    err,
                    Some(PackError::LimitExceeded(Limit::PackFileBytes { .. }))
                ),
                "{err:?}"
            );
        } else {
            // Endless `[`: a shape error stops it long before the cap.
            assert!(err.is_some());
        }
        assert!(
            source.pulled <= MAX_PACK_FILE_BYTES + 1,
            "{}",
            source.pulled
        );
    }
    // An endless string inside a valid prefix also stops at the cap.
    let head = br#"{"format":"aegis-methodology-pack","version":1,"methodology":{"key":""#;
    let mut source = Counting {
        inner: head.chain(std::io::repeat(b'a')),
        pulled: 0,
    };
    let err = MethodologyDef::read_pack(&mut source).err();
    assert!(
        matches!(
            err,
            Some(PackError::LimitExceeded(Limit::PackFileBytes { .. }))
        ),
        "{err:?}"
    );
    assert!(source.pulled <= MAX_PACK_FILE_BYTES + 1);
}

#[test]
fn maximal_pack_loads_below_the_file_cap() {
    let def = max_def();
    let bytes = pack_bytes(&def);
    eprintln!(
        "maximal pack: {} bytes ({:.2} MiB), cap {} bytes",
        bytes.len(),
        bytes.len() as f64 / (1024.0 * 1024.0),
        MAX_PACK_FILE_BYTES
    );
    assert!((bytes.len() as u64) < MAX_PACK_FILE_BYTES);
    // Every count at its max.
    assert_eq!(def.impact_categories.len(), 32);
    assert!(def.impact_categories.iter().all(|c| c.levels.len() == 16));
    assert_eq!(def.properties.len(), 64);
    assert_eq!(def.threat_categories.len(), 64);
    assert!(def.threat_categories.iter().all(|t| t.violates.len() == 64));
    assert_eq!(def.catalog_schemes.len(), 16);
    assert_eq!(def.feasibility_levels.len(), 16);
    assert_eq!(def.risk_levels.len(), 16);
    let ap: &AttackPotentialDef = def.attack_potential.as_ref().expect("ap");
    assert_eq!(ap.factors.len(), 16);
    assert!(ap.factors.iter().all(|f| f.levels.len() == 16));
    assert_eq!(ap.bands.len(), 16);
    assert!(
        def.impact_categories
            .iter()
            .all(|c| c.risk_matrix.len() == 16 && c.risk_matrix.iter().all(|r| r.len() == 16))
    );
    let m = Methodology::read_pack(bytes.as_slice()).expect("maximal pack loads");
    assert_eq!(Methodology::new(def).ok(), Some(m));
}

/// The second definition with 32 interface kinds, 17 attack types and one
/// preset that uses the first `interfaces` interface kinds and the first
/// `rules` attack types.
fn with_preset(interfaces: usize, rules: usize) -> MethodologyDef {
    let entry = |tag: &str, i: usize| LevelDef {
        key: key_of_len(tag, i, 8),
        label: "X".into(),
    };
    let mut d = second_def();
    let low = Arc::clone(&d.feasibility_levels[0].key);
    d.element_names.interface_kinds = (0..32).map(|i| entry("ik", i)).collect();
    d.element_names.attack_types = (0..17).map(|i| entry("at", i)).collect();
    d.element_names.control_kinds = vec![ControlKindDef {
        key: "kind".into(),
        label: "Kind".into(),
        interface_kinds: d.element_names.interface_kinds[..interfaces]
            .iter()
            .map(|e| Arc::clone(&e.key))
            .collect(),
        rules: d.element_names.attack_types[..rules]
            .iter()
            .map(|e| (Arc::clone(&e.key), Arc::clone(&low)))
            .collect(),
        conflicts_with: Vec::new(),
    }];
    d
}

#[test]
fn control_kind_count_cap() {
    let many = |n: usize| {
        let mut d = with_preset(0, 1);
        let one = d.element_names.control_kinds[0].clone();
        d.element_names.control_kinds = (0..n)
            .map(|i| ControlKindDef {
                key: key_of_len("ck", i, 8),
                ..one.clone()
            })
            .collect();
        d
    };
    at_cap(&many(64), "64 control kinds");
    over_cap(
        &many(65),
        Limit::ControlKinds { max: 64 },
        "65 control kinds",
    );
}

#[test]
fn control_kind_rule_cap() {
    at_cap(&with_preset(0, 16), "16 rules");
    over_cap(
        &with_preset(0, 17),
        Limit::ControlRules { max: 16 },
        "17 rules",
    );
}

#[test]
fn control_kind_interface_kind_cap() {
    at_cap(&with_preset(32, 1), "32 interface kinds");
    let mut d = with_preset(32, 1);
    d.element_names.control_kinds[0]
        .interface_kinds
        .push("extra".into());
    over_cap(&d, Limit::InterfaceKinds { max: 32 }, "33 interface kinds");
}

#[test]
fn control_kind_conflicts_cap() {
    // 64 presets: first lists the other 63.
    let many = |n: usize| {
        let mut d = with_preset(0, 1);
        let one = d.element_names.control_kinds[0].clone();
        d.element_names.control_kinds = (0..n)
            .map(|i| {
                let mut k = ControlKindDef {
                    key: key_of_len("ck", i, 8),
                    ..one.clone()
                };
                if i == 0 {
                    k.conflicts_with = (1..n).map(|j| key_of_len("ck", j, 8)).collect();
                }
                k
            })
            .collect();
        d
    };
    at_cap(&many(64), "64 conflicts");
    // The cap is checked at decode, before the keys are resolved.
    let mut d = with_preset(0, 1);
    d.element_names.control_kinds[0].conflicts_with =
        (0..65).map(|i| key_of_len("ck", i + 1, 8)).collect();
    over_cap(&d, Limit::ControlKindConflicts { max: 64 }, "65 conflicts");
}

#[test]
fn control_kind_key_and_label_caps() {
    let mut d = with_preset(0, 1);
    d.element_names.control_kinds[0].key = key_of_len("k", 7, 64);
    at_cap(&d, "64-byte preset key");
    d.element_names.control_kinds[0].key = key_of_len("k", 7, 65);
    over_cap(&d, Limit::KeyBytes { max: 64 }, "65-byte preset key");

    let mut d = with_preset(0, 1);
    d.element_names.control_kinds[0].label = "\u{1f600}".repeat(256).into();
    at_cap(&d, "1024-byte preset label");
    d.element_names.control_kinds[0].label = format!("x{}", "\u{1f600}".repeat(256)).into();
    over_cap(
        &d,
        Limit::LabelBytes { max: 1024 },
        "1025-byte preset label",
    );
}
