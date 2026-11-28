//! Integration and property tests for security control bypass tree evaluation,
//! `StepCeiling::bypass`, `Project::diff` closure, and `ProjectEvalCache` updates (143b).

mod common;
#[path = "common/fragment_oracle.rs"]
mod fragment_oracle;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use aegis_model::{
    AttackFilter, AttackTree, CeilingSource, ControlStatus, ElementRef, FeasibilityLevel, GateKind,
    ItemRef, Lifted, Methodology, ModelError, Node, NodeId, Op, Project, ProjectEvalCache,
    Provenance, StepTarget, ThreatId, TreeId, TreeRole,
};
use common::{
    asset_id, assumption_id, builtin, cat, component_id, condition_id, control_id, damage_id, feas,
    lvl, node_id, risk, save, shared_step_id, threat_cat, threat_id, tree_id,
};

fn prov(n: u64) -> Provenance {
    Provenance::human("tester", n)
}

fn setup_threat_project(m: &Arc<Methodology>) -> (Project, ThreatId, TreeId, NodeId) {
    let mut p = Project::new(Arc::clone(m));
    let asset = asset_id(1);
    let damage = damage_id(1);
    let threat = threat_id(1);
    let tree_id = tree_id(1);
    let step = node_id(1);
    let safety = cat(m, "safety");
    let severe = lvl(m, safety, "severe");
    let category = threat_cat(m, "spoofing");
    let high = feas(m, "high");

    p.apply(Op::AddAsset {
        id: asset,
        name: "Gateway".into(),
        properties: m.all_properties(),
        provenance: prov(1),
    })
    .expect("asset");
    p.apply(Op::AddDamageScenario {
        id: damage,
        name: "Safety loss".into(),
        impacts: vec![(safety, severe)],
        provenance: prov(2),
    })
    .expect("damage");
    p.apply(Op::AddThreatScenario {
        id: threat,
        name: "Spoof CAN".into(),
        asset,
        category,
        catalog_refs: vec![],
        damages: vec![damage],
        provenance: prov(3),
    })
    .expect("threat");
    p.apply(Op::AddAttackTree {
        role: TreeRole::Threat,
        id: tree_id,
        tree: AttackTree::new(step, Node::step("Inject frame")).expect("tree"),
        step_feasibility: vec![(step, high)],
        provenance: prov(4),
    })
    .expect("add tree");
    p.apply(Op::SetThreatAttackTree {
        threat,
        tree: Some(tree_id),
        provenance: prov(5),
    })
    .expect("assign tree");

    (p, threat, tree_id, step)
}

fn add_or_fragment(
    p: &mut Project,
    tid: TreeId,
    root_id: NodeId,
    leaves: &[(NodeId, Option<FeasibilityLevel>)],
) {
    let mut tree = AttackTree::new(root_id, Node::gate(GateKind::Or, "Bypass root")).expect("tree");
    let mut feas_list = Vec::new();
    for &(nid, rating) in leaves {
        tree.add_child(tree.root(), nid, Node::step("Bypass step"))
            .expect("child");
        if let Some(level) = rating {
            feas_list.push((nid, level));
        }
    }
    p.apply(Op::AddAttackTree {
        role: TreeRole::Fragment,
        id: tid,
        tree,
        step_feasibility: feas_list,
        provenance: prov(10),
    })
    .expect("add fragment");
}

