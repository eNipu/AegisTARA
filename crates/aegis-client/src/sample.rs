//! A small example TARA, for demos and first runs.
//!
//! One asset, one damage scenario, two threat scenarios: one with an
//! attack tree (8 nodes, one step rated through attack potential, two
//! security controls that lower step feasibility so the residual risk
//! differs from the inherent one) and one rated directly. Every id is new,
//! so the ops can be applied to any empty project of the built-in
//! methodology.

use aegis_model::{
    AssetId, AttackTree, ControlId, DamageId, GateKind, Methodology, Node, NodeId, Op, Provenance,
    ThreatId, TreeId,
};

/// Builds the example as ops. Returns `None` if `m` lacks a vocabulary the
/// example needs (it is written for the built-in ISO/SAE 21434 pack).
#[must_use]
pub fn sample_ops(m: &Methodology, actor: &str, now_ms: u64) -> Option<Vec<Op>> {
    let p = Provenance::human(actor, now_ms);
    let integrity = m.property("integrity")?;
    let authenticity = m.property("authenticity")?;
    let safety = m.impact_category("safety")?;
    let severe = m.level(safety, "severe")?;
    let financial = m.impact_category("financial")?;
    let major = m.level(financial, "major")?;
    let spoofing = m.threat_category("spoofing")?;
    let tampering = m.threat_category("tampering")?;
    let [very_low, low, medium, high] =
        ["very-low", "low", "medium", "high"].map(|k| m.feasibility_level(k));
    let (very_low, low, medium, high) = (very_low?, low?, medium?, high?);

    let (asset, damage, spoof, tamper) = (
        AssetId::new_v7(),
        DamageId::new_v7(),
        ThreatId::new_v7(),
        ThreatId::new_v7(),
    );
    let (tree_id, tls, debug_lock) = (TreeId::new_v7(), ControlId::new_v7(), ControlId::new_v7());
    let ids: Vec<NodeId> = (0..8).map(|_| NodeId::new_v7()).collect();
    let (root, cellular, rogue, downgrade, backend, phish, bypass, debug) = (
        ids[0], ids[1], ids[2], ids[3], ids[4], ids[5], ids[6], ids[7],
    );

    let mut tree = AttackTree::new(
        root,
        Node::gate(GateKind::Or, "Execute a forged remote command"),
    )
    .ok()?;
    let r = tree.root();
    let cell = tree
        .add_child(
            r,
            cellular,
            Node::gate(GateKind::SeqAnd, "Attack the cellular link"),
        )
        .ok()?;
    tree.add_child(cell, rogue, Node::step("Set up a rogue base station"))
        .ok()?;
    tree.add_child(
        cell,
        downgrade,
        Node::step("Downgrade the connection to 2G"),
    )
    .ok()?;
    let back = tree
        .add_child(
            r,
            backend,
            Node::gate(GateKind::And, "Compromise the backend credentials"),
        )
        .ok()?;
    tree.add_child(back, phish, Node::step("Phish an operator account"))
        .ok()?;
    tree.add_child(back, bypass, Node::step("Bypass the multi-factor prompt"))
        .ok()?;
    tree.add_child(
        r,
        debug,
        Node::step("Exploit the debug port on the TCU board"),
    )
    .ok()?;

    let mut ops = vec![
        Op::AddAsset {
            id: asset,
            name: "Telematics control unit".into(),
            properties: [integrity, authenticity].into_iter().collect(),
            provenance: p.clone(),
        },
        Op::AddDamageScenario {
            id: damage,
            name: "Unauthorized remote command reaches the vehicle".into(),
            impacts: vec![(safety, severe), (financial, major)],
            provenance: p.clone(),
        },
        Op::AddThreatScenario {
            id: spoof,
            name: "Spoof a remote command to the TCU".into(),
            asset,
            category: spoofing,
            catalog_refs: Vec::new(),
            damages: vec![damage],
            provenance: p.clone(),
        },
        Op::AddAttackTree {
            id: tree_id,
            tree,
            step_feasibility: vec![
                (rogue, medium),
                (phish, low),
                (bypass, very_low),
                (debug, high),
            ],
            provenance: p.clone(),
        },
        Op::SetThreatAttackTree {
            threat: spoof,
            tree: Some(tree_id),
            provenance: p.clone(),
        },
        Op::AddSecurityControl {
            id: tls,
            name: "Mutual TLS with device certificates".into(),
            provenance: p.clone(),
        },
        Op::SetStepMitigation {
            tree: tree_id,
            node: downgrade,
            control: tls,
            feasibility: Some(very_low),
            provenance: p.clone(),
        },
        Op::AddSecurityControl {
            id: debug_lock,
            name: "Lock the debug port in production firmware".into(),
            provenance: p.clone(),
        },
        Op::SetStepMitigation {
            tree: tree_id,
            node: debug,
            control: debug_lock,
            feasibility: Some(low),
            provenance: p.clone(),
        },
        Op::AddThreatScenario {
            id: tamper,
            name: "Tamper with the TCU firmware image".into(),
            asset,
            category: tampering,
            catalog_refs: Vec::new(),
            damages: vec![damage],
            provenance: p.clone(),
        },
        Op::SetThreatFeasibility {
            threat: tamper,
            level: Some(low),
            provenance: p.clone(),
        },
    ];

    // The downgrade step is rated through attack potential: 1 + 3 + 3 + 1 + 4 = 12 points.
    for (factor, level) in [
        ("elapsed-time", "le-1w"),
        ("expertise", "proficient"),
        ("knowledge", "restricted"),
        ("window-of-opportunity", "easy"),
        ("equipment", "specialized"),
    ] {
        let f = m.feasibility_factor(factor)?;
        ops.push(Op::SetStepFactor {
            tree: tree_id,
            node: downgrade,
            factor: f,
            level: Some(m.factor_level(f, level)?),
            provenance: p.clone(),
        });
    }
    Some(ops)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use aegis_model::Project;

    use super::*;

    #[test]
    fn the_sample_applies_to_an_empty_project_and_has_the_advertised_shape() {
        let m = Arc::new(Methodology::iso21434_r155().expect("builtin"));
        let ops = sample_ops(&m, "demo", 1).expect("sample");
        let mut project = Project::new(Arc::clone(&m));
        for op in ops {
            project.apply(op).expect("valid");
        }
        assert_eq!(project.asset_count(), 1);
        assert_eq!(project.threat_scenario_count(), 2);
        assert_eq!(project.attack_tree_count(), 1);
        assert_eq!(project.security_control_count(), 2);
        assert!(
            project
                .lint()
                .iter()
                .all(|f| f.severity() != aegis_model::LintSeverity::Error)
        );

        // The control makes the residual risk differ from the inherent risk.
        let (_, spoof, _) = project.threat_scenarios().next().expect("threat");
        let inherent = project.evaluate_risk(spoof).expect("inherent").max_risk();
        let residual = project
            .evaluate_residual_risk(spoof)
            .expect("residual")
            .max_risk();
        // Inherent: the open debug port makes the whole tree easy (high).
        // Residual: both controls bring it down to low.
        let high = m.feasibility_level("high").expect("level");
        let low = m.feasibility_level("low").expect("level");
        assert_eq!(
            project
                .evaluate_tree(tree_of(&project))
                .expect("tree")
                .root_value(),
            &high
        );
        assert_eq!(
            project
                .evaluate_residual_tree(tree_of(&project))
                .expect("tree")
                .root_value(),
            &low
        );
        assert!(inherent.is_some() && residual.is_some());
        assert_ne!(inherent, residual);
    }

    fn tree_of(project: &Project) -> aegis_model::TreeId {
        let (idx, _, _) = project.attack_trees().next().expect("tree");
        project.attack_tree_id(idx).expect("id")
    }

    #[test]
    fn every_application_uses_fresh_ids() {
        let m = Methodology::iso21434_r155().expect("builtin");
        let a = sample_ops(&m, "x", 1).expect("sample");
        let b = sample_ops(&m, "x", 1).expect("sample");
        assert_ne!(a[0], b[0]);
    }
}
