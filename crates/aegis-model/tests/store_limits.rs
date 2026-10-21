//! Decode caps (AC11) and cap parity (AC12): every cap fires while
//! decoding, before model validation, with its own `Limit`; and every
//! project the model accepts, even at every model limit at once, saves,
//! loads, and logs.

mod common;

use std::io::Read;
use std::sync::Arc;

use aegis_model::{
    CatalogSchemeDef, CatalogSyntax, ImpactCategoryDef, LevelDef, Limit, MAX_CATALOG_REFS,
    MAX_DAMAGE_LINKS, MAX_NAME_CHARS, MAX_OP_RECORD_BYTES, MAX_PROJECT_FILE_BYTES, Methodology,
    MethodologyDef, Op, OpLogReader, Project, PropertyDef, SegmentDef, StoreError,
    ThreatCategoryDef,
};
use common::{asset_id, builtin, damage_id, read_log, save, threat_id, write_log};
use serde_json::{Value, json};

// ---------------------------------------------------------------------------
// Hostile inputs.
// ---------------------------------------------------------------------------

fn header(version: &str) -> Value {
    json!({"format": "aegis-oplog", "version": 1,
           "methodology": {"key": "iso21434-r155", "version": version}})
}

/// A log with one record holding `op`.
fn log_with(op: &Value) -> String {
    format!("{}\n{}\n", header("0.1.0"), json!({"seq": 1, "op": op}))
}

/// The error of the first record of `text`, or of its header.
fn log_error(text: &str) -> StoreError {
    let m = builtin();
    match OpLogReader::open(text.as_bytes(), m) {
        Err(e) => e,
        Ok(mut r) => match r.next() {
            Some(Err(e)) => e,
            other => panic!("expected an error, got {other:?}"),
        },
    }
}

/// A snapshot with the given entity lists.
fn snapshot(version: &str, assets: Value, damages: Value, threats: Value) -> String {
    json!({"format": "aegis-project", "version": 1,
           "methodology": {"key": "iso21434-r155", "version": version},
           "assets": assets, "damage_scenarios": damages, "threat_scenarios": threats})
    .to_string()
}

fn snapshot_error(text: &str) -> StoreError {
    match Project::load(text.as_bytes(), builtin()) {
        Err(e) => e,
        Ok(_) => panic!("expected an error"),
    }
}

fn id(n: u64) -> String {
    asset_id(n).to_string()
}

fn refs(n: usize) -> Value {
    (1..=n)
        .map(|i| json!({"scheme": "r155-annex5", "id": format!("{}", i % 32 + 1)}))
        .collect()
}

fn damages(n: usize) -> Value {
    (0..n as u64).map(|i| json!(id(1000 + i))).collect()
}

fn threat_op(refs: Value, damages: Value) -> Value {
    json!({"op": "add_threat_scenario", "id": id(1), "name": "t", "asset": id(2),
           "category": "tampering", "catalog_refs": refs, "damages": damages})
}

fn threat_entity(refs: Value, damages: Value) -> Value {
    json!([{"id": id(1), "name": "t", "asset": id(2), "category": "tampering",
            "catalog_refs": refs, "damages": damages}])
}

fn assert_limit(err: &StoreError, want: Limit) {
    assert!(
        matches!(err, StoreError::LimitExceeded(l) if *l == want),
        "want {want:?}, got {err:?}"
    );
}

#[test]
fn caps_catalog_refs() {
    let want = Limit::CatalogRefs { max: 16 };
    assert_limit(&log_error(&log_with(&threat_op(refs(17), json!([])))), want);
    let s = snapshot(
        "0.1.0",
        json!([]),
        json!([]),
        threat_entity(refs(17), json!([])),
    );
    assert_limit(&snapshot_error(&s), want);
}