#[test]
fn ceiling_follows_bypass_leaf_gate_shared_and_subfragment_edits() {
    let m = builtin();
    let (vlow, low, med, high) = (
        feas(&m, "very-low"),
        feas(&m, "low"),
        feas(&m, "medium"),
        feas(&m, "high"),
    );
    let (mut p, threat, main_tid, main_step) = setup_threat_project(&m);

    let ctrl = control_id(1);
    p.apply(Op::AddSecurityControl {
        id: ctrl,
        name: "SecOC".into(),
        provenance: prov(11),
    })
    .expect("control");

    // Bypass fragment B: OR(b_leaf=High, b_shared->Shared(High), b_frag->SubFrag(High))
    let sub_frag = tree_id(2);
    let sub_root = node_id(20);
    let sub_leaf = node_id(21);
    add_or_fragment(&mut p, sub_frag, sub_root, &[(sub_leaf, Some(high))]);

    let shared = shared_step_id(1);
    p.apply(Op::AddSharedStep {
        id: shared,
        name: "Shared bypass step".into(),
        provenance: prov(12),
    })
    .expect("shared");
    p.apply(Op::SetStepFeasibility {
        target: StepTarget::Shared(shared),
        level: Some(high),
        provenance: prov(13),
    })
    .expect("shared feas");

    let bypass_tid = tree_id(3);
    let b_root = node_id(30);
    let (b_leaf, b_shared, b_frag) = (node_id(31), node_id(32), node_id(33));
    add_or_fragment(
        &mut p,
        bypass_tid,
        b_root,
        &[(b_leaf, Some(high)), (b_shared, None), (b_frag, None)],
    );
    p.apply(Op::UseSharedStep {
        tree: bypass_tid,
        node: b_shared,
        step: shared,
        provenance: prov(14),
    })
    .expect("use shared");
    p.apply(Op::UseFragment {
        tree: bypass_tid,
        node: b_frag,
        fragment: sub_frag,
        provenance: prov(15),
    })
    .expect("use fragment");

    // Step mitigated with low, but control's bypass evaluates to high -> ceiling is high.
    p.apply(Op::SetStepMitigation {
        tree: main_tid,
        node: main_step,
        control: ctrl,
        feasibility: Some(low),
        provenance: prov(16),
    })
    .expect("mitigation");
    p.apply(Op::SetControlBypass {
        control: ctrl,
        tree: Some(bypass_tid),
        provenance: prov(17),
    })
    .expect("set bypass");

    let main_idx = p.attack_tree_idx(&main_tid).expect("main idx");
    let main_root = p.attack_tree(main_idx).expect("tree").root();
    assert_eq!(
        p.residual_step_feasibility(main_idx, main_step_idx(&p, main_tid, main_step)),
        Some(high)
    );
    let ceilings = p.step_ceilings(main_idx, main_root);
    assert_eq!(ceilings.len(), 1);
    assert_eq!(ceilings[0].feasibility(), high);
    assert_eq!(ceilings[0].bypass(), Some(bypass_tid));
    assert_eq!(ceilings[0].source(), CeilingSource::StepMitigation);
    assert_eq!(
        p.evaluate_residual_risk(threat).expect("res").feasibility(),
        high
    );

    // Switch gate in B from OR to AND and drop all 3 branches from High -> Med one by one,
    // verifying bypass ceiling becomes Med and undo restores High.
    let inv_gate = p
        .apply(Op::SetTreeNodeGate {
            tree: bypass_tid,
            id: b_root,
            gate: GateKind::And,
            provenance: prov(18),
        })
        .expect("and gate");

    // 1. Edit leaf in B to Medium -> AND(Med, High, High) == Med
    let inv_leaf = p
        .apply(Op::SetStepFeasibility {
            target: StepTarget::Step(bypass_tid, b_leaf),
            level: Some(med),
            provenance: prov(19),
        })
        .expect("edit leaf");
    assert_eq!(p.residual_step_feasibility(main_idx, main_root), Some(med));
    assert_eq!(p.step_ceilings(main_idx, main_root)[0].feasibility(), med);
    assert_eq!(
        p.evaluate_residual_risk(threat).expect("res").feasibility(),
        med
    );
    assert_eq!(
        p.evaluate_residual_risk(threat).expect("res").max_risk(),
        Some(risk(&m, "r4"))
    );
    p.apply(inv_leaf).expect("undo leaf");
    assert_eq!(p.residual_step_feasibility(main_idx, main_root), Some(high));

    // 2. Edit shared step in B to Medium -> AND(High, Med, High) == Med
    let inv_shared = p
        .apply(Op::SetStepFeasibility {
            target: StepTarget::Shared(shared),
            level: Some(med),
            provenance: prov(20),
        })
        .expect("edit shared");
    assert_eq!(p.residual_step_feasibility(main_idx, main_root), Some(med));
    assert_eq!(p.step_ceilings(main_idx, main_root)[0].feasibility(), med);
    p.apply(inv_shared).expect("undo shared");
    assert_eq!(p.residual_step_feasibility(main_idx, main_root), Some(high));

    // 3. Edit sub-fragment leaf to Medium -> AND(High, High, Med) == Med
    let inv_sub = p
        .apply(Op::SetStepFeasibility {
            target: StepTarget::Step(sub_frag, sub_leaf),
            level: Some(med),
            provenance: prov(21),
        })
        .expect("edit sub");
    assert_eq!(p.residual_step_feasibility(main_idx, main_root), Some(med));
    assert_eq!(p.step_ceilings(main_idx, main_root)[0].feasibility(), med);
    p.apply(inv_sub).expect("undo sub");
    assert_eq!(p.residual_step_feasibility(main_idx, main_root), Some(high));

    // 4. Verify bv replaces m.feasibility even when bv < m.feasibility.
    p.apply(Op::SetStepMitigation {
        tree: main_tid,
        node: main_step,
        control: ctrl,
        feasibility: Some(med),
        provenance: prov(22),
    })
    .expect("mit med");
    p.apply(Op::SetStepFeasibility {
        target: StepTarget::Step(bypass_tid, b_leaf),
        level: Some(vlow),
        provenance: prov(23),
    })
    .expect("leaf vlow");
    // Gate is AND(vlow, high, high) = vlow < m.feasibility(med)
    assert_eq!(p.residual_step_feasibility(main_idx, main_root), Some(vlow));
    assert_eq!(p.step_ceilings(main_idx, main_root)[0].feasibility(), vlow);
    assert_eq!(
        p.evaluate_residual_risk(threat).expect("res").feasibility(),
        vlow
    );

    // Undo gate back to OR -> OR(vlow, high, high) = high > m.feasibility(med)
    p.apply(inv_gate).expect("undo gate");
    assert_eq!(p.residual_step_feasibility(main_idx, main_root), Some(high));
}

fn main_step_idx(p: &Project, tid: TreeId, nid: NodeId) -> aegis_model::NodeIdx {
    let tidx = p.attack_tree_idx(&tid).expect("tree idx");
    p.attack_tree(tidx)
        .expect("tree")
        .idx_of(&nid)
        .expect("node idx")
}

