//! Property tests for `Project::apply` over both methodologies under test:
//! random sequences of valid and invalid ops. A failed op leaves the
//! project unchanged and `validate` agrees with `apply`; each inverse
//! restores the previous project; all inverses in reverse give an empty
//! project.

mod common;

use proptest::prelude::*;
use proptest::test_runner::{Config, TestRunner};

use common::{Stats, accepted, builtin, replay, second, seeds};

fn check_lint_invariants(p: &aegis_model::Project) -> Result<(), TestCaseError> {
    let findings = p.lint();
    prop_assert!(
        findings.windows(2).all(|w| w[0] <= w[1]),
        "lint findings not sorted: {findings:?}"
    );
    let has_errors = findings.iter().any(|f| f.is_error());
    if !has_errors {
        for (_, id, _) in p.attack_trees() {
            prop_assert!(p.evaluate_tree(id).is_ok());
            prop_assert!(p.evaluate_residual_tree(id).is_ok());
        }
        for (_, id, t) in p.threat_scenarios() {
            if t.attack_tree().is_some() {
                prop_assert!(p.evaluate_threat(id).is_ok());
                prop_assert!(p.evaluate_residual_threat(id).is_ok());
            }
            prop_assert!(p.evaluate_risk(id).is_ok());
            prop_assert!(p.evaluate_residual_risk(id).is_ok());
        }
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn ops_first_methodology(seeds in seeds()) {
        replay(&builtin(), &seeds, |_| Ok(()))?;
    }

    #[test]
    fn ops_second_methodology(seeds in seeds()) {
        replay(&second(), &seeds, |_| Ok(()))?;
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn lint_soundness_first_methodology(seeds in seeds()) {
        accepted(&builtin(), &seeds, |p, _, _| check_lint_invariants(p))?;
    }

    #[test]
    fn lint_soundness_second_methodology(seeds in seeds()) {
        accepted(&second(), &seeds, |p, _, _| check_lint_invariants(p))?;
    }

    #[test]
    fn diff_soundness_first_methodology(seeds in seeds()) {
        check_diff_sequence(&builtin(), &seeds)?;
    }

    #[test]
    fn diff_soundness_second_methodology(seeds in seeds()) {
        check_diff_sequence(&second(), &seeds)?;
    }

    #[test]
    fn eval_cache_soundness_first_methodology(seeds in seeds()) {
        check_eval_cache_sequence(&builtin(), &seeds)?;
    }

    #[test]
    fn eval_cache_soundness_second_methodology(seeds in seeds()) {
        check_eval_cache_sequence(&second(), &seeds)?;
    }

    #[test]
    fn sensitivity_and_choke_points_first_methodology(seeds in seeds()) {
        let final_p = accepted(&builtin(), &seeds, |_, _, _| Ok(()))?;
        check_sensitivity_and_choke_points(&final_p)?;
    }

    #[test]
    fn sensitivity_and_choke_points_second_methodology(seeds in seeds()) {
        let final_p = accepted(&second(), &seeds, |_, _, _| Ok(()))?;
        check_sensitivity_and_choke_points(&final_p)?;
    }
}

fn check_sensitivity_and_choke_points(p: &aegis_model::Project) -> Result<(), TestCaseError> {
    let m = p.methodology();
    let feas_levels: Vec<_> = m.feasibility_levels().map(|(l, _, _)| l).collect();

    for (tree_idx, tree_id, tree) in p.attack_trees() {
        if let Ok(eval) = p.evaluate_tree(tree_id) {
            prop_assert_eq!(
                p.tree_choke_points(tree_id, 4).ok(),
                eval.choke_points(tree, 4)
            );
            prop_assert_eq!(
                p.tree_hitting_set(tree_id, 4).ok(),
                eval.top_k_hitting_set(tree, 4)
            );
            let tips = p
                .tree_tipping_points(tree_id)
                .map_err(|e| TestCaseError::fail(format!("tree_tipping_points: {e}")))?;
            for tip in &tips {
                prop_assert_eq!(tip.from_root(), *eval.root_value());
                prop_assert_ne!(tip.to_root(), tip.from_root());
                let perturbed = aegis_model::evaluate(
                    tree,
                    &aegis_model::MaxMin::<aegis_model::FeasibilityLevel>::new(),
                    |idx| {
                        if idx == tip.step() {
                            Some(tip.to_step())
                        } else {
                            p.effective_step_feasibility(tree_idx, idx)
                        }
                    },
                )
                .map_err(|e| TestCaseError::fail(format!("perturbed eval: {e}")))?;
                prop_assert_eq!(*perturbed.root_value(), tip.to_root());
            }
        }
        if let Ok(res_eval) = p.evaluate_residual_tree(tree_id) {
            prop_assert_eq!(
                p.residual_tree_choke_points(tree_id, 4).ok(),
                res_eval.choke_points(tree, 4)
            );
            prop_assert_eq!(
                p.residual_tree_hitting_set(tree_id, 4).ok(),
                res_eval.top_k_hitting_set(tree, 4)
            );
            let res_tips = p
                .residual_tree_tipping_points(tree_id)
                .map_err(|e| TestCaseError::fail(format!("residual_tree_tipping_points: {e}")))?;
            for tip in &res_tips {
                prop_assert_eq!(tip.from_root(), *res_eval.root_value());
                prop_assert_ne!(tip.to_root(), tip.from_root());
            }
        }
    }

    for (_, threat_id, threat) in p.threat_scenarios() {
        if let Ok(risk) = p.evaluate_risk(threat_id) {
            let sens = p
                .threat_sensitivity(threat_id)
                .map_err(|e| TestCaseError::fail(format!("threat_sensitivity: {e}")))?;
            prop_assert_eq!(sens.feasibility(), risk.feasibility());
            prop_assert_eq!(sens.max_risk(), risk.max_risk());
            prop_assert_eq!(
                sens.is_stable(),
                sens.feasibility_tipping_points().is_empty()
                    && sens.impact_tipping_points().is_empty()
            );

            // Check direct feasibility tipping points against a clone oracle.
            if threat.attack_tree().is_none()
                && let Some(from_risk) = risk.max_risk()
            {
                let cur = risk.feasibility().index();
                let mut expected_feas_tips = 0_usize;
                for next_idx in [cur.checked_sub(1), cur.checked_add(1)]
                    .into_iter()
                    .flatten()
                {
                    if let Some(&to_f) = feas_levels.get(next_idx) {
                        let mut clone = p.clone();
                        clone
                            .apply(aegis_model::Op::SetThreatFeasibility {
                                threat: threat_id,
                                level: Some(to_f),
                                provenance: aegis_model::Provenance::default(),
                            })
                            .expect("set threat feas");
                        let new_risk = clone
                            .evaluate_risk(threat_id)
                            .expect("eval risk")
                            .max_risk();
                        if new_risk.is_some_and(|r| r != from_risk) {
                            expected_feas_tips += 1;
                        }
                    }
                }
                prop_assert_eq!(sens.feasibility_tipping_points().len(), expected_feas_tips);
            }

            // Check impact tipping points against a clone oracle.
            if let Some(from_risk) = risk.max_risk() {
                let mut expected_impact_tips = 0_usize;
                for &d_idx in threat.damages() {
                    let Some(d_id) = p.damage_scenario_id(d_idx) else {
                        continue;
                    };
                    let Some(d) = p.damage_scenario(d_idx) else {
                        continue;
                    };
                    for (cat, imp) in d.impacts() {
                        let Some(from_imp) = imp else { continue };
                        let levels: Vec<_> = m.levels(cat).collect();
                        let cur = from_imp.index();
                        for next_idx in [cur.checked_sub(1), cur.checked_add(1)]
                            .into_iter()
                            .flatten()
                        {
                            if let Some(&to_imp) = levels.get(next_idx) {
                                let mut clone = p.clone();
                                clone
                                    .apply(aegis_model::Op::SetImpact {
                                        id: d_id,
                                        category: cat,
                                        level: Some(to_imp),
                                        provenance: aegis_model::Provenance::default(),
                                    })
                                    .expect("set impact");
                                let new_risk = clone
                                    .evaluate_risk(threat_id)
                                    .expect("eval risk")
                                    .max_risk();
                                if new_risk.is_some_and(|r| r != from_risk) {
                                    expected_impact_tips += 1;
                                }
                            }
                        }
                    }
                }
                prop_assert_eq!(sens.impact_tipping_points().len(), expected_impact_tips);
            }
        }
        if let Ok(res_risk) = p.evaluate_residual_risk(threat_id) {
            let res_sens = p
                .residual_threat_sensitivity(threat_id)
                .map_err(|e| TestCaseError::fail(format!("residual_threat_sensitivity: {e}")))?;
            prop_assert_eq!(res_sens.feasibility(), res_risk.feasibility());
            prop_assert_eq!(res_sens.max_risk(), res_risk.max_risk());
        }
    }
    Ok(())
}

fn assert_cache_agrees_with_project(
    p: &aegis_model::Project,
    cache: &aegis_model::ProjectEvalCache,
) -> Result<(), TestCaseError> {
    for (_, id, _) in p.attack_trees() {
        prop_assert_eq!(cache.evaluate_tree(p, id).cloned(), p.evaluate_tree(id));
        prop_assert_eq!(
            cache.evaluate_residual_tree(p, id).cloned(),
            p.evaluate_residual_tree(id)
        );
    }
    let unknown_tree = common::tree_id(999_999);
    prop_assert_eq!(
        cache.evaluate_tree(p, unknown_tree).cloned(),
        p.evaluate_tree(unknown_tree)
    );
    prop_assert_eq!(
        cache.evaluate_residual_tree(p, unknown_tree).cloned(),
        p.evaluate_residual_tree(unknown_tree)
    );

    for (_, id, _) in p.threat_scenarios() {
        prop_assert_eq!(cache.evaluate_threat(p, id).cloned(), p.evaluate_threat(id));
        prop_assert_eq!(
            cache.evaluate_residual_threat(p, id).cloned(),
            p.evaluate_residual_threat(id)
        );
        prop_assert_eq!(cache.evaluate_risk(p, id).cloned(), p.evaluate_risk(id));
        prop_assert_eq!(
            cache.evaluate_residual_risk(p, id).cloned(),
            p.evaluate_residual_risk(id)
        );
    }
    let unknown_threat = common::threat_id(999_999);
    prop_assert_eq!(
        cache.evaluate_threat(p, unknown_threat).cloned(),
        p.evaluate_threat(unknown_threat)
    );
    prop_assert_eq!(
        cache.evaluate_residual_threat(p, unknown_threat).cloned(),
        p.evaluate_residual_threat(unknown_threat)
    );
    prop_assert_eq!(
        cache.evaluate_risk(p, unknown_threat).cloned(),
        p.evaluate_risk(unknown_threat)
    );
    prop_assert_eq!(
        cache.evaluate_residual_risk(p, unknown_threat).cloned(),
        p.evaluate_residual_risk(unknown_threat)
    );
    Ok(())
}

fn check_eval_cache_sequence(
    m: &std::sync::Arc<aegis_model::Methodology>,
    seeds: &[common::Seed],
) -> Result<(), TestCaseError> {
    let mut cache_fwd =
        aegis_model::ProjectEvalCache::new(&aegis_model::Project::new(std::sync::Arc::clone(m)));
    let mut cache_inv = cache_fwd.clone();
    let mut inverses = Vec::new();
    let mut final_project = accepted(m, seeds, |curr, op, inv| {
        cache_fwd.update(curr, op);
        cache_inv.update(curr, inv);
        assert_cache_agrees_with_project(curr, &cache_fwd)?;
        assert_cache_agrees_with_project(curr, &cache_inv)?;
        inverses.push(inv.clone());
        Ok(())
    })?;

    // Undo every accepted op in reverse using `apply_cached` and verify cache agreement at every step.
    for inv in inverses.into_iter().rev() {
        final_project
            .apply_cached(inv, &mut cache_fwd)
            .map_err(|e| TestCaseError::fail(format!("undo failed: {e}")))?;
        assert_cache_agrees_with_project(&final_project, &cache_fwd)?;
    }
    Ok(())
}

fn check_diff_sequence(
    m: &std::sync::Arc<aegis_model::Methodology>,
    seeds: &[common::Seed],
) -> Result<(), TestCaseError> {
    let first = aegis_model::Project::new(std::sync::Arc::clone(m));
    let mut prev = first.clone();
    accepted(m, seeds, |curr, _, _| {
        check_diff_pair(&prev, curr)?;
        check_diff_pair(&first, curr)?;
        prev = curr.clone();
        Ok(())
    })?;
    Ok(())
}

fn check_diff_pair(
    base: &aegis_model::Project,
    curr: &aegis_model::Project,
) -> Result<(), TestCaseError> {
    let self_diff = curr.diff(curr).expect("self diff");
    prop_assert!(self_diff.is_empty());
    prop_assert!(self_diff.risk_deltas().is_empty());

    let fwd = base.diff(curr).expect("fwd diff");
    let rev = curr.diff(base).expect("rev diff");
    prop_assert_eq!(fwd.is_empty(), base == curr);
    prop_assert_eq!(rev.is_empty(), base == curr);

    // Added/removed/modified symmetry across all 5 entity arenas.
    prop_assert_eq!(fwd.assets().added(), rev.assets().removed());
    prop_assert_eq!(fwd.assets().removed(), rev.assets().added());
    prop_assert_eq!(fwd.assets().modified(), rev.assets().modified());
    prop_assert_eq!(
        fwd.damage_scenarios().added(),
        rev.damage_scenarios().removed()
    );
    prop_assert_eq!(
        fwd.damage_scenarios().removed(),
        rev.damage_scenarios().added()
    );
    prop_assert_eq!(
        fwd.damage_scenarios().modified(),
        rev.damage_scenarios().modified()
    );
    prop_assert_eq!(
        fwd.threat_scenarios().added(),
        rev.threat_scenarios().removed()
    );
    prop_assert_eq!(
        fwd.threat_scenarios().removed(),
        rev.threat_scenarios().added()
    );
    prop_assert_eq!(
        fwd.threat_scenarios().modified(),
        rev.threat_scenarios().modified()
    );
    prop_assert_eq!(fwd.attack_trees().added(), rev.attack_trees().removed());
    prop_assert_eq!(fwd.attack_trees().removed(), rev.attack_trees().added());
    prop_assert_eq!(fwd.attack_trees().modified(), rev.attack_trees().modified());
    prop_assert_eq!(
        fwd.security_controls().added(),
        rev.security_controls().removed()
    );
    prop_assert_eq!(
        fwd.security_controls().removed(),
        rev.security_controls().added()
    );
    prop_assert_eq!(
        fwd.security_controls().modified(),
        rev.security_controls().modified()
    );
    prop_assert_eq!(
        fwd.cybersecurity_goals().added(),
        rev.cybersecurity_goals().removed()
    );
    prop_assert_eq!(
        fwd.cybersecurity_goals().removed(),
        rev.cybersecurity_goals().added()
    );
    prop_assert_eq!(
        fwd.cybersecurity_goals().modified(),
        rev.cybersecurity_goals().modified()
    );
    prop_assert_eq!(
        fwd.cybersecurity_claims().added(),
        rev.cybersecurity_claims().removed()
    );
    prop_assert_eq!(
        fwd.cybersecurity_claims().removed(),
        rev.cybersecurity_claims().added()
    );
    prop_assert_eq!(
        fwd.cybersecurity_claims().modified(),
        rev.cybersecurity_claims().modified()
    );

    // Risk deltas are sorted by ThreatId and symmetric between fwd and rev.
    prop_assert!(
        fwd.risk_deltas()
            .windows(2)
            .all(|w| w[0].threat() < w[1].threat())
    );
    prop_assert_eq!(fwd.risk_deltas().len(), rev.risk_deltas().len());
    for (df, dr) in fwd.risk_deltas().iter().zip(rev.risk_deltas().iter()) {
        prop_assert_eq!(df.threat(), dr.threat());
        prop_assert_eq!(df.inherent().before(), dr.inherent().after());
        prop_assert_eq!(df.inherent().after(), dr.inherent().before());
        prop_assert_eq!(df.residual().before(), dr.residual().after());
        prop_assert_eq!(df.residual().after(), dr.residual().before());
    }

    // Completeness: every threat in `base` or `curr` whose inherent or residual risk
    // snapshot differs must appear in `fwd.changed_risks()`.
    let changed_ids: std::collections::BTreeSet<_> =
        fwd.changed_risks().map(|d| d.threat()).collect();
    let all_threats: std::collections::BTreeSet<_> = base
        .threat_scenarios()
        .map(|(_, id, _)| id)
        .chain(curr.threat_scenarios().map(|(_, id, _)| id))
        .collect();
    for id in all_threats {
        let inh_b = base
            .evaluate_risk(id)
            .ok()
            .map(|r| aegis_model::RiskSnapshot::new(r.feasibility(), r.max_risk()));
        let inh_c = curr
            .evaluate_risk(id)
            .ok()
            .map(|r| aegis_model::RiskSnapshot::new(r.feasibility(), r.max_risk()));
        let res_b = base
            .evaluate_residual_risk(id)
            .ok()
            .map(|r| aegis_model::RiskSnapshot::new(r.feasibility(), r.max_risk()));
        let res_c = curr
            .evaluate_residual_risk(id)
            .ok()
            .map(|r| aegis_model::RiskSnapshot::new(r.feasibility(), r.max_risk()));
        prop_assert_eq!(
            changed_ids.contains(&id),
            inh_b != inh_c || res_b != res_c,
            "risk delta completeness mismatch for {:?}",
            id
        );
    }
    Ok(())
}

/// Runs 256 sequences with a fixed seed and merges their statistics.
fn coverage(m: &std::sync::Arc<aegis_model::Methodology>) -> Stats {
    let config = Config {
        failure_persistence: None,
        ..Config::with_cases(256)
    };
    let mut runner = TestRunner::new_with_rng(
        config,
        proptest::test_runner::TestRng::deterministic_rng(
            proptest::test_runner::RngAlgorithm::ChaCha,
        ),
    );
    let total = std::cell::RefCell::new(Stats::default());
    runner
        .run(&seeds(), |s| {
            let stats = replay(m, &s, |_| Ok(()))?;
            let mut t = total.borrow_mut();
            t.ok += stats.ok;
            t.failed += stats.failed;
            t.ok_kinds.extend(stats.ok_kinds);
            t.errors.extend(stats.errors);
            Ok(())
        })
        .expect("replay holds");
    total.into_inner()
}

/// The generator is only useful if it mixes valid and invalid ops: every
/// op variant must succeed sometimes, and most error variants must occur.
#[test]
fn ops_generator_covers_every_op_and_most_errors() {
    for m in [builtin(), second()] {
        let stats = coverage(&m);
        let share = stats.ok as f64 / (stats.ok + stats.failed) as f64;
        assert!((0.25..0.9).contains(&share), "valid share {share}");
        assert_eq!(stats.ok_kinds.len(), 37, "{:?}", stats.ok_kinds);
        assert!(stats.errors.len() >= 38, "{:?}", stats.errors);
        eprintln!(
            "{}: ok {} failed {} errors {:?}",
            m.key(),
            stats.ok,
            stats.failed,
            stats.errors
        );
    }
}
