//! Peak and kept heap of `Project::load`, measured with `dhat`, as a factor
//! of the snapshot's size (ADRs 0009 and 0010 record the numbers). "Peak"
//! is the most heap live at any time during `load`; "kept" is the heap
//! still held when `load` returns, which is the loaded project. One benign
//! shape (the `typical` project) and the hostile shapes that cost the most
//! heap per byte of JSON:
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
//! - `attack_trees`: minimal 3-node attack trees (root OR gate with two
//!   rated steps);
//! - `{assets,damages,threats}_only_{builtin,max}`: one entity kind only,
//!   each entity as small as a valid one can be, over the built-in
//!   methodology (6 properties, 4 impact categories) and over `short`, a
//!   methodology at every vocabulary limit (64 properties, 32 impact
//!   categories). These are the densest shapes: the loaded project's fixed
//!   cost per entity dominates;
//! - `targeted_narrow_{builtin,max}`: minimal assets (the properties of the
//!   first threat category), each targeted by exactly one minimal threat of
//!   that category. The densest file in which every asset has a non-empty
//!   reference-count row;
//! - `targeted_wide_max`: assets carrying all 64 properties, each targeted
//!   by one threat of `short`'s `t63`, which violates every property. The
//!   largest reference-count row per asset (64 entries). `t63` violates all
//!   64 properties only for this case; no other case uses it.
//!
//! Each hostile file is about `HOSTILE_BYTES`; the factors do not depend
//! on the size (load is linear), so the gates hold at the 64 MiB cap too.
//!
//! Gates (heap / file bytes). Load resolves every key while decoding,
//! streams the input through a 64 KiB buffer, and keeps only names and
//! 1-byte indices per entity, so the peak is the loaded project (kept)
//! plus the decoded entity lists. Each gate is the measured factor plus
//! about 35% headroom for allocator and `std` changes, rounded up to 0.1.
//! A kept gate is capped at `SERVER_BUDGET` (3x, engineering standards
//! section 2), and the bench fails if a case that loads has a kept gate
//! above it:
//!
//! | Case | Peak measured / gate | Kept measured / gate | What dominates |
//! |---|---|---|---|
//! | typical | 1.03 / 1.4 | 0.83 / 1.2 | the project |
//! | short property keys | 0.54 / 0.8 | 0.38 / 0.6 | the project; 64 keys become one 8-byte set |
//! | short impact keys | 0.76 / 1.1 | 0.54 / 0.8 | the project |
//! | unknown keys (all three) | 0.02 / 0.1 | 0.00 / 0.1 | the read buffer: the load fails inside the first entity |
//! | long names | 1.08 / 1.5 | 1.04 / 1.5 | the names, moved into the project |
//! | one huge string | 1.02 / 1.4 | 0.00 / 0.1 | the parser's string buffer, which grows by doubling |
//! | minimal entities (mixed kinds) | 2.08 / 2.9 | 1.68 / 2.3 | the project's fixed per-entity cost (arena slot, ID map, counts) |
//! | attack trees | 2.77 / 3.4 | 1.95 / 2.4 | the trees' node arenas, ID maps, and step ratings |
//! | assets only, built-in / max vocabulary | 2.31 / 2.41, gates 3.2 / 3.3 | 1.68 / 1.78, gates 2.3 / 2.2 | the project's fixed per-asset cost; the empty count row is 16 bytes whatever the methodology |
//! | damages only, built-in / max vocabulary | 2.73 / 3.53, gates 3.7 / 4.8 | 1.85 / 2.65, gates 2.5 / 3.0 | the project: per damage scenario, one rating slot per impact category |
//! | threats only, built-in / max vocabulary | 1.91 / 1.93, gates 2.6 / 2.6 | 1.10 / 1.12, gates 1.5 / 1.6 | the project |
//! | targeted narrow, built-in / max vocabulary | 2.53 / 2.57, gates 3.5 / 3.5 | 1.72 / 1.76, gates 2.4 / 2.4 | the project: per asset and threat, the fixed cost plus one 8-byte count entry |
//! | targeted wide, max vocabulary | 1.57 / 2.2 | 1.36 / 1.9 | the project: per asset, a 64-entry count row (512 bytes) |
//!
//! The decoder's own share is the peak minus kept: at most about 0.9x the
//! file in every case. The rest is the project, which any project of that
//! many entities needs. Only `damages_only_max` peaks above 3x while
//! loading, from its dense impact slots; the budget limits the open
//! project (kept), so that peak is gated but not held to 3x (ADR 0010).
//!
//! Run: `cargo bench --bench store_memory` (also runs under
//! `cargo test --benches`). The profiler runs in testing mode, so it never
//! writes `dhat-heap.json`. Fails if a case's peak or kept factor exceeds
//! its gate. `STORE_MEMORY_CASE=<name>` runs one case.

