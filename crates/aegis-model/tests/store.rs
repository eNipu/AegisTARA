//! Save and load through the public API: determinism, a reordered
//! methodology, keys only on disk, the golden v1 files, op log sequencing,
//! framing and header errors, and errors that never contain names.

mod common;

use std::sync::Arc;

use aegis_model::{
    Methodology, ModelError, Op, OpLogReader, OpLogWriter, Project, StoreError, VocabKind,
};
use common::{
    SENTINEL, asset_id, builtin, cat, cref, damage_id, describe, lvl, prop, props, read_log,
    reordered, save, scheme, second, second_def, threat_cat, threat_id, wide, write_log,
};
use serde_json::Value;

const PROJECT_V1: &str = "tests/fixtures/project-v1.json";
const OPLOG_V1: &str = "tests/fixtures/oplog-v1.jsonl";

fn load(bytes: &[u8], m: &Arc<Methodology>) -> Result<Project, StoreError> {
    Project::load(bytes, Arc::clone(m))
}

fn apply_all(m: &Arc<Methodology>, ops: &[Op]) -> Project {
    let mut p = Project::new(Arc::clone(m));
    for op in ops {
        if let Err(e) = p.apply(op.clone()) {
            panic!("{op:?} failed: {e}");
        }
    }
    p
}

// ---------------------------------------------------------------------------
// The golden v1 ops: every op variant, every vocabulary kind, fixed IDs.
// ---------------------------------------------------------------------------

fn golden_ops(m: &Methodology) -> Vec<Op> {
    let annex5 = scheme(m, "r155-annex5");
    let (safety, financial, operational, privacy) = (
        cat(m, "safety"),
        cat(m, "financial"),
        cat(m, "operational"),
        cat(m, "privacy"),
    );
    let (ecu, tcu, spare) = (asset_id(1), asset_id(2), asset_id(3));
    let (crash, leak, temp) = (damage_id(4), damage_id(5), damage_id(6));
    let (firmware, records, fob) = (threat_id(7), threat_id(8), threat_id(9));
    vec![
        Op::AddAsset {
            id: ecu,
            name: "Brake ECU".into(),
            properties: props(m, &["integrity", "availability"]),
        },
        Op::AddAsset {
            id: tcu,
            name: "Telematics unit".into(),
            properties: m.all_properties(),
        },
        Op::RenameAsset {
            id: tcu,
            name: "Telematics control unit".into(),
        },
        Op::SetAssetProperties {
            id: ecu,
            properties: props(m, &["availability", "authenticity", "integrity"]),
        },
        Op::AddAsset {
            id: spare,
            name: "Spare".into(),
            properties: props(m, &["confidentiality"]),
        },
        Op::RemoveAsset { id: spare },
        Op::AddDamageScenario {
            id: crash,
            name: "Vehicle cannot brake".into(),
            impacts: vec![
                (safety, lvl(m, safety, "severe")),
                (operational, lvl(m, operational, "major")),
            ],
        },
        Op::AddDamageScenario {
            id: leak,
            name: "Owner data leaks".into(),
            impacts: vec![(privacy, lvl(m, privacy, "moderate"))],
        },
        Op::RenameDamageScenario {
            id: leak,
            name: "Owner data is disclosed".into(),
        },
        Op::SetImpact {
            id: leak,
            category: financial,
            level: Some(lvl(m, financial, "negligible")),
        },
        Op::SetImpact {
            id: crash,
            category: operational,
            level: None,
        },
        Op::AddDamageScenario {
            id: temp,
            name: "Temporary".into(),
            impacts: vec![],
        },
        Op::RemoveDamageScenario { id: temp },
        Op::AddThreatScenario {
            id: firmware,
            name: "Modify brake firmware \"v2\" \u{2013} \u{dc} \\ tab\t".into(),
            asset: ecu,
            category: threat_cat(m, "tampering"),
            catalog_refs: vec![cref(m, annex5, "5"), cref(m, annex5, "4.1")],
            damages: vec![crash],
        },
        Op::AddThreatScenario {
            id: records,
            name: "Extract owner records".into(),
            asset: tcu,
            category: threat_cat(m, "information-disclosure"),
            catalog_refs: vec![],
            damages: vec![leak],
        },
        Op::RenameThreatScenario {
            id: records,
            name: "Read owner records over the air".into(),
        },
        Op::SetThreatCategory {
            id: firmware,
            category: threat_cat(m, "denial-of-service"),
        },
        Op::AddCatalogRef {
            threat: records,
            catalog_ref: cref(m, annex5, "7.2"),
        },
        Op::RemoveCatalogRef {
            threat: firmware,
            catalog_ref: cref(m, annex5, "5"),
        },
        Op::LinkDamage {
            threat: firmware,
            damage: leak,
        },
        Op::UnlinkDamage {
            threat: firmware,
            damage: leak,
        },
        Op::AddThreatScenario {
            id: fob,
            name: "Spoof key fob".into(),
            asset: tcu,
            category: threat_cat(m, "spoofing"),
            catalog_refs: vec![],
            damages: vec![],
        },
        Op::RemoveThreatScenario { id: fob },
        Op::LinkDamage {
            threat: records,
            damage: crash,
        },
    ]
}

