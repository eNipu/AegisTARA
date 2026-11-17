//! Threat × damage pair rationale (job 146): one text per damage link, set by
//! an op with an exact inverse, compared by public damage ID, and shuffled
//! op order producing equal projects and equal save bytes.

use std::sync::Arc;

use aegis_model::{
    AssetId, DamageId, MAX_DESCRIPTION_CHARS, Methodology, ModelError, Op, OpLogReader,
    OpLogWriter, Project, Provenance, ProvenanceSource, ThreatId,
};

fn methodology() -> Arc<Methodology> {
    Arc::new(Methodology::iso21434_r155().expect("methodology"))
}

fn save(p: &Project) -> Vec<u8> {
    let mut out = Vec::new();
    p.save(&mut out).expect("save");
    out
}

const NAMES: [&str; 2] = ["Brakes fail", "Airbag fails"];

/// A damage with two linked damages and no pair rationale yet.
fn fixture(m: &Arc<Methodology>) -> (Project, ThreatId, [DamageId; 2]) {
    let mut p = Project::new(Arc::clone(m));
    let asset = AssetId::new_v7();
    let threat = ThreatId::new_v7();
    let damages = [DamageId::new_v7(), DamageId::new_v7()];
    p.apply(Op::AddAsset {
        id: asset,
        name: "ECU".into(),
        properties: m.all_properties(),
        provenance: Provenance::default(),
    })
    .expect("asset");
    for (i, id) in damages.iter().enumerate() {
        p.apply(Op::AddDamageScenario {
            id: *id,
            name: NAMES[i].into(),
            impacts: vec![],
            provenance: Provenance::default(),
        })
        .expect("damage");
    }
    p.apply(Op::AddThreatScenario {
        id: threat,
        name: "Spoof CAN".into(),
        asset,
        category: m.threat_categories().next().expect("threat category"),
        catalog_refs: vec![],
        damages: vec![],
        provenance: Provenance::default(),
    })
    .expect("threat");
    (p, threat, damages)
}

fn pair(threat: ThreatId, damage: DamageId, text: &str, provenance: Provenance) -> Op {
    Op::SetDamageLinkRationale {
        threat,
        damage,
        text: text.into(),
        provenance,
    }
}

fn who() -> Provenance {
    Provenance::new("analyst", 1_700_000_000_000, ProvenanceSource::Human)
}

#[test]
fn rationale_is_set_and_undone_exactly() {
    let m = methodology();
    let (mut p, threat, damages) = fixture(&m);
    p.apply(Op::LinkDamage {
        threat,
        damage: damages[0],
        provenance: Provenance::default(),
    })
    .expect("link 0");
    p.apply(Op::LinkDamage {
        threat,
        damage: damages[1],
        provenance: Provenance::default(),
    })
    .expect("link 1");
    let before = p.clone();

    let undo = p
        .apply(pair(threat, damages[0], "Safety relevant", who()))
        .expect("set rationale");
    let idx0 = p.damage_scenario_idx(&damages[0]).expect("damage 0");
    let idx1 = p.damage_scenario_idx(&damages[1]).expect("damage 1");
    let t = p.threat_scenario_idx(&threat).expect("threat");
    let view = p.threat_scenario(t).expect("view");
    assert_eq!(view.damage_rationale(idx0), "Safety relevant");
    assert_eq!(view.damage_rationale(idx1), "");
    let links = view.damage_links();
    assert_eq!(links.len(), 2);
    let mut texts: Vec<_> = links.iter().map(|l| l.rationale.as_ref()).collect();
    texts.sort_unstable();
    assert_eq!(texts, ["", "Safety relevant"]);
    assert_eq!(undo, pair(threat, damages[0], "", Provenance::default()));

    p.apply(undo).expect("undo restores text and provenance");
    assert_eq!(p, before);
}

#[test]
fn unlinked_damage_is_rejected() {
    let m = methodology();
    let (mut p, threat, damages) = fixture(&m);
    p.apply(Op::LinkDamage {
        threat,
        damage: damages[0],
        provenance: Provenance::default(),
    })
    .expect("link 0");
    assert_eq!(
        p.apply(pair(threat, damages[1], "why", who())),
        Err(ModelError::DamageNotLinked {
            threat,
            damage: damages[1],
        })
    );
}

#[test]
fn invalid_text_is_rejected() {
    let m = methodology();
    let (mut p, threat, damages) = fixture(&m);
    p.apply(Op::LinkDamage {
        threat,
        damage: damages[0],
        provenance: Provenance::default(),
    })
    .expect("link 0");
    assert_eq!(
        p.apply(pair(threat, damages[0], "nul\u{0}", who())),
        Err(ModelError::InvalidText)
    );
    let long = "x".repeat(MAX_DESCRIPTION_CHARS + 1);
    assert_eq!(
        p.apply(pair(threat, damages[0], &long, who())),
        Err(ModelError::DescriptionTooLong {
            max: MAX_DESCRIPTION_CHARS
        })
    );
}

#[test]
fn empty_text_clears_the_rationale() {
    let m = methodology();
    let (mut p, threat, damages) = fixture(&m);
    p.apply(Op::LinkDamage {
        threat,
        damage: damages[0],
        provenance: Provenance::default(),
    })
    .expect("link 0");
    let before = p.clone();
    p.apply(pair(threat, damages[0], "why", Provenance::default()))
        .expect("set");
    p.apply(pair(threat, damages[0], "", Provenance::default()))
        .expect("clear");
    assert_eq!(p, before);
}

