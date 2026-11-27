//! Tests for `ControlStatus { Implemented, Planned, Off }` (140-control-status).

use std::sync::Arc;

use aegis_model::{
    AssetId, AttackTree, ComponentId, ControlId, ControlStatus, DamageId, GoalId, ItemRef,
    LintCode, Methodology, Node, NodeId, Op, OpCodec, Project, ProjectEvalCache, Provenance,
    RiskTreatment, StepTarget, ThreatId, TreeId,
};

fn build_fixture() -> (
    Project,
    Arc<Methodology>,
    ThreatId,
    TreeId,
    NodeId,
    ControlId,
    ControlId,
    GoalId,
) {
    let m = Arc::new(Methodology::iso21434_r155().expect("built-in"));
    let mut p = Project::new(Arc::clone(&m));
    let cat = m.threat_categories().next().expect("cat");
    let dmg_cat = m.impact_category("safety").expect("safety");
    let severe = m.level(dmg_cat, "severe").expect("severe");
    let high = m.feasibility_level("high").expect("high");
    let low = m.feasibility_level("low").expect("low");
    let vlow = m.feasibility_level("very-low").expect("very-low");
    let phys = m.attack_type("physical-tampering").expect("phys");

    let comp = ComponentId::new_v7();
    p.apply(Op::AddComponent {
        id: comp,
        name: "Brake ECU".into(),
        parent: None,
        provenance: Provenance::default(),
    })
    .expect("comp");

    let asset = AssetId::new_v7();
    p.apply(Op::AddAsset {
        id: asset,
        name: "Firmware".into(),
        properties: m.all_properties(),
        provenance: Provenance::default(),
    })
    .expect("asset");

    let dmg = DamageId::new_v7();
    p.apply(Op::AddDamageScenario {
        id: dmg,
        name: "Loss of braking".into(),
        impacts: vec![(dmg_cat, severe)],
        provenance: Provenance::default(),
    })
    .expect("dmg");

    let threat = ThreatId::new_v7();
    p.apply(Op::AddThreatScenario {
        id: threat,
        name: "Tamper firmware".into(),
        asset,
        category: cat,
        catalog_refs: vec![],
        damages: vec![dmg],
        provenance: Provenance::default(),
    })
    .expect("threat");

    let tree_id = TreeId::new_v7();
    let step = NodeId::new_v7();
    let tree = AttackTree::new(step, Node::step("Flash via JTAG")).expect("tree");
    p.apply(Op::AddAttackTree {
        role: Default::default(),
        id: tree_id,
        tree,
        step_feasibility: vec![(step, high)],
        provenance: Provenance::default(),
    })
    .expect("add tree");
    p.apply(Op::SetStepVia {
        target: StepTarget::Step(tree_id, step),
        via: Some(ItemRef::Component(comp)),
        provenance: Provenance::default(),
    })
    .expect("via");
    p.apply(Op::SetStepAttackType {
        target: StepTarget::Step(tree_id, step),
        attack_type: Some(phys),
        provenance: Provenance::default(),
    })
    .expect("attack type");
    p.apply(Op::SetThreatAttackTree {
        threat,
        tree: Some(tree_id),
        provenance: Provenance::default(),
    })
    .expect("link tree");

    // Control 1: direct step mitigation -> Low.
    let mit_ctrl = ControlId::new_v7();
    p.apply(Op::AddSecurityControl {
        id: mit_ctrl,
        name: "Signed firmware".into(),
        provenance: Provenance::default(),
    })
    .expect("mit_ctrl");
    p.apply(Op::SetStepMitigation {
        tree: tree_id,
        node: step,
        control: mit_ctrl,
        feasibility: Some(low),
        provenance: Provenance::default(),
    })
    .expect("set mitigation");

    // Control 2: applied rule ceiling -> Very Low.
    let rule_ctrl = ControlId::new_v7();
    p.apply(Op::AddSecurityControl {
        id: rule_ctrl,
        name: "JTAG fuse".into(),
        provenance: Provenance::default(),
    })
    .expect("rule_ctrl");
    p.apply(Op::ApplyControl {
        control: rule_ctrl,
        target: ItemRef::Component(comp),
        provenance: Provenance::default(),
    })
    .expect("apply");
    p.apply(Op::SetControlRule {
        control: rule_ctrl,
        attack_type: phys,
        ceiling: Some(vlow),
        provenance: Provenance::default(),
    })
    .expect("rule");

    let goal = GoalId::new_v7();
    p.apply(Op::AddCybersecurityGoal {
        id: goal,
        name: "Protect firmware".into(),
        controls: vec![mit_ctrl, rule_ctrl],
        provenance: Provenance::default(),
    })
    .expect("goal");
    p.apply(Op::SetRiskTreatment {
        threat,
        treatment: Some(RiskTreatment::Reduce),
        provenance: Provenance::default(),
    })
    .expect("reduce");
    p.apply(Op::SetThreatGoal {
        threat,
        goal: Some(goal),
        provenance: Provenance::default(),
    })
    .expect("goal link");

    (p, m, threat, tree_id, step, mit_ctrl, rule_ctrl, goal)
}