/// Writes the golden files. Run once, after a deliberate format change
/// only: `cargo test --test store regenerate_golden_fixtures -- --ignored`.
#[test]
#[ignore = "rewrites the frozen v1 fixtures"]
fn regenerate_golden_fixtures() {
    let m = builtin();
    let ops = golden_ops(&m);
    std::fs::write(OPLOG_V1, write_log(&m, &ops)).expect("write log");
    std::fs::write(PROJECT_V1, save(&apply_all(&m, &ops))).expect("write snapshot");
}

// ---------------------------------------------------------------------------
// AC6: golden v1 files.
// ---------------------------------------------------------------------------

#[test]
fn golden_v1_snapshot_loads_and_resaves_byte_for_byte() {
    let m = builtin();
    let bytes = std::fs::read(PROJECT_V1).expect("fixture");
    let p = load(&bytes, &m).expect("loads");
    assert_eq!(save(&p), bytes);
    assert_eq!(p, apply_all(&m, &golden_ops(&m)));
    assert_eq!(p.asset_count(), 2);
    assert_eq!(p.damage_scenario_count(), 2);
    assert_eq!(p.threat_scenario_count(), 2);
}

#[test]
fn golden_v1_log_reads_replays_and_reencodes_byte_for_byte() {
    let m = builtin();
    let bytes = std::fs::read(OPLOG_V1).expect("fixture");
    let records = read_log(&m, &bytes);
    let ops: Vec<Op> = records.iter().map(|r| r.op.clone()).collect();
    let seqs: Vec<u64> = records.iter().map(|r| r.seq).collect();
    assert_eq!(seqs, (1..=ops.len() as u64).collect::<Vec<_>>());
    assert_eq!(ops, golden_ops(&m));
    assert_eq!(write_log(&m, &ops), bytes);
    let kinds: std::collections::BTreeSet<_> = ops.iter().map(common::variant).collect();
    assert_eq!(kinds.len(), 16, "{kinds:?}");
    let snapshot = std::fs::read(PROJECT_V1).expect("fixture");
    assert_eq!(apply_all(&m, &ops), load(&snapshot, &m).expect("loads"));
}

#[test]
fn golden_v1_files_are_lf_only_and_end_in_newline() {
    for path in [PROJECT_V1, OPLOG_V1] {
        let bytes = std::fs::read(path).expect("fixture");
        assert!(!bytes.contains(&b'\r'), "{path}");
        assert_eq!(bytes.last(), Some(&b'\n'), "{path}");
    }
}

// ---------------------------------------------------------------------------
// AC3: determinism.
// ---------------------------------------------------------------------------

#[test]
fn save_is_deterministic() {
    let m = builtin();
    let annex5 = scheme(&m, "r155-annex5");
    let safety = cat(&m, "safety");
    let privacy = cat(&m, "privacy");
    let (a1, a2, a3) = (asset_id(1), asset_id(2), asset_id(3));
    let (d1, d2) = (damage_id(4), damage_id(5));
    let t1 = threat_id(6);
    let add_asset = |id, name: &str| Op::AddAsset {
        id,
        name: name.into(),
        properties: m.all_properties(),
    };
    let add_damage = |id, impacts| Op::AddDamageScenario {
        id,
        name: "d".into(),
        impacts,
    };
    let rated = vec![
        (privacy, lvl(&m, privacy, "major")),
        (safety, lvl(&m, safety, "moderate")),
    ];
    let threat = |refs: Vec<&str>, damages| Op::AddThreatScenario {
        id: t1,
        name: "t".into(),
        asset: a1,
        category: threat_cat(&m, "tampering"),
        catalog_refs: refs.iter().map(|r| cref(&m, annex5, r)).collect(),
        damages,
    };

    // One order, with a removed asset and a removed damage scenario.
    let first = apply_all(
        &m,
        &[
            add_asset(a3, "gone"),
            add_asset(a2, "b"),
            add_asset(a1, "a"),
            add_damage(d2, vec![]),
            add_damage(damage_id(9), vec![]),
            add_damage(d1, rated.clone()),
            Op::RemoveAsset { id: a3 },
            Op::RemoveDamageScenario { id: damage_id(9) },
            threat(vec!["5", "4.1", "4"], vec![d2, d1]),
        ],
    );
    // Another order, ratings set one by one, refs and links added later.
    let mut reversed = rated.clone();
    reversed.reverse();
    let second = apply_all(
        &m,
        &[
            add_damage(d1, vec![]),
            Op::SetImpact {
                id: d1,
                category: reversed[0].0,
                level: Some(reversed[0].1),
            },
            Op::SetImpact {
                id: d1,
                category: reversed[1].0,
                level: Some(reversed[1].1),
            },
            add_asset(a1, "a"),
            add_damage(d2, vec![]),
            add_asset(a2, "b"),
            threat(vec!["4"], vec![d1]),
            Op::AddCatalogRef {
                threat: t1,
                catalog_ref: cref(&m, annex5, "4.1"),
            },
            Op::AddCatalogRef {
                threat: t1,
                catalog_ref: cref(&m, annex5, "5"),
            },
            Op::LinkDamage {
                threat: t1,
                damage: d2,
            },
        ],
    );
    assert_eq!(first, second);
    let bytes = save(&first);
    assert_eq!(bytes, save(&second));
    assert_eq!(bytes, save(&first), "saving twice");
    assert_eq!(
        save(&load(&bytes, &m).expect("loads")),
        bytes,
        "load then save"
    );
}

