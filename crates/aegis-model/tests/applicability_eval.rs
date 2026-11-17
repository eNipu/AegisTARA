//! Evaluation with applicability (job 134, slice 134b): excluded steps are
//! left out of attack tree and risk evaluation, cached or not.

mod common;

use aegis_model::{
    AssetId, AttackTree, ConditionId, Derivation, ElementRef, GateKind, ModelError, Node, NodeId,
    NodeIdx, Op, Project, ProjectEvalCache, Provenance, ThreatId, TreeId,
};
use common::{asset_id, builtin, cat, condition_id, control_id, damage_id, feas, goal_id, lvl};
use common::{node_id, threat_id, tree_id};

type Res<T = ()> = Result<T, Box<dyn std::error::Error>>;

fn prov() -> Provenance {
    Provenance::human("alice", 1_700_000_000_000)
}

/// A project with one asset, one damage (safety severe), one threat linked
/// to the damage, and condition `COND` (absent unless `present`).
struct Fx {
    p: Project,
    asset: AssetId,
    threat: ThreatId,
    cond: ConditionId,
}

const COND: u64 = 900;

fn fixture(present: bool, with_condition: bool) -> Res<Fx> {
    let m = builtin();
    let mut p = Project::new(m.clone());
    let (asset, damage, threat, cond) =
        (asset_id(1), damage_id(2), threat_id(3), condition_id(COND));
    p.apply(Op::AddAsset {
        id: asset,
        name: "ECU".into(),
        properties: m.all_properties(),
        provenance: prov(),
    })?;
    let safety = cat(&m, "safety");
    p.apply(Op::AddDamageScenario {
        id: damage,
        name: "Crash".into(),
        impacts: vec![(safety, lvl(&m, safety, "severe"))],
        provenance: prov(),
    })?;
    let category = m.threat_categories().next().ok_or("no category")?;
    p.apply(Op::AddThreatScenario {
        id: threat,
        name: "Spoof".into(),
        asset,
        category,
        catalog_refs: vec![],
        damages: vec![damage],
        provenance: prov(),
    })?;
    if with_condition {
        p.apply(Op::AddCondition {
            id: cond,
            name: "Option fitted".into(),
            description: "".into(),
            present,
            provenance: prov(),
        })?;
    }
    Ok(Fx {
        p,
        asset,
        threat,
        cond,
    })
}

impl Fx {
    /// Adds tree `TREE` (root gate `kind` over the given steps) and links it
    /// to the threat.
    fn tree(&mut self, kind: GateKind, steps: &[(u64, Option<&str>)]) -> Res<TreeId> {
        let m = self.p.methodology().clone();
        let mut tree = AttackTree::new(node_id(100), Node::gate(kind, "root"))?;
        let mut ratings = Vec::new();
        for &(n, level) in steps {
            tree.add_child(tree.root(), node_id(n), Node::step("step"))?;
            if let Some(level) = level {
                ratings.push((node_id(n), feas(&m, level)));
            }
        }
        self.add_tree(tree, ratings)
    }

    fn add_tree(
        &mut self,
        tree: AttackTree,
        step_feasibility: Vec<(NodeId, aegis_model::FeasibilityLevel)>,
    ) -> Res<TreeId> {
        let id = tree_id(10);
        self.p.apply(Op::AddAttackTree {
            id,
            tree,
            step_feasibility,
            provenance: prov(),
        })?;
        self.p.apply(Op::SetThreatAttackTree {
            threat: self.threat,
            tree: Some(id),
            provenance: prov(),
        })?;
        Ok(id)
    }

    fn need_step(&mut self, n: u64) -> Res {
        self.p.apply(Op::NeedStepCondition {
            tree: tree_id(10),
            node: node_id(n),
            condition: self.cond,
            provenance: prov(),
        })?;
        Ok(())
    }

    fn idx(&self, n: u64) -> Res<NodeIdx> {
        let (_, _, tree) = self.p.attack_trees().next().ok_or("no tree")?;
        Ok(tree.idx_of(&node_id(n)).ok_or("no node")?)
    }

    fn level(&self, key: &str) -> aegis_model::FeasibilityLevel {
        feas(self.p.methodology(), key)
    }
}