#[test]
fn three_statuses_across_step_mitigations_and_rule_ceilings() {
    let (mut p, m, threat, tree_id, _step, mit_ctrl, rule_ctrl, goal) = build_fixture();
    let high = m.feasibility_level("high").expect("high");
    let low = m.feasibility_level("low").expect("low");
    let vlow = m.feasibility_level("very-low").expect("very-low");
    let t_idx = p.attack_tree_idx(&tree_id).expect("t_idx");
    let root = p.attack_tree(t_idx).expect("tree").root();

    // Default: both controls are Planned -> Initial is High, Residual is Very Low.
    assert_eq!(p.evaluate_tree(tree_id).expect("init").root_value(), &high);
    assert_eq!(
        p.evaluate_residual_tree(tree_id).expect("res").root_value(),
        &vlow
    );
    let ceilings = p.step_ceilings(t_idx, root);
    assert_eq!(ceilings.len(), 2);
    assert!(
        ceilings
            .iter()
            .all(|c| c.status() == ControlStatus::Planned)
    );

    // Set mit_ctrl to Implemented: Initial drops to Low, Residual stays Very Low.
    p.apply(Op::SetControlStatus {
        control: mit_ctrl,
        status: ControlStatus::Implemented,
        provenance: Provenance::default(),
    })
    .expect("implemented");
    assert_eq!(p.evaluate_tree(tree_id).expect("init").root_value(), &low);
    assert_eq!(
        p.evaluate_residual_tree(tree_id).expect("res").root_value(),
        &vlow
    );
    assert!(
        p.step_ceilings(t_idx, root)
            .iter()
            .any(|c| { c.control_id() == mit_ctrl && c.status() == ControlStatus::Implemented })
    );

    // Set rule_ctrl to Implemented too: Initial drops to Very Low.
    p.apply(Op::SetControlStatus {
        control: rule_ctrl,
        status: ControlStatus::Implemented,
        provenance: Provenance::default(),
    })
    .expect("rule implemented");
    assert_eq!(p.evaluate_tree(tree_id).expect("init").root_value(), &vlow);

    // Turn rule_ctrl Off: Initial reverts to Low (mit_ctrl), Residual reverts to Low.
    p.apply(Op::SetControlStatus {
        control: rule_ctrl,
        status: ControlStatus::Off,
        provenance: Provenance::default(),
    })
    .expect("rule off");
    assert_eq!(p.evaluate_tree(tree_id).expect("init").root_value(), &low);
    assert_eq!(
        p.evaluate_residual_tree(tree_id).expect("res").root_value(),
        &low
    );
    assert_eq!(p.step_ceilings(t_idx, root).len(), 1);
    assert_eq!(p.step_ceilings(t_idx, root)[0].control_id(), mit_ctrl);

    // Turn mit_ctrl Off too: both Initial and Residual revert to High, and lint flags
    // ReduceTreatmentWithoutControls and CybersecurityGoalWithoutControls!
    p.apply(Op::SetControlStatus {
        control: mit_ctrl,
        status: ControlStatus::Off,
        provenance: Provenance::default(),
    })
    .expect("mit off");
    assert_eq!(p.evaluate_tree(tree_id).expect("init").root_value(), &high);
    assert_eq!(
        p.evaluate_residual_tree(tree_id).expect("res").root_value(),
        &high
    );
    assert!(p.step_ceilings(t_idx, root).is_empty());
    let lints = p.lint();
    assert!(
        lints
            .iter()
            .any(|f| f.code() == LintCode::ReduceTreatmentWithoutControls { threat })
    );
    assert!(
        lints
            .iter()
            .any(|f| f.code() == LintCode::CybersecurityGoalWithoutControls { goal })
    );
}

