//! Rating rationale (job 147, FND-09): why an impact or a direct feasibility
//! is rated as it is; exact inverses, removal guards, the store round trip,
//! and the lint that only a pack asking for rationale turns on.

use std::sync::Arc;

use aegis_model::{
    AssetId, DamageId, LintCode, Methodology, MethodologyDef, ModelError, Op, OpCodec, OpLogReader,
    OpLogWriter, Project, Provenance, ThreatId,
};

fn methodology(require_rationale: bool) -> Arc<Methodology> {
    let mut def =
        MethodologyDef::read_pack(Methodology::iso21434_r155_pack().as_bytes()).expect("pack");
    def.require_rationale = require_rationale;
    Arc::new(Methodology::new(def).expect("methodology"))
}

struct Fx {
    p: Project,
    damage: DamageId,
    threat: ThreatId,
}

/// A damage scenario with one impact rating and a threat with a direct
/// feasibility rating, neither with a rationale.
fn fixture(m: &Arc<Methodology>) -> Fx {
    let mut p = Project::new(Arc::clone(m));
    let (asset, damage, threat) = (AssetId::new_v7(), DamageId::new_v7(), ThreatId::new_v7());
    let category = m.impact_categories().next().expect("category");
    let level = m.levels(category).next().expect("level");
    for op in [
        Op::AddAsset {
            id: asset,
            name: "ECU".into(),
            properties: m.all_properties(),
            provenance: Provenance::default(),
        },
        Op::AddDamageScenario {
            id: damage,
            name: "Brakes fail".into(),
            impacts: vec![(category, level)],
            provenance: Provenance::default(),
        },
        Op::AddThreatScenario {
            id: threat,
            name: "Spoof CAN".into(),
            asset,
            category: m.threat_categories().next().expect("threat category"),
            catalog_refs: vec![],
            damages: vec![damage],
            provenance: Provenance::default(),
        },
        Op::SetThreatFeasibility {
            threat,
            level: m.feasibility_levels().next().map(|(l, _, _)| l),
            provenance: Provenance::default(),
        },
    ] {
        p.apply(op).expect("setup");
    }
    Fx { p, damage, threat }
}

fn impact(damage: DamageId, text: &str) -> Op {
    Op::SetImpactRationale {
        damage,
        text: text.into(),
        provenance: Provenance::default(),
    }
}

fn feasibility(threat: ThreatId, text: &str) -> Op {
    Op::SetFeasibilityRationale {
        threat,
        text: text.into(),
        provenance: Provenance::default(),
    }
}

fn rationale_lints(p: &Project) -> Vec<LintCode> {
    p.lint()
        .into_iter()
        .map(|f| f.code())
        .filter(|c| {
            matches!(
                c,
                LintCode::ImpactWithoutRationale { .. }
                    | LintCode::FeasibilityWithoutRationale { .. }
            )
        })
        .collect()
}

#[test]
fn rationale_is_set_and_undone_exactly_and_guards_removal() {
    let m = methodology(false);
    let mut fx = fixture(&m);
    let before = fx.p.clone();
    let u1 =
        fx.p.apply(impact(fx.damage, "Braking is safety relevant"))
            .expect("impact");
    let u2 =
        fx.p.apply(feasibility(fx.threat, "Needs a CAN tool and bus access"))
            .expect("feasibility");
    let d = fx.p.damage_scenario_idx(&fx.damage).expect("damage");
    assert_eq!(
        fx.p.damage_scenario(d).map(|d| d.impact_rationale()),
        Some("Braking is safety relevant")
    );
    let t = fx.p.threat_scenario_idx(&fx.threat).expect("threat");
    assert_eq!(
        fx.p.threat_scenario(t).map(|t| t.feasibility_rationale()),
        Some("Needs a CAN tool and bus access")
    );
    assert_eq!(
        fx.p.apply(Op::RemoveThreatScenario { id: fx.threat }),
        Err(ModelError::ThreatHasDirectFeasibility {
            threat: fx.threat,
            feasibility: m
                .feasibility_levels()
                .next()
                .map(|(l, _, _)| l)
                .expect("level"),
        })
    );
    fx.p.apply(u2).expect("undo feasibility");
    fx.p.apply(u1).expect("undo impact");
    assert_eq!(fx.p, before);

    // The guards: clear the rationale first, so a removal undoes exactly.
    let mut p = Project::new(Arc::clone(&m));
    let damage = DamageId::new_v7();
    p.apply(Op::AddDamageScenario {
        id: damage,
        name: "d".into(),
        impacts: vec![],
        provenance: Provenance::default(),
    })
    .expect("damage");
    p.apply(impact(damage, "why")).expect("rationale");
    assert_eq!(
        p.apply(Op::RemoveDamageScenario { id: damage }),
        Err(ModelError::DamageHasRationale { damage })
    );
    p.apply(impact(damage, "")).expect("clear");
    p.apply(Op::RemoveDamageScenario { id: damage })
        .expect("removes");
    assert_eq!(
        p.apply(impact(damage, "x")),
        Err(ModelError::UnknownDamageScenario(damage))
    );
}

