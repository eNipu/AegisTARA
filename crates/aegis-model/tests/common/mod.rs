//! Shared test kit for the project tests: the methodologies under test, a
//! deterministic ID source, and a random op generator.
//!
//! Compiled into the integration tests (`mod common;`) and into the unit
//! tests (`src/lib.rs` includes it as `testkit`), so it uses only the public
//! API.

#![allow(dead_code)]
// Test code: the crate's pedantic lint set is for library code.
#![allow(clippy::pedantic)]

use std::sync::Arc;

use aegis_model::{
    AssetId, CatalogRef, CatalogScheme, CatalogSchemeDef, CatalogSyntax, DamageId, ImpactCategory,
    ImpactCategoryDef, Level, LevelDef, Methodology, MethodologyDef, Op, Project, Property,
    PropertyDef, PropertySet, SegmentDef, ThreatCategory, ThreatCategoryDef, ThreatId,
};
use proptest::prelude::*;
use proptest::test_runner::TestCaseError;

/// Sentinel name used where tests check that errors never leak names.
pub const SENTINEL: &str = "SECRET-LABEL-9f3";

// ---------------------------------------------------------------------------
// Methodologies.
// ---------------------------------------------------------------------------

/// The built-in methodology.
pub fn builtin() -> Arc<Methodology> {
    Arc::new(Methodology::iso21434_r155().expect("built-in methodology is valid"))
}

fn level_defs(keys: &[&str]) -> Vec<LevelDef> {
    keys.iter()
        .map(|&k| LevelDef {
            key: k.into(),
            label: k.into(),
        })
        .collect()
}

/// The second methodology (AC7): a non-automotive, medical-device-style
/// methodology declared purely as data. Three impact categories with
/// scales of 3, 5, and 2 levels; only the properties C, I, and A; a threat
/// category that violates two properties; and a CAPEC-style scheme.
pub fn second_def() -> MethodologyDef {
    let s = |k: &str| -> Arc<str> { k.into() };
    MethodologyDef {
        key: s("med-device-demo"),
        version: s("2026.1"),
        label: s("Medical device demo"),
        impact_categories: vec![
            ImpactCategoryDef {
                key: s("patient-harm"),
                label: s("Patient harm"),
                levels: level_defs(&["minor", "serious", "critical"]),
            },
            ImpactCategoryDef {
                key: s("data-exposure"),
                label: s("Data exposure"),
                levels: level_defs(&["trivial", "low", "medium", "high", "extreme"]),
            },
            ImpactCategoryDef {
                key: s("service-loss"),
                label: s("Service loss"),
                levels: level_defs(&["tolerable", "intolerable"]),
            },
        ],
        properties: ["c", "i", "a"]
            .iter()
            .map(|&k| PropertyDef {
                key: s(k),
                label: s(&k.to_uppercase()),
            })
            .collect(),
        threat_categories: vec![
            ThreatCategoryDef {
                key: s("data-leak"),
                label: s("Data leak"),
                violates: vec![s("c")],
            },
            ThreatCategoryDef {
                key: s("corruption"),
                label: s("Corruption"),
                violates: vec![s("i")],
            },
            ThreatCategoryDef {
                key: s("outage"),
                label: s("Outage"),
                violates: vec![s("a")],
            },
            ThreatCategoryDef {
                key: s("ransomware"),
                label: s("Ransomware"),
                violates: vec![s("i"), s("a")],
            },
        ],
        catalog_schemes: vec![CatalogSchemeDef {
            key: s("capec"),
            label: s("CAPEC"),
            syntax: CatalogSyntax {
                prefix: s("CAPEC-"),
                segments: vec![SegmentDef {
                    min: 1,
                    max: 9999,
                    digits: None,
                }],
                required: 1,
            },
        }],
    }
}

/// The second methodology, validated.
pub fn second() -> Arc<Methodology> {
    Arc::new(Methodology::new(second_def()).expect("second methodology is valid"))
}

