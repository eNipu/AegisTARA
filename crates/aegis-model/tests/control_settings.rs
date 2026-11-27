//! Control settings on components and interfaces, and attack-type rules
//! (job 123, CFG-03..CFG-05, CFG-08; decision D26).
//!
//! Covers:
//! - exact inverses for `ApplyControl`, `UnapplyControl`, and `SetControlRule`;
//! - interface-only reach: component settings reach steps via the component or
//!   any of its interfaces, interface settings reach only steps via that
//!   interface, and parent component settings never flow to child components;
//! - explain-why (`Project::step_ceilings`) recording manual mitigations and
//!   matching control rules with the deciding (lowest feasibility) ceiling
//!   first;
//! - referential integrity blockers and decode/commit caps
//!   (`MAX_CONTROL_TARGETS`, `MAX_CONTROL_RULES`);
//! - `SecurityControlUnused` and `ReduceTreatmentWithoutControls` lints
//!   accounting for applied controls and active rule ceilings;
//! - snapshot and op-log round trips, omitting empty `targets` and `rules`;
//! - property tests: `residual <= inherent`, order independence across random
//!   permutations of settings/mitigations/via/attack-type ops, and incremental
//!   `ProjectEvalCache` agreement with full evaluation.

mod common;

use std::sync::Arc;

use aegis_model::{
    AssetId, AttackTree, AttackType, CeilingSource, ComponentId, ControlId, ControlRule, DamageId,
    FeasibilityLevel, GateKind, InterfaceId, ItemRef, LintCode, MAX_CONTROL_TARGETS, Methodology,
    ModelError, Node, NodeId, Op, OpCodec, OpLogReader, OpLogWriter, Project, ProjectEvalCache,
    Provenance, RiskTreatment, StepTarget, ThreatId, TreeId,
};
use proptest::prelude::*;

fn methodology() -> Arc<Methodology> {
    Arc::new(Methodology::iso21434_r155().expect("builtin"))
}

struct ReachFx {
    project: Project,
    ecu: ComponentId,
    sub_ecu: ComponentId,
    jtag: InterfaceId,
    can: InterfaceId,
    spi: InterfaceId,
    tree: TreeId,
    s_ecu: NodeId,
    s_jtag: NodeId,
    s_can: NodeId,
    s_sub: NodeId,
    s_spi: NodeId,
    f_high: FeasibilityLevel,
    f_medium: FeasibilityLevel,
    f_low: FeasibilityLevel,
    f_very_low: FeasibilityLevel,
    t_debug: AttackType,
    t_inject: AttackType,
}

fn reach_fixture() -> ReachFx {
    let m = methodology();
    let mut project = Project::new(Arc::clone(&m));
    let f_very_low = m.feasibility_level("very-low").expect("very-low");
    let f_low = m.feasibility_level("low").expect("low");
    let f_medium = m.feasibility_level("medium").expect("medium");
    let f_high = m.feasibility_level("high").expect("high");
    let t_debug = m.attack_type("debug-access").expect("debug-access");
    let t_inject = m
        .attack_type("local-bus-injection")
        .expect("local-bus-injection");
    let k_debug = m.interface_kind("debug").expect("debug");
    let k_can = m.interface_kind("can").expect("can");
    let k_spi = k_debug;

    let ecu = ComponentId::new_v7();
    let sub_ecu = ComponentId::new_v7();
    let jtag = InterfaceId::new_v7();
    let can = InterfaceId::new_v7();
    let spi = InterfaceId::new_v7();
    let tree = TreeId::new_v7();
    let root = NodeId::new_v7();
    let s_ecu = NodeId::new_v7();
    let s_jtag = NodeId::new_v7();
    let s_can = NodeId::new_v7();
    let s_sub = NodeId::new_v7();
    let s_spi = NodeId::new_v7();

    let mut t = AttackTree::new(root, Node::gate(GateKind::Or, "Goal")).expect("tree");
    for (id, label) in [
        (s_ecu, "Attack ECU directly"),
        (s_jtag, "Open JTAG"),
        (s_can, "Inject CAN frame"),
        (s_sub, "Attack sub-ECU"),
        (s_spi, "Probe sub-ECU SPI"),
    ] {
        t.add_child(t.root(), id, Node::step(label)).expect("step");
    }

    for op in [
        Op::AddComponent {
            id: ecu,
            name: "ADAS ECU".into(),
            parent: None,
            provenance: Provenance::default(),
        },
        Op::AddComponent {
            id: sub_ecu,
            name: "Safety MCU".into(),
            parent: Some(ecu),
            provenance: Provenance::default(),
        },
        Op::AddInterface {
            id: jtag,
            name: "JTAG".into(),
            component: ecu,
            kind: k_debug,
            provenance: Provenance::default(),
        },
        Op::AddInterface {
            id: can,
            name: "CAN1".into(),
            component: ecu,
            kind: k_can,
            provenance: Provenance::default(),
        },
        Op::AddInterface {
            id: spi,
            name: "SPI0".into(),
            component: sub_ecu,
            kind: k_spi,
            provenance: Provenance::default(),
        },
        Op::AddAttackTree {
            role: Default::default(),
            id: tree,
            tree: t,
            step_feasibility: vec![
                (s_ecu, f_high),
                (s_jtag, f_high),
                (s_can, f_high),
                (s_sub, f_high),
                (s_spi, f_high),
            ],
            provenance: Provenance::default(),
        },
        Op::SetStepVia {
            target: StepTarget::Step(tree, s_ecu),
            via: Some(ItemRef::Component(ecu)),
            provenance: Provenance::default(),
        },
        Op::SetStepVia {
            target: StepTarget::Step(tree, s_jtag),
            via: Some(ItemRef::Interface(jtag)),
            provenance: Provenance::default(),
        },
        Op::SetStepVia {
            target: StepTarget::Step(tree, s_can),
            via: Some(ItemRef::Interface(can)),
            provenance: Provenance::default(),
        },
        Op::SetStepVia {
            target: StepTarget::Step(tree, s_sub),
            via: Some(ItemRef::Component(sub_ecu)),
            provenance: Provenance::default(),
        },
        Op::SetStepVia {
            target: StepTarget::Step(tree, s_spi),
            via: Some(ItemRef::Interface(spi)),
            provenance: Provenance::default(),
        },
        Op::SetStepAttackType {
            target: StepTarget::Step(tree, s_ecu),
            attack_type: Some(t_debug),
            provenance: Provenance::default(),
        },
        Op::SetStepAttackType {
            target: StepTarget::Step(tree, s_jtag),
            attack_type: Some(t_debug),
            provenance: Provenance::default(),
        },
        Op::SetStepAttackType {
            target: StepTarget::Step(tree, s_can),
            attack_type: Some(t_inject),
            provenance: Provenance::default(),
        },
        Op::SetStepAttackType {
            target: StepTarget::Step(tree, s_sub),
            attack_type: Some(t_debug),
            provenance: Provenance::default(),
        },
        Op::SetStepAttackType {
            target: StepTarget::Step(tree, s_spi),
            attack_type: Some(t_debug),
            provenance: Provenance::default(),
        },
    ] {
        project.apply(op).expect("setup");
    }

    ReachFx {
        project,
        ecu,
        sub_ecu,
        jtag,
        can,
        spi,
        tree,
        s_ecu,
        s_jtag,
        s_can,
        s_sub,
        s_spi,
        f_high,
        f_medium,
        f_low,
        f_very_low,
        t_debug,
        t_inject,
    }
}

