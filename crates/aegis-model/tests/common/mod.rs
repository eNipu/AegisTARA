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
    AssetId, AttackTree, CatalogRef, CatalogScheme, CatalogSchemeDef, CatalogSyntax, ControlId,
    DamageId, FeasibilityLevel, GateKind, ImpactCategory, ImpactCategoryDef, Level, LevelDef,
    Methodology, MethodologyDef, Node, NodeId, Op, Project, Property, PropertyDef, PropertySet,
    Provenance, ProvenanceSource, RiskLevel, RiskTreatment, SegmentDef, ThreatCategory,
    ThreatCategoryDef, ThreatId, TreeId,
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
    let matrix = |rows: &[&[&str]]| -> Vec<Vec<Arc<str>>> {
        rows.iter()
            .map(|r| r.iter().map(|&k| s(k)).collect())
            .collect()
    };
    MethodologyDef {
        key: s("med-device-demo"),
        version: s("2026.1"),
        label: s("Medical device demo"),
        impact_categories: vec![
            ImpactCategoryDef {
                key: s("patient-harm"),
                label: s("Patient harm"),
                levels: level_defs(&["minor", "serious", "critical"]),
                risk_matrix: matrix(&[
                    &[
                        "acceptable",
                        "acceptable",
                        "acceptable",
                        "monitor",
                        "monitor",
                    ],
                    &[
                        "acceptable",
                        "monitor",
                        "monitor",
                        "unacceptable",
                        "unacceptable",
                    ],
                    &[
                        "monitor",
                        "unacceptable",
                        "unacceptable",
                        "unacceptable",
                        "unacceptable",
                    ],
                ]),
            },
            ImpactCategoryDef {
                key: s("data-exposure"),
                label: s("Data exposure"),
                levels: level_defs(&["trivial", "low", "medium", "high", "extreme"]),
                risk_matrix: matrix(&[
                    &[
                        "acceptable",
                        "acceptable",
                        "acceptable",
                        "acceptable",
                        "acceptable",
                    ],
                    &[
                        "acceptable",
                        "acceptable",
                        "acceptable",
                        "monitor",
                        "monitor",
                    ],
                    &[
                        "acceptable",
                        "acceptable",
                        "monitor",
                        "monitor",
                        "unacceptable",
                    ],
                    &[
                        "acceptable",
                        "monitor",
                        "monitor",
                        "unacceptable",
                        "unacceptable",
                    ],
                    &[
                        "monitor",
                        "monitor",
                        "unacceptable",
                        "unacceptable",
                        "unacceptable",
                    ],
                ]),
            },
            ImpactCategoryDef {
                key: s("service-loss"),
                label: s("Service loss"),
                levels: level_defs(&["tolerable", "intolerable"]),
                risk_matrix: matrix(&[
                    &[
                        "acceptable",
                        "acceptable",
                        "acceptable",
                        "monitor",
                        "monitor",
                    ],
                    &[
                        "monitor",
                        "monitor",
                        "unacceptable",
                        "unacceptable",
                        "unacceptable",
                    ],
                ]),
            },
        ],
        feasibility_levels: vec![
            LevelDef {
                key: s("improbable"),
                label: s("Improbable"),
            },
            LevelDef {
                key: s("remote"),
                label: s("Remote"),
            },
            LevelDef {
                key: s("occasional"),
                label: s("Occasional"),
            },
            LevelDef {
                key: s("probable"),
                label: s("Probable"),
            },
            LevelDef {
                key: s("frequent"),
                label: s("Frequent"),
            },
        ],
        risk_levels: vec![
            LevelDef {
                key: s("acceptable"),
                label: s("Acceptable"),
            },
            LevelDef {
                key: s("monitor"),
                label: s("Monitor"),
            },
            LevelDef {
                key: s("unacceptable"),
                label: s("Unacceptable"),
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
                risk_matrix: (0..16)
                    .map(|l| (0..16).map(|f| s(format!("r{}", (l + f) % 16))).collect())
                    .collect(),
            })
            .collect(),
        feasibility_levels: (0..16)
            .map(|l| LevelDef {
                key: s(format!("f{l}")),
                label: s(format!("f{l}")),
            })
            .collect(),
        risk_levels: (0..16)
            .map(|r| LevelDef {
                key: s(format!("r{r}")),
                label: s(format!("r{r}")),
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

pub fn tree_id(n: u64) -> TreeId {
    TreeId::from_uuid(v7(n)).expect("v7")
}

pub fn node_id(n: u64) -> NodeId {
    NodeId::from_uuid(v7(n)).expect("v7")
}

pub fn control_id(n: u64) -> ControlId {
    ControlId::from_uuid(v7(n)).expect("v7")
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

pub fn feas(m: &Methodology, key: &str) -> FeasibilityLevel {
    m.feasibility_level(key).expect(key)
}

pub fn risk(m: &Methodology, key: &str) -> RiskLevel {
    m.risk_level(key).expect(key)
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
        0u8..36,
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

    fn provenance(&self, sel: u16, bits: u64) -> Provenance {
        let source = match bits % 4 {
            0 => ProvenanceSource::Human,
            1 => ProvenanceSource::Import,
            2 => ProvenanceSource::Catalog,
            _ => ProvenanceSource::Ai,
        };
        let ts = bits.rotate_left(19);
        match sel % 40 {
            0 => Provenance::new("  \t ", ts, source),
            1 => Provenance::new("x".repeat(257), ts, source),
            2..=9 => Provenance::default(),
            n => Provenance::new(format!("{SENTINEL}-actor-{n}"), ts, source),
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

    fn tree(&mut self, p: &Project, sel: u16) -> TreeId {
        let existing: Vec<_> = p.attack_trees().map(|(_, id, _)| id).collect();
        match nth(existing, sel / 8) {
            Some(id) if sel % 8 != 0 => id,
            _ => tree_id(self.fresh()),
        }
    }

    fn control(&mut self, p: &Project, sel: u16) -> ControlId {
        let existing: Vec<_> = p.security_controls().map(|(_, id, _)| id).collect();
        match nth(existing, sel / 8) {
            Some(id) if sel % 8 != 0 => id,
            _ => control_id(self.fresh()),
        }
    }

    fn treatment(&self, sel: u16) -> RiskTreatment {
        match sel % 4 {
            0 => RiskTreatment::Avoid,
            1 => RiskTreatment::Reduce,
            2 => RiskTreatment::Share,
            _ => RiskTreatment::Retain,
        }
    }

    fn feasibility_level(&self, sel: u16) -> FeasibilityLevel {
        let n = self.m.feasibility_levels().count();
        if sel % 10 == 0 {
            let extra = self.wide.feasibility_levels().count() - n;
            self.wide
                .feasibility_levels()
                .nth(n + usize::from(sel / 10) % extra)
                .expect("wide feasibility level")
                .0
        } else {
            self.m
                .feasibility_levels()
                .nth(usize::from(sel) % n)
                .expect("feasibility level")
                .0
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
        let provenance = self.provenance(s.c.rotate_left(3), s.bits);
        match s.kind {
            0..=2 => Op::AddAsset {
                id: if s.a % 16 == 0 {
                    self.asset(p, s.b)
                } else {
                    asset_id(self.fresh())
                },
                name: self.name(s.c),
                properties: self.property_set(s.bits),
                provenance,
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
                    provenance,
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
                    provenance,
                }
            }
            9 => Op::RenameAsset {
                id: self.asset(p, s.a),
                name: self.name(s.c),
                provenance,
            },
            10 => Op::SetAssetProperties {
                id: self.asset(p, s.a),
                properties: self.property_set(s.bits),
                provenance,
            },
            11 => Op::RemoveAsset {
                id: self.asset(p, s.a),
            },
            12 => Op::RenameDamageScenario {
                id: self.damage(p, s.a),
                name: self.name(s.c),
                provenance,
            },
            13 => {
                let category = self.impact_category(s.b);
                Op::SetImpact {
                    id: self.damage(p, s.a),
                    category,
                    level: (s.c % 5 != 0).then(|| self.level(category, s.c)),
                    provenance,
                }
            }
            14 => Op::RemoveDamageScenario {
                id: self.damage(p, s.a),
            },
            15 => Op::RenameThreatScenario {
                id: self.threat(p, s.a),
                name: self.name(s.c),
                provenance,
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
                Op::SetThreatCategory {
                    id,
                    category,
                    provenance,
                }
            }
            17 => Op::AddCatalogRef {
                threat: self.threat(p, s.a),
                catalog_ref: self.catalog_ref(s.b, s.bits),
                provenance,
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
                    provenance,
                }
            }
            19 => Op::LinkDamage {
                threat: self.threat(p, s.a),
                damage: self.damage(p, s.b),
                provenance,
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
                Op::UnlinkDamage {
                    threat,
                    damage,
                    provenance,
                }
            }
            21 => Op::RemoveThreatScenario {
                id: self.threat(p, s.a),
            },
            22..=23 => {
                let root = node_id(self.fresh());
                let s1 = node_id(self.fresh());
                let s2 = node_id(self.fresh());
                let kind = match s.b % 3 {
                    0 => GateKind::Or,
                    1 => GateKind::And,
                    _ => GateKind::SeqAnd,
                };
                let mut tree = AttackTree::new(root, Node::gate(kind, format!("{SENTINEL}-g")))
                    .expect("valid root");
                let s1_idx = tree
                    .add_child(tree.root(), s1, Node::step(format!("{SENTINEL}-s1")))
                    .expect("valid s1");
                tree.add_child(tree.root(), s2, Node::step(format!("{SENTINEL}-s2")))
                    .expect("valid s2");
                if s.c % 32 == 0 {
                    for i in 0..30 {
                        let extra = node_id(self.fresh());
                        tree.add_child(tree.root(), extra, Node::step(format!("{SENTINEL}-e{i}")))
                            .expect("valid extra");
                    }
                } else if s.c % 4 == 1 {
                    let g1 = node_id(self.fresh());
                    let g1_idx = tree
                        .add_child(
                            tree.root(),
                            g1,
                            Node::gate(GateKind::And, format!("{SENTINEL}-g1")),
                        )
                        .expect("valid g1");
                    tree.link(g1_idx, s1_idx).expect("link s1");
                }
                let step_feasibility = match s.c % 16 {
                    0 => vec![(node_id(self.fresh()), self.feasibility_level(1))],
                    1 => vec![(root, self.feasibility_level(1))],
                    2 => vec![(s1, self.feasibility_level(0))],
                    3 => vec![
                        (s1, self.feasibility_level(1)),
                        (s1, self.feasibility_level(2)),
                    ],
                    4 => vec![],
                    5..=8 => vec![(s1, self.feasibility_level(s.a))],
                    _ => vec![
                        (s2, self.feasibility_level(s.a)),
                        (s1, self.feasibility_level(s.b)),
                    ],
                };
                Op::AddAttackTree {
                    id: if s.a % 12 == 0 {
                        self.tree(p, s.b)
                    } else {
                        tree_id(self.fresh())
                    },
                    tree,
                    step_feasibility,
                    provenance,
                }
            }
            24..=25 => {
                let tree = self.tree(p, s.a);
                let node = match p.attack_tree_idx(&tree).and_then(|idx| p.attack_tree(idx)) {
                    Some(_) if s.b % 8 == 0 => node_id(self.fresh()),
                    Some(t) if s.b % 8 == 1 => t.id_of(t.root()).expect("root"),
                    Some(t) => {
                        let steps: Vec<_> = t
                            .iter()
                            .filter(|(_, _, n)| matches!(n, Node::Step { .. }))
                            .map(|(_, id, _)| id)
                            .collect();
                        nth(steps, s.b / 8).unwrap_or_else(|| node_id(self.fresh()))
                    }
                    None => node_id(self.fresh()),
                };
                Op::SetStepFeasibility {
                    tree,
                    node,
                    level: (s.c % 5 != 0).then(|| self.feasibility_level(s.c)),
                    provenance,
                }
            }
            26 => Op::SetThreatAttackTree {
                threat: self.threat(p, s.a),
                tree: (s.b % 4 != 0).then(|| self.tree(p, s.b)),
                provenance,
            },
            27 => Op::SetThreatFeasibility {
                threat: self.threat(p, s.a),
                level: (s.c % 5 != 0).then(|| self.feasibility_level(s.c)),
                provenance,
            },
            28 => Op::RemoveAttackTree {
                id: self.tree(p, s.a),
            },
            29 => Op::SetRiskTreatment {
                threat: self.threat(p, s.a),
                treatment: (s.c % 4 != 0).then(|| self.treatment(s.b)),
                provenance,
            },
            30..=31 => Op::AddSecurityControl {
                id: if s.a % 12 == 0 {
                    self.control(p, s.b)
                } else {
                    control_id(self.fresh())
                },
                name: self.name(s.c),
                provenance,
            },
            32 => Op::RenameSecurityControl {
                id: self.control(p, s.a),
                name: self.name(s.c),
                provenance,
            },
            33 => Op::RemoveSecurityControl {
                id: self.control(p, s.a),
            },
            _ => {
                let tree = self.tree(p, s.a);
                let node = match p.attack_tree_idx(&tree).and_then(|idx| p.attack_tree(idx)) {
                    Some(_) if s.b % 8 == 0 => node_id(self.fresh()),
                    Some(t) if s.b % 8 == 1 => t.id_of(t.root()).expect("root"),
                    Some(t) => {
                        let steps: Vec<_> = t
                            .iter()
                            .filter(|(_, _, n)| matches!(n, Node::Step { .. }))
                            .map(|(_, id, _)| id)
                            .collect();
                        nth(steps, s.b / 8).unwrap_or_else(|| node_id(self.fresh()))
                    }
                    None => node_id(self.fresh()),
                };
                let control = self.control(p, s.c);
                let feasibility =
                    (s.bits % 4 != 0).then(|| self.feasibility_level((s.bits >> 8) as u16));
                Op::SetStepMitigation {
                    tree,
                    node,
                    control,
                    feasibility,
                    provenance,
                }
            }
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

// ---------------------------------------------------------------------------
// Save/load helpers.
// ---------------------------------------------------------------------------

/// `def` with its vocabularies reordered (AC4 of job 010): the impact
/// categories, properties, threat categories, each `violates` list, and the
/// catalog schemes are reversed, and one unused item is prepended to each
/// vocabulary. Level scales keep their order (a scale is ordered by
/// meaning). The key and version stay the same, so a file written under
/// `def` must read under the result with the same meaning.
pub fn reordered(mut def: MethodologyDef) -> MethodologyDef {
    let unused_risk: Arc<str> = "zz-unused-risk".into();
    for c in &mut def.impact_categories {
        for row in &mut c.risk_matrix {
            row.insert(0, Arc::clone(&unused_risk));
        }
    }
    def.feasibility_levels.insert(
        0,
        LevelDef {
            key: "zz-unused-feasibility".into(),
            label: "Unused feasibility".into(),
        },
    );
    let cols = def.feasibility_levels.len();
    def.impact_categories.reverse();
    def.impact_categories.insert(
        0,
        ImpactCategoryDef {
            key: "zz-unused-impact".into(),
            label: "Unused impact".into(),
            levels: level_defs(&["u0", "u1"]),
            risk_matrix: vec![vec![Arc::clone(&unused_risk); cols]; 2],
        },
    );
    def.risk_levels.insert(
        0,
        LevelDef {
            key: unused_risk,
            label: "Unused risk".into(),
        },
    );
    def.properties.reverse();
    def.properties.insert(
        0,
        PropertyDef {
            key: "zz-unused-property".into(),
            label: "Unused property".into(),
        },
    );
    for t in &mut def.threat_categories {
        t.violates.reverse();
    }
    def.threat_categories.reverse();
    def.threat_categories.insert(
        0,
        ThreatCategoryDef {
            key: "zz-unused-threat".into(),
            label: "Unused threat".into(),
            violates: vec!["zz-unused-property".into()],
        },
    );
    def.catalog_schemes.reverse();
    def.catalog_schemes.insert(
        0,
        CatalogSchemeDef {
            key: "zz-unused-scheme".into(),
            label: "Unused scheme".into(),
            syntax: CatalogSyntax {
                prefix: "U-".into(),
                segments: vec![SegmentDef {
                    min: 1,
                    max: 9,
                    digits: None,
                }],
                required: 1,
            },
        },
    );
    def
}

/// Replays `seeds` on a fresh project over `m` and calls `f(project, op,
/// inverse)` after every accepted op, with the project after the op.
/// Returns the final project.
pub fn accepted(
    m: &Arc<Methodology>,
    seeds: &[Seed],
    mut f: impl FnMut(&Project, &Op, &Op) -> Result<(), TestCaseError>,
) -> Result<Project, TestCaseError> {
    let mut generator = Gen::new(Arc::clone(m));
    let mut project = Project::new(Arc::clone(m));
    for s in seeds {
        let op = generator.op(&project, s);
        if let Ok(inverse) = project.apply(op.clone()) {
            f(&project, &op, &inverse)?;
        }
    }
    Ok(project)
}

/// Saves `p` to bytes.
pub fn save(p: &Project) -> Vec<u8> {
    let mut out = Vec::new();
    p.save(&mut out).expect("save");
    out
}

/// Writes `ops` as an op log over `m`.
pub fn write_log<'a>(m: &Arc<Methodology>, ops: impl IntoIterator<Item = &'a Op>) -> Vec<u8> {
    let mut log = aegis_model::OpLogWriter::create(Vec::new(), Arc::clone(m)).expect("header");
    for op in ops {
        log.append(op).expect("append");
    }
    log.into_inner().expect("flush")
}

/// Reads every record of an op log over `m`, panicking on any error.
pub fn read_log(m: &Arc<Methodology>, bytes: &[u8]) -> Vec<aegis_model::LoggedOp> {
    aegis_model::OpLogReader::open(bytes, Arc::clone(m))
        .expect("header")
        .collect::<Result<Vec<_>, _>>()
        .expect("records")
}

/// A key-based description of every entity, one line each, independent of
/// vocabulary indices: names, property keys, ratings as category and level
/// keys, threat category keys, catalog ref texts, attack trees, step
/// feasibility keys, and provenance.
pub fn describe(p: &Project) -> Vec<String> {
    let m = p.methodology();
    let mut out = Vec::new();
    for (_, id, a) in p.assets() {
        let mut keys: Vec<_> = a
            .properties()
            .iter()
            .filter_map(|x| m.property_key(x))
            .collect();
        keys.sort_unstable();
        out.push(format!(
            "asset {id} {:?} {keys:?} {:?}",
            a.name(),
            a.provenance()
        ));
    }
    for (_, id, d) in p.damage_scenarios() {
        let mut rated: Vec<_> = d
            .impacts()
            .filter_map(|(c, l)| {
                let l = l?;
                Some(format!(
                    "{}={}",
                    m.impact_category_key(c)?,
                    m.level_key(c, l)?
                ))
            })
            .collect();
        rated.sort_unstable();
        out.push(format!(
            "damage {id} {:?} {rated:?} {:?}",
            d.name(),
            d.provenance()
        ));
    }
    for (_, id, t) in p.threat_scenarios() {
        let mut refs: Vec<_> = t
            .catalog_refs()
            .iter()
            .map(|r| {
                format!(
                    "{}:{}",
                    m.catalog_scheme_key(r.scheme()).unwrap_or("?"),
                    m.display_catalog_ref(r)
                )
            })
            .collect();
        refs.sort_unstable();
        let damages: Vec<_> = t
            .damages()
            .iter()
            .filter_map(|&d| p.damage_scenario_id(d))
            .collect();
        let direct = t
            .direct_feasibility()
            .and_then(|f| m.feasibility_level_key(f));
        let treatment = t.treatment();
        let tree = t.attack_tree().and_then(|idx| p.attack_tree_id(idx));
        out.push(format!(
            "threat {id} {:?} asset {:?} {:?} {refs:?} {damages:?} feas {direct:?} treat {treatment:?} tree {tree:?} {:?}",
            t.name(),
            p.asset_id(t.asset()),
            m.threat_category_key(t.category()),
            t.provenance(),
        ));
    }
    for (idx, id, tree) in p.attack_trees() {
        let nodes: Vec<_> = tree
            .iter()
            .map(|(n_idx, n_id, node)| {
                let parents: Vec<_> = tree.parents(n_idx).collect();
                (n_idx, n_id, node, tree.owner(n_idx), parents)
            })
            .collect();
        let feas: Vec<_> = p
            .step_feasibilities(idx)
            .unwrap_or(&[])
            .iter()
            .map(|l| l.and_then(|f| m.feasibility_level_key(f)))
            .collect();
        let mits: Vec<_> = tree
            .iter()
            .map(|(n_idx, _, _)| {
                p.step_mitigations(idx, n_idx)
                    .iter()
                    .map(|sm| {
                        (
                            p.security_control_id(sm.control()),
                            m.feasibility_level_key(sm.feasibility()),
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .collect();
        out.push(format!(
            "tree {id} {nodes:?} {feas:?} {mits:?} {:?}",
            p.attack_tree_provenance(idx),
        ));
    }
    for (_, id, c) in p.security_controls() {
        out.push(format!("control {id} {:?} {:?}", c.name(), c.provenance()));
    }
    out
}

// ---------------------------------------------------------------------------
// Ops over the second methodology.
// ---------------------------------------------------------------------------

/// Applies every `Op` variant over `m`, which must be the second
/// methodology (built from `second_def()` or loaded from its pack), checks
/// the resulting state, and tears the project down again (AC7 of job 009,
/// AC6 of job 012, AC7 of job 013).
pub fn second_every_op(m: Arc<Methodology>) {
    fn ok(p: &mut Project, op: Op) -> Op {
        match p.apply(op.clone()) {
            Ok(inverse) => inverse,
            Err(e) => panic!("{op:?} failed: {e}"),
        }
    }
    fn rejects(p: &mut Project, op: Op, want: &aegis_model::ModelError) {
        let before = p.clone();
        assert_eq!(p.validate(&op).as_ref(), Err(want), "validate {op:?}");
        assert_eq!(p.apply(op.clone()).as_ref(), Err(want), "apply {op:?}");
        assert_eq!(*p, before);
    }
    let mut p = Project::new(Arc::clone(&m));
    let capec = scheme(&m, "capec");
    let harm = cat(&m, "patient-harm");
    let exposure = cat(&m, "data-exposure");
    let loss = cat(&m, "service-loss");
    let (pump, monitor) = (asset_id(1), asset_id(2));
    let (overdose, leak) = (damage_id(3), damage_id(4));
    let (lock, sniff) = (threat_id(5), threat_id(6));
    let tree = tree_id(7);
    let (root, s1, s2) = (node_id(8), node_id(9), node_id(10));
    let ctrl = control_id(11);
    let p_human = Provenance::human("alice", 100);
    let p_import = Provenance::import("csv", 200);
    let p_catalog = Provenance::catalog("capec", 300);
    let p_ai = Provenance::ai("tara-assist", 400);

    // Assets.
    ok(
        &mut p,
        Op::AddAsset {
            id: pump,
            name: "Infusion pump".into(),
            properties: props(&m, &["i", "a"]),
            provenance: p_human.clone(),
        },
    );
    ok(
        &mut p,
        Op::AddAsset {
            id: monitor,
            name: "Monitor".into(),
            properties: props(&m, &["c"]),
            provenance: p_import.clone(),
        },
    );
    ok(
        &mut p,
        Op::RenameAsset {
            id: monitor,
            name: "Bedside monitor".into(),
            provenance: p_human.clone(),
        },
    );
    ok(
        &mut p,
        Op::SetAssetProperties {
            id: monitor,
            properties: m.all_properties(),
            provenance: p_ai.clone(),
        },
    );

    // Damage scenarios, rated on the 5-level and 2-level scales.
    ok(
        &mut p,
        Op::AddDamageScenario {
            id: overdose,
            name: "Overdose".into(),
            impacts: vec![
                (harm, lvl(&m, harm, "critical")),
                (loss, lvl(&m, loss, "intolerable")),
            ],
            provenance: p_human.clone(),
        },
    );
    ok(
        &mut p,
        Op::AddDamageScenario {
            id: leak,
            name: "Record leak".into(),
            impacts: vec![],
            provenance: p_import.clone(),
        },
    );
    ok(
        &mut p,
        Op::RenameDamageScenario {
            id: leak,
            name: "Patient record leak".into(),
            provenance: p_human.clone(),
        },
    );
    ok(
        &mut p,
        Op::SetImpact {
            id: leak,
            category: exposure,
            level: Some(lvl(&m, exposure, "extreme")),
            provenance: p_ai.clone(),
        },
    );

    // Threats; ransomware violates two properties.
    ok(
        &mut p,
        Op::AddThreatScenario {
            id: lock,
            name: "Encrypt pump firmware".into(),
            asset: pump,
            category: threat_cat(&m, "ransomware"),
            catalog_refs: vec![cref(&m, capec, "CAPEC-549")],
            damages: vec![overdose],
            provenance: p_catalog.clone(),
        },
    );
    ok(
        &mut p,
        Op::AddThreatScenario {
            id: sniff,
            name: "Sniff telemetry".into(),
            asset: monitor,
            category: threat_cat(&m, "corruption"),
            catalog_refs: vec![],
            damages: vec![],
            provenance: p_human.clone(),
        },
    );
    ok(
        &mut p,
        Op::RenameThreatScenario {
            id: sniff,
            name: "Sniff patient telemetry".into(),
            provenance: p_human.clone(),
        },
    );
    ok(
        &mut p,
        Op::SetThreatCategory {
            id: sniff,
            category: threat_cat(&m, "data-leak"),
            provenance: p_import.clone(),
        },
    );
    ok(
        &mut p,
        Op::AddCatalogRef {
            threat: sniff,
            catalog_ref: cref(&m, capec, "CAPEC-157"),
            provenance: p_catalog.clone(),
        },
    );
    ok(
        &mut p,
        Op::LinkDamage {
            threat: sniff,
            damage: leak,
            provenance: p_ai.clone(),
        },
    );
    ok(
        &mut p,
        Op::SetThreatFeasibility {
            threat: sniff,
            level: Some(feas(&m, "occasional")),
            provenance: p_ai.clone(),
        },
    );

    // Attack tree rated on the 5-level feasibility scale.
    let mut attack_tree =
        AttackTree::new(root, Node::gate(GateKind::Or, "Compromise pump")).expect("root");
    attack_tree
        .add_child(attack_tree.root(), s1, Node::step("USB update"))
        .expect("s1");
    attack_tree
        .add_child(attack_tree.root(), s2, Node::step("Serial debug"))
        .expect("s2");
    ok(
        &mut p,
        Op::AddAttackTree {
            id: tree,
            tree: attack_tree,
            step_feasibility: vec![(s1, feas(&m, "remote")), (s2, feas(&m, "occasional"))],
            provenance: p_human.clone(),
        },
    );
    ok(
        &mut p,
        Op::SetStepFeasibility {
            tree,
            node: s1,
            level: Some(feas(&m, "probable")),
            provenance: p_ai.clone(),
        },
    );
    ok(
        &mut p,
        Op::SetThreatAttackTree {
            threat: lock,
            tree: Some(tree),
            provenance: p_human.clone(),
        },
    );
    ok(
        &mut p,
        Op::SetRiskTreatment {
            threat: lock,
            treatment: Some(RiskTreatment::Reduce),
            provenance: p_human.clone(),
        },
    );
    ok(
        &mut p,
        Op::AddSecurityControl {
            id: ctrl,
            name: "Signed firmware".into(),
            provenance: p_human.clone(),
        },
    );
    ok(
        &mut p,
        Op::RenameSecurityControl {
            id: ctrl,
            name: "Signed USB firmware".into(),
            provenance: p_ai.clone(),
        },
    );
    ok(
        &mut p,
        Op::SetStepMitigation {
            tree,
            node: s1,
            control: ctrl,
            feasibility: Some(feas(&m, "improbable")),
            provenance: p_human.clone(),
        },
    );

    // Check the state.
    let lock_idx = p.threat_scenario_idx(&lock).expect("lock");
    assert_eq!(p.targets(lock_idx), Some(props(&m, &["i", "a"])));
    let tree_idx = p.attack_tree_idx(&tree).expect("tree");
    assert_eq!(
        p.threat_scenario(lock_idx).and_then(|t| t.attack_tree()),
        Some(tree_idx)
    );
    assert_eq!(
        p.threat_scenario(lock_idx).and_then(|t| t.treatment()),
        Some(RiskTreatment::Reduce)
    );
    assert_eq!(
        p.threat_scenario(lock_idx).map(|t| t.provenance()),
        Some(&p_human)
    );
    assert_eq!(p.attack_tree_provenance(tree_idx), Some(&p_human));
    let ctrl_idx = p.security_control_idx(&ctrl).expect("ctrl");
    let ctrl_ent = p.security_control(ctrl_idx).expect("live");
    assert_eq!(ctrl_ent.name(), "Signed USB firmware");
    assert_eq!(ctrl_ent.provenance(), &p_ai);
    let eval = p.evaluate_threat(lock).expect("evaluate threat");
    assert_eq!(eval.root_value(), &feas(&m, "probable"));
    let res_eval = p
        .evaluate_residual_threat(lock)
        .expect("evaluate residual threat");
    assert_eq!(res_eval.root_value(), &feas(&m, "occasional"));
    let threat_risk = p.evaluate_risk(lock).expect("evaluate risk");
    assert_eq!(threat_risk.feasibility(), feas(&m, "probable"));
    assert_eq!(threat_risk.max_risk(), Some(risk(&m, "unacceptable")));
    assert_eq!(threat_risk.entries().len(), 2);
    let deciding = threat_risk.deciding_entry().expect("deciding");
    assert_eq!(deciding.damage(), overdose);
    assert_eq!(deciding.category(), harm);
    assert_eq!(deciding.risk(), risk(&m, "unacceptable"));
    let res_risk = p
        .evaluate_residual_risk(lock)
        .expect("evaluate residual risk");
    assert_eq!(res_risk.feasibility(), feas(&m, "occasional"));
    assert_eq!(res_risk.max_risk(), Some(risk(&m, "unacceptable")));
    let sniff_idx = p.threat_scenario_idx(&sniff).expect("sniff");
    assert_eq!(p.targets(sniff_idx), Some(props(&m, &["c"])));
    let d = p
        .damage_scenario(p.damage_scenario_idx(&leak).expect("leak"))
        .expect("live");
    assert_eq!(d.name(), "Patient record leak");
    assert_eq!(d.impacts().count(), 3);
    assert_eq!(d.impact(exposure), Some(lvl(&m, exposure, "extreme")));
    assert_eq!(d.impact(harm), None);
    assert_eq!(d.provenance(), &p_ai);
    let t = p.threat_scenario(sniff_idx).expect("live");
    assert_eq!(t.name(), "Sniff patient telemetry");
    assert_eq!(t.catalog_refs(), [cref(&m, capec, "CAPEC-157")]);
    assert_eq!(t.damages(), [p.damage_scenario_idx(&leak).expect("leak")]);
    assert_eq!(t.direct_feasibility(), Some(feas(&m, "occasional")));
    assert_eq!(t.provenance(), &p_ai);
    let sniff_risk = p.evaluate_risk(sniff).expect("evaluate sniff risk");
    assert_eq!(sniff_risk.feasibility(), feas(&m, "occasional"));
    assert!(sniff_risk.tree_evaluation().is_none());
    assert_eq!(sniff_risk.max_risk(), Some(risk(&m, "unacceptable")));

    // Ransomware relies on both I and A of the pump.
    rejects(
        &mut p,
        Op::SetAssetProperties {
            id: pump,
            properties: props(&m, &["i"]),
            provenance: Provenance::default(),
        },
        &aegis_model::ModelError::PropertyInUse {
            asset: pump,
            property: prop(&m, "a"),
            threats: 1,
        },
    );
    // Linked attack tree, control mitigation, and risk treatment block removal.
    rejects(
        &mut p,
        Op::RemoveSecurityControl { id: ctrl },
        &aegis_model::ModelError::SecurityControlInUse {
            control: ctrl,
            steps: 1,
        },
    );
    rejects(
        &mut p,
        Op::RemoveAttackTree { id: tree },
        &aegis_model::ModelError::AttackTreeInUse { tree, threats: 1 },
    );
    rejects(
        &mut p,
        Op::RemoveThreatScenario { id: lock },
        &aegis_model::ModelError::ThreatHasAttackTree { threat: lock, tree },
    );
    rejects(
        &mut p,
        Op::RemoveThreatScenario { id: sniff },
        &aegis_model::ModelError::ThreatHasDirectFeasibility {
            threat: sniff,
            feasibility: feas(&m, "occasional"),
        },
    );

    // Tear down with the remaining ops.
    ok(
        &mut p,
        Op::SetThreatAttackTree {
            threat: lock,
            tree: None,
            provenance: Provenance::default(),
        },
    );
    rejects(
        &mut p,
        Op::RemoveAttackTree { id: tree },
        &aegis_model::ModelError::AttackTreeHasMitigations {
            tree,
            mitigations: 1,
        },
    );
    rejects(
        &mut p,
        Op::RemoveThreatScenario { id: lock },
        &aegis_model::ModelError::ThreatHasRiskTreatment {
            threat: lock,
            treatment: RiskTreatment::Reduce,
        },
    );
    ok(
        &mut p,
        Op::SetStepMitigation {
            tree,
            node: s1,
            control: ctrl,
            feasibility: None,
            provenance: Provenance::default(),
        },
    );
    ok(&mut p, Op::RemoveSecurityControl { id: ctrl });
    ok(
        &mut p,
        Op::SetRiskTreatment {
            threat: lock,
            treatment: None,
            provenance: Provenance::default(),
        },
    );
    ok(
        &mut p,
        Op::SetThreatFeasibility {
            threat: sniff,
            level: None,
            provenance: Provenance::default(),
        },
    );
    ok(&mut p, Op::RemoveAttackTree { id: tree });
    ok(
        &mut p,
        Op::RemoveCatalogRef {
            threat: sniff,
            catalog_ref: cref(&m, capec, "CAPEC-157"),
            provenance: Provenance::default(),
        },
    );
    ok(
        &mut p,
        Op::UnlinkDamage {
            threat: sniff,
            damage: leak,
            provenance: Provenance::default(),
        },
    );
    ok(&mut p, Op::RemoveDamageScenario { id: leak });
    ok(&mut p, Op::RemoveThreatScenario { id: sniff });
    ok(&mut p, Op::RemoveThreatScenario { id: lock });
    ok(&mut p, Op::RemoveDamageScenario { id: overdose });
    ok(&mut p, Op::RemoveAsset { id: pump });
    ok(&mut p, Op::RemoveAsset { id: monitor });
    assert_eq!(p, Project::new(m));
}

// ---------------------------------------------------------------------------
// Pack helpers.
// ---------------------------------------------------------------------------

/// `def` written as a pack.
pub fn pack_bytes(def: &MethodologyDef) -> Vec<u8> {
    let mut out = Vec::new();
    def.write_pack(&mut out).expect("write_pack into a Vec");
    out
}

/// A key of exactly `len` bytes: `tag`, `-`, then `i` zero-padded.
pub fn key_of_len(tag: &str, i: usize, len: usize) -> Arc<str> {
    let width = len - tag.len() - 1;
    format!("{tag}-{i:0>width$}").into()
}

/// The largest definition `Methodology::new` accepts, as measured by its
/// pack size (AC9 of job 012): every vocabulary at its maximum count, every
/// key 64 bytes, every label 256 control chars (each escaped to 6 bytes in
/// JSON), a 64-char version of control chars, every threat category
/// violating all 64 properties, and every catalog scheme with a 16-byte
/// prefix and 4 fixed-width segments.
pub fn max_def() -> MethodologyDef {
    let label: Arc<str> = "\u{1}".repeat(256).into();
    let entry = |tag: &str, i: usize| (key_of_len(tag, i, 64), Arc::clone(&label));
    let properties: Vec<PropertyDef> = (0..64)
        .map(|i| {
            let (key, label) = entry("p", i);
            PropertyDef { key, label }
        })
        .collect();
    let all: Vec<Arc<str>> = properties.iter().map(|p| Arc::clone(&p.key)).collect();
    let risk_levels: Vec<LevelDef> = (0..16)
        .map(|r| {
            let (key, label) = entry("r", r);
            LevelDef { key, label }
        })
        .collect();
    let risk_row: Vec<Arc<str>> = risk_levels.iter().map(|r| Arc::clone(&r.key)).collect();
    MethodologyDef {
        key: key_of_len("m", 0, 64),
        version: "\u{1}".repeat(64).into(),
        label: Arc::clone(&label),
        impact_categories: (0..32)
            .map(|i| {
                let (key, label) = entry("c", i);
                ImpactCategoryDef {
                    key,
                    label,
                    levels: (0..16)
                        .map(|l| {
                            let (key, label) = entry("l", l);
                            LevelDef { key, label }
                        })
                        .collect(),
                    risk_matrix: vec![risk_row.clone(); 16],
                }
            })
            .collect(),
        feasibility_levels: (0..16)
            .map(|l| {
                let (key, label) = entry("f", l);
                LevelDef { key, label }
            })
            .collect(),
        risk_levels,
        properties,
        threat_categories: (0..64)
            .map(|i| {
                let (key, label) = entry("t", i);
                ThreatCategoryDef {
                    key,
                    label,
                    violates: all.clone(),
                }
            })
            .collect(),
        catalog_schemes: (0..16)
            .map(|i| {
                let (key, label) = entry("s", i);
                CatalogSchemeDef {
                    key,
                    label,
                    syntax: CatalogSyntax {
                        prefix: "ABCDEFGHIJKLMNOP".into(),
                        segments: vec![
                            SegmentDef {
                                min: 0,
                                max: 65535,
                                digits: Some(5),
                            };
                            4
                        ],
                        required: 4,
                    },
                }
            })
            .collect(),
    }
}