/// A methodology larger than both under test in every vocabulary. Its
/// indices are out of range for them, so the generator draws invalid
/// indices from it.
pub fn wide() -> Methodology {
    let s = |k: String| -> Arc<str> { k.into() };
    let free = SegmentDef {
        min: 0,
        max: 65535,
        digits: None,
    };
    let scheme = |k: &str| CatalogSchemeDef {
        key: k.into(),
        label: k.into(),
        syntax: CatalogSyntax {
            prefix: "".into(),
            segments: vec![free; 4],
            required: 1,
        },
    };
    Methodology::new(MethodologyDef {
        key: "wide".into(),
        version: "1".into(),
        label: "Wide".into(),
        impact_categories: (0..8)
            .map(|i| ImpactCategoryDef {
                key: s(format!("c{i}")),
                label: s(format!("c{i}")),
                levels: (0..16)
                    .map(|l| LevelDef {
                        key: s(format!("l{l}")),
                        label: s(format!("l{l}")),
                    })
                    .collect(),
            })
            .collect(),
        properties: (0..12)
            .map(|i| PropertyDef {
                key: s(format!("p{i}")),
                label: s(format!("p{i}")),
            })
            .collect(),
        threat_categories: (0..12)
            .map(|i| ThreatCategoryDef {
                key: s(format!("t{i}")),
                label: s(format!("t{i}")),
                violates: vec!["p0".into()],
            })
            .collect(),
        catalog_schemes: vec![scheme("w0"), scheme("w1"), scheme("w2")],
    })
    .expect("wide methodology is valid")
}

// ---------------------------------------------------------------------------
// Deterministic IDs.
// ---------------------------------------------------------------------------

/// A `UUIDv7` built from a counter, so runs are reproducible.
pub fn v7(n: u64) -> uuid::Uuid {
    let mut bytes = [0u8; 10];
    bytes[2..].copy_from_slice(&n.to_be_bytes());
    uuid::Builder::from_unix_timestamp_millis(1_760_000_000_000 + n, &bytes).into_uuid()
}

pub fn asset_id(n: u64) -> AssetId {
    AssetId::from_uuid(v7(n)).expect("v7")
}

pub fn damage_id(n: u64) -> DamageId {
    DamageId::from_uuid(v7(n)).expect("v7")
}

pub fn threat_id(n: u64) -> ThreatId {
    ThreatId::from_uuid(v7(n)).expect("v7")
}

// ---------------------------------------------------------------------------
// Vocabulary helpers.
// ---------------------------------------------------------------------------

pub fn prop(m: &Methodology, key: &str) -> Property {
    m.property(key).expect(key)
}

pub fn cat(m: &Methodology, key: &str) -> ImpactCategory {
    m.impact_category(key).expect(key)
}

pub fn lvl(m: &Methodology, c: ImpactCategory, key: &str) -> Level {
    m.level(c, key).expect(key)
}

pub fn threat_cat(m: &Methodology, key: &str) -> ThreatCategory {
    m.threat_category(key).expect(key)
}

pub fn scheme(m: &Methodology, key: &str) -> CatalogScheme {
    m.catalog_scheme(key).expect(key)
}

pub fn cref(m: &Methodology, s: CatalogScheme, text: &str) -> CatalogRef {
    m.parse_catalog_ref(s, text).expect(text)
}

pub fn props(m: &Methodology, keys: &[&str]) -> PropertySet {
    keys.iter().map(|k| prop(m, k)).collect()
}

// ---------------------------------------------------------------------------
// Random ops.
// ---------------------------------------------------------------------------

/// Raw random choices for one op. [`Gen::op`] turns a seed into a concrete
/// op against the current project, so ops hit existing entities often.
#[derive(Clone, Debug)]
pub struct Seed {
    kind: u8,
    a: u16,
    b: u16,
    c: u16,
    bits: u64,
    list: Vec<u16>,
}

/// One seed.
pub fn seed() -> impl Strategy<Value = Seed> {
    (
        0u8..22,
        any::<u16>(),
        any::<u16>(),
        any::<u16>(),
        any::<u64>(),
        prop::collection::vec(any::<u16>(), 0..5),
    )
        .prop_map(|(kind, a, b, c, bits, list)| Seed {
            kind,
            a,
            b,
            c,
            bits,
            list,
        })
}

/// Sequences of 0 to 64 seeds.
pub fn seeds() -> impl Strategy<Value = Vec<Seed>> {
    prop::collection::vec(seed(), 0..=64)
}

/// Turns seeds into ops for one methodology.
pub struct Gen {
    m: Arc<Methodology>,
    wide: Methodology,
    next: u64,
}

/// Picks item `n` modulo the length, or `None` if there are no items.
fn nth<T>(mut items: Vec<T>, n: u16) -> Option<T> {
    if items.is_empty() {
        None
    } else {
        let i = usize::from(n) % items.len();
        Some(items.swap_remove(i))
    }
}