fn residual_of(fx: &ReachFx, step: NodeId) -> FeasibilityLevel {
    let t_idx = fx.project.attack_tree_idx(&fx.tree).expect("tree");
    let s_idx = fx
        .project
        .attack_tree(t_idx)
        .and_then(|t| t.idx_of(&step))
        .expect("step");
    fx.project
        .residual_step_feasibility(t_idx, s_idx)
        .expect("feasibility")
}

#[test]
fn apply_unapply_and_set_rule_have_exact_inverses() {
    let mut fx = reach_fixture();
    let ctrl = ControlId::new_v7();
    let u0 = fx
        .project
        .apply(Op::AddSecurityControl {
            id: ctrl,
            name: "JTAG locked".into(),
            provenance: Provenance::default(),
        })
        .expect("add");
    let after_add = fx.project.clone();

    let u1 = fx
        .project
        .apply(Op::ApplyControl {
            control: ctrl,
            target: ItemRef::Interface(fx.jtag),
            provenance: Provenance::default(),
        })
        .expect("apply jtag");
    let u2 = fx
        .project
        .apply(Op::ApplyControl {
            control: ctrl,
            target: ItemRef::Component(fx.ecu),
            provenance: Provenance::default(),
        })
        .expect("apply ecu");
    let u3 = fx
        .project
        .apply(Op::SetControlRule {
            control: ctrl,
            attack_type: fx.t_debug,
            ceiling: Some(fx.f_low),
            provenance: Provenance::default(),
        })
        .expect("rule 1");
    let u4 = fx
        .project
        .apply(Op::SetControlRule {
            control: ctrl,
            attack_type: fx.t_debug,
            ceiling: Some(fx.f_very_low),
            provenance: Provenance::default(),
        })
        .expect("rule overwrite");

    let c_idx = fx.project.security_control_idx(&ctrl).expect("ctrl");
    let control = fx.project.security_control(c_idx).expect("ctrl");
    assert_eq!(control.target_count(), 2);
    assert_eq!(
        control.rules(),
        &[ControlRule::new(fx.t_debug, fx.f_very_low)]
    );
    assert_eq!(control.rule_for(fx.t_debug), Some(fx.f_very_low));
    assert_eq!(
        fx.project.control_targets(c_idx).collect::<Vec<_>>(),
        vec![ItemRef::Component(fx.ecu), ItemRef::Interface(fx.jtag)]
    );
    assert_eq!(
        fx.project
            .controls_applied_to(ItemRef::Interface(fx.jtag))
            .map(|(idx, id, _)| (idx, id))
            .collect::<Vec<_>>(),
        vec![(c_idx, ctrl)]
    );

    let u5 = fx
        .project
        .apply(Op::UnapplyControl {
            control: ctrl,
            target: ItemRef::Interface(fx.jtag),
            provenance: Provenance::default(),
        })
        .expect("unapply");
    let u6 = fx
        .project
        .apply(Op::SetControlRule {
            control: ctrl,
            attack_type: fx.t_debug,
            ceiling: None,
            provenance: Provenance::default(),
        })
        .expect("clear rule");

    for inv in [u6, u5, u4, u3, u2, u1] {
        fx.project.apply(inv).expect("undo");
    }
    assert_eq!(fx.project, after_add);
    fx.project.apply(u0).expect("undo add");
}