#[test]
fn shuffled_link_and_rationale_ops_agree() {
    let m = methodology();
    let (base, threat, damages) = fixture(&m);
    let mut p1 = base.clone();
    let mut p2 = base;

    for (p, order) in [(&mut p1, [0, 1]), (&mut p2, [1, 0])] {
        for i in order {
            p.apply(Op::LinkDamage {
                threat,
                damage: damages[i],
                provenance: Provenance::default(),
            })
            .expect("link");
            p.apply(pair(threat, damages[i], NAMES[i], Provenance::default()))
                .expect("rationale");
        }
    }

    assert_eq!(p1, p2);
    assert_eq!(save(&p1), save(&p2));
}

#[test]
fn one_rationale_marks_only_that_threat_modified() {
    let m = methodology();
    let (mut base, threat, damages) = fixture(&m);
    for d in damages {
        base.apply(Op::LinkDamage {
            threat,
            damage: d,
            provenance: Provenance::default(),
        })
        .expect("link");
    }
    let mut current = base.clone();
    current
        .apply(pair(threat, damages[0], "Because", who()))
        .expect("rationale");

    let diff = base.diff(&current).expect("diff");
    assert_eq!(diff.threat_scenarios().modified(), [threat]);
    assert!(diff.damage_scenarios().modified().is_empty());
}

#[test]
fn set_damage_link_rationale_survives_op_log_round_trip() {
    let m = methodology();
    let (mut p, threat, damages) = fixture(&m);
    p.apply(Op::LinkDamage {
        threat,
        damage: damages[0],
        provenance: Provenance::default(),
    })
    .expect("link 0");

    let text = "Brake line corroded";
    let op = pair(threat, damages[0], text, who());
    let mut bytes = Vec::new();
    let mut log = OpLogWriter::create(&mut bytes, Arc::clone(&m)).expect("header");
    log.append(&op).expect("append");
    drop(log);
    let back: Vec<_> = OpLogReader::open(bytes.as_slice(), Arc::clone(&m))
        .expect("open")
        .map(|r| r.expect("record").op)
        .collect();
    assert_eq!(back.len(), 1);
    match &back[0] {
        Op::SetDamageLinkRationale {
            threat: t,
            damage: d,
            text: x,
            provenance: pr,
        } => {
            assert_eq!(t, &threat);
            assert_eq!(d, &damages[0]);
            assert_eq!(x.as_ref(), text);
            assert_eq!(pr, &who());
        }
        other => panic!("unexpected op: {other:?}"),
    }
    assert_eq!(back[0], op);
}

#[test]
fn v1_fixture_resaves_byte_for_byte_without_rationale_key() {
    let m = methodology();
    let bytes = std::fs::read("tests/fixtures/project-v1.json").expect("v1 fixture");
    let p = Project::load(bytes.as_slice(), Arc::clone(&m)).expect("load v1");
    assert_eq!(save(&p), bytes);
    assert!(!String::from_utf8_lossy(&bytes).contains("rationale"));
}

#[test]
fn object_form_round_trips_byte_for_byte() {
    let m = methodology();
    let (mut p, threat, damages) = fixture(&m);
    for d in damages {
        p.apply(Op::LinkDamage {
            threat,
            damage: d,
            provenance: Provenance::default(),
        })
        .expect("link");
    }
    p.apply(pair(threat, damages[0], "Safety relevant", who()))
        .expect("rationale");

    let bytes = save(&p);
    let json: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
    let arr = json["threat_scenarios"][0]["damages"]
        .as_array()
        .expect("damages is an array");
    assert_eq!(arr.len(), 2);
    for (i, d) in damages.iter().enumerate() {
        assert_eq!(
            arr[i]["damage"].as_str().expect("damage is a string"),
            d.to_string()
        );
        assert!(arr[i].get("rationale").is_some(), "rationale key present");
    }
    assert_eq!(arr[0]["rationale"].as_str(), Some("Safety relevant"));
    assert_eq!(arr[1]["rationale"].as_str(), Some(""));

    let loaded = Project::load(bytes.as_slice(), Arc::clone(&m)).expect("load");
    assert_eq!(loaded, p);
    assert_eq!(save(&loaded), bytes);
}

#[test]
fn empty_rationale_reverts_to_v1_string_array_form() {
    let m = methodology();
    let (mut p, threat, damages) = fixture(&m);
    for d in damages {
        p.apply(Op::LinkDamage {
            threat,
            damage: d,
            provenance: Provenance::default(),
        })
        .expect("link");
    }
    p.apply(pair(threat, damages[0], "temporary", who()))
        .expect("set");
    p.apply(pair(threat, damages[0], "", Provenance::default()))
        .expect("clear");

    let bytes = save(&p);
    let json: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
    let arr = json["threat_scenarios"][0]["damages"]
        .as_array()
        .expect("damages is an array");
    assert_eq!(arr.len(), 2);
    assert!(
        arr.iter().all(serde_json::Value::is_string),
        "damages are plain ID strings"
    );
    assert!(!String::from_utf8_lossy(&bytes).contains("rationale"));
}