/// The cache built from `p` gives the same results as `p` for every tree
/// and threat.
fn assert_cache_agrees(p: &Project) {
    let cache = ProjectEvalCache::new(p);
    assert_eq!(cache.applicability(), &p.applicability());
    for (_, id, _) in p.attack_trees() {
        assert_eq!(cache.evaluate_tree(p, id).cloned(), p.evaluate_tree(id));
        assert_eq!(
            cache.evaluate_residual_tree(p, id).cloned(),
            p.evaluate_residual_tree(id)
        );
    }
    for (_, id, _) in p.threat_scenarios() {
        assert_eq!(cache.evaluate_threat(p, id).cloned(), p.evaluate_threat(id));
        assert_eq!(
            cache.evaluate_residual_threat(p, id).cloned(),
            p.evaluate_residual_threat(id)
        );
        assert_eq!(cache.evaluate_risk(p, id).cloned(), p.evaluate_risk(id));
        assert_eq!(
            cache.evaluate_residual_risk(p, id).cloned(),
            p.evaluate_residual_risk(id)
        );
    }
}

/// Toggles the condition through `apply_cached` and back; after each step
/// the cache equals a fresh one and agrees with the project.
fn assert_toggle_agrees(fx: &mut Fx) -> Res {
    assert_cache_agrees(&fx.p);
    let mut cache = ProjectEvalCache::new(&fx.p);
    let present =
        fx.p.condition(fx.p.condition_idx(&fx.cond).ok_or("no condition")?)
            .ok_or("no condition")?
            .present();
    for next in [!present, present] {
        fx.p.apply_cached(
            Op::SetConditionPresent {
                id: fx.cond,
                present: next,
                provenance: prov(),
            },
            &mut cache,
        )?;
        assert_eq!(cache, ProjectEvalCache::new(&fx.p));
        assert_cache_agrees(&fx.p);
    }
    Ok(())
}

#[test]
fn or_root_ignores_excluded_steps() -> Res {
    let mut fx = fixture(false, true)?;
    let t = fx.tree(GateKind::Or, &[(1, Some("high")), (2, Some("low"))])?;
    assert_eq!(fx.p.evaluate_tree(t)?.root_value(), &fx.level("high"));
    assert_toggle_agrees(&mut fx)?;

    fx.need_step(1)?;
    assert_eq!(fx.p.evaluate_tree(t)?.root_value(), &fx.level("low"));
    assert_eq!(fx.p.evaluate_tree(t)?.get(fx.idx(1)?), None);
    assert_toggle_agrees(&mut fx)?;

    fx.need_step(2)?;
    assert_eq!(fx.p.evaluate_tree(t), Err(ModelError::TreeNotApplicable(t)));
    assert_eq!(
        fx.p.evaluate_threat(fx.threat),
        Err(ModelError::TreeNotApplicable(t))
    );
    assert_eq!(
        fx.p.evaluate_risk(fx.threat),
        Err(ModelError::ThreatNotApplicable(fx.threat))
    );
    assert_eq!(
        fx.p.evaluate_residual_risk(fx.threat),
        Err(ModelError::ThreatNotApplicable(fx.threat))
    );
    assert_eq!(
        fx.p.threat_sensitivity(fx.threat),
        Err(ModelError::ThreatNotApplicable(fx.threat))
    );
    assert_eq!(
        fx.p.tree_tipping_points(t),
        Err(ModelError::TreeNotApplicable(t))
    );
    assert_toggle_agrees(&mut fx)
}

#[test]
fn and_and_seq_roots_are_excluded_with_any_child() -> Res {
    for kind in [GateKind::And, GateKind::SeqAnd] {
        let mut fx = fixture(false, true)?;
        let t = fx.tree(kind, &[(1, Some("high")), (2, Some("low"))])?;
        assert_eq!(fx.p.evaluate_tree(t)?.root_value(), &fx.level("low"));
        fx.need_step(2)?;
        assert_eq!(fx.p.evaluate_tree(t), Err(ModelError::TreeNotApplicable(t)));
        assert_eq!(
            fx.p.evaluate_residual_tree(t),
            Err(ModelError::TreeNotApplicable(t))
        );
        assert_eq!(
            fx.p.evaluate_risk(fx.threat),
            Err(ModelError::ThreatNotApplicable(fx.threat))
        );
        assert_toggle_agrees(&mut fx)?;
    }
    Ok(())
}