#[test]
fn interface_only_reach_and_no_parent_component_inheritance() {
    let mut fx = reach_fixture();
    let c_if = ControlId::new_v7();
    let c_comp = ControlId::new_v7();

    for (id, name) in [(c_if, "JTAG fuse"), (c_comp, "ECU lockdown")] {
        fx.project
            .apply(Op::AddSecurityControl {
                id,
                name: name.into(),
                provenance: Provenance::default(),
            })
            .expect("add");
    }

    // 1. Control applied to an interface reaches ONLY steps via that interface.
    fx.project
        .apply(Op::ApplyControl {
            control: c_if,
            target: ItemRef::Interface(fx.jtag),
            provenance: Provenance::default(),
        })
        .expect("apply jtag");
    fx.project
        .apply(Op::SetControlRule {
            control: c_if,
            attack_type: fx.t_debug,
            ceiling: Some(fx.f_very_low),
            provenance: Provenance::default(),
        })
        .expect("rule");

    assert_eq!(residual_of(&fx, fx.s_jtag), fx.f_very_low);
    assert_eq!(residual_of(&fx, fx.s_ecu), fx.f_high);
    assert_eq!(residual_of(&fx, fx.s_can), fx.f_high);
    assert_eq!(residual_of(&fx, fx.s_sub), fx.f_high);
    assert_eq!(residual_of(&fx, fx.s_spi), fx.f_high);
    assert_eq!(
        fx.project
            .controls_applied_to(ItemRef::Interface(fx.can))
            .count(),
        0
    );

    // 2. Control applied to a component reaches steps via that component AND
    //    steps via any of its interfaces (when the rule matches their attack
    //    type), but NEVER steps via a child component or the child's interfaces.
    fx.project
        .apply(Op::ApplyControl {
            control: c_comp,
            target: ItemRef::Component(fx.ecu),
            provenance: Provenance::default(),
        })
        .expect("apply ecu");
    fx.project
        .apply(Op::SetControlRule {
            control: c_comp,
            attack_type: fx.t_debug,
            ceiling: Some(fx.f_low),
            provenance: Provenance::default(),
        })
        .expect("rule debug");
    fx.project
        .apply(Op::SetControlRule {
            control: c_comp,
            attack_type: fx.t_inject,
            ceiling: Some(fx.f_medium),
            provenance: Provenance::default(),
        })
        .expect("rule inject");

    // s_ecu is via Component(ecu) with t_debug -> lowered to f_low.
    assert_eq!(residual_of(&fx, fx.s_ecu), fx.f_low);
    // s_jtag has both c_if (f_very_low) and c_comp (f_low) -> min is f_very_low.
    assert_eq!(residual_of(&fx, fx.s_jtag), fx.f_very_low);
    // s_can is via Interface(can) on ecu with t_inject -> lowered to f_medium by c_comp.
    assert_eq!(residual_of(&fx, fx.s_can), fx.f_medium);
    // Child component and its interface are untouched!
    assert_eq!(residual_of(&fx, fx.s_sub), fx.f_high);
    assert_eq!(residual_of(&fx, fx.s_spi), fx.f_high);
}

#[test]
fn explain_why_step_ceilings_puts_deciding_ceiling_first() {
    let mut fx = reach_fixture();
    let c_manual = ControlId::new_v7();
    let c_comp = ControlId::new_v7();
    let c_if = ControlId::new_v7();

    for (id, name) in [
        (c_manual, "Manual review"),
        (c_comp, "ECU hardening"),
        (c_if, "JTAG password"),
    ] {
        fx.project
            .apply(Op::AddSecurityControl {
                id,
                name: name.into(),
                provenance: Provenance::default(),
            })
            .expect("add");
    }

    // Manual mitigation at f_medium, component rule at f_very_low, interface rule at f_low.
    for op in [
        Op::SetStepMitigation {
            tree: fx.tree,
            node: fx.s_jtag,
            control: c_manual,
            feasibility: Some(fx.f_medium),
            provenance: Provenance::default(),
        },
        Op::ApplyControl {
            control: c_comp,
            target: ItemRef::Component(fx.ecu),
            provenance: Provenance::default(),
        },
        Op::SetControlRule {
            control: c_comp,
            attack_type: fx.t_debug,
            ceiling: Some(fx.f_very_low),
            provenance: Provenance::default(),
        },
        Op::ApplyControl {
            control: c_if,
            target: ItemRef::Interface(fx.jtag),
            provenance: Provenance::default(),
        },
        Op::SetControlRule {
            control: c_if,
            attack_type: fx.t_debug,
            ceiling: Some(fx.f_low),
            provenance: Provenance::default(),
        },
    ] {
        fx.project.apply(op).expect("setup ceilings");
    }

    let t_idx = fx.project.attack_tree_idx(&fx.tree).expect("tree");
    let s_idx = fx
        .project
        .attack_tree(t_idx)
        .and_then(|t| t.idx_of(&fx.s_jtag))
        .expect("step");

    let ceilings = fx.project.step_ceilings(t_idx, s_idx);
    let summary: Vec<_> = ceilings
        .iter()
        .map(|c| (c.control_id(), c.feasibility(), c.source()))
        .collect();
    assert_eq!(
        summary,
        vec![
            (
                c_comp,
                fx.f_very_low,
                CeilingSource::ControlRule {
                    target: ItemRef::Component(fx.ecu),
                    attack_type: fx.t_debug,
                }
            ),
            (
                c_if,
                fx.f_low,
                CeilingSource::ControlRule {
                    target: ItemRef::Interface(fx.jtag),
                    attack_type: fx.t_debug,
                }
            ),
            (c_manual, fx.f_medium, CeilingSource::StepMitigation),
        ]
    );
    assert_eq!(
        fx.project.residual_step_feasibility(t_idx, s_idx),
        Some(ceilings[0].feasibility())
    );
}