#[test]
fn d28_status_matrix_off_removes_ceiling_and_control_rules_stay_bypass_none() {
    let m = builtin();
    let (vlow, low, med, high) = (
        feas(&m, "very-low"),
        feas(&m, "low"),
        feas(&m, "medium"),
        feas(&m, "high"),
    );
    let spoofing = m.attack_types().next().expect("attack type");
    let (mut p, threat, main_tid, main_step) = setup_threat_project(&m);

    let comp = component_id(1);
    p.apply(Op::AddComponent {
        id: comp,
        name: "ECU".into(),
        parent: None,
        provenance: prov(30),
    })
    .expect("comp");
    p.apply(Op::SetStepVia {
        target: StepTarget::Step(main_tid, main_step),
        via: Some(ItemRef::Component(comp)),
        provenance: prov(31),
    })
    .expect("via");
    p.apply(Op::SetStepAttackType {
        target: StepTarget::Step(main_tid, main_step),
        attack_type: Some(spoofing),
        provenance: prov(32),
    })
    .expect("attack type");

    let ctrl_bypass = control_id(1);
    let ctrl_rule = control_id(2);
    p.apply(Op::AddSecurityControl {
        id: ctrl_bypass,
        name: "BypassCtrl".into(),
        provenance: prov(33),
    })
    .expect("c1");
    p.apply(Op::AddSecurityControl {
        id: ctrl_rule,
        name: "RuleCtrl".into(),
        provenance: prov(34),
    })
    .expect("c2");
    // Give ctrl_bypass BOTH a bypass tree (Med) AND a ControlRule (High) to ensure
    // ControlRule never picks up bypass().
    p.apply(Op::ApplyControl {
        control: ctrl_bypass,
        target: ItemRef::Component(comp),
        provenance: prov(35),
    })
    .expect("apply c1");
    p.apply(Op::SetControlRule {
        control: ctrl_bypass,
        attack_type: spoofing,
        ceiling: Some(high),
        provenance: prov(36),
    })
    .expect("rule c1");
    p.apply(Op::ApplyControl {
        control: ctrl_rule,
        target: ItemRef::Component(comp),
        provenance: prov(37),
    })
    .expect("apply c2");
    p.apply(Op::SetControlRule {
        control: ctrl_rule,
        attack_type: spoofing,
        ceiling: Some(high),
        provenance: prov(38),
    })
    .expect("rule c2");

    let bypass_tid = tree_id(2);
    let b_root = node_id(20);
    let b_leaf = node_id(21);
    add_or_fragment(&mut p, bypass_tid, b_root, &[(b_leaf, Some(med))]);

    p.apply(Op::SetControlBypass {
        control: ctrl_bypass,
        tree: Some(bypass_tid),
        provenance: prov(39),
    })
    .expect("bypass");
    p.apply(Op::SetStepMitigation {
        tree: main_tid,
        node: main_step,
        control: ctrl_bypass,
        feasibility: Some(vlow),
        provenance: prov(40),
    })
    .expect("mit");

    let main_idx = p.attack_tree_idx(&main_tid).expect("main idx");
    let step_idx = main_step_idx(&p, main_tid, main_step);

    // Implemented: applies bypass ceiling (med) in both initial & residual.
    p.apply(Op::SetControlStatus {
        control: ctrl_bypass,
        status: ControlStatus::Implemented,
        provenance: prov(41),
    })
    .expect("impl");
    assert_eq!(p.evaluate_tree(main_tid).expect("init").root_value(), &med);
    assert_eq!(
        p.evaluate_risk(threat).expect("init risk").feasibility(),
        med
    );
    assert_eq!(
        p.evaluate_residual_tree(main_tid)
            .expect("res")
            .root_value(),
        &med
    );
    assert_eq!(p.residual_step_feasibility(main_idx, step_idx), Some(med));
    let ceilings = p.step_ceilings(main_idx, step_idx);
    assert_eq!(ceilings.len(), 3);
    assert_eq!(ceilings[0].control_id(), ctrl_bypass);
    assert_eq!(ceilings[0].source(), CeilingSource::StepMitigation);
    assert_eq!(ceilings[0].feasibility(), med);
    assert_eq!(ceilings[0].bypass(), Some(bypass_tid));
    for c in &ceilings[1..] {
        assert!(matches!(c.source(), CeilingSource::ControlRule { .. }));
        assert_eq!(c.bypass(), None);
        assert_eq!(c.feasibility(), high);
    }

    // Planned: initial is High (bypass ignored), residual is Med.
    p.apply(Op::SetControlStatus {
        control: ctrl_bypass,
        status: ControlStatus::Planned,
        provenance: prov(42),
    })
    .expect("planned");
    assert_eq!(p.evaluate_tree(main_tid).expect("init").root_value(), &high);
    assert_eq!(
        p.evaluate_risk(threat).expect("init risk").feasibility(),
        high
    );
    assert_eq!(
        p.evaluate_residual_tree(main_tid)
            .expect("res")
            .root_value(),
        &med
    );
    assert_eq!(p.residual_step_feasibility(main_idx, step_idx), Some(med));

    // Also make ctrl_rule Implemented at Low, and set ctrl_bypass Off with an unrated leaf in B:
    // Off removes ctrl_bypass completely, and unrated leaf in B must NOT fail evaluation!
    p.apply(Op::SetControlStatus {
        control: ctrl_rule,
        status: ControlStatus::Implemented,
        provenance: prov(43),
    })
    .expect("rule impl");
    p.apply(Op::SetControlRule {
        control: ctrl_rule,
        attack_type: spoofing,
        ceiling: Some(low),
        provenance: prov(44),
    })
    .expect("rule low");
    p.apply(Op::SetStepFeasibility {
        target: StepTarget::Step(bypass_tid, b_leaf),
        level: None,
        provenance: prov(45),
    })
    .expect("unrate bypass");

    // While Planned with unrated bypass: initial still succeeds (High -> capped by ctrl_rule=Low),
    // residual fails with MissingLeafValue.
    assert_eq!(
        p.evaluate_tree(main_tid).expect("init ok").root_value(),
        &low
    );
    assert!(matches!(
        p.evaluate_residual_tree(main_tid),
        Err(ModelError::MissingLeafValue(_))
    ));

    // Switch ctrl_bypass to Off -> unrated bypass is ignored, ceilings only contain ctrl_rule (bypass=None).
    p.apply(Op::SetControlStatus {
        control: ctrl_bypass,
        status: ControlStatus::Off,
        provenance: prov(46),
    })
    .expect("off");
    assert_eq!(p.evaluate_tree(main_tid).expect("init").root_value(), &low);
    assert_eq!(
        p.evaluate_residual_tree(main_tid)
            .expect("res")
            .root_value(),
        &low
    );
    assert_eq!(p.residual_step_feasibility(main_idx, step_idx), Some(low));
    let ceilings_off = p.step_ceilings(main_idx, step_idx);
    assert_eq!(ceilings_off.len(), 1);
    assert_eq!(ceilings_off[0].control_id(), ctrl_rule);
    assert_eq!(ceilings_off[0].bypass(), None);
}

