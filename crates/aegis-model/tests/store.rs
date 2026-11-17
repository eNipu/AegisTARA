//! Save and load through the public API: determinism, a reordered
//! methodology, keys only on disk, the golden v1 files, op log sequencing,
//! framing and header errors, and errors that never contain names.

mod common;

use std::sync::Arc;

use aegis_model::{
    AttackTree, GateKind, Methodology, ModelError, Node, Op, OpLogReader, OpLogWriter, PartyKind,
    Project, Provenance, ProvenanceSource, RiskTreatment, StoreError, VocabKind,
};
use common::{
    SENTINEL, asset_id, builtin, cat, claim_id, control_id, cref, damage_id, describe, factor,
    factor_lvl, feas, goal_id, lvl, node_id, party_id, prop, props, read_log, reordered, save,
    scheme, second, second_def, threat_cat, threat_id, tree_id, wide, write_log,
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
    let provenance = Provenance::default();
    vec![
        Op::AddAsset {
            id: ecu,
            name: "Brake ECU".into(),
            properties: props(m, &["integrity", "availability"]),
            provenance: provenance.clone(),
        },
        Op::AddAsset {
            id: tcu,
            name: "Telematics unit".into(),
            properties: m.all_properties(),
            provenance: provenance.clone(),
        },
        Op::RenameAsset {
            id: tcu,
            name: "Telematics control unit".into(),
            provenance: provenance.clone(),
        },
        Op::SetAssetProperties {
            id: ecu,
            properties: props(m, &["availability", "authenticity", "integrity"]),
            provenance: provenance.clone(),
        },
        Op::AddAsset {
            id: spare,
            name: "Spare".into(),
            properties: props(m, &["confidentiality"]),
            provenance: provenance.clone(),
        },
        Op::RemoveAsset { id: spare },
        Op::AddDamageScenario {
            id: crash,
            name: "Vehicle cannot brake".into(),
            impacts: vec![
                (safety, lvl(m, safety, "severe")),
                (operational, lvl(m, operational, "major")),
            ],
            provenance: provenance.clone(),
        },
        Op::AddDamageScenario {
            id: leak,
            name: "Owner data leaks".into(),
            impacts: vec![(privacy, lvl(m, privacy, "moderate"))],
            provenance: provenance.clone(),
        },
        Op::RenameDamageScenario {
            id: leak,
            name: "Owner data is disclosed".into(),
            provenance: provenance.clone(),
        },
        Op::SetImpact {
            id: leak,
            category: financial,
            level: Some(lvl(m, financial, "negligible")),
            provenance: provenance.clone(),
        },
        Op::SetImpact {
            id: crash,
            category: operational,
            level: None,
            provenance: provenance.clone(),
        },
        Op::AddDamageScenario {
            id: temp,
            name: "Temporary".into(),
            impacts: vec![],
            provenance: provenance.clone(),
        },
        Op::RemoveDamageScenario { id: temp },
        Op::AddThreatScenario {
            id: firmware,
            name: "Modify brake firmware \"v2\" \u{2013} \u{dc} \\ tab\t".into(),
            asset: ecu,
            category: threat_cat(m, "tampering"),
            catalog_refs: vec![cref(m, annex5, "5"), cref(m, annex5, "4.1")],
            damages: vec![crash],
            provenance: provenance.clone(),
        },
        Op::AddThreatScenario {
            id: records,
            name: "Extract owner records".into(),
            asset: tcu,
            category: threat_cat(m, "information-disclosure"),
            catalog_refs: vec![],
            damages: vec![leak],
            provenance: provenance.clone(),
        },
        Op::RenameThreatScenario {
            id: records,
            name: "Read owner records over the air".into(),
            provenance: provenance.clone(),
        },
        Op::SetThreatCategory {
            id: firmware,
            category: threat_cat(m, "denial-of-service"),
            provenance: provenance.clone(),
        },
        Op::AddCatalogRef {
            threat: records,
            catalog_ref: cref(m, annex5, "7.2"),
            provenance: provenance.clone(),
        },
        Op::RemoveCatalogRef {
            threat: firmware,
            catalog_ref: cref(m, annex5, "5"),
            provenance: provenance.clone(),
        },
        Op::LinkDamage {
            threat: firmware,
            damage: leak,
            provenance: provenance.clone(),
        },
        Op::UnlinkDamage {
            threat: firmware,
            damage: leak,
            provenance: provenance.clone(),
        },
        Op::AddThreatScenario {
            id: fob,
            name: "Spoof key fob".into(),
            asset: tcu,
            category: threat_cat(m, "spoofing"),
            catalog_refs: vec![],
            damages: vec![],
            provenance: provenance.clone(),
        },
        Op::RemoveThreatScenario { id: fob },
        Op::LinkDamage {
            threat: records,
            damage: crash,
            provenance,
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

#[test]
fn v1_fixture_without_parties_loads_and_saves_identical_bytes() {
    let m = builtin();
    let bytes = std::fs::read(PROJECT_V1).expect("fixture");
    let p = load(&bytes, &m).expect("loads");
    let saved = save(&p);
    assert_eq!(saved, bytes);
    let text = String::from_utf8(saved).expect("utf8");
    assert!(!text.contains("\"parties\""), "no parties key written");
    assert!(
        !text.contains("\"responsible\""),
        "no responsible key written"
    );
}

#[test]
fn parties_and_responsible_round_trip_byte_for_byte() {
    let m = builtin();
    let mut p = Project::new(Arc::clone(&m));
    // More than 100 parties, with a zero-padded visual ID gap (party 0 is
    // removed) so position-based visual IDs on read-back cannot match.
    let mut ids = Vec::new();
    for i in 0..101u64 {
        let id = party_id(1000 + i);
        p.apply(Op::AddParty {
            id,
            name: format!("party {i}").into(),
            kind: match i % 4 {
                0 => PartyKind::Us,
                1 => PartyKind::Customer,
                2 => PartyKind::Supplier,
                _ => PartyKind::Other,
            },
            provenance: Provenance::default(),
        })
        .expect("add party");
        ids.push(id);
    }
    p.apply(Op::RemoveParty { id: ids[0] })
        .expect("remove party");
    let goal = goal_id(1);
    p.apply(Op::AddCybersecurityGoal {
        id: goal,
        name: "goal".into(),
        controls: Vec::new(),
        provenance: Provenance::default(),
    })
    .expect("add goal");
    let claim = claim_id(2);
    p.apply(Op::AddCybersecurityClaim {
        id: claim,
        name: "claim".into(),
        provenance: Provenance::default(),
    })
    .expect("add claim");
    for &party in &ids[1..6] {
        p.apply(Op::LinkGoalParty {
            goal,
            party,
            provenance: Provenance::default(),
        })
        .expect("link goal party");
    }
    for &party in &ids[6..11] {
        p.apply(Op::LinkClaimParty {
            claim,
            party,
            provenance: Provenance::default(),
        })
        .expect("link claim party");
    }
    let saved = save(&p);
    let back = load(&saved, &m).expect("loads");
    assert_eq!(save(&back), saved);
    assert_eq!(back.parties().count(), 100);
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
        provenance: Provenance::default(),
    };
    let add_damage = |id, impacts| Op::AddDamageScenario {
        id,
        name: "d".into(),
        impacts,
        provenance: Provenance::default(),
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
        provenance: Provenance::default(),
    };

    // One order, with a removed asset, a removed damage scenario, a removed
    // attack tree, a live attack tree with step feasibility on add, and
    // non-default provenance.
    let (tr1, tr2) = (tree_id(10), tree_id(11));
    let (root, s1, s2) = (node_id(12), node_id(13), node_id(14));
    let build_tree = || {
        let mut tree = AttackTree::new(root, Node::gate(GateKind::Or, "Goal")).expect("root");
        tree.add_child(tree.root(), s1, Node::step("Step 1"))
            .expect("s1");
        tree.add_child(tree.root(), s2, Node::step("Step 2"))
            .expect("s2");
        tree
    };
    let p_tree = Provenance::ai("tara-assist", 777);
    let p_link = Provenance::human("alice", 888);
    let first = apply_all(
        &m,
        &[
            add_asset(a1, "a"),
            add_asset(a2, "b"),
            add_asset(a3, "gone"),
            add_damage(d1, rated.clone()),
            add_damage(d2, vec![]),
            add_damage(damage_id(9), vec![]),
            Op::RemoveAsset { id: a3 },
            Op::RemoveDamageScenario { id: damage_id(9) },
            threat(vec!["5", "4.1", "4"], vec![d2, d1]),
            Op::AddAttackTree {
                id: tr1,
                tree: build_tree(),
                step_feasibility: vec![(s2, feas(&m, "high")), (s1, feas(&m, "low"))],
                provenance: p_tree.clone(),
            },
            Op::AddAttackTree {
                id: tr2,
                tree: AttackTree::new(node_id(99), Node::step("temp")).expect("temp"),
                step_feasibility: vec![],
                provenance: Provenance::default(),
            },
            Op::RemoveAttackTree { id: tr2 },
            Op::SetStepFactor {
                tree: tr1,
                node: s1,
                factor: factor(&m, "knowledge"),
                level: Some(factor_lvl(&m, factor(&m, "knowledge"), "public")),
                provenance: p_tree.clone(),
            },
            Op::SetStepFactor {
                tree: tr1,
                node: s1,
                factor: factor(&m, "elapsed-time"),
                level: Some(factor_lvl(&m, factor(&m, "elapsed-time"), "le-1w")),
                provenance: p_tree.clone(),
            },
            Op::SetThreatAttackTree {
                threat: t1,
                tree: Some(tr1),
                provenance: p_link.clone(),
            },
        ],
    );
    // Another order, ratings set one by one, refs and links added later.
    let mut reversed = rated.clone();
    reversed.reverse();
    let second = apply_all(
        &m,
        &[
            Op::AddAttackTree {
                id: tr1,
                tree: build_tree(),
                step_feasibility: vec![],
                provenance: Provenance::default(),
            },
            Op::SetStepFeasibility {
                tree: tr1,
                node: s1,
                level: Some(feas(&m, "low")),
                provenance: Provenance::default(),
            },
            Op::SetStepFactor {
                tree: tr1,
                node: s1,
                factor: factor(&m, "elapsed-time"),
                level: Some(factor_lvl(&m, factor(&m, "elapsed-time"), "le-1w")),
                provenance: Provenance::default(),
            },
            Op::SetStepFactor {
                tree: tr1,
                node: s1,
                factor: factor(&m, "knowledge"),
                level: Some(factor_lvl(&m, factor(&m, "knowledge"), "public")),
                provenance: Provenance::default(),
            },
            Op::SetStepFeasibility {
                tree: tr1,
                node: s2,
                level: Some(feas(&m, "high")),
                provenance: p_tree,
            },
            add_damage(d1, vec![]),
            Op::SetImpact {
                id: d1,
                category: reversed[0].0,
                level: Some(reversed[0].1),
                provenance: Provenance::default(),
            },
            Op::SetImpact {
                id: d1,
                category: reversed[1].0,
                level: Some(reversed[1].1),
                provenance: Provenance::default(),
            },
            add_asset(a1, "a"),
            add_damage(d2, vec![]),
            add_asset(a2, "b"),
            threat(vec!["4"], vec![d1]),
            Op::AddCatalogRef {
                threat: t1,
                catalog_ref: cref(&m, annex5, "4.1"),
                provenance: Provenance::default(),
            },
            Op::AddCatalogRef {
                threat: t1,
                catalog_ref: cref(&m, annex5, "5"),
                provenance: Provenance::default(),
            },
            Op::LinkDamage {
                threat: t1,
                damage: d2,
                provenance: Provenance::default(),
            },
            Op::SetThreatAttackTree {
                threat: t1,
                tree: Some(tr1),
                provenance: p_link,
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

/// A project over the second methodology that uses every vocabulary kind,
/// attack trees (including a shared DAG node), and all provenance sources.
fn second_ops(m: &Methodology) -> Vec<Op> {
    let capec = scheme(m, "capec");
    let harm = cat(m, "patient-harm");
    let exposure = cat(m, "data-exposure");
    let loss = cat(m, "service-loss");
    let (root, g1, s1, s2) = (node_id(10), node_id(11), node_id(12), node_id(13));
    let mut tree =
        AttackTree::new(root, Node::gate(GateKind::Or, "Compromise pump")).expect("root");
    let g1_idx = tree
        .add_child(
            tree.root(),
            g1,
            Node::gate(GateKind::SeqAnd, "Chained access"),
        )
        .expect("g1");
    let s1_idx = tree
        .add_child(tree.root(), s1, Node::step("USB debug port"))
        .expect("s1");
    tree.link(g1_idx, s1_idx).expect("link s1");
    tree.add_child(g1_idx, s2, Node::step("Bypass signature check"))
        .expect("s2");
    vec![
        Op::AddAsset {
            id: asset_id(1),
            name: "Infusion pump".into(),
            properties: props(m, &["i", "a"]),
            provenance: Provenance::human("alice", 100),
        },
        Op::AddAsset {
            id: asset_id(2),
            name: "Monitor".into(),
            properties: props(m, &["c", "a"]),
            provenance: Provenance::import("reqif", 200),
        },
        Op::AddDamageScenario {
            id: damage_id(3),
            name: "Overdose".into(),
            impacts: vec![
                (loss, lvl(m, loss, "intolerable")),
                (harm, lvl(m, harm, "critical")),
            ],
            provenance: Provenance::default(),
        },
        Op::AddDamageScenario {
            id: damage_id(4),
            name: "Record leak".into(),
            impacts: vec![(exposure, lvl(m, exposure, "high"))],
            provenance: Provenance::catalog("capec", 300),
        },
        Op::SetImpact {
            id: damage_id(4),
            category: harm,
            level: Some(lvl(m, harm, "minor")),
            provenance: Provenance::ai("tara-assist", 400),
        },
        Op::AddThreatScenario {
            id: threat_id(5),
            name: "Lock the pump".into(),
            asset: asset_id(1),
            category: threat_cat(m, "ransomware"),
            catalog_refs: vec![cref(m, capec, "CAPEC-77"), cref(m, capec, "CAPEC-5")],
            damages: vec![damage_id(4), damage_id(3)],
            provenance: Provenance::human("bob", 500),
        },
        Op::AddThreatScenario {
            id: threat_id(6),
            name: "Sniff monitor traffic".into(),
            asset: asset_id(2),
            category: threat_cat(m, "data-leak"),
            catalog_refs: vec![],
            damages: vec![damage_id(4)],
            provenance: Provenance::default(),
        },
        Op::SetAssetProperties {
            id: asset_id(2),
            properties: m.all_properties(),
            provenance: Provenance::default(),
        },
        Op::SetThreatCategory {
            id: threat_id(6),
            category: threat_cat(m, "outage"),
            provenance: Provenance::default(),
        },
        Op::AddCatalogRef {
            threat: threat_id(6),
            catalog_ref: cref(m, capec, "CAPEC-1"),
            provenance: Provenance::catalog("capec", 600),
        },
        Op::AddAttackTree {
            id: tree_id(7),
            tree,
            step_feasibility: vec![(s2, feas(m, "remote")), (s1, feas(m, "occasional"))],
            provenance: Provenance::human("alice", 700),
        },
        Op::SetStepFeasibility {
            tree: tree_id(7),
            node: s1,
            level: Some(feas(m, "probable")),
            provenance: Provenance::ai("tara-assist", 800),
        },
        Op::SetStepFactor {
            tree: tree_id(7),
            node: s1,
            factor: factor(m, "exposure"),
            level: Some(factor_lvl(m, factor(m, "exposure"), "local")),
            provenance: Provenance::human("alice", 810),
        },
        Op::SetStepFactor {
            tree: tree_id(7),
            node: s1,
            factor: factor(m, "skill"),
            level: Some(factor_lvl(m, factor(m, "skill"), "expert")),
            provenance: Provenance::human("alice", 820),
        },
        Op::SetStepFactor {
            tree: tree_id(7),
            node: s1,
            factor: factor(m, "resources"),
            level: Some(factor_lvl(m, factor(m, "resources"), "standard")),
            provenance: Provenance::human("alice", 830),
        },
        Op::SetThreatAttackTree {
            threat: threat_id(5),
            tree: Some(tree_id(7)),
            provenance: Provenance::human("alice", 900),
        },
        Op::SetThreatFeasibility {
            threat: threat_id(6),
            level: Some(feas(m, "improbable")),
            provenance: Provenance::human("bob", 950),
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
        m.feasibility_level("probable"),
        r.feasibility_level("probable")
    );
    assert_ne!(
        m.threat_category("ransomware"),
        r.threat_category("ransomware")
    );
    assert_ne!(m.catalog_scheme("capec"), r.catalog_scheme("capec"));
    assert_ne!(
        m.feasibility_factor("exposure"),
        r.feasibility_factor("exposure")
    );
    assert_ne!(
        m.factor_level(factor(&m, "skill"), "expert"),
        r.factor_level(factor(&r, "skill"), "expert")
    );

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
            keys.iter()
                .all(|k| k == "version" || k == "seq" || k == "timestamp_ms"),
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
        provenance: Provenance::default(),
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
    let mut big_tree = AttackTree::new(node_id(1), Node::gate(GateKind::Or, "root")).expect("root");
    for i in 2..=33 {
        big_tree
            .add_child(big_tree.root(), node_id(i), Node::step("s"))
            .expect("step");
    }
    let foreign = [
        Op::SetAssetProperties {
            id: asset_id(1),
            properties: w.all_properties(),
            provenance: Provenance::default(),
        },
        Op::SetThreatCategory {
            id: threat_id(1),
            category: w.threat_categories().last().expect("category"),
            provenance: Provenance::default(),
        },
        Op::SetImpact {
            id: damage_id(1),
            category: w.impact_categories().last().expect("category"),
            level: None,
            provenance: Provenance::default(),
        },
        Op::SetThreatFeasibility {
            threat: threat_id(1),
            level: Some(w.feasibility_levels().last().expect("feasibility").0),
            provenance: Provenance::default(),
        },
        Op::SetStepMitigation {
            tree: tree_id(1),
            node: node_id(1),
            control: control_id(1),
            feasibility: Some(w.feasibility_levels().last().expect("feasibility").0),
            provenance: Provenance::default(),
        },
        Op::AddSecurityControl {
            id: control_id(1),
            name: "c".repeat(1025).into(),
            provenance: Provenance::default(),
        },
        Op::AddCatalogRef {
            threat: threat_id(1),
            catalog_ref: cref(&w, scheme(&w, "w2"), "1.2.3"),
            provenance: Provenance::default(),
        },
        // A repeated impact category could not be read back either.
        Op::AddDamageScenario {
            id: damage_id(2),
            name: "d".into(),
            impacts: vec![
                (cat(&m, "safety"), lvl(&m, cat(&m, "safety"), "major")),
                (cat(&m, "safety"), lvl(&m, cat(&m, "safety"), "severe")),
            ],
            provenance: Provenance::default(),
        },
        // A repeated step feasibility node could not be read back either.
        Op::AddAttackTree {
            id: tree_id(2),
            tree: AttackTree::new(node_id(1), Node::step("s")).expect("step"),
            step_feasibility: vec![
                (node_id(1), feas(&m, "low")),
                (node_id(1), feas(&m, "high")),
            ],
            provenance: Provenance::default(),
        },
        // An attack tree with more than MAX_ATTACK_TREE_NODES nodes.
        Op::AddAttackTree {
            id: tree_id(3),
            tree: big_tree,
            step_feasibility: vec![],
            provenance: Provenance::default(),
        },
        // A provenance actor outside the 1,024-byte cap.
        Op::RenameAsset {
            id: asset_id(1),
            name: "ok".into(),
            provenance: Provenance::new("a".repeat(1025), 0, ProvenanceSource::Human),
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

    // FeasibilityLevel vocabulary kind and duplicate step_feasibility entries.
    let s2 = second();
    let s2_snap = String::from_utf8(save(&apply_all(&s2, &second_ops(&s2)))).expect("utf8");
    let s2_log = log_text(&s2, &second_ops(&s2));
    assert!(s2_snap.contains("\"remote\"") && s2_log.contains("\"remote\""));
    let (_, err) = read_until_error(&s2, &s2_log.replacen("\"remote\"", "\"nope\"", 1));
    assert!(
        matches!(&err, Some(StoreError::UnknownKey { kind: VocabKind::FeasibilityLevel, key }) if &**key == "nope"),
        "{err:?}"
    );
    let err = load(
        s2_snap.replacen("\"remote\"", "\"nope\"", 1).as_bytes(),
        &s2,
    )
    .err();
    assert!(
        matches!(
            err,
            Some(StoreError::UnknownKey {
                kind: VocabKind::FeasibilityLevel,
                ..
            })
        ),
        "{err:?}"
    );
    let n12 = node_id(12).to_string();
    let n13 = node_id(13).to_string();
    let dup_log = s2_log.replacen(
        &format!("\"{n13}\":\"remote\",\"{n12}\":\"occasional\""),
        &format!("\"{n13}\":\"remote\",\"{n13}\":\"occasional\""),
        1,
    );
    let (_, err) = read_until_error(&s2, &dup_log);
    assert!(matches!(err, Some(StoreError::DuplicateEntry)), "{err:?}");
    let dup_snap = s2_snap.replacen(
        &format!("\"{n12}\":\"probable\",\"{n13}\":\"remote\""),
        &format!("\"{n12}\":\"probable\",\"{n12}\":\"remote\""),
        1,
    );
    let err = load(dup_snap.as_bytes(), &s2).err();
    assert!(matches!(err, Some(StoreError::DuplicateEntry)), "{err:?}");
}

// ---------------------------------------------------------------------------
// AC14: errors never contain names.
// ---------------------------------------------------------------------------

#[test]
fn errors_have_no_labels() {
    let m = builtin();
    let header = log_text(&m, &[]);
    let id = asset_id(1).to_string();
    let tid = tree_id(2).to_string();
    let nid = node_id(3).to_string();
    let rec = |body: String| format!("{header}{body}\n");
    let long = SENTINEL.repeat(70);
    let logs = [
        // A name, failing at the length cap.
        rec(format!(
            r#"{{"seq":1,"op":{{"op":"rename_asset","id":"{id}","name":"{long}"}}}}"#
        )),
        // A provenance actor, failing at the actor length cap.
        rec(format!(
            r#"{{"seq":1,"op":{{"op":"rename_asset","id":"{id}","name":"ok","provenance":{{"actor":"{long}","timestamp_ms":0,"source":"human"}}}}}}"#
        )),
        // An attack tree node label, failing at a later feasibility key lookup.
        rec(format!(
            r#"{{"seq":1,"op":{{"op":"add_attack_tree","id":"{tid}","nodes":[{{"id":"{nid}","label":"{SENTINEL}"}}],"step_feasibility":{{"{nid}":"nope"}},"provenance":{{"actor":"{SENTINEL}","timestamp_ms":1,"source":"ai"}}}}}}"#
        )),
        // An attack tree node label, failing on a malformed DAG (self-cycle).
        rec(format!(
            r#"{{"seq":1,"op":{{"op":"add_attack_tree","id":"{tid}","nodes":[{{"id":"{nid}","label":"{SENTINEL}","gate":"or","children":["{nid}"]}}],"step_feasibility":{{}}}}}}"#
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
        provenance: Provenance::human(SENTINEL, 1),
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
fn attack_trees_and_provenance_round_trip_and_evaluate() {
    let m = builtin();
    let mut ops = golden_ops(&m);
    let (tr1, tr2) = (tree_id(50), tree_id(51));
    let (root, g_and, g_seq, s1, s2, s3) = (
        node_id(52),
        node_id(53),
        node_id(54),
        node_id(55),
        node_id(56),
        node_id(57),
    );
    let mut tree =
        AttackTree::new(root, Node::gate(GateKind::Or, "Compromise brake ECU")).expect("root");
    let g_and_idx = tree
        .add_child(tree.root(), g_and, Node::gate(GateKind::And, "OBD path"))
        .expect("g_and");
    let g_seq_idx = tree
        .add_child(
            tree.root(),
            g_seq,
            Node::gate(GateKind::SeqAnd, "OTA chain"),
        )
        .expect("g_seq");
    let s1_idx = tree
        .add_child(g_and_idx, s1, Node::step("Physical OBD access"))
        .expect("s1");
    tree.add_child(g_and_idx, s2, Node::step("Exploit UDS session"))
        .expect("s2");
    tree.link(g_seq_idx, s1_idx).expect("shared s1");
    tree.add_child(g_seq_idx, s3, Node::step("Bypass signature check"))
        .expect("s3");

    ops.extend([
        Op::RenameAsset {
            id: asset_id(1),
            name: "Brake ECU v2".into(),
            provenance: Provenance::human("alice", 1_700_000_000_000),
        },
        Op::RenameDamageScenario {
            id: damage_id(4),
            name: "Loss of braking".into(),
            provenance: Provenance::import("reqif", 1_700_000_000_100),
        },
        Op::AddAttackTree {
            id: tr2,
            tree: AttackTree::new(node_id(90), Node::step("Temp")).expect("temp"),
            step_feasibility: vec![],
            provenance: Provenance::catalog("r155", 1_700_000_000_200),
        },
        Op::RemoveAttackTree { id: tr2 },
        Op::AddAttackTree {
            id: tr1,
            tree,
            step_feasibility: vec![(s1, feas(&m, "medium")), (s2, feas(&m, "high"))],
            provenance: Provenance::ai("tara-assist", 1_700_000_000_300),
        },
        Op::SetStepFeasibility {
            tree: tr1,
            node: s3,
            level: Some(feas(&m, "low")),
            provenance: Provenance::human("bob", 1_700_000_000_400),
        },
        Op::SetThreatAttackTree {
            threat: threat_id(7),
            tree: Some(tr1),
            provenance: Provenance::human("alice", 1_700_000_000_500),
        },
        Op::SetThreatFeasibility {
            threat: threat_id(8),
            level: Some(feas(&m, "low")),
            provenance: Provenance::human("alice", 1_700_000_000_600),
        },
        Op::SetRiskTreatment {
            threat: threat_id(7),
            treatment: Some(RiskTreatment::Reduce),
            provenance: Provenance::human("alice", 1_700_000_000_700),
        },
        Op::AddSecurityControl {
            id: control_id(60),
            name: "SecOC".into(),
            provenance: Provenance::human("alice", 1_700_000_000_800),
        },
        Op::RenameSecurityControl {
            id: control_id(60),
            name: "SecOC v2".into(),
            provenance: Provenance::human("bob", 1_700_000_000_900),
        },
        Op::SetStepMitigation {
            tree: tr1,
            node: s1,
            control: control_id(60),
            feasibility: Some(feas(&m, "very-low")),
            provenance: Provenance::human("alice", 1_700_000_001_000),
        },
    ]);

    let p = apply_all(&m, &ops);
    let snap = save(&p);
    let snap_text = String::from_utf8(snap.clone()).expect("utf8");
    // Shared node `s1` carries `parents`, while single-parent nodes omit it.
    assert!(snap_text.contains("\"parents\":"), "{snap_text}");
    assert!(snap_text.contains("\"attack_trees\":"), "{snap_text}");
    assert!(snap_text.contains("\"security_controls\":"), "{snap_text}");
    assert!(snap_text.contains("\"step_mitigations\":"), "{snap_text}");
    assert!(
        snap_text.contains("\"treatment\":\"reduce\""),
        "{snap_text}"
    );
    assert!(
        snap_text.contains("\"direct_feasibility\":\"low\""),
        "{snap_text}"
    );
    assert!(snap_text.contains("\"provenance\":"), "{snap_text}");

    let from_snap = load(&snap, &m).expect("loads snapshot");
    assert_eq!(from_snap, p);
    assert_eq!(save(&from_snap), snap);

    let log = write_log(&m, &ops);
    let reread: Vec<Op> = read_log(&m, &log).into_iter().map(|r| r.op).collect();
    assert_eq!(reread, ops);
    let from_log = apply_all(&m, &reread);
    assert_eq!(from_log, p);

    // Threat evaluation produces identical inherent and residual results and explain-why traces.
    let eval_orig = p.evaluate_threat(threat_id(7)).expect("eval orig");
    let eval_snap = from_snap.evaluate_threat(threat_id(7)).expect("eval snap");
    let eval_log = from_log.evaluate_threat(threat_id(7)).expect("eval log");
    assert_eq!(eval_orig.root_value(), &feas(&m, "medium"));
    assert_eq!(eval_orig, eval_snap);
    assert_eq!(eval_orig, eval_log);

    let res_orig = p
        .evaluate_residual_threat(threat_id(7))
        .expect("res eval orig");
    let res_snap = from_snap
        .evaluate_residual_threat(threat_id(7))
        .expect("res eval snap");
    let res_log = from_log
        .evaluate_residual_threat(threat_id(7))
        .expect("res eval log");
    assert_eq!(res_orig.root_value(), &feas(&m, "very-low"));
    assert_eq!(res_orig, res_snap);
    assert_eq!(res_orig, res_log);
}

#[test]
fn rejects_malformed_attack_tree_and_provenance_wire_shapes() {
    let m = builtin();
    let header = log_text(&m, &[]);
    let rec = |body: &str| format!("{header}{body}\n");
    let id = asset_id(1);
    let tid = tree_id(10);
    let (n1, n2, n3) = (node_id(11), node_id(12), node_id(13));

    let bad_logs = [
        // Explicit default provenance must be omitted, not written out.
        rec(&format!(
            r#"{{"seq":1,"op":{{"op":"rename_asset","id":"{id}","name":"x","provenance":{{"actor":"","timestamp_ms":0,"source":"human"}}}}}}"#
        )),
        // `provenance: null` is rejected when present.
        rec(&format!(
            r#"{{"seq":1,"op":{{"op":"rename_asset","id":"{id}","name":"x","provenance":null}}}}"#
        )),
        // Empty `nodes` array.
        rec(&format!(
            r#"{{"seq":1,"op":{{"op":"add_attack_tree","id":"{tid}","nodes":[],"step_feasibility":{{}}}}}}"#
        )),
        // Gate without `children`.
        rec(&format!(
            r#"{{"seq":1,"op":{{"op":"add_attack_tree","id":"{tid}","nodes":[{{"id":"{n1}","label":"g","gate":"or"}}],"step_feasibility":{{}}}}}}"#
        )),
        // Gate with duplicate child in `children`.
        rec(&format!(
            r#"{{"seq":1,"op":{{"op":"add_attack_tree","id":"{tid}","nodes":[{{"id":"{n1}","label":"g","gate":"or","children":["{n2}","{n2}"]}},{{"id":"{n2}","label":"s"}}],"step_feasibility":{{}}}}}}"#
        )),
        // Disconnected node (non-root node with no incoming edges).
        rec(&format!(
            r#"{{"seq":1,"op":{{"op":"add_attack_tree","id":"{tid}","nodes":[{{"id":"{n1}","label":"s1"}},{{"id":"{n2}","label":"s2"}}],"step_feasibility":{{}}}}}}"#
        )),
        // Step with `children`.
        rec(&format!(
            r#"{{"seq":1,"op":{{"op":"add_attack_tree","id":"{tid}","nodes":[{{"id":"{n1}","label":"s","children":["{n2}"]}},{{"id":"{n2}","label":"s2"}}],"step_feasibility":{{}}}}}}"#
        )),
        // Root with `parents`.
        rec(&format!(
            r#"{{"seq":1,"op":{{"op":"add_attack_tree","id":"{tid}","nodes":[{{"id":"{n1}","label":"s","parents":["{n1}"]}}],"step_feasibility":{{}}}}}}"#
        )),
        // Single-parent node with explicit `parents` (must be omitted).
        rec(&format!(
            r#"{{"seq":1,"op":{{"op":"add_attack_tree","id":"{tid}","nodes":[{{"id":"{n1}","label":"g","gate":"or","children":["{n2}"]}},{{"id":"{n2}","label":"s","parents":["{n1}"]}}],"step_feasibility":{{}}}}}}"#
        )),
        // Shared node (2 incoming edges) missing `parents`.
        rec(&format!(
            r#"{{"seq":1,"op":{{"op":"add_attack_tree","id":"{tid}","nodes":[{{"id":"{n1}","label":"r","gate":"or","children":["{n2}","{n3}"]}},{{"id":"{n2}","label":"g","gate":"and","children":["{n3}"]}},{{"id":"{n3}","label":"s"}}],"step_feasibility":{{}}}}}}"#
        )),
        // Shared node with mismatched `parents` set.
        rec(&format!(
            r#"{{"seq":1,"op":{{"op":"add_attack_tree","id":"{tid}","nodes":[{{"id":"{n1}","label":"r","gate":"or","children":["{n2}","{n3}"]}},{{"id":"{n2}","label":"g","gate":"and","children":["{n3}"]}},{{"id":"{n3}","label":"s","parents":["{n2}","{n3}"]}}],"step_feasibility":{{}}}}}}"#
        )),
        // Shared node with only 1 parent in `parents`.
        rec(&format!(
            r#"{{"seq":1,"op":{{"op":"add_attack_tree","id":"{tid}","nodes":[{{"id":"{n1}","label":"r","gate":"or","children":["{n2}","{n3}"]}},{{"id":"{n2}","label":"g","gate":"and","children":["{n3}"]}},{{"id":"{n3}","label":"s","parents":["{n2}"]}}],"step_feasibility":{{}}}}}}"#
        )),
        // Cycle in `nodes`.
        rec(&format!(
            r#"{{"seq":1,"op":{{"op":"add_attack_tree","id":"{tid}","nodes":[{{"id":"{n1}","label":"r","gate":"or","children":["{n2}"]}},{{"id":"{n2}","label":"g","gate":"and","children":["{n1}"]}}],"step_feasibility":{{}}}}}}"#
        )),
        // Duplicate node ID in `nodes`.
        rec(&format!(
            r#"{{"seq":1,"op":{{"op":"add_attack_tree","id":"{tid}","nodes":[{{"id":"{n1}","label":"s1"}},{{"id":"{n1}","label":"s2"}}],"step_feasibility":{{}}}}}}"#
        )),
    ];
    for text in &bad_logs {
        let (_, err) = read_until_error(&m, text);
        assert!(
            matches!(err, Some(StoreError::Decode { line: 2, .. })),
            "{text}: {err:?}"
        );
    }

    // Snapshot with a threat pointing to a missing attack tree fails with UnknownAttackTree.
    let snap = String::from_utf8(save(&apply_all(&m, &golden_ops(&m)))).expect("utf8");
    let bad_snap = snap.replacen(
        "\"damages\":[",
        &format!("\"attack_tree\":\"{tid}\",\"damages\":["),
        1,
    );
    let err = load(bad_snap.as_bytes(), &m).err();
    assert!(
        matches!(err, Some(StoreError::Model(ModelError::UnknownAttackTree(t))) if t == tid),
        "{err:?}"
    );

    // Snapshot with `step_mitigations` referencing an unknown control, a gate node, an unknown
    // node, or an empty inner map fails deterministically.
    let cid = control_id(99);
    let with_tree = |step_mits: &str| {
        snap.replacen(
            "\"threat_scenarios\":[",
            &format!(
                "\"attack_trees\":[{{\"id\":\"{tid}\",\"nodes\":[{{\"id\":\"{n1}\",\"label\":\"g\",\"gate\":\"or\",\"children\":[\"{n2}\"]}},{{\"id\":\"{n2}\",\"label\":\"s\"}}],\"step_feasibility\":{{}}{step_mits}}}],\"threat_scenarios\":["
            ),
            1,
        )
    };
    let unknown_ctrl = with_tree(
        &format!(",\"step_mitigations\":{{\"{n2}\":{{\" {cid}\":\"low\"}}}}").replace(" ", ""),
    );
    assert!(
        matches!(
            load(unknown_ctrl.as_bytes(), &m).err(),
            Some(StoreError::Model(ModelError::UnknownSecurityControl(c))) if c == cid
        ),
        "unknown control should fail"
    );
    let gate_mit = with_tree(&format!(
        ",\"security_controls\":[{{\"id\":\"{cid}\",\"name\":\"C\"}}],\"step_mitigations\":{{\"{n1}\":{{\"{cid}\":\"low\"}}}}"
    ));
    // Place `security_controls` at root level for gate/unknown-node tests.
    let with_ctrl_and_tree = |step_mits: &str| {
        snap.replacen(
            "\"threat_scenarios\":[",
            &format!(
                "\"attack_trees\":[{{\"id\":\"{tid}\",\"nodes\":[{{\"id\":\"{n1}\",\"label\":\"g\",\"gate\":\"or\",\"children\":[\"{n2}\"]}},{{\"id\":\"{n2}\",\"label\":\"s\"}}],\"step_feasibility\":{{}}{step_mits}}}],\"security_controls\":[{{\"id\":\"{cid}\",\"name\":\"C\"}}],\"threat_scenarios\":["
            ),
            1,
        )
    };
    let _ = gate_mit;
    let gate_snap = with_ctrl_and_tree(&format!(
        ",\"step_mitigations\":{{\"{n1}\":{{\"{cid}\":\"low\"}}}}"
    ));
    assert!(
        matches!(
            load(gate_snap.as_bytes(), &m).err(),
            Some(StoreError::Model(ModelError::NotAStep { tree, node })) if tree == tid && node == n1
        ),
        "gate node in step_mitigations should fail"
    );
    let unknown_node_snap = with_ctrl_and_tree(&format!(
        ",\"step_mitigations\":{{\"{n3}\":{{\"{cid}\":\"low\"}}}}"
    ));
    assert!(
        matches!(
            load(unknown_node_snap.as_bytes(), &m).err(),
            Some(StoreError::Model(ModelError::UnknownTreeNode { tree, node })) if tree == tid && node == n3
        ),
        "unknown node in step_mitigations should fail"
    );
    let empty_inner_snap = with_ctrl_and_tree(&format!(",\"step_mitigations\":{{\"{n2}\":{{}}}}"));
    assert!(
        matches!(
            load(empty_inner_snap.as_bytes(), &m).err(),
            Some(StoreError::Decode { .. })
        ),
        "empty inner map in step_mitigations should fail"
    );

    // Snapshot with a threat pointing to a missing goal or claim fails with UnknownCybersecurityGoal / UnknownCybersecurityClaim.
    let gid = common::goal_id(98);
    let clid = common::claim_id(99);
    let bad_goal_snap = snap.replacen(
        "\"damages\":[",
        &format!("\"goal\":\"{gid}\",\"damages\":["),
        1,
    );
    assert!(
        matches!(
            load(bad_goal_snap.as_bytes(), &m).err(),
            Some(StoreError::Model(ModelError::UnknownCybersecurityGoal(g))) if g == gid
        ),
        "unknown goal on threat should fail"
    );
    let bad_claim_snap = snap.replacen(
        "\"damages\":[",
        &format!("\"claim\":\"{clid}\",\"damages\":["),
        1,
    );
    assert!(
        matches!(
            load(bad_claim_snap.as_bytes(), &m).err(),
            Some(StoreError::Model(ModelError::UnknownCybersecurityClaim(cl))) if cl == clid
        ),
        "unknown claim on threat should fail"
    );
}

#[test]
fn visual_ids_round_trip_after_deletion_and_reject_invalid_or_duplicate() {
    let m = builtin();
    let (a1, a2, a3, a4) = (asset_id(1), asset_id(2), asset_id(3), asset_id(4));
    let mut p = apply_all(
        &m,
        &[
            Op::AddAsset {
                id: a3,
                name: "First created".into(),
                properties: m.all_properties(),
                provenance: Provenance::default(),
            },
            Op::AddAsset {
                id: a2,
                name: "Second created".into(),
                properties: m.all_properties(),
                provenance: Provenance::default(),
            },
            Op::AddAsset {
                id: a1,
                name: "Third created".into(),
                properties: m.all_properties(),
                provenance: Provenance::default(),
            },
        ],
    );
    // Creation order visual IDs: a3 -> A00, a2 -> A01, a1 -> A02.
    assert_eq!(
        p.visual_id(aegis_model::ElementRef::Asset(a3)).as_deref(),
        Some("A00")
    );
    assert_eq!(
        p.visual_id(aegis_model::ElementRef::Asset(a2)).as_deref(),
        Some("A01")
    );
    assert_eq!(
        p.visual_id(aegis_model::ElementRef::Asset(a1)).as_deref(),
        Some("A02")
    );

    // Delete a3 (A00); undo restores A00; redo removes it again.
    let undo = p.apply(Op::RemoveAsset { id: a3 }).expect("remove a3");
    p.apply(undo).expect("undo remove a3");
    assert_eq!(
        p.visual_id(aegis_model::ElementRef::Asset(a3)).as_deref(),
        Some("A00")
    );
    p.apply(Op::RemoveAsset { id: a3 })
        .expect("remove a3 again");

    // Save and reload: a2 stays A01, a1 stays A02, and adding a4 gets A03 (never reusing A00).
    let bytes = save(&p);
    let text = String::from_utf8(bytes.clone()).expect("utf8");
    assert!(text.contains(r#""visual_id":"A02""#), "{text}");

    let mut loaded = load(&bytes, &m).expect("loads");
    assert_eq!(save(&loaded), bytes);
    assert_eq!(
        loaded
            .visual_id(aegis_model::ElementRef::Asset(a2))
            .as_deref(),
        Some("A01")
    );
    assert_eq!(
        loaded
            .visual_id(aegis_model::ElementRef::Asset(a1))
            .as_deref(),
        Some("A02")
    );
    loaded
        .apply(Op::AddAsset {
            id: a4,
            name: "Fourth created".into(),
            properties: m.all_properties(),
            provenance: Provenance::default(),
        })
        .expect("add a4");
    assert_eq!(
        loaded
            .visual_id(aegis_model::ElementRef::Asset(a4))
            .as_deref(),
        Some("A03")
    );

    // Duplicate visual_id in a snapshot is rejected with DuplicateEntry.
    let dup = text.replacen(r#""visual_id":"A02""#, r#""visual_id":"A01""#, 1);
    assert!(matches!(
        load(dup.as_bytes(), &m).err(),
        Some(StoreError::DuplicateEntry)
    ));

    // Malformed or wrong-prefix visual_id is rejected with InvalidId without leaking input.
    for bad in [r#""D02""#, r#""A2""#, r#""A002""#, r#""Axx""#] {
        let malformed = text.replacen(r#""visual_id":"A02""#, &format!(r#""visual_id":{bad}"#), 1);
        assert!(
            matches!(
                load(malformed.as_bytes(), &m).err(),
                Some(StoreError::InvalidId)
            ),
            "{bad}"
        );
    }
}