#![forbid(unsafe_code)]

#[path = "store_kit/mod.rs"]
mod store_kit;

use std::fmt::Write as _;
use std::sync::Arc;

use aegis_model::{
    ImpactCategoryDef, LevelDef, Methodology, MethodologyDef, Project, PropertyDef, ThreatCategory,
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
    /// Upper bound on kept heap (the loaded project, still held when
    /// `load` returns) / file bytes. At most `SERVER_BUDGET` for a case
    /// that loads.
    max_kept: f64,
}

/// Engineering standards section 2: server memory per open project is at
/// most 3x the project's serialized size. Every case that loads keeps at
/// most this factor (`max_kept`), and no `max_kept` may exceed it.
const SERVER_BUDGET: f64 = 3.0;

/// A methodology with short keys at every vocabulary limit: properties
/// `p0`..`p63`, impact categories `c0`..`c31` rated `a` < `b`, threat
/// categories `t0`..`t63` (`tN` violates `pN` for N < 63; `t63` violates
/// every property, the widest reference-count row one threat can make).
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
                    risk_matrix: vec![vec!["a".into(), "b".into()], vec!["b".into(), "b".into()]],
                }
            })
            .collect(),
        feasibility_levels: ["a", "b"]
            .iter()
            .map(|&l| LevelDef {
                key: l.into(),
                label: l.into(),
            })
            .collect(),
        risk_levels: ["a", "b"]
            .iter()
            .map(|&l| LevelDef {
                key: l.into(),
                label: l.into(),
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
                    violates: if i == 63 {
                        (0..64).map(|p| format!("p{p}").into()).collect()
                    } else {
                        vec![format!("p{i}").into()]
                    },
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
    let trees = if kind == "attack_trees" {
        format!(",\"attack_trees\":[{list}]")
    } else {
        String::new()
    };
    format!(
        "{{\"format\":\"aegis-project\",\"version\":1,\"methodology\":{{\"key\":\"short\",\"version\":\"1\"}},\
         \"assets\":[{}],\"damage_scenarios\":[{}],\"threat_scenarios\":[{}]{trees}}}\n",
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

/// A snapshot over `m` of assets each targeted by exactly one threat of
/// `category`: assets with one name byte and exactly the properties
/// `category` violates, threats with no refs or links. Every asset then
/// has a non-empty reference-count row of `|violates(category)|` entries,
/// the densest file per byte for that row length.
fn targeted(m: &Methodology, category: Option<ThreatCategory>) -> Vec<u8> {
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
    let mut assets = String::new();
    let mut threats = String::new();
    let mut i = 0u64;
    while assets.len() + threats.len() < HOSTILE_BYTES {
        let sep = if i > 0 { "," } else { "" };
        let _ = write!(
            assets,
            "{sep}{{\"id\":\"{}\",\"name\":\"a\",\"properties\":[{props}]}}",
            id(i)
        );
        let _ = write!(
            threats,
            "{sep}{{\"id\":\"{}\",\"name\":\"a\",\"asset\":\"{}\",\"category\":\"{category_key}\",\"catalog_refs\":[],\"damages\":[]}}",
            id(2 << 40 | i),
            id(i)
        );
        i += 1;
    }
    format!(
        "{{\"format\":\"aegis-project\",\"version\":1,\"methodology\":{{\"key\":\"{}\",\"version\":\"{}\"}},\
         \"assets\":[{assets}],\"damage_scenarios\":[],\"threat_scenarios\":[{threats}]}}\n",
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
            max_factor: 1.4,
            max_kept: 1.2,
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
            max_factor: 0.8,
            max_kept: 0.6,
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
            max_factor: 1.1,
            max_kept: 0.8,
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
            max_kept: 0.1,
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
            max_kept: 0.1,
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
            max_kept: 0.1,
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
            max_factor: 1.5,
            max_kept: 1.5,
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
            max_factor: 1.4,
            max_kept: 0.1,
        },
        Case {
            name: "minimal_entities",
            methodology: Arc::clone(&short),
            bytes: minimal(),
            loads: true,
            max_factor: 2.9,
            max_kept: 2.3,
        },
        Case {
            name: "attack_trees",
            methodology: Arc::clone(&short),
            bytes: snapshot("attack_trees", |i| {
                let (r, a, b) = (id(3 * i + 1), id(3 * i + 2), id(3 * i + 3));
                format!(
                    "{{\"id\":\"{}\",\"nodes\":[\
                     {{\"id\":\"{r}\",\"label\":\"g\",\"gate\":\"or\",\"children\":[\"{a}\",\"{b}\"]}},\
                     {{\"id\":\"{a}\",\"label\":\"a\"}},\
                     {{\"id\":\"{b}\",\"label\":\"b\"}}\
                     ],\"step_feasibility\":{{\"{a}\":\"a\",\"{b}\":\"b\"}}}}",
                    id(i),
                )
            }),
            loads: true,
            max_factor: 3.4,
            max_kept: 2.4,
        },
    ];
    let builtin = store_kit::methodology();
    // (name, methodology, kind, peak gate, kept gate)
    for (name, m, kind, max_factor, max_kept) in [
        ("assets_only_builtin", &builtin, "assets", 3.2, 2.3),
        ("assets_only_max", &short, "assets", 3.3, 2.2),
        (
            "damages_only_builtin",
            &builtin,
            "damage_scenarios",
            3.7,
            2.5,
        ),
        ("damages_only_max", &short, "damage_scenarios", 4.8, 3.0),
        (
            "threats_only_builtin",
            &builtin,
            "threat_scenarios",
            2.6,
            1.5,
        ),
        ("threats_only_max", &short, "threat_scenarios", 2.6, 1.6),
    ] {
        cases.push(Case {
            name,
            methodology: Arc::clone(m),
            bytes: single_kind(m, kind),
            loads: true,
            max_factor,
            max_kept,
        });
    }
    let first = |m: &Methodology| m.threat_categories().next();
    let all = short.threat_category("t63");
    for (name, m, category, max_factor, max_kept) in [
        (
            "targeted_narrow_builtin",
            &builtin,
            first(&builtin),
            3.5,
            2.4,
        ),
        ("targeted_narrow_max", &short, first(&short), 3.5, 2.4),
        ("targeted_wide_max", &short, all, 2.2, 1.9),
    ] {
        cases.push(Case {
            name,
            methodology: Arc::clone(m),
            bytes: targeted(m, category),
            loads: true,
            max_factor,
            max_kept,
        });
    }
    cases
}

fn main() {
    let only = std::env::var("STORE_MEMORY_CASE").ok();
    let mut failed = Vec::new();
    for case in cases() {
        // A configuration check, so no edit can raise a kept gate past the
        // budget unnoticed.
        if case.loads && case.max_kept > SERVER_BUDGET {
            failed.push(format!(
                "{}: kept gate {} exceeds the server budget {SERVER_BUDGET}",
                case.name, case.max_kept
            ));
        }
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
                p.asset_count()
                    + p.damage_scenario_count()
                    + p.threat_scenario_count()
                    + p.attack_tree_count()
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
            "{:<24} {:>9} bytes  peak {:>10} bytes ({peak:>5.2} x)  kept {kept:>5.2} x  {:>8} allocs  {outcome}",
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
        if kept > case.max_kept {
            failed.push(format!(
                "{}: kept {kept:.2} x file exceeds {}",
                case.name, case.max_kept
            ));
        }
    }
    assert!(failed.is_empty(), "memory gates failed: {failed:#?}");
}