#[test]
fn removal_blockers_and_caps_are_enforced() {
    let mut fx = reach_fixture();
    let ctrl = ControlId::new_v7();
    fx.project
        .apply(Op::AddSecurityControl {
            id: ctrl,
            name: "Guard".into(),
            provenance: Provenance::default(),
        })
        .expect("add");

    // Unknown targets and controls are rejected.
    let ghost_ctrl = ControlId::new_v7();
    assert_eq!(
        fx.project.apply(Op::ApplyControl {
            control: ghost_ctrl,
            target: ItemRef::Component(fx.ecu),
            provenance: Provenance::default(),
        }),
        Err(ModelError::UnknownSecurityControl(ghost_ctrl))
    );
    let ghost_comp = ComponentId::new_v7();
    assert_eq!(
        fx.project.apply(Op::ApplyControl {
            control: ctrl,
            target: ItemRef::Component(ghost_comp),
            provenance: Provenance::default(),
        }),
        Err(ModelError::UnknownComponent(ghost_comp))
    );
    let ghost_if = InterfaceId::new_v7();
    assert_eq!(
        fx.project.apply(Op::ApplyControl {
            control: ctrl,
            target: ItemRef::Interface(ghost_if),
            provenance: Provenance::default(),
        }),
        Err(ModelError::UnknownInterface(ghost_if))
    );

    // Unapplying a non-linked target fails.
    assert_eq!(
        fx.project.apply(Op::UnapplyControl {
            control: ctrl,
            target: ItemRef::Interface(fx.jtag),
            provenance: Provenance::default(),
        }),
        Err(ModelError::ControlTargetNotLinked {
            control: ctrl,
            target: ItemRef::Interface(fx.jtag),
        })
    );

    // Applying twice fails with DuplicateControlTarget.
    fx.project
        .apply(Op::ApplyControl {
            control: ctrl,
            target: ItemRef::Interface(fx.jtag),
            provenance: Provenance::default(),
        })
        .expect("apply");
    assert_eq!(
        fx.project.apply(Op::ApplyControl {
            control: ctrl,
            target: ItemRef::Interface(fx.jtag),
            provenance: Provenance::default(),
        }),
        Err(ModelError::DuplicateControlTarget {
            control: ctrl,
            target: ItemRef::Interface(fx.jtag),
        })
    );

    // Removing the control while it has targets fails.
    assert_eq!(
        fx.project.apply(Op::RemoveSecurityControl { id: ctrl }),
        Err(ModelError::SecurityControlHasTargets {
            control: ctrl,
            targets: 1,
        })
    );

    // Clear step via on jtag so only the control blocks removing jtag.
    fx.project
        .apply(Op::SetStepVia {
            target: StepTarget::Step(fx.tree, fx.s_jtag),
            via: None,
            provenance: Provenance::default(),
        })
        .expect("clear via");
    assert_eq!(
        fx.project.apply(Op::RemoveInterface { id: fx.jtag }),
        Err(ModelError::InterfaceHasControls {
            interface: fx.jtag,
            controls: 1,
        })
    );

    // Unapply from jtag, apply to sub_ecu, and check ComponentHasControls.
    fx.project
        .apply(Op::UnapplyControl {
            control: ctrl,
            target: ItemRef::Interface(fx.jtag),
            provenance: Provenance::default(),
        })
        .expect("unapply");
    for n in [fx.s_sub, fx.s_spi] {
        fx.project
            .apply(Op::SetStepVia {
                target: StepTarget::Step(fx.tree, n),
                via: None,
                provenance: Provenance::default(),
            })
            .expect("clear");
    }
    fx.project
        .apply(Op::RemoveInterface { id: fx.spi })
        .expect("remove spi");
    fx.project
        .apply(Op::ApplyControl {
            control: ctrl,
            target: ItemRef::Component(fx.sub_ecu),
            provenance: Provenance::default(),
        })
        .expect("apply sub_ecu");
    assert_eq!(
        fx.project.apply(Op::RemoveComponent { id: fx.sub_ecu }),
        Err(ModelError::ComponentHasControls {
            component: fx.sub_ecu,
            controls: 1,
        })
    );
    fx.project
        .apply(Op::UnapplyControl {
            control: ctrl,
            target: ItemRef::Component(fx.sub_ecu),
            provenance: Provenance::default(),
        })
        .expect("unapply sub_ecu");

    // Removing a control with rules fails with SecurityControlHasRules.
    fx.project
        .apply(Op::SetControlRule {
            control: ctrl,
            attack_type: fx.t_debug,
            ceiling: Some(fx.f_low),
            provenance: Provenance::default(),
        })
        .expect("set rule");
    assert_eq!(
        fx.project.apply(Op::RemoveSecurityControl { id: ctrl }),
        Err(ModelError::SecurityControlHasRules {
            control: ctrl,
            rules: 1,
        })
    );

    // Cap MAX_CONTROL_TARGETS (64).
    for i in 0..MAX_CONTROL_TARGETS {
        let cid = ComponentId::new_v7();
        fx.project
            .apply(Op::AddComponent {
                id: cid,
                name: format!("C{i}").into(),
                parent: None,
                provenance: Provenance::default(),
            })
            .expect("add comp");
        fx.project
            .apply(Op::ApplyControl {
                control: ctrl,
                target: ItemRef::Component(cid),
                provenance: Provenance::default(),
            })
            .expect("apply target");
    }
    let extra = ComponentId::new_v7();
    fx.project
        .apply(Op::AddComponent {
            id: extra,
            name: "Extra".into(),
            parent: None,
            provenance: Provenance::default(),
        })
        .expect("add extra");
    assert_eq!(
        fx.project.apply(Op::ApplyControl {
            control: ctrl,
            target: ItemRef::Component(extra),
            provenance: Provenance::default(),
        }),
        Err(ModelError::TooManyControlTargets {
            control: ctrl,
            max: MAX_CONTROL_TARGETS,
        })
    );
}