#[test]
fn caps_damage_links() {
    let want = Limit::DamageLinks { max: 256 };
    assert_limit(
        &log_error(&log_with(&threat_op(json!([]), damages(257)))),
        want,
    );
    let s = snapshot(
        "0.1.0",
        json!([]),
        json!([]),
        threat_entity(json!([]), damages(257)),
    );
    assert_limit(&snapshot_error(&s), want);
}

#[test]
fn caps_impacts() {
    let want = Limit::Impacts { max: 32 };
    let impacts: serde_json::Map<String, Value> =
        (0..33).map(|i| (format!("c{i}"), json!("major"))).collect();
    let op = json!({"op": "add_damage_scenario", "id": id(1), "name": "d", "impacts": impacts});
    assert_limit(&log_error(&log_with(&op)), want);
    let d = json!([{"id": id(1), "name": "d", "impacts": impacts}]);
    assert_limit(
        &snapshot_error(&snapshot("0.1.0", json!([]), d, json!([]))),
        want,
    );
}

#[test]
fn caps_properties() {
    let want = Limit::Properties { max: 64 };
    let props: Vec<String> = (0..65).map(|i| format!("p{i}")).collect();
    for op in [
        json!({"op": "add_asset", "id": id(1), "name": "a", "properties": props}),
        json!({"op": "set_asset_properties", "id": id(1), "properties": props}),
    ] {
        assert_limit(&log_error(&log_with(&op)), want);
    }
    let a = json!([{"id": id(1), "name": "a", "properties": props}]);
    assert_limit(
        &snapshot_error(&snapshot("0.1.0", a, json!([]), json!([]))),
        want,
    );
}

#[test]
fn caps_names() {
    let want = Limit::NameBytes { max: 1024 };
    let name = "n".repeat(1025);
    for op in [
        json!({"op": "add_asset", "id": id(1), "name": name, "properties": ["integrity"]}),
        json!({"op": "rename_asset", "id": id(1), "name": name}),
        json!({"op": "add_damage_scenario", "id": id(1), "name": name, "impacts": {}}),
        json!({"op": "rename_damage_scenario", "id": id(1), "name": name}),
        json!({"op": "rename_threat_scenario", "id": id(1), "name": name}),
    ] {
        assert_limit(&log_error(&log_with(&op)), want);
    }
    let a = json!([{"id": id(1), "name": name, "properties": ["integrity"]}]);
    assert_limit(
        &snapshot_error(&snapshot("0.1.0", a, json!([]), json!([]))),
        want,
    );
}

#[test]
fn caps_keys() {
    let want = Limit::KeyBytes { max: 64 };
    let key = "k".repeat(65);
    for op in [
        json!({"op": "add_asset", "id": id(1), "name": "a", "properties": [key]}),
        json!({"op": "set_impact", "id": id(1), "category": key, "level": null}),
        json!({"op": "set_impact", "id": id(1), "category": "safety", "level": key}),
        json!({"op": "set_threat_category", "id": id(1), "category": key}),
        json!({"op": "add_catalog_ref", "threat": id(1),
               "catalog_ref": {"scheme": key, "id": "1"}}),
    ] {
        assert_limit(&log_error(&log_with(&op)), want);
    }
    let text = format!("{}\n", header("0.1.0")).replace("iso21434-r155", &key);
    assert_limit(&log_error(&text), want);
    let t = json!([{"id": id(1), "name": "t", "asset": id(2), "category": key,
                    "catalog_refs": [], "damages": []}]);
    assert_limit(
        &snapshot_error(&snapshot("0.1.0", json!([]), json!([]), t)),
        want,
    );
    let s = snapshot("0.1.0", json!([]), json!([]), json!([])).replace("iso21434-r155", &key);
    assert_limit(&snapshot_error(&s), want);
}