#[test]
fn bypass_excluded_by_condition_assumption_or_profile_and_missing_leaf_precedence() {
    let m = builtin();
    let (low, high) = (feas(&m, "low"), feas(&m, "high"));

    // 1. Condition exclusion: sole path -> TreeNotApplicable / ThreatNotApplicable;
    //    OR sibling -> sibling survives.
    let (mut p, threat, main_tid, main_step) = setup_threat_project(&m);
    let ctrl = control_id(1);
    p.apply(Op::AddSecurityControl {
        id: ctrl,
        name: "CondBypass".into(),
        provenance: prov(50),
    })
    .expect("ctrl");
    let cond = condition_id(1);
    p.apply(Op::AddCondition {
        id: cond,
        name: "Feature enabled".into(),
        description: "".into(),
        present: false,
        provenance: prov(51),
    })
    .expect("cond");
    let bypass_tid = tree_id(2);
    add_or_fragment(
        &mut p,
        bypass_tid,
        node_id(20),
        &[(node_id(21), Some(high))],
    );
    p.apply(Op::NeedCondition {
        element: ElementRef::Tree(bypass_tid),
        condition: cond,
        provenance: prov(52),
    })
    .expect("need cond");
    p.apply(Op::SetControlBypass {
        control: ctrl,
        tree: Some(bypass_tid),
        provenance: prov(53),
    })
    .expect("bypass");
    p.apply(Op::SetStepMitigation {
        tree: main_tid,
        node: main_step,
        control: ctrl,
        feasibility: Some(low),
        provenance: prov(54),
    })
    .expect("mit");

    let main_idx = p.attack_tree_idx(&main_tid).expect("main idx");
    let step_idx = main_step_idx(&p, main_tid, main_step);
    assert_eq!(
        p.evaluate_residual_tree(main_tid),
        Err(ModelError::TreeNotApplicable(main_tid))
    );
    assert_eq!(
        p.evaluate_residual_risk(threat),
        Err(ModelError::ThreatNotApplicable(threat))
    );
    assert_eq!(p.residual_step_feasibility(main_idx, step_idx), None);
    assert!(p.step_ceilings(main_idx, step_idx).is_empty());

    // Add OR parent and unmitigated sibling -> sibling Low survives!
    let alt_step = node_id(2);
    let or_root = node_id(3);
    // Test OR sibling in a fresh tree with OR root.
    let (mut p_or_only, _, or_tid, or_s1) = setup_threat_project(&m);
    let mut or_tree = AttackTree::new(or_root, Node::gate(GateKind::Or, "Root")).expect("or");
    or_tree
        .add_child(or_tree.root(), or_s1, Node::step("Mitigated"))
        .expect("s1");
    or_tree
        .add_child(or_tree.root(), alt_step, Node::step("Alt"))
        .expect("s2");
    p_or_only
        .apply(Op::SetThreatAttackTree {
            threat,
            tree: None,
            provenance: prov(55),
        })
        .expect("detach");
    p_or_only
        .apply(Op::RemoveAttackTree { id: or_tid })
        .expect("rm");
    p_or_only
        .apply(Op::AddAttackTree {
            role: TreeRole::Threat,
            id: or_tid,
            tree: or_tree,
            step_feasibility: vec![(or_s1, high), (alt_step, low)],
            provenance: prov(56),
        })
        .expect("add or");
    p_or_only
        .apply(Op::SetThreatAttackTree {
            threat,
            tree: Some(or_tid),
            provenance: prov(56),
        })
        .expect("reattach");
    p_or_only
        .apply(Op::AddSecurityControl {
            id: ctrl,
            name: "CondBypass".into(),
            provenance: prov(57),
        })
        .expect("ctrl");
    p_or_only
        .apply(Op::AddCondition {
            id: cond,
            name: "Feature".into(),
            description: "".into(),
            present: false,
            provenance: prov(58),
        })
        .expect("cond");
    add_or_fragment(
        &mut p_or_only,
        bypass_tid,
        node_id(20),
        &[(node_id(21), Some(high))],
    );
    p_or_only
        .apply(Op::NeedCondition {
            element: ElementRef::Tree(bypass_tid),
            condition: cond,
            provenance: prov(59),
        })
        .expect("need");
    p_or_only
        .apply(Op::SetControlBypass {
            control: ctrl,
            tree: Some(bypass_tid),
            provenance: prov(60),
        })
        .expect("bypass");
    p_or_only
        .apply(Op::SetStepMitigation {
            tree: or_tid,
            node: or_s1,
            control: ctrl,
            feasibility: Some(low),
            provenance: prov(61),
        })
        .expect("mit");
    assert_eq!(
        p_or_only
            .evaluate_residual_tree(or_tid)
            .expect("or")
            .root_value(),
        &low
    );

    // 2. Assumption exclusion on bypass tree.
    p.apply(Op::SetConditionPresent {
        id: cond,
        present: true,
        provenance: prov(63),
    })
    .expect("present");
    assert_eq!(
        p.evaluate_residual_tree(main_tid).expect("ok").root_value(),
        &high
    );
    let assump = assumption_id(1);
    p.apply(Op::AddAssumption {
        id: assump,
        name: "No JTAG".into(),
        holds: true,
        provenance: prov(64),
    })
    .expect("assump");
    p.apply(Op::SetAssumptionExcludes {
        id: assump,
        trees: vec![bypass_tid],
        steps: vec![],
        provenance: prov(65),
    })
    .expect("exclude");
    assert_eq!(
        p.evaluate_residual_tree(main_tid),
        Err(ModelError::TreeNotApplicable(main_tid))
    );
    assert_eq!(p.residual_step_feasibility(main_idx, step_idx), None);

    // 3. Attacker profile exclusion on bypass tree's step factor.
    p.apply(Op::SetAssumptionHolds {
        id: assump,
        holds: false,
        provenance: prov(66),
    })
    .expect("unhold");
    assert_eq!(
        p.evaluate_residual_tree(main_tid).expect("ok").root_value(),
        &high
    );
    let factor = m.feasibility_factors().next().expect("factor");
    let flevel = m.factor_levels(factor).next().expect("factor level");
    p.apply(Op::SetStepFactor {
        target: StepTarget::Step(bypass_tid, node_id(21)),
        factor,
        level: Some(flevel),
        provenance: prov(67),
    })
    .expect("set factor");
    p.apply(Op::SetAttackFilter {
        filter: AttackFilter::default().without(factor, flevel),
        provenance: prov(68),
    })
    .expect("filter");
    assert_eq!(
        p.evaluate_residual_tree(main_tid),
        Err(ModelError::TreeNotApplicable(main_tid))
    );
    assert_eq!(p.residual_step_feasibility(main_idx, step_idx), None);
    assert!(p.step_ceilings(main_idx, step_idx).is_empty());

    // 4. Unrated bypass leaf + precedence over excluded sibling regardless of order.
    for order_first_unrated in [false, true] {
        let (mut pr, _, mtid, mstep) = setup_threat_project(&m);
        let c_excl = control_id(10);
        let c_unrated = control_id(11);
        for (cid, name) in [(c_excl, "Excl"), (c_unrated, "Unrated")] {
            pr.apply(Op::AddSecurityControl {
                id: cid,
                name: name.into(),
                provenance: prov(70),
            })
            .expect("ctrl");
        }
        let cnd = condition_id(2);
        pr.apply(Op::AddCondition {
            id: cnd,
            name: "Off".into(),
            description: "".into(),
            present: false,
            provenance: prov(71),
        })
        .expect("cond");
        let b_excl = tree_id(10);
        let b_unrated = tree_id(11);
        add_or_fragment(&mut pr, b_excl, node_id(100), &[(node_id(101), Some(high))]);
        add_or_fragment(&mut pr, b_unrated, node_id(110), &[(node_id(111), None)]);
        pr.apply(Op::NeedCondition {
            element: ElementRef::Tree(b_excl),
            condition: cnd,
            provenance: prov(72),
        })
        .expect("need");
        pr.apply(Op::SetControlBypass {
            control: c_excl,
            tree: Some(b_excl),
            provenance: prov(73),
        })
        .expect("b excl");
        pr.apply(Op::SetControlBypass {
            control: c_unrated,
            tree: Some(b_unrated),
            provenance: prov(74),
        })
        .expect("b unrated");

        let seq = if order_first_unrated {
            [c_unrated, c_excl]
        } else {
            [c_excl, c_unrated]
        };
        for cid in seq {
            pr.apply(Op::SetStepMitigation {
                tree: mtid,
                node: mstep,
                control: cid,
                feasibility: Some(low),
                provenance: prov(75),
            })
            .expect("mit");
        }
        let mt_idx = pr.attack_tree_idx(&mtid).expect("idx");
        let ms_idx = main_step_idx(&pr, mtid, mstep);
        assert_eq!(pr.residual_step_feasibility(mt_idx, ms_idx), None);
        assert!(pr.step_ceilings(mt_idx, ms_idx).is_empty());
        assert!(
            matches!(
                pr.evaluate_residual_tree(mtid),
                Err(ModelError::MissingLeafValue(_))
            ),
            "MissingLeafValue must win regardless of mitigation order ({order_first_unrated})"
        );
    }
}