#[test]
fn lints_count_applied_controls_and_active_rule_ceilings() {
    let mut fx = reach_fixture();
    let m = fx.project.methodology().clone();
    let asset = AssetId::new_v7();
    let damage = DamageId::new_v7();
    let threat = ThreatId::new_v7();
    let ctrl = ControlId::new_v7();
    let cat = m.threat_categories().next().expect("cat");
    let ic = m.impact_categories().next().expect("ic");
    let il = m.levels(ic).last().expect("il");

    for op in [
        Op::AddAsset {
            id: asset,
            name: "Firmware".into(),
            properties: m.all_properties(),
            provenance: Provenance::default(),
        },
        Op::AddDamageScenario {
            id: damage,
            name: "Unexpected braking".into(),
            impacts: vec![(ic, il)],
            provenance: Provenance::default(),
        },
        Op::AddThreatScenario {
            id: threat,
            name: "Read debug memory".into(),
            asset,
            category: cat,
            catalog_refs: vec![],
            damages: vec![damage],
            provenance: Provenance::default(),
        },
        Op::SetThreatAttackTree {
            threat,
            tree: Some(fx.tree),
            provenance: Provenance::default(),
        },
        Op::SetRiskTreatment {
            threat,
            treatment: Some(RiskTreatment::Reduce),
            provenance: Provenance::default(),
        },
        Op::AddSecurityControl {
            id: ctrl,
            name: "Debug lock".into(),
            provenance: Provenance::default(),
        },
    ] {
        fx.project.apply(op).expect("setup");
    }

    let codes = |p: &Project| -> Vec<LintCode> { p.lint().into_iter().map(|f| f.code()).collect() };
    let initial = codes(&fx.project);
    assert!(initial.contains(&LintCode::SecurityControlUnused { control: ctrl }));
    assert!(initial.contains(&LintCode::ReduceTreatmentWithoutControls { threat }));

    // Applying the control to JTAG marks the control used (SecurityControlUnused clears),
    // even before it has any rules.
    fx.project
        .apply(Op::ApplyControl {
            control: ctrl,
            target: ItemRef::Interface(fx.jtag),
            provenance: Provenance::default(),
        })
        .expect("apply");
    let after_apply = codes(&fx.project);
    assert!(!after_apply.contains(&LintCode::SecurityControlUnused { control: ctrl }));
    assert!(after_apply.contains(&LintCode::ReduceTreatmentWithoutControls { threat }));

    // Adding a matching rule for debug-access caps s_jtag in the linked tree,
    // which satisfies ReduceTreatmentWithoutControls!
    fx.project
        .apply(Op::SetControlRule {
            control: ctrl,
            attack_type: fx.t_debug,
            ceiling: Some(fx.f_low),
            provenance: Provenance::default(),
        })
        .expect("rule");
    let after_rule = codes(&fx.project);
    assert!(!after_rule.contains(&LintCode::ReduceTreatmentWithoutControls { threat }));
}

/// A step with an attack type but no through item (or the reverse) can never
/// match a rule, so it is flagged, but only once some control carries rules.
#[test]
fn a_step_with_only_a_through_item_or_only_a_type_is_flagged_when_rules_exist() {
    let mut fx = reach_fixture();
    let ctrl = ControlId::new_v7();
    let incomplete = |p: &Project| -> Vec<NodeId> {
        p.lint()
            .into_iter()
            .filter_map(|f| match f.code() {
                LintCode::StepSettingsIncomplete { node, .. } => Some(node),
                _ => None,
            })
            .collect()
    };
    let set_type = |p: &mut Project, tree, node, attack_type| {
        p.apply(Op::SetStepAttackType {
            target: StepTarget::Step(tree, node),
            attack_type,
            provenance: Provenance::default(),
        })
        .expect("type");
    };
    let set_via = |p: &mut Project, tree, node, via| {
        p.apply(Op::SetStepVia {
            target: StepTarget::Step(tree, node),
            via,
            provenance: Provenance::default(),
        })
        .expect("via");
    };

    // No control has rules yet: a half-filled step is not a problem.
    set_type(&mut fx.project, fx.tree, fx.s_jtag, None);
    assert!(incomplete(&fx.project).is_empty());

    for op in [
        Op::AddSecurityControl {
            id: ctrl,
            name: "Debug lock".into(),
            provenance: Provenance::default(),
        },
        Op::SetControlRule {
            control: ctrl,
            attack_type: fx.t_debug,
            ceiling: Some(fx.f_low),
            provenance: Provenance::default(),
        },
    ] {
        fx.project.apply(op).expect("control");
    }
    // A through item without a type: flagged, and only that step.
    assert_eq!(incomplete(&fx.project), vec![fx.s_jtag]);

    // A type without a through item: flagged too.
    set_via(&mut fx.project, fx.tree, fx.s_can, None);
    let mut flagged = incomplete(&fx.project);
    flagged.sort();
    let mut want = vec![fx.s_jtag, fx.s_can];
    want.sort();
    assert_eq!(flagged, want);

    // Neither set is simply an unfilled step; both set is complete.
    set_via(&mut fx.project, fx.tree, fx.s_jtag, None);
    set_type(&mut fx.project, fx.tree, fx.s_can, None);
    assert!(incomplete(&fx.project).is_empty());
    set_via(
        &mut fx.project,
        fx.tree,
        fx.s_jtag,
        Some(ItemRef::Interface(fx.jtag)),
    );
    set_type(&mut fx.project, fx.tree, fx.s_jtag, Some(fx.t_debug));
    assert!(incomplete(&fx.project).is_empty());
}

