//! Threat scaling (job 138, FND-10): whether a threat scenario scales to many
//! vehicles, with a reasoning text; the exact inverse and the store round trip.

use std::sync::Arc;

use aegis_model::{
    AssetId, MAX_DESCRIPTION_CHARS, Methodology, ModelError, Op, OpCodec, OpLogReader, OpLogWriter,
    Project, Provenance, ThreatId,
};

fn methodology() -> Arc<Methodology> {
    Arc::new(Methodology::iso21434_r155().expect("builtin"))
}

fn fixture() -> (Project, ThreatId) {
    let m = methodology();
    let mut project = Project::new(Arc::clone(&m));
    let (asset, threat) = (AssetId::new_v7(), ThreatId::new_v7());
    project
        .apply(Op::AddAsset {
            id: asset,
            name: "ECU".into(),
            properties: m.all_properties(),
            provenance: Provenance::default(),
        })
        .expect("asset");
    project
        .apply(Op::AddThreatScenario {
            id: threat,
            name: "Relay attack on keyless entry".into(),
            asset,
            category: m.threat_categories().next().expect("category"),
            catalog_refs: vec![],
            damages: vec![],
            provenance: Provenance::default(),
        })
        .expect("threat");
    (project, threat)
}

fn scaling(threat: ThreatId, scales: Option<bool>, reason: &str) -> Op {
    Op::SetThreatScaling {
        threat,
        scales,
        reason: reason.into(),
        provenance: Provenance::default(),
    }
}

fn read(p: &Project) -> (Option<bool>, String) {
    let (_, _, t) = p.threat_scenarios().next().expect("threat");
    (t.scales(), t.scaling_reason().to_owned())
}

#[test]
fn scaling_is_set_read_and_undone_exactly() {
    let (mut p, threat) = fixture();
    assert_eq!(read(&p), (None, String::new()));
    let before = p.clone();
    let undo = p
        .apply(scaling(
            threat,
            Some(true),
            "One relay kit works on every car",
        ))
        .expect("set");
    assert_eq!(
        read(&p),
        (Some(true), "One relay kit works on every car".into())
    );
    assert_eq!(undo, scaling(threat, None, ""));
    p.apply(undo).expect("undo");
    assert_eq!(p, before);
}

#[test]
fn unknown_threats_and_bad_text_are_rejected() {
    let (mut p, threat) = fixture();
    let other = ThreatId::new_v7();
    assert_eq!(
        p.apply(scaling(other, Some(false), "")),
        Err(ModelError::UnknownThreatScenario(other))
    );
    assert_eq!(
        p.apply(scaling(threat, Some(false), "nul\u{0}")),
        Err(ModelError::InvalidText)
    );
    let long = "x".repeat(MAX_DESCRIPTION_CHARS + 1);
    assert_eq!(
        p.apply(scaling(threat, Some(false), &long)),
        Err(ModelError::DescriptionTooLong {
            max: MAX_DESCRIPTION_CHARS
        })
    );
    assert_eq!(read(&p), (None, String::new()));
}

#[test]
fn snapshot_and_op_log_round_trip_and_stay_absent_when_unrated() {
    let (mut p, threat) = fixture();
    let mut out = Vec::new();
    p.save(&mut out).expect("save");
    let plain = String::from_utf8(out).expect("utf8");
    assert!(!plain.contains("scales") && !plain.contains("scaling_reason"));

    for (scales, reason) in [
        (Some(false), "Needs physical access to each car"),
        (None, "Not rated yet, see workshop notes"),
    ] {
        p.apply(scaling(threat, scales, reason)).expect("set");
        let mut out = Vec::new();
        p.save(&mut out).expect("save");
        let loaded = Project::load(out.as_slice(), methodology()).expect("load");
        assert_eq!(loaded, p);
        assert_eq!(read(&loaded), (scales, reason.to_owned()));
    }

    let mut bytes = Vec::new();
    let mut log = OpLogWriter::create(&mut bytes, methodology()).expect("header");
    let ops = [
        scaling(threat, Some(true), "Remote, over the cellular modem"),
        scaling(threat, None, ""),
    ];
    for op in &ops {
        log.append(op).expect("append");
    }
    drop(log);
    let read_back: Vec<_> = OpLogReader::open(bytes.as_slice(), methodology())
        .expect("open")
        .map(|r| r.expect("record").op)
        .collect();
    assert_eq!(read_back, ops);
    let codec = OpCodec::new(methodology());
    assert_eq!(
        codec.decode(&codec.encode(&ops[0]).expect("e")).expect("d"),
        ops[0]
    );
}