impl Gen {
    pub fn new(m: Arc<Methodology>) -> Self {
        Gen {
            m,
            wide: wide(),
            next: 0,
        }
    }

    fn fresh(&mut self) -> u64 {
        self.next += 1;
        1_000_000 + self.next
    }

    fn name(&self, sel: u16) -> Arc<str> {
        match sel % 40 {
            0 => "".into(),
            1 => "  \t ".into(),
            2 => "x".repeat(257).into(),
            n => format!("{SENTINEL}-{n}").into(),
        }
    }

    fn asset(&mut self, p: &Project, sel: u16) -> AssetId {
        let existing: Vec<_> = p.assets().map(|(_, id, _)| id).collect();
        match nth(existing, sel / 8) {
            Some(id) if sel % 8 != 0 => id,
            _ => asset_id(self.fresh()),
        }
    }

    fn damage(&mut self, p: &Project, sel: u16) -> DamageId {
        let existing: Vec<_> = p.damage_scenarios().map(|(_, id, _)| id).collect();
        match nth(existing, sel / 8) {
            Some(id) if sel % 8 != 0 => id,
            _ => damage_id(self.fresh()),
        }
    }

    fn threat(&mut self, p: &Project, sel: u16) -> ThreatId {
        let existing: Vec<_> = p.threat_scenarios().map(|(_, id, _)| id).collect();
        match nth(existing, sel / 8) {
            Some(id) if sel % 8 != 0 => id,
            _ => threat_id(self.fresh()),
        }
    }

    /// A property, out of range for the methodology about 1 time in 12.
    fn property(&self, sel: u16) -> Property {
        let n = self.m.property_count();
        if sel % 12 == 0 {
            let extra = self.wide.property_count() - n;
            self.wide
                .properties()
                .nth(n + usize::from(sel / 12) % extra)
                .expect("wide property")
        } else {
            self.m
                .properties()
                .nth(usize::from(sel) % n)
                .expect("property")
        }
    }

    fn property_set(&self, bits: u64) -> PropertySet {
        match bits % 16 {
            0..=2 => self.m.all_properties(),
            3 => PropertySet::empty(),
            4 => self.m.all_properties().with(self.property(0)),
            _ => self
                .m
                .properties()
                .filter(|p| (bits >> (8 + p.index())) & 1 == 1)
                .collect(),
        }
    }

    fn impact_category(&self, sel: u16) -> ImpactCategory {
        let n = self.m.impact_category_count();
        if sel % 12 == 0 {
            let extra = self.wide.impact_category_count() - n;
            self.wide
                .impact_categories()
                .nth(n + usize::from(sel / 12) % extra)
                .expect("wide category")
        } else {
            self.m
                .impact_categories()
                .nth(usize::from(sel) % n)
                .expect("category")
        }
    }

    /// A level for `c`, out of that scale about 1 time in 10.
    fn level(&self, c: ImpactCategory, sel: u16) -> Level {
        let n = self.m.level_count(c);
        let w = self.wide.impact_categories().next().expect("wide category");
        if n == 0 || sel % 10 == 0 {
            self.wide
                .levels(w)
                .nth(n + usize::from(sel / 10) % (16 - n))
                .expect("wide level")
        } else {
            self.m.levels(c).nth(usize::from(sel) % n).expect("level")
        }
    }

    fn threat_category(&self, sel: u16) -> ThreatCategory {
        let n = self.m.threat_category_count();
        if sel % 12 == 0 {
            let extra = self.wide.threat_category_count() - n;
            self.wide
                .threat_categories()
                .nth(n + usize::from(sel / 12) % extra)
                .expect("wide threat category")
        } else {
            self.m
                .threat_categories()
                .nth(usize::from(sel) % n)
                .expect("threat category")
        }
    }

    /// A threat category, usually one the asset can satisfy.
    fn fitting_category(&self, p: &Project, asset: AssetId, sel: u16) -> ThreatCategory {
        let props = p
            .asset_idx(&asset)
            .and_then(|i| p.asset(i))
            .map(|a| a.properties());
        let fitting: Vec<_> = self
            .m
            .threat_categories()
            .filter(|&t| props.is_some_and(|ps| self.m.violates(t).is_subset(ps)))
            .collect();
        match nth(fitting, sel / 4) {
            Some(t) if sel % 4 != 0 => t,
            _ => self.threat_category(sel),
        }
    }