#[test]
fn diff_reports_threats_when_bypass_tree_or_subfragment_changes() {
    let m = builtin();
    let (low, med, high) = (feas(&m, "low"), feas(&m, "medium"), feas(&m, "high"));
    let (mut base, threat, main_tid, main_step) = setup_threat_project(&m);

    let cond_b = condition_id(1);
    let cond_f = condition_id(2);
    for (cid, name) in [(cond_b, "B cond"), (cond_f, "F cond")] {
        base.apply(Op::AddCondition {
            id: cid,
            name: name.into(),
            description: "".into(),
            present: true,
            provenance: prov(80),
        })
        .expect("cond");
    }

    let sh_b = shared_step_id(1);
    let sh_f = shared_step_id(2);
    for sid in [sh_b, sh_f] {
        base.apply(Op::AddSharedStep {
            id: sid,
            name: "Shared".into(),
            provenance: prov(81),
        })
        .expect("shared");
        base.apply(Op::SetStepFeasibility {
            target: StepTarget::Shared(sid),
            level: Some(high),
            provenance: prov(82),
        })
        .expect("shared feas");
    }

    // Sub-fragment F: AND(f_leaf, f_shared)
    let sub_tid = tree_id(2);
    let (f_root, f_leaf, f_sh) = (node_id(20), node_id(21), node_id(22));
    add_or_fragment(
        &mut base,
        sub_tid,
        f_root,
        &[(f_leaf, Some(high)), (f_sh, None)],
    );
    base.apply(Op::SetTreeNodeGate {
        tree: sub_tid,
        id: f_root,
        gate: GateKind::And,
        provenance: prov(83),
    })
    .expect("and");
    base.apply(Op::UseSharedStep {
        tree: sub_tid,
        node: f_sh,
        step: sh_f,
        provenance: prov(84),
    })
    .expect("use sh_f");
    base.apply(Op::NeedCondition {
        element: ElementRef::Tree(sub_tid),
        condition: cond_f,
        provenance: prov(85),
    })
    .expect("need f");

    // Bypass B: AND(b_leaf, b_shared, b_sub)
    let bypass_tid = tree_id(3);
    let (b_root, b_leaf, b_sh, b_sub) = (node_id(30), node_id(31), node_id(32), node_id(33));
    add_or_fragment(
        &mut base,
        bypass_tid,
        b_root,
        &[(b_leaf, Some(high)), (b_sh, None), (b_sub, None)],
    );
    base.apply(Op::SetTreeNodeGate {
        tree: bypass_tid,
        id: b_root,
        gate: GateKind::And,
        provenance: prov(86),
    })
    .expect("and");
    base.apply(Op::UseSharedStep {
        tree: bypass_tid,
        node: b_sh,
        step: sh_b,
        provenance: prov(87),
    })
    .expect("use sh_b");
    base.apply(Op::UseFragment {
        tree: bypass_tid,
        node: b_sub,
        fragment: sub_tid,
        provenance: prov(88),
    })
    .expect("use sub");
    base.apply(Op::NeedCondition {
        element: ElementRef::Tree(bypass_tid),
        condition: cond_b,
        provenance: prov(89),
    })
    .expect("need b");

    let ctrl = control_id(1);
    base.apply(Op::AddSecurityControl {
        id: ctrl,
        name: "Bypass".into(),
        provenance: prov(90),
    })
    .expect("ctrl");
    base.apply(Op::SetControlBypass {
        control: ctrl,
        tree: Some(bypass_tid),
        provenance: prov(91),
    })
    .expect("set bypass");
    base.apply(Op::SetStepMitigation {
        tree: main_tid,
        node: main_step,
        control: ctrl,
        feasibility: Some(low),
        provenance: prov(92),
    })
    .expect("mit");

    let cases: [(&str, Op); 7] = [
        (
            "bypass leaf",
            Op::SetStepFeasibility {
                target: StepTarget::Step(bypass_tid, b_leaf),
                level: Some(med),
                provenance: prov(100),
            },
        ),
        (
            "bypass shared",
            Op::SetStepFeasibility {
                target: StepTarget::Shared(sh_b),
                level: Some(med),
                provenance: prov(101),
            },
        ),
        (
            "bypass condition",
            Op::SetConditionPresent {
                id: cond_b,
                present: false,
                provenance: prov(102),
            },
        ),
        (
            "subfragment leaf",
            Op::SetStepFeasibility {
                target: StepTarget::Step(sub_tid, f_leaf),
                level: Some(med),
                provenance: prov(103),
            },
        ),
        (
            "subfragment shared",
            Op::SetStepFeasibility {
                target: StepTarget::Shared(sh_f),
                level: Some(med),
                provenance: prov(104),
            },
        ),
        (
            "subfragment condition",
            Op::SetConditionPresent {
                id: cond_f,
                present: false,
                provenance: prov(105),
            },
        ),
        (
            "detach bypass",
            Op::SetControlBypass {
                control: ctrl,
                tree: None,
                provenance: prov(106),
            },
        ),
    ];

    for (label, op) in cases {
        let mut modified = base.clone();
        modified.apply(op).expect(label);
        let diff = base.diff(&modified).expect("diff");
        let changed: Vec<ThreatId> = diff.changed_risks().map(|d| d.threat()).collect();
        assert_eq!(changed, vec![threat], "{label} did not report threat");
    }
}

// ---------------------------------------------------------------------------
// Incremental cache proptest against fresh Project and fragment_oracle (143b)
// ---------------------------------------------------------------------------

const LEVELS: [&str; 4] = ["very-low", "low", "medium", "high"];
const GATES: [GateKind; 3] = [GateKind::Or, GateKind::And, GateKind::SeqAnd];
const NUM_THREATS: usize = 2;
const NUM_TREES: usize = 6; // 0..2 Threat, 2..6 Fragment
const NUM_CONTROLS: usize = 3;

struct Rng(u64);

impl Rng {
    fn next(&mut self, n: usize) -> usize {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        ((z ^ (z >> 31)) % n as u64) as usize
    }
    fn chance(&mut self, percent: usize) -> bool {
        self.next(100) < percent
    }
}

#[derive(Clone, Debug, PartialEq)]
enum LeafKind {
    Own(Option<usize>),
    Shared(usize),
    Frag(usize),
}

#[derive(Clone, Debug)]
struct SpecStep {
    id: NodeId,
    leaf: LeafKind,
    cond: Option<usize>,
    mits: BTreeMap<usize, usize>, // control_idx -> feasibility level idx
}

#[derive(Clone, Debug)]
enum SpecKid {
    Step(SpecStep),
    Gate(NodeId, GateKind, Vec<SpecStep>),
}

#[derive(Clone, Debug)]
struct SpecTree {
    gate: GateKind,
    kids: Vec<SpecKid>,
    cond: Option<usize>,
}