/// The version diff reports a control as changed when it is applied to or
/// removed from an item or when a rule changes, whatever order the settings
/// were made in.
#[test]
fn the_diff_reports_applied_targets_and_rule_changes() {
    let fx = reach_fixture();
    let mut base = fx.project.clone();
    let ctrl = ControlId::new_v7();
    let add = Op::AddSecurityControl {
        id: ctrl,
        name: "Debug lock".into(),
        provenance: Provenance::default(),
    };
    base.apply(add.clone()).expect("add");
    let apply = |c: ControlId, i: ItemRef| Op::ApplyControl {
        control: c,
        target: i,
        provenance: Provenance::default(),
    };
    let rule = |level| Op::SetControlRule {
        control: ctrl,
        attack_type: fx.t_debug,
        ceiling: level,
        provenance: Provenance::default(),
    };

    let mut applied = base.clone();
    applied
        .apply(apply(ctrl, ItemRef::Interface(fx.jtag)))
        .expect("apply");
    assert_eq!(
        base.diff(&applied)
            .expect("diff")
            .security_controls()
            .modified(),
        [ctrl]
    );

    let mut ruled = base.clone();
    ruled.apply(rule(Some(fx.f_low))).expect("rule");
    assert_eq!(
        base.diff(&ruled)
            .expect("diff")
            .security_controls()
            .modified(),
        [ctrl]
    );

    // A different ceiling on the same rule is a change too.
    let mut ruled_lower = base.clone();
    ruled_lower.apply(rule(Some(fx.f_very_low))).expect("rule");
    assert_eq!(
        ruled
            .diff(&ruled_lower)
            .expect("diff")
            .security_controls()
            .modified(),
        [ctrl]
    );

    // Same settings made in another order: nothing to report.
    let (mut one, mut two) = (base.clone(), base.clone());
    for op in [
        apply(ctrl, ItemRef::Interface(fx.jtag)),
        apply(ctrl, ItemRef::Component(fx.ecu)),
        rule(Some(fx.f_low)),
    ] {
        one.apply(op).expect("one");
    }
    for op in [
        rule(Some(fx.f_low)),
        apply(ctrl, ItemRef::Component(fx.ecu)),
        apply(ctrl, ItemRef::Interface(fx.jtag)),
    ] {
        two.apply(op).expect("two");
    }
    let diff = one.diff(&two).expect("diff");
    assert!(diff.security_controls().is_empty());
}

#[test]
fn snapshot_and_op_log_round_trip_and_omit_when_empty() {
    let mut fx = reach_fixture();
    let ctrl = ControlId::new_v7();
    fx.project
        .apply(Op::AddSecurityControl {
            id: ctrl,
            name: "Setting".into(),
            provenance: Provenance::default(),
        })
        .expect("add");

    let empty_json = {
        let mut out = Vec::new();
        fx.project.save(&mut out).expect("save");
        String::from_utf8(out).expect("utf8")
    };
    assert!(!empty_json.contains("\"targets\""));
    assert!(!empty_json.contains("\"rules\""));

    let u_apply = fx
        .project
        .apply(Op::ApplyControl {
            control: ctrl,
            target: ItemRef::Interface(fx.jtag),
            provenance: Provenance::default(),
        })
        .expect("apply");
    let u_rule = fx
        .project
        .apply(Op::SetControlRule {
            control: ctrl,
            attack_type: fx.t_debug,
            ceiling: Some(fx.f_low),
            provenance: Provenance::default(),
        })
        .expect("rule");

    let mut out = Vec::new();
    fx.project.save(&mut out).expect("save");
    let text = String::from_utf8(out.clone()).expect("utf8");
    assert!(text.contains("\"targets\"") && text.contains("\"rules\""));
    let loaded = Project::load(out.as_slice(), methodology()).expect("load");
    assert_eq!(loaded, fx.project);

    let mut bytes = Vec::new();
    let mut log = OpLogWriter::create(&mut bytes, methodology()).expect("header");
    let ops_to_write = [
        Op::ApplyControl {
            control: ctrl,
            target: ItemRef::Component(fx.ecu),
            provenance: Provenance::default(),
        },
        u_apply,
        u_rule,
    ];
    for op in &ops_to_write {
        log.append(op).expect("append");
    }
    drop(log);
    let read_back: Vec<_> = OpLogReader::open(bytes.as_slice(), methodology())
        .expect("open")
        .map(|r| r.expect("record").op)
        .collect();
    assert_eq!(read_back, ops_to_write);

    let codec = OpCodec::new(methodology());
    for op in &ops_to_write {
        let encoded = codec.encode(op).expect("encode");
        assert_eq!(codec.decode(&encoded).expect("decode"), *op);
    }
}