#[test]
fn nested_or_over_and_drops_the_excluded_branch() -> Res {
    let mut fx = fixture(false, true)?;
    let m = fx.p.methodology().clone();
    let mut tree = AttackTree::new(node_id(100), Node::gate(GateKind::Or, "root"))?;
    let and = tree.add_child(tree.root(), node_id(101), Node::gate(GateKind::And, "and"))?;
    tree.add_child(and, node_id(1), Node::step("a"))?;
    tree.add_child(and, node_id(2), Node::step("b"))?;
    tree.add_child(tree.root(), node_id(3), Node::step("c"))?;
    let ratings = vec![
        (node_id(1), feas(&m, "high")),
        (node_id(2), feas(&m, "high")),
        (node_id(3), feas(&m, "low")),
    ];
    let t = fx.add_tree(tree, ratings)?;
    assert_eq!(fx.p.evaluate_tree(t)?.root_value(), &fx.level("high"));

    fx.need_step(1)?;
    let eval = fx.p.evaluate_tree(t)?;
    let (a, and, c, root) = (fx.idx(1)?, fx.idx(101)?, fx.idx(3)?, fx.idx(100)?);
    assert_eq!(eval.root_value(), &fx.level("low"));
    assert_eq!(eval.get(a), None);
    assert_eq!(eval.get(and), None);
    assert_eq!(eval.get(c), Some(&fx.level("low")));
    assert_eq!(
        eval.derivation(root),
        Some(Derivation::Selected {
            rule: GateKind::Or,
            child: c
        })
    );
    let (_, _, tree) = fx.p.attack_trees().next().ok_or("no tree")?;
    let paths: Vec<Vec<_>> = eval
        .attack_paths(tree)
        .ok_or("no paths")?
        .map(|p| p.steps().to_vec())
        .collect();
    assert!(!paths.is_empty());
    assert_eq!(paths, vec![vec![c]]);
    let top: Vec<Vec<_>> = eval
        .top_k_paths(tree, 10)
        .ok_or("no top paths")?
        .iter()
        .map(|p| p.steps().to_vec())
        .collect();
    assert!(!top.is_empty());
    assert_eq!(top, vec![vec![c]]);
    assert_toggle_agrees(&mut fx)
}

#[test]
fn excluded_unrated_step_is_not_missing() -> Res {
    let mut fx = fixture(false, true)?;
    let t = fx.tree(GateKind::Or, &[(1, None), (2, Some("medium")), (3, None)])?;
    assert!(matches!(
        fx.p.evaluate_tree(t),
        Err(ModelError::MissingLeafValue(_))
    ));
    fx.need_step(1)?;
    assert_eq!(
        fx.p.evaluate_tree(t),
        Err(ModelError::MissingLeafValue(node_id(3)))
    );
    assert_toggle_agrees(&mut fx)?;
    fx.need_step(3)?;
    assert_eq!(fx.p.evaluate_tree(t)?.root_value(), &fx.level("medium"));
    assert_eq!(
        fx.p.evaluate_residual_tree(t)?.root_value(),
        &fx.level("medium")
    );
    assert_toggle_agrees(&mut fx)
}

#[test]
fn threat_excluded_by_its_asset_is_not_applicable() -> Res {
    for with_tree in [true, false] {
        let mut fx = fixture(false, true)?;
        if with_tree {
            fx.tree(GateKind::Or, &[(1, Some("high"))])?;
        } else {
            let high = fx.level("high");
            fx.p.apply(Op::SetThreatFeasibility {
                threat: fx.threat,
                level: Some(high),
                provenance: prov(),
            })?;
        }
        let goal = goal_id(5);
        fx.p.apply(Op::AddCybersecurityGoal {
            id: goal,
            name: "Protect".into(),
            controls: vec![],
            provenance: prov(),
        })?;
        fx.p.apply(Op::SetThreatGoal {
            threat: fx.threat,
            goal: Some(goal),
            provenance: prov(),
        })?;
        assert!(fx.p.evaluate_risk(fx.threat).is_ok());
        fx.p.apply(Op::NeedCondition {
            element: ElementRef::Asset(fx.asset),
            condition: fx.cond,
            provenance: prov(),
        })?;
        let na = Err(ModelError::ThreatNotApplicable(fx.threat));
        assert_eq!(fx.p.evaluate_risk(fx.threat), na);
        assert_eq!(fx.p.evaluate_residual_risk(fx.threat), na);
        assert_eq!(
            fx.p.threat_sensitivity(fx.threat).err(),
            Some(ModelError::ThreatNotApplicable(fx.threat))
        );
        assert_eq!(
            fx.p.residual_threat_sensitivity(fx.threat).err(),
            Some(ModelError::ThreatNotApplicable(fx.threat))
        );
        if with_tree {
            assert_eq!(
                fx.p.evaluate_threat(fx.threat),
                Err(ModelError::ThreatNotApplicable(fx.threat))
            );
        }
        assert!(!fx.p.applicability().is_excluded(ElementRef::Goal(goal)));
        assert_toggle_agrees(&mut fx)?;
    }
    Ok(())
}

