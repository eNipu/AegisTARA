//! Tests for multiple cybersecurity goals per threat scenario (161-threat-goals).

use std::sync::Arc;

use aegis_model::{
    AssetId, ControlId, GoalId, LintCode, MAX_THREAT_GOALS, Methodology, ModelError, Op, OpCodec,
    Project, Provenance, RiskTreatment, ThreatId,
};

fn base_project() -> (Project, Arc<Methodology>, ThreatId, GoalId, GoalId) {
    let m = Arc::new(Methodology::iso21434_r155().expect("built-in"));
    let mut p = Project::new(Arc::clone(&m));
    let asset = AssetId::new_v7();
    p.apply(Op::AddAsset {
        id: asset,
        name: "ECU".into(),
        properties: m.all_properties(),
        provenance: Provenance::default(),
    })
    .expect("asset");
    let cat = m.threat_categories().next().expect("cat");
    let threat = ThreatId::new_v7();
    p.apply(Op::AddThreatScenario {
        id: threat,
        name: "Tamper firmware".into(),
        asset,
        category: cat,
        catalog_refs: vec![],
        damages: vec![],
        provenance: Provenance::default(),
    })
    .expect("threat");
    let ctrl = ControlId::new_v7();
    p.apply(Op::AddSecurityControl {
        id: ctrl,
        name: "Secure boot".into(),
        provenance: Provenance::default(),
    })
    .expect("ctrl");
    let g1 = GoalId::new_v7();
    let g2 = GoalId::new_v7();
    for (id, name) in [(g1, "Protect boot"), (g2, "Protect updates")] {
        p.apply(Op::AddCybersecurityGoal {
            id,
            name: name.into(),
            controls: vec![ctrl],
            provenance: Provenance::default(),
        })
        .expect("goal");
    }
    (p, m, threat, g1, g2)
}

#[test]
fn lints_with_zero_one_and_two_goals() {
    let (mut p, _m, threat, g1, g2) = base_project();
    p.apply(Op::SetRiskTreatment {
        threat,
        treatment: Some(RiskTreatment::Reduce),
        provenance: Provenance::default(),
    })
    .expect("reduce");

    // 0 goals: ReduceTreatmentWithoutGoal and both goals unlinked.
    let lints = p.lint();
    assert!(
        lints
            .iter()
            .any(|f| f.code() == LintCode::ReduceTreatmentWithoutGoal { threat })
    );
    assert!(
        lints
            .iter()
            .any(|f| f.code() == LintCode::CybersecurityGoalUnlinked { goal: g1 })
    );
    assert!(
        lints
            .iter()
            .any(|f| f.code() == LintCode::CybersecurityGoalUnlinked { goal: g2 })
    );

    // 1 goal: ReduceTreatmentWithoutGoal clears; g1 linked, g2 unlinked.
    p.apply(Op::LinkThreatGoal {
        threat,
        goal: g1,
        provenance: Provenance::default(),
    })
    .expect("link g1");
    let lints = p.lint();
    assert!(
        !lints
            .iter()
            .any(|f| f.code() == LintCode::ReduceTreatmentWithoutGoal { threat })
    );
    assert!(
        !lints
            .iter()
            .any(|f| f.code() == LintCode::CybersecurityGoalUnlinked { goal: g1 })
    );
    assert!(
        lints
            .iter()
            .any(|f| f.code() == LintCode::CybersecurityGoalUnlinked { goal: g2 })
    );

    // 2 goals: both g1 and g2 linked.
    p.apply(Op::LinkThreatGoal {
        threat,
        goal: g2,
        provenance: Provenance::default(),
    })
    .expect("link g2");
    let lints = p.lint();
    assert!(
        !lints
            .iter()
            .any(|f| matches!(f.code(), LintCode::CybersecurityGoalUnlinked { .. }))
    );

    // Switch treatment to Avoid: GoalOnNonReduceThreat fires for BOTH goals!
    p.apply(Op::SetRiskTreatment {
        threat,
        treatment: Some(RiskTreatment::Avoid),
        provenance: Provenance::default(),
    })
    .expect("avoid");
    let lints = p.lint();
    assert!(
        lints
            .iter()
            .any(|f| f.code() == LintCode::GoalOnNonReduceThreat { threat, goal: g1 })
    );
    assert!(
        lints
            .iter()
            .any(|f| f.code() == LintCode::GoalOnNonReduceThreat { threat, goal: g2 })
    );
}