#[test]
fn caps_catalog_ref_text() {
    let want = Limit::CatalogRefBytes { max: 64 };
    let text = "1".repeat(65);
    let r = json!({"scheme": "r155-annex5", "id": text});
    for op in [
        json!({"op": "add_catalog_ref", "threat": id(1), "catalog_ref": r}),
        json!({"op": "remove_catalog_ref", "threat": id(1), "catalog_ref": r}),
        threat_op(json!([r]), json!([])),
    ] {
        assert_limit(&log_error(&log_with(&op)), want);
    }
    let s = snapshot(
        "0.1.0",
        json!([]),
        json!([]),
        threat_entity(json!([r]), json!([])),
    );
    assert_limit(&snapshot_error(&s), want);
}

#[test]
fn caps_methodology_version() {
    let want = Limit::MethodologyVersionBytes { max: 256 };
    let version = "v".repeat(257);
    assert_limit(&log_error(&format!("{}\n", header(&version))), want);
    let s = snapshot(&version, json!([]), json!([]), json!([]));
    assert_limit(&snapshot_error(&s), want);
    // 256 bytes is accepted.
    let ok = "v".repeat(256);
    let text = format!("{}\n", header(&ok));
    let reader = OpLogReader::open(text.as_bytes(), builtin());
    assert_eq!(
        reader.map(|r| r.methodology_version().len()).ok(),
        Some(256)
    );
}

#[test]
fn caps_op_record_bytes() {
    let want = Limit::OpRecordBytes { max: 65_536 };
    let long_line = "x".repeat(MAX_OP_RECORD_BYTES + 1);
    assert_limit(
        &log_error(&format!("{}\n{long_line}\n", header("0.1.0"))),
        want,
    );
    // Without a newline, and as the header.
    assert_limit(
        &log_error(&format!("{}\n{long_line}", header("0.1.0"))),
        want,
    );
    assert_limit(&log_error(&long_line), want);
    // A valid record padded with spaces to the cap still decodes.
    let record = json!({"seq": 1, "op": {"op": "remove_asset", "id": id(1)}}).to_string();
    let padded = format!("{record}{}", " ".repeat(MAX_OP_RECORD_BYTES - record.len()));
    let text = format!("{}\n{padded}\n", header("0.1.0"));
    let read = OpLogReader::open(text.as_bytes(), builtin()).map(|r| r.count());
    assert_eq!(read.ok(), Some(1));
}

#[test]
fn caps_project_file_bytes() {
    let want = Limit::ProjectFileBytes {
        max: 64 * 1024 * 1024,
    };
    let input = std::io::repeat(b' ').take(MAX_PROJECT_FILE_BYTES + 1);
    match Project::load(input, builtin()) {
        Err(e) => assert_limit(&e, want),
        Ok(_) => panic!("expected an error"),
    }
}

// ---------------------------------------------------------------------------
// AC12: cap parity.
// ---------------------------------------------------------------------------

/// A 64-byte key: `prefix`, `i`, then `x` padding.
fn key64(prefix: &str, i: usize) -> Arc<str> {
    let mut k = format!("{prefix}{i}");
    while k.len() < 64 {
        k.push('x');
    }
    k.into()
}

/// A methodology at every vocabulary limit, with 64-byte keys: 32 impact
/// categories of 16 levels, 64 properties, 64 threat categories, and 16
/// catalog schemes with a 16-byte prefix and 4 segments of 5 digits.
fn maxed() -> Arc<Methodology> {
    let label = |k: &Arc<str>| k.clone();
    let def = MethodologyDef {
        key: key64("maxed", 0),
        version: "\u{1d11e}".repeat(64).into(),
        label: "Maxed".into(),
        impact_categories: (0..32)
            .map(|i| {
                let key = key64("c", i);
                ImpactCategoryDef {
                    label: label(&key),
                    key,
                    levels: (0..16)
                        .map(|l| {
                            let key = key64("l", l);
                            LevelDef {
                                label: label(&key),
                                key,
                            }
                        })
                        .collect(),
                }
            })
            .collect(),
        properties: (0..64)
            .map(|i| {
                let key = key64("p", i);
                PropertyDef {
                    label: label(&key),
                    key,
                }
            })
            .collect(),
        threat_categories: (0..64)
            .map(|i| {
                let key = key64("t", i);
                ThreatCategoryDef {
                    label: label(&key),
                    key,
                    violates: vec![key64("p", i)],
                }
            })
            .collect(),
        catalog_schemes: (0..16)
            .map(|i| {
                let key = key64("s", 15 - i);
                CatalogSchemeDef {
                    label: label(&key),
                    key,
                    syntax: CatalogSyntax {
                        prefix: "ABCDEFGHIJKLMNOP".into(),
                        segments: vec![
                            SegmentDef {
                                min: 0,
                                max: 65535,
                                digits: None,
                            };
                            4
                        ],
                        required: 1,
                    },
                }
            })
            .collect(),
    };
    Arc::new(Methodology::new(def).expect("maxed methodology is valid"))
}