#[test]
fn bad_text_is_rejected() {
    let m = methodology(false);
    let mut fx = fixture(&m);
    assert_eq!(
        fx.p.apply(feasibility(fx.threat, "nul\u{0}")),
        Err(ModelError::InvalidText)
    );
}

#[test]
fn the_lint_runs_only_when_the_pack_asks() {
    let quiet = fixture(&methodology(false));
    assert!(rationale_lints(&quiet.p).is_empty());

    let mut fx = fixture(&methodology(true));
    assert_eq!(
        rationale_lints(&fx.p),
        [
            LintCode::ImpactWithoutRationale { damage: fx.damage },
            LintCode::FeasibilityWithoutRationale { threat: fx.threat },
        ]
    );
    fx.p.apply(impact(fx.damage, "Safety relevant"))
        .expect("impact");
    fx.p.apply(feasibility(fx.threat, "  ")).expect("blank");
    assert_eq!(
        rationale_lints(&fx.p),
        [LintCode::FeasibilityWithoutRationale { threat: fx.threat }]
    );
}

#[test]
fn the_pack_flag_round_trips_and_is_absent_by_default() {
    let mut def =
        MethodologyDef::read_pack(Methodology::iso21434_r155_pack().as_bytes()).expect("pack");
    let mut plain = Vec::new();
    def.write_pack(&mut plain).expect("write");
    assert!(!String::from_utf8_lossy(&plain).contains("require_rationale"));
    def.require_rationale = true;
    let mut out = Vec::new();
    def.write_pack(&mut out).expect("write");
    let back = MethodologyDef::read_pack(out.as_slice()).expect("read");
    assert!(back.require_rationale);
    assert!(Methodology::new(back).expect("m").requires_rationale());
}

#[test]
fn snapshot_and_op_log_round_trip_and_stay_absent_when_empty() {
    let m = methodology(false);
    let mut fx = fixture(&m);
    let mut out = Vec::new();
    fx.p.save(&mut out).expect("save");
    assert!(!String::from_utf8_lossy(&out).contains("rationale"));

    fx.p.apply(impact(fx.damage, "影響は安全に関わる"))
        .expect("impact");
    fx.p.apply(feasibility(fx.threat, "Bus access needed"))
        .expect("feas");
    let mut out = Vec::new();
    fx.p.save(&mut out).expect("save");
    let loaded = Project::load(out.as_slice(), Arc::clone(&m)).expect("load");
    assert_eq!(loaded, fx.p);

    let ops = [impact(fx.damage, "one"), feasibility(fx.threat, "two")];
    let mut bytes = Vec::new();
    let mut log = OpLogWriter::create(&mut bytes, Arc::clone(&m)).expect("header");
    for op in &ops {
        log.append(op).expect("append");
    }
    drop(log);
    let back: Vec<_> = OpLogReader::open(bytes.as_slice(), Arc::clone(&m))
        .expect("open")
        .map(|r| r.expect("record").op)
        .collect();
    assert_eq!(back, ops);
    let codec = OpCodec::new(m);
    assert_eq!(
        codec.decode(&codec.encode(&ops[1]).expect("e")).expect("d"),
        ops[1]
    );
}