// ---------------------------------------------------------------------------
// AC4: a reordered methodology keeps the meaning.
// ---------------------------------------------------------------------------

/// A project over the second methodology that uses every vocabulary kind.
fn second_ops(m: &Methodology) -> Vec<Op> {
    let capec = scheme(m, "capec");
    let harm = cat(m, "patient-harm");
    let exposure = cat(m, "data-exposure");
    let loss = cat(m, "service-loss");
    vec![
        Op::AddAsset {
            id: asset_id(1),
            name: "Infusion pump".into(),
            properties: props(m, &["i", "a"]),
        },
        Op::AddAsset {
            id: asset_id(2),
            name: "Monitor".into(),
            properties: props(m, &["c", "a"]),
        },
        Op::AddDamageScenario {
            id: damage_id(3),
            name: "Overdose".into(),
            impacts: vec![
                (loss, lvl(m, loss, "intolerable")),
                (harm, lvl(m, harm, "critical")),
            ],
        },
        Op::AddDamageScenario {
            id: damage_id(4),
            name: "Record leak".into(),
            impacts: vec![(exposure, lvl(m, exposure, "high"))],
        },
        Op::SetImpact {
            id: damage_id(4),
            category: harm,
            level: Some(lvl(m, harm, "minor")),
        },
        Op::AddThreatScenario {
            id: threat_id(5),
            name: "Lock the pump".into(),
            asset: asset_id(1),
            category: threat_cat(m, "ransomware"),
            catalog_refs: vec![cref(m, capec, "CAPEC-77"), cref(m, capec, "CAPEC-5")],
            damages: vec![damage_id(4), damage_id(3)],
        },
        Op::AddThreatScenario {
            id: threat_id(6),
            name: "Sniff monitor traffic".into(),
            asset: asset_id(2),
            category: threat_cat(m, "data-leak"),
            catalog_refs: vec![],
            damages: vec![damage_id(4)],
        },
        Op::SetAssetProperties {
            id: asset_id(2),
            properties: m.all_properties(),
        },
        Op::SetThreatCategory {
            id: threat_id(6),
            category: threat_cat(m, "outage"),
        },
        Op::AddCatalogRef {
            threat: threat_id(6),
            catalog_ref: cref(m, capec, "CAPEC-1"),
        },
    ]
}

#[test]
fn reordered_methodology_keeps_meaning() {
    let m = second();
    let r = Arc::new(Methodology::new(reordered(second_def())).expect("valid"));
    assert_eq!((r.key(), r.version()), (m.key(), m.version()));
    // The indices really differ.
    assert_ne!(m.property("i"), r.property("i"));
    assert_ne!(
        m.impact_category("patient-harm"),
        r.impact_category("patient-harm")
    );
    assert_ne!(
        m.threat_category("ransomware"),
        r.threat_category("ransomware")
    );
    assert_ne!(m.catalog_scheme("capec"), r.catalog_scheme("capec"));

    // Snapshot: same keys per entity, identical bytes when saved again.
    let ops = second_ops(&m);
    let p = apply_all(&m, &ops);
    let bytes = save(&p);
    let loaded = load(&bytes, &r).expect("loads under the reordered methodology");
    assert_eq!(describe(&loaded), describe(&p));
    assert_eq!(save(&loaded), bytes);

    // Op log: every op reads back, and re-encodes to identical bytes.
    let log = write_log(&m, &ops);
    let records = read_log(&r, &log);
    let reread: Vec<Op> = records.into_iter().map(|x| x.op).collect();
    assert_ne!(
        reread, ops,
        "indices differ under the reordered methodology"
    );
    assert_eq!(write_log(&r, &reread), log);
    assert_eq!(describe(&apply_all(&r, &reread)), describe(&p));
}

// ---------------------------------------------------------------------------
// AC5: no indices on disk.
// ---------------------------------------------------------------------------

/// Walks `v` and records every number with the key it sits under.
fn numbers_under(v: &Value, key: &str, out: &mut Vec<String>) {
    match v {
        Value::Number(_) => out.push(key.to_string()),
        Value::Array(items) => items.iter().for_each(|x| numbers_under(x, key, out)),
        Value::Object(map) => map.iter().for_each(|(k, x)| numbers_under(x, k, out)),
        _ => {}
    }
}