// ---------------------------------------------------------------------------
// Property tests: residual <= inherent, order independence, cache agreement
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct SettingsCase {
    /// Inherent feasibility index (0..4) for each of the 4 steps.
    inherent: [usize; 4],
    /// Manual mitigation `Option<usize>` (0..4) for each of the 4 steps and 2 controls.
    manual: [[Option<usize>; 2]; 4],
    /// Whether each of the 2 controls is applied to each of the 3 targets
    /// (0 = Component(ecu), 1 = Interface(jtag), 2 = Interface(can)).
    applied: [[bool; 3]; 2],
    /// Rule ceiling `Option<usize>` (0..4) for each of the 2 controls and 3 attack types.
    rules: [[Option<usize>; 3]; 2],
    /// Via assignment `Option<usize>` (0..3) for each of the 4 steps.
    via: [Option<usize>; 4],
    /// Attack type `Option<usize>` (0..3) for each of the 4 steps.
    attack_type: [Option<usize>; 4],
    /// Seed used to permute the op application order.
    perm_seed: u64,
}

fn opt_idx(max: usize) -> impl Strategy<Value = Option<usize>> {
    prop_oneof![
        1 => Just(None),
        2 => (0..max).prop_map(Some),
    ]
}

fn settings_case() -> impl Strategy<Value = SettingsCase> {
    (
        [0usize..4, 0..4, 0..4, 0..4],
        [
            [opt_idx(4), opt_idx(4)],
            [opt_idx(4), opt_idx(4)],
            [opt_idx(4), opt_idx(4)],
            [opt_idx(4), opt_idx(4)],
        ],
        [
            [any::<bool>(), any::<bool>(), any::<bool>()],
            [any::<bool>(), any::<bool>(), any::<bool>()],
        ],
        [
            [opt_idx(4), opt_idx(4), opt_idx(4)],
            [opt_idx(4), opt_idx(4), opt_idx(4)],
        ],
        [opt_idx(3), opt_idx(3), opt_idx(3), opt_idx(3)],
        [opt_idx(3), opt_idx(3), opt_idx(3), opt_idx(3)],
        any::<u64>(),
    )
        .prop_map(
            |(inherent, manual, applied, rules, via, attack_type, perm_seed)| SettingsCase {
                inherent,
                manual,
                applied,
                rules,
                via,
                attack_type,
                perm_seed,
            },
        )
}

