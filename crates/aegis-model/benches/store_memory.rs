//! Peak heap of `Project::load`, measured with `dhat`, as a factor of the
//! snapshot's size (ADR 0009 records the numbers). One benign shape (the
//! `typical` project) and the hostile shapes that cost the most heap per
//! byte of JSON:
//!
//! - `short_property_keys`: assets carrying 64 two- or three-byte property
//!   keys each (valid, so the whole file loads);
//! - `short_impact_keys`: damage scenarios rating 32 categories with short
//!   keys and one-byte levels (valid);
//! - `unknown_property_keys` / `unknown_impact_keys`: the same with keys the
//!   methodology does not have, so the load fails with `UnknownKey`;
//! - `unknown_one_byte_keys`: assets with 62 distinct one-byte unknown keys
//!   (the densest key list JSON allows);
//! - `long_names`: assets with 1,024-byte names (256 four-byte chars, the
//!   largest valid name);
//! - `one_huge_string`: one name as long as the whole file, so the
//!   parser's string buffer is the cost (the load fails with the name cap);
//! - `minimal_entities`: the smallest valid assets, damage scenarios, and
//!   threats in one file;
//! - `{assets,damages,threats}_only_{builtin,max}`: one entity kind only,
//!   each entity as small as a valid one can be, over the built-in
//!   methodology (6 properties, 4 impact categories) and over `short`, a
//!   methodology at every vocabulary limit (64 properties, 32 impact
//!   categories). These are the densest shapes: the loaded project's fixed
//!   cost per entity dominates, and it grows with the methodology's size.
//!
//! Each hostile file is about `HOSTILE_BYTES`; the factor does not depend
//! on the size (load is linear), so the gate holds at the 64 MiB cap too.
//!
//! Bounds (peak heap / file bytes). Load resolves every key while decoding,
//! streams the input through a 64 KiB buffer, and keeps only names and
//! 1-byte indices per entity, so the peak is the loaded project ("kept")
//! plus the decoded entity lists. Each bound is the measured factor
//! (ADR 0009) plus about 35% headroom for allocator and `std` changes:
//!
//! | Case | Measured | Bound | What dominates |
//! |---|---|---|---|
//! | typical | 1.03 | 1.5 | the project |
//! | short property keys | 1.47 | 2.0 | the project's per-asset reference counts (64 x 4 bytes) |
//! | short impact keys | 0.76 | 1.5 | the project |
//! | unknown keys (all three) | 0.02 | 0.1 | the read buffer: the load fails inside the first entity |
//! | long names | 1.31 | 2.0 | the names, moved into the project |
//! | one huge string | 1.02 | 2.5 | the parser's string buffer, which grows by doubling |
//! | minimal entities (mixed kinds) | 2.99 | 4.0 | the project's fixed per-entity cost (arena slot, ID map, counts) |
//! | assets only, built-in / max vocabulary | 2.43 / 6.16 | 3.3 / 8.3 | the project: per asset, a reference-count row of 4 bytes per methodology property (64 x 4 = 256 bytes at the max) |
//! | damages only, built-in / max vocabulary | 2.73 / 3.53 | 3.7 / 4.8 | the project: per damage scenario, one rating slot per impact category |
//! | threats only, built-in / max vocabulary | 1.91 / 1.93 | 2.6 / 2.6 | the project |
//!
//! The decoder's own share is the peak minus "kept" (the loaded project):
//! at most about 0.9x the file in every case. The rest is the project,
//! which any project of that many entities needs.
//!
//! Run: `cargo bench --bench store_memory` (also runs under
//! `cargo test --benches`). The profiler runs in testing mode, so it never
//! writes `dhat-heap.json`. Fails if a case's peak exceeds its bound.

#![forbid(unsafe_code)]

#[path = "store_kit/mod.rs"]
mod store_kit;

use std::fmt::Write as _;
use std::sync::Arc;

use aegis_model::{
    ImpactCategoryDef, LevelDef, Methodology, MethodologyDef, Project, PropertyDef,
    ThreatCategoryDef,
};

#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

/// Size of each hostile snapshot, in bytes (about).
const HOSTILE_BYTES: usize = 4 << 20;

/// One measured load.
struct Case {
    name: &'static str,
    methodology: Arc<Methodology>,
    bytes: Vec<u8>,
    /// Whether the load must succeed (else it must fail with `UnknownKey`).
    loads: bool,
    /// Upper bound on peak heap / file bytes.
    max_factor: f64,
}