#[derive(Clone, Debug)]
struct Spec {
    trees: Vec<SpecTree>,
    shared: Vec<Option<usize>>,
    present: Vec<bool>,
    status: Vec<ControlStatus>,
    bypass: Vec<Option<usize>>,
}

impl SpecTree {
    fn steps(&self) -> impl Iterator<Item = &SpecStep> {
        self.kids.iter().flat_map(|k| match k {
            SpecKid::Step(s) => std::slice::from_ref(s).iter(),
            SpecKid::Gate(_, _, s) => s.iter(),
        })
    }
    fn steps_mut(&mut self) -> impl Iterator<Item = &mut SpecStep> {
        self.kids.iter_mut().flat_map(|k| match k {
            SpecKid::Step(s) => std::slice::from_mut(s).iter_mut(),
            SpecKid::Gate(_, _, s) => s.iter_mut(),
        })
    }
}

impl Spec {
    /// Direct dependencies from tree `t` (fragments + bypasses of attached controls).
    fn deps(&self, t: usize) -> BTreeSet<usize> {
        let mut out = BTreeSet::new();
        for s in self.trees[t].steps() {
            if let LeafKind::Frag(f) = s.leaf {
                out.insert(f);
            }
            for &c in s.mits.keys() {
                if let Some(b) = self.bypass[c] {
                    out.insert(b);
                }
            }
        }
        out
    }

    fn reaches(&self, from: usize, target: usize) -> bool {
        let mut seen = BTreeSet::from([from]);
        let mut stack = vec![from];
        while let Some(cur) = stack.pop() {
            if cur == target {
                return true;
            }
            for next in self.deps(cur) {
                if seen.insert(next) {
                    stack.push(next);
                }
            }
        }
        false
    }

    fn can_attach_mitigation(&self, tree: usize, ctrl: usize) -> bool {
        self.bypass[ctrl].is_none_or(|b| !self.reaches(b, tree))
    }

    fn can_set_bypass(&self, ctrl: usize, target: Option<usize>) -> bool {
        let Some(b) = target else {
            return true;
        };
        for t in 0..NUM_TREES {
            if self.trees[t].steps().any(|s| s.mits.contains_key(&ctrl)) && self.reaches(b, t) {
                return false;
            }
        }
        true
    }
}

fn ptid(t: usize) -> TreeId {
    tree_id(700 + t as u64)
}

fn proot(t: usize) -> NodeId {
    node_id(70_000 + t as u64)
}

fn gen_spec(rng: &mut Rng) -> Spec {
    let mut next_node = 0u64;
    let mut fresh = |t: usize| {
        next_node += 1;
        node_id(80_000 + 1000 * t as u64 + next_node)
    };
    let status: Vec<_> = (0..NUM_CONTROLS)
        .map(|_| ControlStatus::ALL[rng.next(3)])
        .collect();
    // Initial bypasses stay strictly acyclic: control k -> fragment 3 + (k % 3) or None;
    // only trees < 3 (0, 1, 2) get initial mitigations, and only fragments > t get referenced.
    let bypass: Vec<_> = (0..NUM_CONTROLS)
        .map(|k| rng.chance(70).then_some(3 + (k % 3)))
        .collect();
    let mut trees = Vec::new();
    for t in 0..NUM_TREES {
        let make_step = |rng: &mut Rng, id: NodeId| {
            let leaf = match rng.next(10) {
                0 => LeafKind::Own(None),
                1 | 2 => LeafKind::Shared(rng.next(2)),
                3 | 4 if t < NUM_TREES - 1 => {
                    let lo = t.max(1) + 1;
                    LeafKind::Frag(lo + rng.next(NUM_TREES - lo))
                }
                _ => LeafKind::Own(Some(rng.next(LEVELS.len()))),
            };
            let mut mits = BTreeMap::new();
            if t < 3 && rng.chance(45) {
                let c = rng.next(NUM_CONTROLS);
                mits.insert(c, rng.next(LEVELS.len()));
                if rng.chance(25) {
                    let c2 = rng.next(NUM_CONTROLS);
                    mits.insert(c2, rng.next(LEVELS.len()));
                }
            }
            SpecStep {
                id,
                leaf,
                cond: rng.chance(20).then(|| rng.next(2)),
                mits,
            }
        };
        let mut kids = Vec::new();
        for _ in 0..1 + rng.next(3) {
            if rng.chance(30) {
                let gid = fresh(t);
                let steps = (0..1 + rng.next(2))
                    .map(|_| {
                        let id = fresh(t);
                        make_step(rng, id)
                    })
                    .collect();
                kids.push(SpecKid::Gate(gid, GATES[rng.next(3)], steps));
            } else {
                let id = fresh(t);
                kids.push(SpecKid::Step(make_step(rng, id)));
            }
        }
        trees.push(SpecTree {
            gate: GATES[rng.next(3)],
            kids,
            cond: ((2..NUM_TREES).contains(&t) && rng.chance(25)).then(|| rng.next(2)),
        });
    }
    Spec {
        trees,
        shared: (0..2)
            .map(|_| (!rng.chance(15)).then(|| rng.next(LEVELS.len())))
            .collect(),
        present: (0..2).map(|_| !rng.chance(30)).collect(),
        status,
        bypass,
    }
}