    /// A catalog ref: usually valid for the methodology's schemes; else
    /// parsed under the wide methodology, so its scheme or segments may be
    /// out of range.
    fn catalog_ref(&self, sel: u16, bits: u64) -> CatalogRef {
        let schemes: Vec<_> = self.m.catalog_schemes().collect();
        if let Some(s) = nth(schemes, sel).filter(|_| sel % 10 != 0) {
            let syntax = self.m.catalog_syntax(s).expect("syntax");
            let required = usize::from(syntax.required);
            let len = required + (bits as usize) % (syntax.segments.len() - required + 1);
            let mut text = syntax.prefix.to_string();
            for (i, seg) in syntax.segments.iter().take(len).enumerate() {
                if i > 0 {
                    text.push('.');
                }
                let span = u64::from(seg.max - seg.min) + 1;
                // Small values make duplicates likely.
                let v = u64::from(seg.min) + (bits >> (4 + 8 * i)) % span.min(6);
                match seg.digits {
                    Some(w) => text.push_str(&format!("{v:0w$}", w = usize::from(w))),
                    None => text.push_str(&v.to_string()),
                }
            }
            self.m
                .parse_catalog_ref(s, &text)
                .expect("generated ref is valid")
        } else {
            let ws = self
                .wide
                .catalog_schemes()
                .nth(usize::from(sel / 10) % self.wide.catalog_scheme_count())
                .expect("wide scheme");
            let text = format!("{}.{}", bits % 40, (bits >> 8) % 120);
            self.wide
                .parse_catalog_ref(ws, &text)
                .expect("wide ref is valid")
        }
    }

    /// Builds a concrete op from a seed against the current project.
    pub fn op(&mut self, p: &Project, s: &Seed) -> Op {
        match s.kind {
            0..=2 => Op::AddAsset {
                id: if s.a % 16 == 0 {
                    self.asset(p, s.b)
                } else {
                    asset_id(self.fresh())
                },
                name: self.name(s.c),
                properties: self.property_set(s.bits),
            },
            3..=5 => {
                let impacts = s
                    .list
                    .iter()
                    .map(|&x| {
                        let c = self.impact_category(x);
                        (c, self.level(c, x.rotate_left(5)))
                    })
                    .collect();
                Op::AddDamageScenario {
                    id: if s.a % 16 == 0 {
                        self.damage(p, s.b)
                    } else {
                        damage_id(self.fresh())
                    },
                    name: self.name(s.c),
                    impacts,
                }
            }
            6..=8 => {
                let asset = self.asset(p, s.b);
                let category = self.fitting_category(p, asset, s.a);
                let n_refs = if s.bits % 64 == 0 {
                    17
                } else {
                    s.list.len() % 3
                };
                let catalog_refs = (0..n_refs)
                    .map(|i| {
                        let x = s.list.get(i).copied().unwrap_or(s.a);
                        self.catalog_ref(x.wrapping_add(i as u16), s.bits.rotate_left(i as u32 * 7))
                    })
                    .collect();
                let damages = s.list.iter().skip(1).map(|&x| self.damage(p, x)).collect();
                Op::AddThreatScenario {
                    id: if s.c % 16 == 0 {
                        self.threat(p, s.a)
                    } else {
                        threat_id(self.fresh())
                    },
                    name: self.name(s.c),
                    asset,
                    category,
                    catalog_refs,
                    damages,
                }
            }
            9 => Op::RenameAsset {
                id: self.asset(p, s.a),
                name: self.name(s.c),
            },
            10 => Op::SetAssetProperties {
                id: self.asset(p, s.a),
                properties: self.property_set(s.bits),
            },
            11 => Op::RemoveAsset {
                id: self.asset(p, s.a),
            },
            12 => Op::RenameDamageScenario {
                id: self.damage(p, s.a),
                name: self.name(s.c),
            },
            13 => {
                let category = self.impact_category(s.b);
                Op::SetImpact {
                    id: self.damage(p, s.a),
                    category,
                    level: (s.c % 5 != 0).then(|| self.level(category, s.c)),
                }
            }
            14 => Op::RemoveDamageScenario {
                id: self.damage(p, s.a),
            },
            15 => Op::RenameThreatScenario {
                id: self.threat(p, s.a),
                name: self.name(s.c),
            },
            16 => {
                let id = self.threat(p, s.a);
                let asset = p
                    .threat_scenario_idx(&id)
                    .and_then(|t| p.threat_scenario(t))
                    .and_then(|t| p.asset_id(t.asset()));
                let category = match asset {
                    Some(a) => self.fitting_category(p, a, s.b),
                    None => self.threat_category(s.b),
                };
                Op::SetThreatCategory { id, category }
            }
            17 => Op::AddCatalogRef {
                threat: self.threat(p, s.a),
                catalog_ref: self.catalog_ref(s.b, s.bits),
            },
            18 => {
                let threat = self.threat(p, s.a);
                let existing = p
                    .threat_scenario_idx(&threat)
                    .and_then(|t| p.threat_scenario(t))
                    .map(|t| t.catalog_refs().to_vec())
                    .unwrap_or_default();
                let catalog_ref = match nth(existing, s.b) {
                    Some(r) if s.c % 3 != 0 => r,
                    _ => self.catalog_ref(s.b, s.bits),
                };
                Op::RemoveCatalogRef {
                    threat,
                    catalog_ref,
                }
            }
            19 => Op::LinkDamage {
                threat: self.threat(p, s.a),
                damage: self.damage(p, s.b),
            },
            20 => {
                let threat = self.threat(p, s.a);
                let linked: Vec<_> = p
                    .threat_scenario_idx(&threat)
                    .and_then(|t| p.threat_scenario(t))
                    .map(|t| {
                        t.damages()
                            .iter()
                            .filter_map(|&d| p.damage_scenario_id(d))
                            .collect()
                    })
                    .unwrap_or_default();
                let damage = match nth(linked, s.b) {
                    Some(d) if s.c % 3 != 0 => d,
                    _ => self.damage(p, s.b),
                };
                Op::UnlinkDamage { threat, damage }
            }
            _ => Op::RemoveThreatScenario {
                id: self.threat(p, s.a),
            },
        }
    }
}