/// A methodology with short keys at every vocabulary limit: properties
/// `p0`..`p63`, impact categories `c0`..`c31` rated `a` < `b`, threat
/// categories `t0`..`t63` (`tN` violates `pN`).
fn short_keys() -> Arc<Methodology> {
    let entry = |key: String| {
        (
            Arc::<str>::from(key.as_str()),
            Arc::<str>::from(key.as_str()),
        )
    };
    let def = MethodologyDef {
        key: "short".into(),
        version: "1".into(),
        label: "Short keys".into(),
        impact_categories: (0..32)
            .map(|i| {
                let (key, label) = entry(format!("c{i}"));
                ImpactCategoryDef {
                    key,
                    label,
                    levels: ["a", "b"]
                        .iter()
                        .map(|&l| LevelDef {
                            key: l.into(),
                            label: l.into(),
                        })
                        .collect(),
                }
            })
            .collect(),
        properties: (0..64)
            .map(|i| {
                let (key, label) = entry(format!("p{i}"));
                PropertyDef { key, label }
            })
            .collect(),
        threat_categories: (0..64)
            .map(|i| {
                let (key, label) = entry(format!("t{i}"));
                ThreatCategoryDef {
                    key,
                    label,
                    violates: vec![format!("p{i}").into()],
                }
            })
            .collect(),
        catalog_schemes: vec![],
    };
    match Methodology::new(def) {
        Ok(m) => Arc::new(m),
        Err(e) => panic!("short-key methodology: {e}"),
    }
}

/// Canonical text of the `n`-th test ID (a valid `UUIDv7`).
fn id(n: u64) -> String {
    format!("0199c82c-c001-7000-8000-{n:012x}")
}

/// A snapshot over the `short` methodology whose entity lists are built by
/// `entity(i)` until the file reaches `HOSTILE_BYTES`. `kind` picks the list.
fn snapshot(kind: &str, mut entity: impl FnMut(u64) -> String) -> Vec<u8> {
    let mut list = String::new();
    let mut i = 0u64;
    while list.len() < HOSTILE_BYTES {
        if i > 0 {
            list.push(',');
        }
        list.push_str(&entity(i));
        i += 1;
    }
    let field = |k: &str| if k == kind { list.as_str() } else { "" };
    format!(
        "{{\"format\":\"aegis-project\",\"version\":1,\"methodology\":{{\"key\":\"short\",\"version\":\"1\"}},\
         \"assets\":[{}],\"damage_scenarios\":[{}],\"threat_scenarios\":[{}]}}\n",
        field("assets"),
        field("damage_scenarios"),
        field("threat_scenarios"),
    )
    .into_bytes()
}

/// A snapshot over `m` with one entity kind only, each entity as small as
/// a valid one can be: assets with one name byte and the properties of the
/// first threat category; damage scenarios with no ratings; or one such
/// asset plus threats of the first category with no refs or links. This
/// is the densest shape per byte for each kind, so the in-memory project's
/// fixed cost per entity dominates.
fn single_kind(m: &Methodology, kind: &str) -> Vec<u8> {
    let category = m.threat_categories().next();
    let category_key = category
        .and_then(|t| m.threat_category_key(t))
        .unwrap_or("");
    let props: Vec<String> = category
        .map(|t| m.violates(t))
        .unwrap_or_default()
        .iter()
        .filter_map(|p| m.property_key(p))
        .map(|k| format!("\"{k}\""))
        .collect();
    let props = props.join(",");
    let asset = |i: u64| {
        format!(
            "{{\"id\":\"{}\",\"name\":\"a\",\"properties\":[{props}]}}",
            id(i)
        )
    };
    let mut list = String::new();
    let mut i = 0u64;
    while list.len() < HOSTILE_BYTES {
        if i > 0 {
            list.push(',');
        }
        let entity = match kind {
            "assets" => asset(i),
            "damage_scenarios" => {
                format!(
                    "{{\"id\":\"{}\",\"name\":\"a\",\"impacts\":{{}}}}",
                    id(1 << 40 | i)
                )
            }
            _ => format!(
                "{{\"id\":\"{}\",\"name\":\"a\",\"asset\":\"{}\",\"category\":\"{category_key}\",\"catalog_refs\":[],\"damages\":[]}}",
                id(2 << 40 | i),
                id(0)
            ),
        };
        list.push_str(&entity);
        i += 1;
    }
    let one_asset = asset(0);
    let (assets, damages, threats) = match kind {
        "assets" => (list.as_str(), "", ""),
        "damage_scenarios" => ("", list.as_str(), ""),
        _ => (one_asset.as_str(), "", list.as_str()),
    };
    format!(
        "{{\"format\":\"aegis-project\",\"version\":1,\"methodology\":{{\"key\":\"{}\",\"version\":\"{}\"}},\
         \"assets\":[{assets}],\"damage_scenarios\":[{damages}],\"threat_scenarios\":[{threats}]}}\n",
        m.key(),
        m.version(),
    )
    .into_bytes()
}