fn shuffle<T>(slice: &mut [T], mut state: u64) {
    for i in (1..slice.len()).rev() {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        let j = usize::try_from(state >> 32).unwrap_or(0) % (i + 1);
        slice.swap(i, j);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(96))]

    #[test]
    fn residual_never_exceeds_inherent_and_order_independent(case in settings_case()) {
        let m = methodology();
        let f_levels: Vec<_> = m.feasibility_levels().map(|(l, _, _)| l).collect();
        let a_types: Vec<_> = m.attack_types().take(3).collect();
        let k_debug = m.interface_kind("debug").expect("debug");
        let k_can = m.interface_kind("can").expect("can");
        let cat = m.threat_categories().next().expect("cat");
        let ic = m.impact_categories().next().expect("ic");
        let il = m.levels(ic).last().expect("il");

        let ecu = ComponentId::new_v7();
        let jtag = InterfaceId::new_v7();
        let can = InterfaceId::new_v7();
        let asset = AssetId::new_v7();
        let damage = DamageId::new_v7();
        let threat = ThreatId::new_v7();
        let tree = TreeId::new_v7();
        let root = NodeId::new_v7();
        let gate_and = NodeId::new_v7();
        let steps = [
            NodeId::new_v7(),
            NodeId::new_v7(),
            NodeId::new_v7(),
            NodeId::new_v7(),
        ];
        let controls = [ControlId::new_v7(), ControlId::new_v7()];

        let mut t = AttackTree::new(root, Node::gate(GateKind::Or, "Root")).expect("tree");
        let g_idx = t
            .add_child(t.root(), gate_and, Node::gate(GateKind::And, "Sub"))
            .expect("gate");
        t.add_child(t.root(), steps[0], Node::step("S0")).expect("s0");
        t.add_child(t.root(), steps[1], Node::step("S1")).expect("s1");
        t.add_child(g_idx, steps[2], Node::step("S2")).expect("s2");
        t.add_child(g_idx, steps[3], Node::step("S3")).expect("s3");

        let mut base = Project::new(Arc::clone(&m));
        for op in [
            Op::AddComponent {
                id: ecu,
                name: "ECU".into(),
                parent: None,
                provenance: Provenance::default(),
            },
            Op::AddInterface {
                id: jtag,
                name: "JTAG".into(),
                component: ecu,
                kind: k_debug,
                provenance: Provenance::default(),
            },
            Op::AddInterface {
                id: can,
                name: "CAN".into(),
                component: ecu,
                kind: k_can,
                provenance: Provenance::default(),
            },
            Op::AddSecurityControl {
                id: controls[0],
                name: "C0".into(),
                provenance: Provenance::default(),
            },
            Op::AddSecurityControl {
                id: controls[1],
                name: "C1".into(),
                provenance: Provenance::default(),
            },
            Op::AddAttackTree {
                role: Default::default(),
                id: tree,
                tree: t,
                step_feasibility: steps
                    .iter()
                    .enumerate()
                    .map(|(i, &s)| (s, f_levels[case.inherent[i]]))
                    .collect(),
                provenance: Provenance::default(),
            },
            Op::AddAsset {
                id: asset,
                name: "Asset".into(),
                properties: m.all_properties(),
                provenance: Provenance::default(),
            },
            Op::AddDamageScenario {
                id: damage,
                name: "Damage".into(),
                impacts: vec![(ic, il)],
                provenance: Provenance::default(),
            },
            Op::AddThreatScenario {
                id: threat,
                name: "Threat".into(),
                asset,
                category: cat,
                catalog_refs: vec![],
                damages: vec![damage],
                provenance: Provenance::default(),
            },
            Op::SetThreatAttackTree {
                threat,
                tree: Some(tree),
                provenance: Provenance::default(),
            },
        ] {
            base.apply(op).expect("base setup");
        }

        let targets = [
            ItemRef::Component(ecu),
            ItemRef::Interface(jtag),
            ItemRef::Interface(can),
        ];

        let mut ops = Vec::new();
        for (s_i, &step_id) in steps.iter().enumerate() {
            if let Some(v_i) = case.via[s_i] {
                ops.push(Op::SetStepVia {
                    target: StepTarget::Step(tree, step_id),
                    via: Some(targets[v_i]),
                    provenance: Provenance::default(),
                });
            }
            if let Some(t_i) = case.attack_type[s_i] {
                ops.push(Op::SetStepAttackType {
                    target: StepTarget::Step(tree, step_id),
                    attack_type: Some(a_types[t_i]),
                    provenance: Provenance::default(),
                });
            }
            for (c_i, &ctrl_id) in controls.iter().enumerate() {
                if let Some(f_i) = case.manual[s_i][c_i] {
                    ops.push(Op::SetStepMitigation {
                        tree,
                        node: step_id,
                        control: ctrl_id,
                        feasibility: Some(f_levels[f_i]),
                        provenance: Provenance::default(),
                    });
                }
            }
        }
        for (c_i, &ctrl_id) in controls.iter().enumerate() {
            for (t_i, &target) in targets.iter().enumerate() {
                if case.applied[c_i][t_i] {
                    ops.push(Op::ApplyControl {
                        control: ctrl_id,
                        target,
                        provenance: Provenance::default(),
                    });
                }
            }
            for (a_i, &attack_type) in a_types.iter().enumerate() {
                if let Some(f_i) = case.rules[c_i][a_i] {
                    ops.push(Op::SetControlRule {
                        control: ctrl_id,
                        attack_type,
                        ceiling: Some(f_levels[f_i]),
                        provenance: Provenance::default(),
                    });
                }
            }
        }

        let mut p_canonical = base.clone();
        for op in &ops {
            p_canonical.apply(op.clone()).expect("apply canonical");
        }

        let mut permuted = ops.clone();
        shuffle(&mut permuted, case.perm_seed);
        let mut p_permuted = base;
        let mut cache = ProjectEvalCache::new(&p_permuted);
        for op in &permuted {
            p_permuted.apply(op.clone()).expect("apply permuted");
            cache.update(&p_permuted, op);
        }

        let t_idx = p_canonical.attack_tree_idx(&tree).expect("tree");
        let attack_tree = p_canonical.attack_tree(t_idx).expect("tree");

        for &step_id in &steps {
            let s_idx = attack_tree.idx_of(&step_id).expect("step");
            let inh = p_canonical
                .step_feasibility(t_idx, s_idx)
                .expect("inherent");
            let res = p_canonical
                .residual_step_feasibility(t_idx, s_idx)
                .expect("residual");
            prop_assert!(res <= inh, "residual {res:?} > inherent {inh:?}");

            let ceilings = p_canonical.step_ceilings(t_idx, s_idx);
            let min_ceiling = ceilings.iter().map(|c| c.feasibility()).min();
            let expected = min_ceiling.map_or(inh, |c| inh.min(c));
            prop_assert_eq!(res, expected);
            if let Some(first) = ceilings.first() {
                prop_assert_eq!(Some(first.feasibility()), min_ceiling);
            }
            for pair in ceilings.windows(2) {
                prop_assert!(pair[0].feasibility() <= pair[1].feasibility());
            }

            prop_assert_eq!(
                p_permuted.residual_step_feasibility(t_idx, s_idx),
                Some(res)
            );
            prop_assert_eq!(p_permuted.step_ceilings(t_idx, s_idx), ceilings);
        }

        let inh_tree = p_canonical.evaluate_tree(tree).expect("inh tree");
        let res_tree = p_canonical.evaluate_residual_tree(tree).expect("res tree");
        for (n_idx, _, _) in attack_tree.iter() {
            prop_assert!(res_tree.get(n_idx) <= inh_tree.get(n_idx));
        }
        prop_assert_eq!(
            p_permuted.evaluate_residual_tree(tree),
            Ok(res_tree.clone())
        );
        prop_assert_eq!(
            cache.evaluate_residual_tree(&p_permuted, tree),
            Ok(&res_tree)
        );

        let inh_risk = p_canonical.evaluate_risk(threat).expect("inh risk");
        let res_risk = p_canonical.evaluate_residual_risk(threat).expect("res risk");
        prop_assert!(res_risk.feasibility() <= inh_risk.feasibility());
        prop_assert!(res_risk.max_risk() <= inh_risk.max_risk());
        prop_assert_eq!(p_permuted.evaluate_residual_risk(threat), Ok(res_risk.clone()));
        prop_assert_eq!(cache.evaluate_residual_risk(&p_permuted, threat), Ok(&res_risk));
    }
}