fn build_project(m: &Arc<Methodology>, spec: &Spec) -> Project {
    let mut p = Project::new(Arc::clone(m));
    let pr = || Provenance::human("prop", 1);
    let level = |i: usize| feas(m, LEVELS[i]);
    let safety = cat(m, "safety");
    let severe = lvl(m, safety, "severe");
    let category = threat_cat(m, "spoofing");

    p.apply(Op::AddAsset {
        id: asset_id(1),
        name: "ECU".into(),
        properties: m.all_properties(),
        provenance: pr(),
    })
    .expect("asset");
    p.apply(Op::AddDamageScenario {
        id: damage_id(1),
        name: "Crash".into(),
        impacts: vec![(safety, severe)],
        provenance: pr(),
    })
    .expect("damage");
    for (c, &present) in spec.present.iter().enumerate() {
        p.apply(Op::AddCondition {
            id: condition_id(c as u64 + 1),
            name: "Cond".into(),
            description: "".into(),
            present,
            provenance: pr(),
        })
        .expect("cond");
    }
    for (s, rating) in spec.shared.iter().enumerate() {
        let sid = shared_step_id(s as u64 + 1);
        p.apply(Op::AddSharedStep {
            id: sid,
            name: "Shared".into(),
            provenance: pr(),
        })
        .expect("shared");
        p.apply(Op::SetStepFeasibility {
            target: StepTarget::Shared(sid),
            level: rating.map(level),
            provenance: pr(),
        })
        .expect("shared feas");
    }
    for (k, &status) in spec.status.iter().enumerate() {
        let cid = control_id(k as u64 + 1);
        p.apply(Op::AddSecurityControl {
            id: cid,
            name: "Ctrl".into(),
            provenance: pr(),
        })
        .expect("ctrl");
        p.apply(Op::SetControlStatus {
            control: cid,
            status,
            provenance: pr(),
        })
        .expect("status");
    }
    let mut deferred = Vec::new();
    for (t, st) in spec.trees.iter().enumerate() {
        let mut at = AttackTree::new(proot(t), Node::gate(st.gate, "Root")).expect("tree");
        let mut rated = Vec::new();
        let tid = ptid(t);
        let mut push_step = |at: &mut AttackTree, parent, s: &SpecStep| {
            at.add_child(parent, s.id, Node::step("Step"))
                .expect("step");
            match s.leaf {
                LeafKind::Own(Some(l)) => rated.push((s.id, level(l))),
                LeafKind::Own(None) => {}
                LeafKind::Shared(sh) => deferred.push(Op::UseSharedStep {
                    tree: tid,
                    node: s.id,
                    step: shared_step_id(sh as u64 + 1),
                    provenance: pr(),
                }),
                LeafKind::Frag(f) => deferred.push(Op::UseFragment {
                    tree: tid,
                    node: s.id,
                    fragment: ptid(f),
                    provenance: pr(),
                }),
            }
            if let Some(c) = s.cond {
                deferred.push(Op::NeedStepCondition {
                    tree: tid,
                    node: s.id,
                    condition: condition_id(c as u64 + 1),
                    provenance: pr(),
                });
            }
            for (&c, &l) in &s.mits {
                deferred.push(Op::SetStepMitigation {
                    tree: tid,
                    node: s.id,
                    control: control_id(c as u64 + 1),
                    feasibility: Some(level(l)),
                    provenance: pr(),
                });
            }
        };
        let root_idx = at.root();
        for kid in &st.kids {
            match kid {
                SpecKid::Step(s) => push_step(&mut at, root_idx, s),
                SpecKid::Gate(gid, kind, steps) => {
                    let gidx = at
                        .add_child(root_idx, *gid, Node::gate(*kind, "Gate"))
                        .expect("gate");
                    for s in steps {
                        push_step(&mut at, gidx, s);
                    }
                }
            }
        }
        p.apply(Op::AddAttackTree {
            role: if t < NUM_THREATS {
                TreeRole::Threat
            } else {
                TreeRole::Fragment
            },
            id: tid,
            tree: at,
            step_feasibility: rated,
            provenance: pr(),
        })
        .expect("add tree");
        if let Some(c) = st.cond {
            deferred.push(Op::NeedCondition {
                element: ElementRef::Tree(tid),
                condition: condition_id(c as u64 + 1),
                provenance: pr(),
            });
        }
    }
    for (k, &b) in spec.bypass.iter().enumerate() {
        if let Some(bt) = b {
            deferred.push(Op::SetControlBypass {
                control: control_id(k as u64 + 1),
                tree: Some(ptid(bt)),
                provenance: pr(),
            });
        }
    }
    for op in deferred {
        p.apply(op).expect("deferred");
    }
    for t in 0..NUM_THREATS {
        let th = threat_id(t as u64 + 1);
        p.apply(Op::AddThreatScenario {
            id: th,
            name: "Threat".into(),
            asset: asset_id(1),
            category,
            catalog_refs: vec![],
            damages: vec![damage_id(1)],
            provenance: pr(),
        })
        .expect("threat");
        p.apply(Op::SetThreatAttackTree {
            threat: th,
            tree: Some(ptid(t)),
            provenance: pr(),
        })
        .expect("assign");
    }
    p
}

fn check_oracle_and_ceilings(p: &Project, cache: &ProjectEvalCache, spec: &Spec) {
    let mut oracle = fragment_oracle::Oracle::new(p);
    for t in 0..NUM_TREES {
        let tid = ptid(t);
        for residual in [false, true] {
            let live = if residual {
                p.evaluate_residual_tree(tid)
            } else {
                p.evaluate_tree(tid)
            };
            let cached = if residual {
                cache.evaluate_residual_tree(p, tid).cloned()
            } else {
                cache.evaluate_tree(p, tid).cloned()
            };
            assert_eq!(live, cached, "tree {t} residual {residual}");

            let want = oracle.root(tid, residual);
            match (&live, &want) {
                (Ok(eval), Ok(Lifted::Value(v))) => assert_eq!(eval.root_value(), v),
                (Err(ModelError::TreeNotApplicable(_)), Ok(Lifted::Excluded))
                | (Err(ModelError::MissingLeafValue(_)), Err(_)) => {}
                _ => panic!("oracle mismatch tree {t} residual {residual}: {live:?} vs {want:?}"),
            }
        }
        // Check residual_step_feasibility and step_ceilings against oracle on every step.
        let tree_idx = p.attack_tree_idx(&tid).expect("tree idx");
        let tree = p.attack_tree(tree_idx).expect("tree");
        for step in spec.trees[t].steps() {
            let node_idx = tree.idx_of(&step.id).expect("node idx");
            let mut expected_ceilings = Vec::new();
            let mut missing_bypass = false;
            let mut excluded_bypass = false;
            for (&c, &l) in &step.mits {
                if !spec.status[c].counts_for_residual() {
                    continue;
                }
                let cid = control_id(c as u64 + 1);
                if let Some(b) = spec.bypass[c] {
                    match oracle.root(ptid(b), true) {
                        Ok(Lifted::Value(bv)) => expected_ceilings.push((bv, cid, Some(ptid(b)))),
                        Ok(Lifted::Excluded) => excluded_bypass = true,
                        Err(_) => missing_bypass = true,
                    }
                } else {
                    expected_ceilings.push((feas(p.methodology(), LEVELS[l]), cid, None));
                }
            }
            expected_ceilings.sort_unstable_by_key(|&(f, cid, _)| (f, cid));
            let got_ceilings: Vec<_> = p
                .step_ceilings(tree_idx, node_idx)
                .into_iter()
                .map(|c| (c.feasibility(), c.control_id(), c.bypass()))
                .collect();
            assert_eq!(
                got_ceilings, expected_ceilings,
                "ceilings tree {t} {:?}",
                step.id
            );

            let want_res = if missing_bypass || excluded_bypass {
                None
            } else {
                p.effective_step_feasibility(tree_idx, node_idx).map(|own| {
                    expected_ceilings
                        .iter()
                        .map(|&(f, _, _)| f)
                        .fold(own, std::cmp::min)
                })
            };
            assert_eq!(
                p.residual_step_feasibility(tree_idx, node_idx),
                want_res,
                "residual_step_feasibility tree {t} {:?}",
                step.id
            );
        }
    }
    for t in 0..NUM_THREATS {
        let th = threat_id(t as u64 + 1);
        for residual in [false, true] {
            let live = if residual {
                p.evaluate_residual_risk(th)
            } else {
                p.evaluate_risk(th)
            };
            let cached = if residual {
                cache.evaluate_residual_risk(p, th).cloned()
            } else {
                cache.evaluate_risk(p, th).cloned()
            };
            assert_eq!(live, cached, "threat {t} residual {residual}");
            match (live, oracle.root(ptid(t), residual)) {
                (Ok(r), Ok(Lifted::Value(v))) => assert_eq!(r.feasibility(), v),
                (Err(ModelError::ThreatNotApplicable(_)), Ok(Lifted::Excluded))
                | (Err(ModelError::MissingLeafValue(_)), Err(_)) => {}
                other => panic!("risk oracle mismatch threat {t} residual {residual}: {other:?}"),
            }
        }
    }
    assert_eq!(
        cache,
        &ProjectEvalCache::new(p),
        "incremental cache != fresh"
    );
}