#[test]
fn only_keys_on_disk() {
    for m in [builtin(), second()] {
        let ops = if m.key() == "med-device-demo" {
            second_ops(&m)
        } else {
            golden_ops(&m)
        };
        let snapshot = save(&apply_all(&m, &ops));
        let log = write_log(&m, &ops);
        let mut keys = Vec::new();
        for line in [snapshot.as_slice(), log.as_slice()]
            .into_iter()
            .flat_map(|b| b.split(|&c| c == b'\n'))
            .filter(|l| !l.is_empty())
        {
            let v: Value = serde_json::from_slice(line).expect("json");
            numbers_under(&v, "", &mut keys);
        }
        assert!(!keys.is_empty());
        assert!(
            keys.iter().all(|k| k == "version" || k == "seq"),
            "{keys:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// AC9: append-only and sequencing.
// ---------------------------------------------------------------------------

fn rename(n: u64) -> Op {
    Op::RenameAsset {
        id: asset_id(1),
        name: format!("name {n}").into(),
    }
}

#[test]
fn op_log_sequencing_counts_from_one() {
    let m = builtin();
    let mut log = OpLogWriter::create(Vec::new(), Arc::clone(&m)).expect("header");
    assert_eq!(log.last_seq(), 0);
    let seqs: Vec<u64> = (1..=3)
        .map(|n| log.append(&rename(n)).expect("append"))
        .collect();
    assert_eq!(seqs, [1, 2, 3]);
    assert_eq!(log.last_seq(), 3);
    let bytes = log.into_inner().expect("flush");
    let text = String::from_utf8(bytes.clone()).expect("utf8");
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 4);
    for (i, line) in lines.iter().skip(1).enumerate() {
        assert!(line.starts_with(&format!("{{\"seq\":{},", i + 1)), "{line}");
    }
    let mut reader = OpLogReader::open(bytes.as_slice(), m).expect("header");
    assert_eq!((reader.line(), reader.last_seq()), (1, 0));
    let read: Vec<_> = reader.by_ref().map(|r| r.expect("record")).collect();
    assert_eq!(read.iter().map(|r| r.seq).collect::<Vec<_>>(), [1, 2, 3]);
    assert_eq!((reader.line(), reader.last_seq()), (4, 3));
}

#[test]
fn op_log_sequencing_resume_continues() {
    let m = builtin();
    let mut log = OpLogWriter::create(Vec::new(), Arc::clone(&m)).expect("header");
    log.append(&rename(1)).expect("append");
    log.append(&rename(2)).expect("append");
    let bytes = log.into_inner().expect("flush");
    let last = {
        let mut reader = OpLogReader::open(bytes.as_slice(), Arc::clone(&m)).expect("header");
        reader.by_ref().for_each(|r| drop(r.expect("record")));
        reader.last_seq()
    };
    assert_eq!(last, 2);
    // Resuming writes nothing by itself.
    let same = OpLogWriter::resume(bytes.clone(), Arc::clone(&m), last)
        .into_inner()
        .expect("flush");
    assert_eq!(same, bytes);
    let mut log = OpLogWriter::resume(bytes, Arc::clone(&m), last);
    assert_eq!(log.append(&rename(3)).expect("append"), 3);
    let bytes = log.into_inner().expect("flush");
    let seqs: Vec<u64> = read_log(&m, &bytes).iter().map(|r| r.seq).collect();
    assert_eq!(seqs, [1, 2, 3]);
}

#[test]
fn op_log_sequencing_rejects_gap_repeat_and_step_back() {
    let m = builtin();
    let bytes = write_log(&m, &[rename(1), rename(2), rename(3)]);
    let text = String::from_utf8(bytes).expect("utf8");
    for (found, label) in [(4u64, "gap"), (2, "repeat"), (1, "step back")] {
        let bad = text.replace("{\"seq\":3,", &format!("{{\"seq\":{found},"));
        let results: Vec<_> = OpLogReader::open(bad.as_bytes(), Arc::clone(&m))
            .expect("header")
            .collect();
        assert_eq!(results.len(), 3, "{label}: fused after the error");
        assert!(results[0].is_ok() && results[1].is_ok(), "{label}");
        assert!(
            matches!(
                results[2],
                Err(StoreError::Sequence { expected: 3, found: f }) if f == found
            ),
            "{label}: {:?}",
            results[2]
        );
    }
    // A log that does not start at 1.
    let bad = text.replace("{\"seq\":1,", "{\"seq\":0,");
    let first = OpLogReader::open(bad.as_bytes(), m).expect("header").next();
    assert!(matches!(
        first,
        Some(Err(StoreError::Sequence {
            expected: 1,
            found: 0
        }))
    ));
}

#[test]
fn op_log_sequencing_unencodable_op_writes_nothing() {
    let m = builtin();
    let w = wide();
    let bytes = write_log(&m, &[rename(1)]);
    let foreign = [
        Op::SetAssetProperties {
            id: asset_id(1),
            properties: w.all_properties(),
        },
        Op::SetThreatCategory {
            id: threat_id(1),
            category: w.threat_categories().last().expect("category"),
        },
        Op::SetImpact {
            id: damage_id(1),
            category: w.impact_categories().last().expect("category"),
            level: None,
        },
        Op::AddCatalogRef {
            threat: threat_id(1),
            catalog_ref: cref(&w, scheme(&w, "w2"), "1.2.3"),
        },
        // A repeated impact category could not be read back either.
        Op::AddDamageScenario {
            id: damage_id(2),
            name: "d".into(),
            impacts: vec![
                (cat(&m, "safety"), lvl(&m, cat(&m, "safety"), "major")),
                (cat(&m, "safety"), lvl(&m, cat(&m, "safety"), "severe")),
            ],
        },
    ];
    for op in &foreign {
        let mut log = OpLogWriter::resume(bytes.clone(), Arc::clone(&m), 1);
        assert!(
            matches!(log.append(op), Err(StoreError::UnencodableOp)),
            "{op:?}"
        );
        assert_eq!(log.last_seq(), 1);
        assert_eq!(log.append(&rename(2)).expect("still usable"), 2);
        let after = log.into_inner().expect("flush");
        assert_eq!(after, write_log(&m, &[rename(1), rename(2)]), "{op:?}");
    }
}

// ---------------------------------------------------------------------------
// AC10: framing and header errors.
// ---------------------------------------------------------------------------

fn log_text(m: &Arc<Methodology>, ops: &[Op]) -> String {
    String::from_utf8(write_log(m, ops)).expect("utf8")
}

/// Reads all records of `text`; returns the Ok count and the first error.
fn read_until_error(m: &Arc<Methodology>, text: &str) -> (usize, Option<StoreError>) {
    let mut reader = match OpLogReader::open(text.as_bytes(), Arc::clone(m)) {
        Ok(r) => r,
        Err(e) => return (0, Some(e)),
    };
    let mut ok = 0;
    for r in reader.by_ref() {
        match r {
            Ok(_) => ok += 1,
            Err(e) => return (ok, Some(e)),
        }
    }
    (ok, None)
}

#[test]
fn framing_and_header_errors_framing() {
    let m = builtin();
    let text = log_text(&m, &[rename(1), rename(2)]);

    let truncated = text.trim_end_matches('\n');
    let (ok, err) = read_until_error(&m, truncated);
    assert_eq!(ok, 1);
    assert!(
        matches!(err, Some(StoreError::TruncatedRecord { line: 3 })),
        "{err:?}"
    );

    let blank = text.replacen("\n{\"seq\":2", "\n\n{\"seq\":2", 1);
    let (ok, err) = read_until_error(&m, &blank);
    assert_eq!(ok, 1);
    assert!(
        matches!(err, Some(StoreError::Decode { line: 3, .. })),
        "{err:?}"
    );

    let crlf = text.replace('\n', "\r\n");
    let (_, err) = read_until_error(&m, &crlf);
    assert!(
        matches!(err, Some(StoreError::Decode { line: 1, .. })),
        "{err:?}"
    );
    let crlf_records = text.replacen("}\n{\"seq\":2", "}\r\n{\"seq\":2", 1);
    let (ok, err) = read_until_error(&m, &crlf_records);
    assert_eq!(ok, 0);
    assert!(
        matches!(err, Some(StoreError::Decode { line: 2, .. })),
        "{err:?}"
    );

    let trailing = text.replacen("}\n{\"seq\":2", "} x\n{\"seq\":2", 1);
    let (_, err) = read_until_error(&m, &trailing);
    assert!(
        matches!(err, Some(StoreError::Decode { line: 2, .. })),
        "{err:?}"
    );
    let two_values = text.replacen("}\n{\"seq\":2", "}{}\n{\"seq\":2", 1);
    let (_, err) = read_until_error(&m, &two_values);
    assert!(matches!(err, Some(StoreError::Decode { .. })), "{err:?}");

    let (_, err) = read_until_error(&m, "");
    assert!(matches!(err, Some(StoreError::MissingHeader)), "{err:?}");
    let header_only = text.lines().next().expect("header");
    let (_, err) = read_until_error(&m, header_only);
    assert!(
        matches!(err, Some(StoreError::TruncatedRecord { line: 1 })),
        "{err:?}"
    );

    let unknown_field = text.replacen("{\"seq\":2,", "{\"seq\":2,\"extra\":true,", 1);
    let (ok, err) = read_until_error(&m, &unknown_field);
    assert_eq!(ok, 1);
    assert!(
        matches!(err, Some(StoreError::Decode { line: 3, .. })),
        "{err:?}"
    );
    let unknown_op_field = text.replacen("\"name\":\"name 2\"", "\"name\":\"n\",\"x\":1", 1);
    let (_, err) = read_until_error(&m, &unknown_op_field);
    assert!(matches!(err, Some(StoreError::Decode { .. })), "{err:?}");

    // Snapshot: trailing data and unknown fields.
    let snapshot = String::from_utf8(save(&apply_all(&m, &golden_ops(&m)))).expect("utf8");
    let err = load(format!("{snapshot}{{}}").as_bytes(), &m).err();
    assert!(matches!(err, Some(StoreError::Decode { .. })), "{err:?}");
    let err = load(
        snapshot
            .replacen("\"assets\"", "\"extra\":1,\"assets\"", 1)
            .as_bytes(),
        &m,
    )
    .err();
    assert!(matches!(err, Some(StoreError::Decode { .. })), "{err:?}");
    let err = load(b"", &m).err();
    assert!(matches!(err, Some(StoreError::Decode { .. })), "{err:?}");
}

#[test]
fn framing_and_header_errors_header() {
    let m = builtin();
    let text = log_text(&m, &[rename(1)]);
    let snapshot = String::from_utf8(save(&apply_all(&m, &golden_ops(&m)))).expect("utf8");

    // A wrong format name is "not this kind of file"; a newer version
    // carries its number, so an application can say "update".
    for (from, to, version) in [
        ("\"aegis-oplog\"", "\"aegis-oplog2\"", None),
        ("\"version\":1,", "\"version\":2,", Some(2)),
    ] {
        let (_, err) = read_until_error(&m, &text.replacen(from, to, 1));
        assert!(
            matches!(err, Some(StoreError::UnsupportedFormat { version: v }) if v == version),
            "{to}: {err:?}"
        );
    }
    for (from, to, version) in [
        ("\"aegis-project\"", "\"aegis-oplog\"", None),
        ("\"version\":1,", "\"version\":2,", Some(2)),
    ] {
        let err = load(snapshot.replacen(from, to, 1).as_bytes(), &m).err();
        assert!(
            matches!(err, Some(StoreError::UnsupportedFormat { version: v }) if v == version),
            "{to}: {err:?}"
        );
    }
    let newer = StoreError::UnsupportedFormat { version: Some(2) }.to_string();
    assert!(
        newer.contains("version 2") && newer.contains("newer"),
        "{newer}"
    );
    // A snapshot is not a log, and a log is not a snapshot.
    let (_, err) = read_until_error(&m, &snapshot);
    assert!(
        matches!(err, Some(StoreError::UnsupportedFormat { version: None })),
        "{err:?}"
    );
    let err = load(text.as_bytes(), &m).err();
    assert!(
        matches!(err, Some(StoreError::UnsupportedFormat { version: None })),
        "{err:?}"
    );

    // Another methodology key.
    let s = second();
    let (_, err) = read_until_error(&s, &text);
    assert!(
        matches!(&err, Some(StoreError::MethodologyMismatch { expected, found })
            if &**expected == "med-device-demo" && &**found == m.key()),
        "{err:?}"
    );
    let err = load(snapshot.as_bytes(), &s).err();
    assert!(
        matches!(err, Some(StoreError::MethodologyMismatch { .. })),
        "{err:?}"
    );

    // Another methodology version with the same key is accepted.
    let newer = text.replacen("\"version\":\"0.1.0\"", "\"version\":\"0.2.0-beta\"", 1);
    let reader = OpLogReader::open(newer.as_bytes(), Arc::clone(&m)).expect("header");
    assert_eq!(reader.methodology_version(), "0.2.0-beta");
    assert_eq!(reader.filter(Result::is_ok).count(), 1);
    let newer = snapshot.replacen("\"version\":\"0.1.0\"", "\"version\":\"9\"", 1);
    assert_eq!(
        load(newer.as_bytes(), &m).expect("loads"),
        apply_all(&m, &golden_ops(&m))
    );
}

#[test]
fn framing_and_header_errors_keys_ids_and_entries() {
    let m = builtin();
    let snapshot = String::from_utf8(save(&apply_all(&m, &golden_ops(&m)))).expect("utf8");
    let log = log_text(&m, &golden_ops(&m));

    // An unknown key of each vocabulary kind, in the log and the snapshot.
    let cases = [
        ("\"integrity\"", "\"nope\"", VocabKind::Property),
        ("\"privacy\":", "\"nope\":", VocabKind::ImpactCategory),
        ("\"moderate\"", "\"nope\"", VocabKind::Level),
        (
            "\"information-disclosure\"",
            "\"nope\"",
            VocabKind::ThreatCategory,
        ),
        ("\"r155-annex5\"", "\"nope\"", VocabKind::CatalogScheme),
    ];
    for (from, to, kind) in cases {
        assert!(log.contains(from) && snapshot.contains(from), "{from}");
        let (_, err) = read_until_error(&m, &log.replacen(from, to, 1));
        assert!(
            matches!(&err, Some(StoreError::UnknownKey { kind: k, key }) if *k == kind && &**key == "nope"),
            "log {kind:?}: {err:?}"
        );
        let err = load(snapshot.replacen(from, to, 1).as_bytes(), &m).err();
        assert!(
            matches!(&err, Some(StoreError::UnknownKey { kind: k, .. }) if *k == kind),
            "snapshot {kind:?}: {err:?}"
        );
    }
    // `SetImpact` with an unknown level.
    let set_impact = log
        .lines()
        .find(|l| l.contains("\"set_impact\"") && l.contains("\"negligible\""))
        .expect("set_impact line");
    let header = log.lines().next().expect("header");
    let bad = format!(
        "{header}\n{}\n",
        set_impact
            .replacen("\"negligible\"", "\"huge\"", 1)
            .replacen(
                &set_impact[..set_impact.find(',').expect("comma")],
                "{\"seq\":1",
                1
            )
    );
    let (_, err) = read_until_error(&m, &bad);
    assert!(
        matches!(
            err,
            Some(StoreError::UnknownKey {
                kind: VocabKind::Level,
                ..
            })
        ),
        "{err:?}"
    );

    // IDs: another UUID version, malformed text, uppercase.
    let id = asset_id(1).to_string();
    for bad_id in [
        "67e55044-10b1-426f-9247-bb680e5fe0c8".to_string(),
        "not-a-uuid".to_string(),
        id.to_uppercase(),
    ] {
        let (_, err) = read_until_error(&m, &log.replacen(&id, &bad_id, 1));
        assert!(
            matches!(err, Some(StoreError::InvalidId)),
            "{bad_id}: {err:?}"
        );
        let err = load(snapshot.replacen(&id, &bad_id, 1).as_bytes(), &m).err();
        assert!(
            matches!(err, Some(StoreError::InvalidId)),
            "{bad_id}: {err:?}"
        );
    }

    // A catalog ref text outside the scheme's syntax.
    let (_, err) = read_until_error(&m, &log.replacen("\"id\":\"4.1\"", "\"id\":\"4.0\"", 1));
    assert!(
        matches!(&err, Some(StoreError::InvalidCatalogRef { scheme }) if &**scheme == "r155-annex5"),
        "{err:?}"
    );
    let err = load(
        snapshot
            .replacen("\"id\":\"4.1\"", "\"id\":\"33\"", 1)
            .as_bytes(),
        &m,
    )
    .err();
    assert!(
        matches!(err, Some(StoreError::InvalidCatalogRef { .. })),
        "{err:?}"
    );

    // A repeated key in a property set and in an impact map.
    let set = "[\"availability\",\"integrity\"]";
    assert!(log.contains(set));
    let (_, err) = read_until_error(
        &m,
        &log.replacen(set, "[\"availability\",\"integrity\",\"availability\"]", 1),
    );
    assert!(matches!(err, Some(StoreError::DuplicateEntry)), "{err:?}");
    let err = load(
        snapshot
            .replacen(
                "[\"authenticity\",\"availability\"",
                "[\"authenticity\",\"authenticity\"",
                1,
            )
            .as_bytes(),
        &m,
    )
    .err();
    assert!(matches!(err, Some(StoreError::DuplicateEntry)), "{err:?}");
    let (_, err) = read_until_error(
        &m,
        &log.replacen(
            "{\"safety\":\"severe\"",
            "{\"safety\":\"severe\",\"safety\":\"major\"",
            1,
        ),
    );
    assert!(matches!(err, Some(StoreError::DuplicateEntry)), "{err:?}");
    let err = load(
        snapshot
            .replacen(
                "{\"safety\":\"severe\"}",
                "{\"safety\":\"severe\",\"safety\":\"major\"}",
                1,
            )
            .as_bytes(),
        &m,
    )
    .err();
    assert!(matches!(err, Some(StoreError::DuplicateEntry)), "{err:?}");

    // A snapshot threat that names a missing asset.
    let ecu = asset_id(1).to_string();
    let missing = asset_id(99).to_string();
    let i = snapshot.find("\"threat_scenarios\"").expect("threats");
    let (head, tail) = snapshot.split_at(i);
    let bad = format!("{head}{}", tail.replacen(&ecu, &missing, 1));
    let err = load(bad.as_bytes(), &m).err();
    assert!(
        matches!(err, Some(StoreError::Model(ModelError::UnknownAsset(a))) if a == asset_id(99)),
        "{err:?}"
    );
}

// ---------------------------------------------------------------------------
// AC14: errors never contain names.
// ---------------------------------------------------------------------------

#[test]
fn errors_have_no_labels() {
    let m = builtin();
    let header = log_text(&m, &[]);
    let id = asset_id(1).to_string();
    let rec = |body: String| format!("{header}{body}\n");
    let long = SENTINEL.repeat(70);
    let logs = [
        // A name, failing at the length cap.
        rec(format!(
            r#"{{"seq":1,"op":{{"op":"rename_asset","id":"{id}","name":"{long}"}}}}"#
        )),
        // A name, failing at a later key lookup.
        rec(format!(
            r#"{{"seq":1,"op":{{"op":"add_asset","id":"{id}","name":"{SENTINEL}","properties":["nope"]}}}}"#
        )),
        // A name, failing at the JSON syntax right after it.
        rec(format!(
            r#"{{"seq":1,"op":{{"op":"rename_asset","id":"{id}","name":"{SENTINEL}" x}}}}"#
        )),
        // A name, failing on an unknown variant tag.
        rec(format!(
            r#"{{"seq":1,"op":{{"op":"{SENTINEL}","id":"{id}"}}}}"#
        )),
        // An unknown field named by the sentinel.
        rec(format!(
            r#"{{"seq":1,"op":{{"op":"remove_asset","id":"{id}","{SENTINEL}":1}}}}"#
        )),
        // An ID position.
        rec(format!(
            r#"{{"seq":1,"op":{{"op":"remove_asset","id":"{SENTINEL}"}}}}"#
        )),
        // A catalog ref text.
        rec(format!(
            r#"{{"seq":1,"op":{{"op":"add_catalog_ref","threat":"{id}","catalog_ref":{{"scheme":"r155-annex5","id":"{SENTINEL}"}}}}}}"#
        )),
        // A string where `seq` is expected.
        rec(format!(
            r#"{{"seq":"{SENTINEL}","op":{{"op":"remove_asset","id":"{id}"}}}}"#
        )),
        // The format string.
        format!("{}\n", header.trim_end().replace("aegis-oplog", SENTINEL)),
    ];
    let mut errors = Vec::new();
    for text in &logs {
        let (_, err) = read_until_error(&m, text);
        errors.push(err.expect("must fail"));
    }
    // The snapshot: a name in an entity whose threat names a missing asset.
    let mut p = Project::new(Arc::clone(&m));
    p.apply(Op::AddAsset {
        id: asset_id(1),
        name: SENTINEL.into(),
        properties: m.all_properties(),
    })
    .expect("valid");
    let snapshot = String::from_utf8(save(&p)).expect("utf8");
    let bad = snapshot.replace(
        "\"threat_scenarios\":[]",
        &format!(
            r#""threat_scenarios":[{{"id":"{}","name":"{SENTINEL}","asset":"{}","category":"tampering","catalog_refs":[],"damages":[]}}]"#,
            threat_id(2),
            asset_id(3)
        ),
    );
    errors.push(load(bad.as_bytes(), &m).expect_err("missing asset"));
    errors.push(load(snapshot.replace(&id, SENTINEL).as_bytes(), &m).expect_err("bad id"));

    let mut kinds = std::collections::BTreeSet::new();
    for err in &errors {
        let display = err.to_string();
        let debug = format!("{err:?}");
        assert!(!display.contains(SENTINEL), "{display}");
        assert!(!debug.contains(SENTINEL), "{debug}");
        assert!(!display.is_empty());
        kinds.insert(common::variant(err));
    }
    // The inputs fail at different stages.
    for kind in [
        "LimitExceeded",
        "UnknownKey",
        "Decode",
        "InvalidId",
        "InvalidCatalogRef",
        "UnsupportedFormat",
        "Model",
    ] {
        assert!(kinds.contains(kind), "{kind} not covered: {kinds:?}");
    }
}

#[test]
fn every_vocabulary_kind_reads_back_under_its_key() {
    // Property keys as stored match the op's own keys.
    let m = builtin();
    let p = apply_all(&m, &golden_ops(&m));
    let loaded = load(&save(&p), &m).expect("loads");
    assert_eq!(describe(&loaded), describe(&p));
    let integrity = prop(&m, "integrity");
    let ecu = loaded.asset_idx(&asset_id(1)).and_then(|i| loaded.asset(i));
    assert!(ecu.is_some_and(|a| a.properties().contains(integrity)));
}

#[test]
fn save_and_append_refuse_attack_tree_ops_with_unencodable_op() {
    let m = builtin();
    let mut p = apply_all(&m, &golden_ops(&m));
    let tid = common::tree_id(50);
    let step = common::node_id(51);
    let tree = aegis_model::AttackTree::new(step, aegis_model::Node::step("Step")).expect("step");
    p.apply(Op::AddAttackTree {
        id: tid,
        tree: tree.clone(),
        step_feasibility: vec![],
    })
    .expect("add tree");
    let mut out = Vec::new();
    assert!(matches!(p.save(&mut out), Err(StoreError::UnencodableOp)));
    assert!(out.is_empty());

    let header = log_text(&m, &[]);
    for op in [
        Op::AddAttackTree {
            id: tid,
            tree,
            step_feasibility: vec![],
        },
        Op::SetStepFeasibility {
            tree: tid,
            node: step,
            level: None,
        },
        Op::SetThreatAttackTree {
            threat: threat_id(1),
            tree: Some(tid),
        },
        Op::RemoveAttackTree { id: tid },
    ] {
        let mut writer =
            aegis_model::OpLogWriter::create(Vec::new(), Arc::clone(&m)).expect("header");
        assert!(matches!(writer.append(&op), Err(StoreError::UnencodableOp)));
        let bytes = writer.into_inner().expect("flush");
        assert_eq!(bytes, header.as_bytes());
    }
}