#[test]
fn limits_admit_every_valid_project() {
    let m = maxed();
    let long_name: Arc<str> = "\u{1f512}".repeat(MAX_NAME_CHARS).into();
    assert_eq!(long_name.len(), 1024);
    let mut ops = vec![Op::AddAsset {
        id: asset_id(1),
        name: long_name.clone(),
        properties: m.all_properties(),
    }];
    let top: Vec<_> = m
        .impact_categories()
        .map(|c| (c, m.levels(c).last().expect("level")))
        .collect();
    let damage_ids: Vec<_> = (0..MAX_DAMAGE_LINKS as u64)
        .map(|i| damage_id(100 + i))
        .collect();
    for &d in &damage_ids {
        ops.push(Op::AddDamageScenario {
            id: d,
            name: long_name.clone(),
            impacts: top.clone(),
        });
    }
    // 16 refs over all 16 schemes, each 39 bytes of text.
    let catalog_refs: Vec<_> = m
        .catalog_schemes()
        .map(|s| {
            m.parse_catalog_ref(s, "ABCDEFGHIJKLMNOP65535.65535.65535.65535")
                .expect("ref")
        })
        .collect();
    assert_eq!(catalog_refs.len(), MAX_CATALOG_REFS);
    ops.push(Op::AddThreatScenario {
        id: threat_id(1),
        name: long_name.clone(),
        asset: asset_id(1),
        category: m.threat_categories().last().expect("category"),
        catalog_refs,
        damages: damage_ids.iter().rev().copied().collect(),
    });
    let mut p = Project::new(Arc::clone(&m));
    for op in &ops {
        p.apply(op.clone()).expect("valid at every limit");
    }

    // Snapshot.
    let bytes = save(&p);
    let loaded = Project::load(bytes.as_slice(), Arc::clone(&m)).expect("loads");
    assert_eq!(loaded, p);
    assert_eq!(save(&loaded), bytes);
    // Catalog refs are written by scheme key, which here is the reverse of
    // the declaration order.
    let v: Value = serde_json::from_slice(&bytes).expect("json");
    let schemes: Vec<&str> = v["threat_scenarios"][0]["catalog_refs"]
        .as_array()
        .expect("refs")
        .iter()
        .filter_map(|r| r["scheme"].as_str())
        .collect();
    let mut sorted = schemes.clone();
    sorted.sort_unstable();
    assert_eq!(schemes, sorted);
    assert_ne!(
        m.catalog_schemes()
            .next()
            .and_then(|s| m.catalog_scheme_key(s)),
        Some(sorted[0])
    );

    // Op log: every op encodes within the record cap and reads back.
    let log = write_log(&m, &ops);
    let longest = log.split(|&b| b == b'\n').map(<[u8]>::len).max();
    assert!(
        longest.is_some_and(|n| n <= MAX_OP_RECORD_BYTES),
        "{longest:?}"
    );
    let read: Vec<Op> = read_log(&m, &log).into_iter().map(|r| r.op).collect();
    assert_eq!(read, ops);
    eprintln!(
        "maxed: snapshot {} bytes, longest log line {longest:?} bytes",
        bytes.len()
    );
}