fn gen_op(m: &Methodology, rng: &mut Rng, spec: &Spec) -> (Op, Spec) {
    let pr = Provenance::human("prop", 2);
    let level = |i: usize| feas(m, LEVELS[i]);
    let mut next = spec.clone();
    loop {
        let t = rng.next(NUM_TREES);
        let count = spec.trees[t].steps().count();
        let pick = rng.next(count);
        let step_id = spec.trees[t].steps().nth(pick).expect("step").id;
        let tid = ptid(t);
        match rng.next(7) {
            0 => {
                let c = rng.next(2);
                next.present[c] = !next.present[c];
                return (
                    Op::SetConditionPresent {
                        id: condition_id(c as u64 + 1),
                        present: next.present[c],
                        provenance: pr,
                    },
                    next,
                );
            }
            1 => {
                let k = rng.next(NUM_CONTROLS);
                next.status[k] = ControlStatus::ALL[rng.next(3)];
                return (
                    Op::SetControlStatus {
                        control: control_id(k as u64 + 1),
                        status: next.status[k],
                        provenance: pr,
                    },
                    next,
                );
            }
            2 => {
                let k = rng.next(NUM_CONTROLS);
                let target = rng.chance(75).then(|| 2 + rng.next(NUM_TREES - 2));
                if target != spec.bypass[k] && spec.can_set_bypass(k, target) {
                    next.bypass[k] = target;
                    return (
                        Op::SetControlBypass {
                            control: control_id(k as u64 + 1),
                            tree: target.map(ptid),
                            provenance: pr,
                        },
                        next,
                    );
                }
            }
            3 => {
                let k = rng.next(NUM_CONTROLS);
                let step = next.trees[t].steps_mut().nth(pick).expect("step");
                if step.mits.contains_key(&k) && rng.chance(35) {
                    step.mits.remove(&k);
                    return (
                        Op::SetStepMitigation {
                            tree: tid,
                            node: step_id,
                            control: control_id(k as u64 + 1),
                            feasibility: None,
                            provenance: pr,
                        },
                        next,
                    );
                } else if spec.can_attach_mitigation(t, k) {
                    let l = rng.next(LEVELS.len());
                    step.mits.insert(k, l);
                    return (
                        Op::SetStepMitigation {
                            tree: tid,
                            node: step_id,
                            control: control_id(k as u64 + 1),
                            feasibility: Some(level(l)),
                            provenance: pr,
                        },
                        next,
                    );
                }
            }
            4 => {
                let s = rng.next(2);
                next.shared[s] = (!rng.chance(25)).then(|| rng.next(LEVELS.len()));
                return (
                    Op::SetStepFeasibility {
                        target: StepTarget::Shared(shared_step_id(s as u64 + 1)),
                        level: next.shared[s].map(level),
                        provenance: pr,
                    },
                    next,
                );
            }
            5 => {
                let step = next.trees[t].steps_mut().nth(pick).expect("step");
                if matches!(step.leaf, LeafKind::Own(_)) {
                    let l = (!rng.chance(25)).then(|| rng.next(LEVELS.len()));
                    step.leaf = LeafKind::Own(l);
                    return (
                        Op::SetStepFeasibility {
                            target: StepTarget::Step(tid, step_id),
                            level: l.map(level),
                            provenance: pr,
                        },
                        next,
                    );
                }
            }
            6 => {
                let leaf = spec.trees[t].steps().nth(pick).expect("step").leaf.clone();
                match leaf {
                    LeafKind::Frag(_) => {
                        next.trees[t].steps_mut().nth(pick).expect("step").leaf =
                            LeafKind::Own(None);
                        return (
                            Op::UnuseFragment {
                                tree: tid,
                                node: step_id,
                                provenance: pr,
                            },
                            next,
                        );
                    }
                    LeafKind::Own(None) => {
                        let f = 2 + rng.next(NUM_TREES - 2);
                        if !spec.reaches(f, t) {
                            next.trees[t].steps_mut().nth(pick).expect("step").leaf =
                                LeafKind::Frag(f);
                            return (
                                Op::UseFragment {
                                    tree: tid,
                                    node: step_id,
                                    fragment: ptid(f),
                                    provenance: pr,
                                },
                                next,
                            );
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
}

proptest::proptest! {
    #![proptest_config(proptest::prelude::ProptestConfig::with_cases(64))]

    #[test]
    fn bypass_cache_matches_fresh_project_and_oracle(seed in proptest::prelude::any::<u64>()) {
        let m = builtin();
        let mut rng = Rng(seed);
        let mut spec = gen_spec(&mut rng);
        let mut p = build_project(&m, &spec);
        let mut cache = ProjectEvalCache::new(&p);
        check_oracle_and_ceilings(&p, &cache, &spec);
        for _ in 0..10 {
            let (op, next) = gen_op(&m, &mut rng, &spec);
            let inv = p
                .apply_cached(op.clone(), &mut cache)
                .unwrap_or_else(|e| panic!("{op:?}: {e}"));
            check_oracle_and_ceilings(&p, &cache, &next);
            p.apply_cached(inv, &mut cache).expect("undo");
            check_oracle_and_ceilings(&p, &cache, &spec);
            p.apply_cached(op, &mut cache).expect("redo");
            check_oracle_and_ceilings(&p, &cache, &next);
            spec = next;
        }
        let reloaded = Project::load(save(&p).as_slice(), Arc::clone(&m)).expect("load");
        assert_eq!(&ProjectEvalCache::new(&reloaded), &cache);
    }
}
