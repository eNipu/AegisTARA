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
        assert_eq!(stats.ok_kinds.len(), 26, "{:?}", stats.ok_kinds);
        assert!(stats.errors.len() >= 30, "{:?}", stats.errors);
        eprintln!(
            "{}: ok {} failed {} errors {:?}",
            m.key(),
            stats.ok,
            stats.failed,
            stats.errors
        );
    }
}