#[test]
fn snapshot_writes_goal_for_one_and_goals_for_two_and_shuffled_order_is_equal() {
    let (mut p1, m, threat, g1, g2) = base_project();
    let mut p2 = p1.clone();

    // Single goal writes legacy `"goal"` field (never `"goals"`).
    p1.apply(Op::LinkThreatGoal {
        threat,
        goal: g1,
        provenance: Provenance::default(),
    })
    .expect("link g1");
    let mut single_bytes = Vec::new();
    p1.save(&mut single_bytes).expect("save single");
    let single_text = String::from_utf8(single_bytes.clone()).expect("utf8");
    assert!(single_text.contains(r#""goal":"#));
    assert!(!single_text.contains(r#""goals":"#));
    let reloaded_single = Project::load(single_bytes.as_slice(), Arc::clone(&m)).expect("reload");
    assert_eq!(reloaded_single, p1);

    // Two goals in opposite insertion orders compare equal and save identical bytes.
    p1.apply(Op::LinkThreatGoal {
        threat,
        goal: g2,
        provenance: Provenance::default(),
    })
    .expect("p1 link g2");

    p2.apply(Op::LinkThreatGoal {
        threat,
        goal: g2,
        provenance: Provenance::default(),
    })
    .expect("p2 link g2");
    p2.apply(Op::LinkThreatGoal {
        threat,
        goal: g1,
        provenance: Provenance::default(),
    })
    .expect("p2 link g1");

    assert_eq!(p1, p2);
    let (mut b1, mut b2) = (Vec::new(), Vec::new());
    p1.save(&mut b1).expect("save p1");
    p2.save(&mut b2).expect("save p2");
    assert_eq!(b1, b2);
    let multi_text = String::from_utf8(b1.clone()).expect("utf8");
    assert!(multi_text.contains(r#""goals":["#));
    assert!(!multi_text.contains(r#""goal":"#));

    let reloaded = Project::load(b1.as_slice(), Arc::clone(&m)).expect("reload multi");
    assert_eq!(reloaded, p1);

    // Workspace files round-trip.
    let files = p1.to_workspace_files().expect("to workspace");
    let from_ws = Project::from_workspace_files(
        files.iter().map(|f| (f.path.as_str(), f.bytes.as_slice())),
        Arc::clone(&m),
    )
    .expect("from workspace");
    assert_eq!(from_ws, p1);
}

#[test]
fn link_unlink_inverses_guards_cap_and_oplog_codec() {
    let (mut p, m, threat, g1, g2) = base_project();
    let link1 = Op::LinkThreatGoal {
        threat,
        goal: g1,
        provenance: Provenance::default(),
    };
    let inv1 = p.apply(link1.clone()).expect("link1");
    assert!(
        matches!(inv1, Op::UnlinkThreatGoal { threat: t, goal: g, .. } if t == threat && g == g1)
    );

    // Duplicate link refused.
    assert!(matches!(
        p.apply(link1.clone()),
        Err(ModelError::ThreatGoalAlreadyLinked { .. })
    ));

    // Unlink missing refused.
    assert!(matches!(
        p.apply(Op::UnlinkThreatGoal {
            threat,
            goal: g2,
            provenance: Provenance::default(),
        }),
        Err(ModelError::ThreatGoalNotLinked { .. })
    ));

    // OpCodec round-trip.
    let codec = OpCodec::new(Arc::clone(&m));
    for op in [link1, inv1.clone()] {
        let raw = codec.encode(&op).expect("encode");
        assert_eq!(codec.decode(&raw).expect("decode"), op);
    }

    // Undo restores empty goals.
    p.apply(inv1).expect("undo");
    let t_idx = p.threat_scenario_idx(&threat).expect("idx");
    assert!(p.threat_scenario(t_idx).expect("t").goals().is_empty());

    // Cap enforcement (`MAX_THREAT_GOALS`).
    for i in 0..MAX_THREAT_GOALS {
        let g = GoalId::new_v7();
        p.apply(Op::AddCybersecurityGoal {
            id: g,
            name: format!("G{i}").into(),
            controls: vec![],
            provenance: Provenance::default(),
        })
        .expect("add g");
        p.apply(Op::LinkThreatGoal {
            threat,
            goal: g,
            provenance: Provenance::default(),
        })
        .expect("link g");
    }
    let extra = GoalId::new_v7();
    p.apply(Op::AddCybersecurityGoal {
        id: extra,
        name: "Overflow".into(),
        controls: vec![],
        provenance: Provenance::default(),
    })
    .expect("extra");
    assert!(matches!(
        p.apply(Op::LinkThreatGoal {
            threat,
            goal: extra,
            provenance: Provenance::default(),
        }),
        Err(ModelError::TooManyThreatGoals {
            max: MAX_THREAT_GOALS,
            ..
        })
    ));
}

#[test]
fn v1_golden_fixture_loads_and_resaves_byte_identical() {
    let m = Arc::new(Methodology::iso21434_r155().expect("built-in"));
    let raw = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/project-v1.json"
    ))
    .expect("read v1");
    let loaded = Project::load(raw.as_slice(), Arc::clone(&m)).expect("load v1");
    let mut out = Vec::new();
    loaded.save(&mut out).expect("save v1");
    assert_eq!(out, raw);
}