#[test]
fn incremental_cache_agrees_with_fresh_evaluation_across_status_transitions() {
    let (mut p, _m, threat, tree_id, _step, mit_ctrl, rule_ctrl, _goal) = build_fixture();
    let mut cache = ProjectEvalCache::new(&p);

    let transitions = [
        (mit_ctrl, ControlStatus::Implemented),
        (rule_ctrl, ControlStatus::Off),
        (rule_ctrl, ControlStatus::Implemented),
        (mit_ctrl, ControlStatus::Off),
        (rule_ctrl, ControlStatus::Planned),
        (mit_ctrl, ControlStatus::Planned),
        (mit_ctrl, ControlStatus::Off),
        (rule_ctrl, ControlStatus::Off),
        (rule_ctrl, ControlStatus::Implemented),
    ];

    let mut undos = Vec::new();
    for (control, status) in transitions {
        let op = Op::SetControlStatus {
            control,
            status,
            provenance: Provenance::default(),
        };
        let inv = p.apply_cached(op, &mut cache).expect("apply");
        undos.push(inv);

        assert_eq!(
            cache.evaluate_tree(&p, tree_id).expect("cached init"),
            &p.evaluate_tree(tree_id).expect("fresh init")
        );
        assert_eq!(
            cache
                .evaluate_residual_tree(&p, tree_id)
                .expect("cached res"),
            &p.evaluate_residual_tree(tree_id).expect("fresh res")
        );
        assert_eq!(
            cache.evaluate_risk(&p, threat).expect("cached risk"),
            &p.evaluate_risk(threat).expect("fresh risk")
        );
        assert_eq!(
            cache
                .evaluate_residual_risk(&p, threat)
                .expect("cached res risk"),
            &p.evaluate_residual_risk(threat).expect("fresh res risk")
        );
    }

    // Undo every transition and assert cache still matches fresh evaluation.
    while let Some(inv) = undos.pop() {
        p.apply_cached(inv, &mut cache).expect("undo");
        assert_eq!(
            cache.evaluate_tree(&p, tree_id).expect("cached init undo"),
            &p.evaluate_tree(tree_id).expect("fresh init undo")
        );
        assert_eq!(
            cache
                .evaluate_residual_tree(&p, tree_id)
                .expect("cached res undo"),
            &p.evaluate_residual_tree(tree_id).expect("fresh res undo")
        );
    }
}

#[test]
fn golden_v1_fixture_loads_as_planned_with_unchanged_risks_and_bytes() {
    let m = Arc::new(Methodology::iso21434_r155().expect("built-in"));
    let raw = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/project-v1.json"
    ))
    .expect("read v1 fixture");
    let loaded = Project::load(raw.as_slice(), Arc::clone(&m)).expect("load v1");

    // Every control defaults to Planned.
    for (_, _, c) in loaded.security_controls() {
        assert_eq!(c.status(), ControlStatus::Planned);
    }

    // Re-saving produces identical bytes (omits default Planned status).
    let mut out = Vec::new();
    loaded.save(&mut out).expect("save");
    assert_eq!(out, raw);

    // Every threat evaluates cleanly and consistently.
    for (_, id, _) in loaded.threat_scenarios() {
        let _ = loaded.evaluate_risk(id);
        let _ = loaded.evaluate_residual_risk(id);
    }
}

#[test]
fn status_round_trips_in_snapshot_oplog_workspace_and_diff() {
    let (mut p, m, _threat, _tree, _step, mit_ctrl, rule_ctrl, _goal) = build_fixture();
    let baseline = p.clone();

    let op1 = Op::SetControlStatus {
        control: mit_ctrl,
        status: ControlStatus::Implemented,
        provenance: Provenance::default(),
    };
    let op2 = Op::SetControlStatus {
        control: rule_ctrl,
        status: ControlStatus::Off,
        provenance: Provenance::default(),
    };
    p.apply(op1.clone()).expect("op1");
    p.apply(op2.clone()).expect("op2");

    // Diff detects both modified controls.
    let diff = baseline.diff(&p).expect("diff");
    assert!(diff.security_controls().modified().contains(&mit_ctrl));
    assert!(diff.security_controls().modified().contains(&rule_ctrl));

    // Snapshot round-trip.
    let mut bytes = Vec::new();
    p.save(&mut bytes).expect("save");
    let reloaded = Project::load(bytes.as_slice(), Arc::clone(&m)).expect("reload");
    assert_eq!(reloaded, p);
    assert_eq!(
        reloaded
            .security_control(reloaded.security_control_idx(&mit_ctrl).expect("idx"))
            .expect("c")
            .status(),
        ControlStatus::Implemented
    );
    assert_eq!(
        reloaded
            .security_control(reloaded.security_control_idx(&rule_ctrl).expect("idx"))
            .expect("c")
            .status(),
        ControlStatus::Off
    );

    // OpCodec round-trip.
    let codec = OpCodec::new(Arc::clone(&m));
    for op in [op1, op2] {
        let buf = codec.encode(&op).expect("encode");
        let decoded = codec.decode(&buf).expect("decode");
        assert_eq!(decoded, op);
    }

    // Workspace files round-trip.
    let files = p.to_workspace_files().expect("to workspace");
    let from_ws = Project::from_workspace_files(
        files.iter().map(|f| (f.path.as_str(), f.bytes.as_slice())),
        Arc::clone(&m),
    )
    .expect("from workspace");
    assert_eq!(from_ws, p);
}