#[test]
fn inherent_and_residual_follow_exclusions() -> Res {
    let mut fx = fixture(false, true)?;
    let t = fx.tree(GateKind::Or, &[(1, Some("high")), (2, Some("medium"))])?;
    let ctrl = control_id(7);
    fx.p.apply(Op::AddSecurityControl {
        id: ctrl,
        name: "SecOC".into(),
        provenance: prov(),
    })?;
    let very_low = fx.level("very-low");
    fx.p.apply(Op::SetStepMitigation {
        tree: t,
        node: node_id(1),
        control: ctrl,
        feasibility: Some(very_low),
        provenance: prov(),
    })?;
    assert_eq!(fx.p.evaluate_tree(t)?.root_value(), &fx.level("high"));
    assert_eq!(
        fx.p.evaluate_residual_tree(t)?.root_value(),
        &fx.level("medium")
    );
    assert_toggle_agrees(&mut fx)?;

    fx.need_step(2)?;
    assert_eq!(fx.p.evaluate_tree(t)?.root_value(), &fx.level("high"));
    assert_eq!(fx.p.evaluate_residual_tree(t)?.root_value(), &very_low);
    assert_eq!(
        fx.p.evaluate_residual_risk(fx.threat)?.feasibility(),
        very_low
    );
    assert_toggle_agrees(&mut fx)
}

#[test]
fn mitigated_step_excluded_leaves_the_other() -> Res {
    let mut fx = fixture(false, true)?;
    let t = fx.tree(GateKind::Or, &[(1, Some("high")), (2, Some("medium"))])?;
    let ctrl = control_id(7);
    fx.p.apply(Op::AddSecurityControl {
        id: ctrl,
        name: "SecOC".into(),
        provenance: prov(),
    })?;
    fx.p.apply(Op::SetStepMitigation {
        tree: t,
        node: node_id(1),
        control: ctrl,
        feasibility: Some(fx.level("very-low")),
        provenance: prov(),
    })?;
    fx.need_step(1)?;
    assert_eq!(fx.p.evaluate_tree(t)?.root_value(), &fx.level("medium"));
    assert_eq!(
        fx.p.evaluate_residual_tree(t)?.root_value(),
        &fx.level("medium")
    );
    assert_toggle_agrees(&mut fx)
}

#[test]
fn cached_rating_change_keeps_exclusions() -> Res {
    let mut fx = fixture(false, true)?;
    let t = fx.tree(GateKind::Or, &[(1, Some("high")), (2, Some("low"))])?;
    fx.need_step(1)?;
    let mut cache = ProjectEvalCache::new(&fx.p);
    let medium = fx.level("medium");
    for (n, level) in [(2, medium), (1, fx.level("low"))] {
        fx.p.apply_cached(
            Op::SetStepFeasibility {
                tree: t,
                node: node_id(n),
                level: Some(level),
                provenance: prov(),
            },
            &mut cache,
        )?;
        assert_eq!(cache, ProjectEvalCache::new(&fx.p));
        assert_eq!(cache.evaluate_tree(&fx.p, t)?.root_value(), &medium);
    }
    Ok(())
}

#[test]
fn all_present_conditions_change_no_risk() -> Res {
    let build = |with_condition: bool| -> Res<Fx> {
        let mut fx = fixture(true, with_condition)?;
        fx.tree(
            GateKind::Or,
            &[(1, Some("high")), (2, Some("low")), (3, Some("medium"))],
        )?;
        if with_condition {
            fx.need_step(1)?;
            fx.need_step(3)?;
            fx.p.apply(Op::NeedCondition {
                element: ElementRef::Asset(fx.asset),
                condition: fx.cond,
                provenance: prov(),
            })?;
        }
        Ok(fx)
    };
    let (with, without) = (build(true)?, build(false)?);
    assert!(with.p.applicability().is_empty());
    for (_, id, _) in without.p.threat_scenarios() {
        assert_eq!(with.p.evaluate_risk(id), without.p.evaluate_risk(id));
        assert_eq!(
            with.p.evaluate_residual_risk(id),
            without.p.evaluate_residual_risk(id)
        );
        assert_eq!(
            with.p.threat_sensitivity(id),
            without.p.threat_sensitivity(id)
        );
    }
    assert_eq!(
        with.p.evaluate_tree(tree_id(10)),
        without.p.evaluate_tree(tree_id(10))
    );
    assert_cache_agrees(&with.p);
    Ok(())
}