/// Statistics of one run, to check the generator exercises valid ops.
#[derive(Debug, Default)]
pub struct Stats {
    pub ok: usize,
    pub failed: usize,
    /// Op variants that succeeded at least once.
    pub ok_kinds: std::collections::BTreeSet<String>,
    /// Error variants seen at least once.
    pub errors: std::collections::BTreeSet<String>,
}

/// The variant name of a `Debug` value: the text before the first space,
/// `(`, or `{`.
pub fn variant(v: &impl std::fmt::Debug) -> String {
    let text = format!("{v:?}");
    text.split([' ', '(', '{']).next().unwrap_or("").to_string()
}

/// Replays `seeds` on a fresh project over `m` and checks, for every op:
/// `validate` agrees with `apply`; a failed op leaves the project
/// unchanged; the inverse restores the previous project. At the end the
/// inverses, applied in reverse, give `Project::new(m)`. `check` runs on the
/// project after every op and every undo.
pub fn replay(
    m: &Arc<Methodology>,
    seeds: &[Seed],
    check: impl Fn(&Project) -> Result<(), TestCaseError>,
) -> Result<Stats, TestCaseError> {
    let mut generator = Gen::new(Arc::clone(m));
    let mut project = Project::new(Arc::clone(m));
    let mut undo = Vec::new();
    let mut stats = Stats::default();
    for s in seeds {
        let op = generator.op(&project, s);
        let before = project.clone();
        let validated = project.validate(&op);
        match project.apply(op.clone()) {
            Err(e) => {
                stats.failed += 1;
                stats.errors.insert(variant(&e));
                prop_assert_eq!(validated, Err(e));
                prop_assert_eq!(&project, &before);
            }
            Ok(inverse) => {
                stats.ok += 1;
                stats.ok_kinds.insert(variant(&op));
                prop_assert_eq!(validated, Ok(()));
                let mut back = project.clone();
                let again = back.apply(inverse.clone());
                prop_assert!(
                    again.is_ok(),
                    "inverse {:?} of {:?} failed: {:?}",
                    inverse,
                    op,
                    again
                );
                prop_assert_eq!(&back, &before);
                check(&back)?;
                undo.push(inverse);
            }
        }
        check(&project)?;
    }
    while let Some(inverse) = undo.pop() {
        let r = project.apply(inverse);
        prop_assert!(r.is_ok(), "undo failed: {:?}", r);
        check(&project)?;
    }
    prop_assert_eq!(&project, &Project::new(Arc::clone(m)));
    Ok(stats)
}