fn keys(prefix: &str, n: usize) -> String {
    let mut out = String::new();
    for i in 0..n {
        let sep = if i > 0 { "," } else { "" };
        let _ = write!(out, "{sep}\"{prefix}{i}\"");
    }
    out
}

fn impacts(prefix: &str) -> String {
    let mut out = String::new();
    for i in 0..32 {
        let sep = if i > 0 { "," } else { "" };
        let _ = write!(out, "{sep}\"{prefix}{i}\":\"b\"");
    }
    out
}

/// The minimal valid entities: `n` assets, `n` damage scenarios, `n` threats.
fn minimal() -> Vec<u8> {
    let per = HOSTILE_BYTES / 3;
    let mut assets = String::new();
    let mut damages = String::new();
    let mut threats = String::new();
    let mut i = 0u64;
    while assets.len() + damages.len() + threats.len() < 3 * per {
        let sep = if i > 0 { "," } else { "" };
        let _ = write!(
            assets,
            "{sep}{{\"id\":\"{}\",\"name\":\"a\",\"properties\":[\"p0\"]}}",
            id(i)
        );
        let _ = write!(
            damages,
            "{sep}{{\"id\":\"{}\",\"name\":\"d\",\"impacts\":{{}}}}",
            id(1 << 40 | i)
        );
        let _ = write!(
            threats,
            "{sep}{{\"id\":\"{}\",\"name\":\"t\",\"asset\":\"{}\",\"category\":\"t0\",\"catalog_refs\":[],\"damages\":[]}}",
            id(2 << 40 | i),
            id(i)
        );
        i += 1;
    }
    format!(
        "{{\"format\":\"aegis-project\",\"version\":1,\"methodology\":{{\"key\":\"short\",\"version\":\"1\"}},\
         \"assets\":[{assets}],\"damage_scenarios\":[{damages}],\"threat_scenarios\":[{threats}]}}\n"
    )
    .into_bytes()
}

fn cases() -> Vec<Case> {
    let typical_m = store_kit::methodology();
    let mut typical = Vec::new();
    if let Err(e) = store_kit::typical(&typical_m).save(&mut typical) {
        panic!("save: {e}");
    }
    let short = short_keys();
    let long_name = "\u{1f512}".repeat(256);
    let mut cases = vec![
        Case {
            name: "typical",
            methodology: typical_m,
            bytes: typical,
            loads: true,
            max_factor: 1.5,
        },
        Case {
            name: "short_property_keys",
            methodology: Arc::clone(&short),
            bytes: snapshot("assets", |i| {
                format!(
                    "{{\"id\":\"{}\",\"name\":\"a\",\"properties\":[{}]}}",
                    id(i),
                    keys("p", 64)
                )
            }),
            loads: true,
            max_factor: 2.0,
        },
        Case {
            name: "short_impact_keys",
            methodology: Arc::clone(&short),
            bytes: snapshot("damage_scenarios", |i| {
                format!(
                    "{{\"id\":\"{}\",\"name\":\"d\",\"impacts\":{{{}}}}}",
                    id(i),
                    impacts("c")
                )
            }),
            loads: true,
            max_factor: 1.5,
        },
        Case {
            name: "unknown_property_keys",
            methodology: Arc::clone(&short),
            bytes: snapshot("assets", |i| {
                format!(
                    "{{\"id\":\"{}\",\"name\":\"a\",\"properties\":[{}]}}",
                    id(i),
                    keys("x", 64)
                )
            }),
            loads: false,
            max_factor: 0.1,
        },
        Case {
            name: "unknown_impact_keys",
            methodology: Arc::clone(&short),
            bytes: snapshot("damage_scenarios", |i| {
                format!(
                    "{{\"id\":\"{}\",\"name\":\"d\",\"impacts\":{{{}}}}}",
                    id(i),
                    impacts("x")
                )
            }),
            loads: false,
            max_factor: 0.1,
        },
        Case {
            name: "unknown_one_byte_keys",
            methodology: Arc::clone(&short),
            bytes: snapshot("assets", |i| {
                let one_byte: Vec<String> = ('a'..='z')
                    .chain('A'..='Z')
                    .chain('0'..='9')
                    .map(|c| format!("\"{c}\""))
                    .collect();
                format!(
                    "{{\"id\":\"{}\",\"name\":\"a\",\"properties\":[{}]}}",
                    id(i),
                    one_byte.join(",")
                )
            }),
            loads: false,
            max_factor: 0.1,
        },
        Case {
            name: "long_names",
            methodology: Arc::clone(&short),
            bytes: snapshot("assets", |i| {
                format!(
                    "{{\"id\":\"{}\",\"name\":\"{long_name}\",\"properties\":[\"p0\"]}}",
                    id(i)
                )
            }),
            loads: true,
            max_factor: 2.0,
        },
        Case {
            name: "one_huge_string",
            methodology: Arc::clone(&short),
            bytes: snapshot("assets", |i| {
                format!(
                    "{{\"id\":\"{}\",\"name\":\"{}\",\"properties\":[\"p0\"]}}",
                    id(i),
                    "n".repeat(HOSTILE_BYTES)
                )
            }),
            loads: false,
            max_factor: 2.5,
        },
        Case {
            name: "minimal_entities",
            methodology: Arc::clone(&short),
            bytes: minimal(),
            loads: true,
            max_factor: 4.0,
        },
    ];
    let builtin = store_kit::methodology();
    for (name, m, kind, max_factor) in [
        ("assets_only_builtin", &builtin, "assets", 3.3),
        ("assets_only_max", &short, "assets", 8.3),
        ("damages_only_builtin", &builtin, "damage_scenarios", 3.7),
        ("damages_only_max", &short, "damage_scenarios", 4.8),
        ("threats_only_builtin", &builtin, "threat_scenarios", 2.6),
        ("threats_only_max", &short, "threat_scenarios", 2.6),
    ] {
        cases.push(Case {
            name,
            methodology: Arc::clone(m),
            bytes: single_kind(m, kind),
            loads: true,
            max_factor,
        });
    }
    cases
}

fn main() {
    let only = std::env::var("STORE_MEMORY_CASE").ok();
    let mut failed = Vec::new();
    for case in cases() {
        if only.as_deref().is_some_and(|o| o != case.name) {
            continue;
        }
        let file = case.bytes.len();
        let profiler = dhat::Profiler::builder().testing().build();
        let result = Project::load(case.bytes.as_slice(), Arc::clone(&case.methodology));
        let stats = dhat::HeapStats::get();
        let outcome = match &result {
            Ok(p) => format!(
                "ok ({} entities)",
                p.asset_count() + p.damage_scenario_count() + p.threat_scenario_count()
            ),
            Err(e) => format!("{e}"),
        };
        drop(result);
        drop(profiler);
        #[allow(clippy::cast_precision_loss)] // Sizes far below 2^52.
        let (peak, kept) = (
            stats.max_bytes as f64 / file as f64,
            stats.curr_bytes as f64 / file as f64,
        );
        println!(
            "{:<22} {:>9} bytes  peak {:>10} bytes ({peak:>5.2} x)  kept {kept:>5.2} x  {:>8} allocs  {outcome}",
            case.name, file, stats.max_bytes, stats.total_blocks,
        );
        let ok_as_expected = outcome.starts_with("ok") == case.loads
            && (case.loads || outcome.starts_with("unknown") || outcome.starts_with("name"));
        if !ok_as_expected {
            failed.push(format!("{}: unexpected outcome {outcome}", case.name));
        }
        if peak > case.max_factor {
            failed.push(format!(
                "{}: peak {peak:.2} x file exceeds {}",
                case.name, case.max_factor
            ));
        }
    }
    assert!(failed.is_empty(), "memory gates failed: {failed:#?}");
}
