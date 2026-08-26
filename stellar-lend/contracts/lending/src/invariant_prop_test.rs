// invariant_prop_test.rs
// Property-based fuzz testing for lending protocol invariants.
//
// This module defines property-based tests that verify core lending
// invariants hold under random state transitions.

#![allow(unused_imports)]

use soroban_sdk::{Address, Env};
use crate::invariant_test_suite::{InvariantTestConfig, InvariantTestSuite, setup_test_environment};

#[cfg(test)]
mod prop_tests {
    use super::*;

    #[test]
    fn test_solvency_invariant_initial_state() {
        let env = Env::default();
        let config = InvariantTestConfig::default();
        let (users, assets) = setup_test_environment(&env, 4, 2);

        let mut suite = InvariantTestSuite::new(env, config);
        for u in &users {
            suite.add_user(u.clone());
        }
        for a in &assets {
            suite.add_asset(a.clone());
        }

        let report = suite.run();
        assert!(
            report.result.violations_found.is_empty(),
            "Invariant violations found in initial state: {:?}",
            report.result.violations_found
        );
    }

    #[test]
    fn test_negative_balance_impossible() {
        let env = Env::default();
        let config = InvariantTestConfig {
            enable_exhaustive_testing: false,
            ..Default::default()
        };
        let (users, assets) = setup_test_environment(&env, 3, 2);

        let mut suite = InvariantTestSuite::new(env, config);
        for u in &users {
            suite.add_user(u.clone());
        }
        for a in &assets {
            suite.add_asset(a.clone());
        }

        let report = suite.run();
        let negative_violations: Vec<_> = report
            .result
            .violations_found
            .iter()
            .filter(|v| v.invariant_id.contains("NEGATIVE") || v.message.contains("negative"))
            .collect();

        assert!(
            negative_violations.is_empty(),
            "Negative balance invariant violated: {:?}",
            negative_violations
        );
    }

    #[test]
    fn test_health_factor_zero_debt_sentinel() {
        let env = Env::default();
        let config = InvariantTestConfig::default();
        let (users, assets) = setup_test_environment(&env, 2, 1);

        let mut suite = InvariantTestSuite::new(env, config);
        for u in &users {
            suite.add_user(u.clone());
        }
        for a in &assets {
            suite.add_asset(a.clone());
        }

        let report = suite.run();
        assert!(
            report.summary.status != crate::invariant_test_suite::TestStatus::Failed,
            "Health factor invariant failed: {}",
            report.summary.message
        );
    }

    #[test]
    fn test_admin_stability() {
        let env = Env::default();
        let config = InvariantTestConfig::default();
        let (users, assets) = setup_test_environment(&env, 2, 1);

        let mut suite = InvariantTestSuite::new(env, config);
        for u in &users {
            suite.add_user(u.clone());
        }
        for a in &assets {
            suite.add_asset(a.clone());
        }

        let report = suite.run();
        let admin_violations: Vec<_> = report
            .result
            .violations_found
            .iter()
            .filter(|v| v.invariant_id.contains("INV-006"))
            .collect();

        assert!(
            admin_violations.is_empty(),
            "Admin stability invariant violated: {:?}",
            admin_violations
        );
    }

    #[test]
    fn test_ci_report_output() {
        let env = Env::default();
        let config = InvariantTestConfig::default();
        let (users, assets) = setup_test_environment(&env, 2, 1);

        let mut suite = InvariantTestSuite::new(env, config);
        for u in &users {
            suite.add_user(u.clone());
        }
        for a in &assets {
            suite.add_asset(a.clone());
        }

        let report = suite.run();
        let ci_report = suite.generate_ci_report(&report);

        assert!(ci_report.contains("# Invariant Testing Report"));
        assert!(ci_report.contains("## Results"));
        assert!(ci_report.contains("## Coverage"));
        assert!(ci_report.contains("## Summary"));
    }

    #[test]
    fn test_confidence_score_calculation() {
        let env = Env::default();
        let config = InvariantTestConfig::default();
        let (users, assets) = setup_test_environment(&env, 2, 1);

        let mut suite = InvariantTestSuite::new(env, config);
        for u in &users {
            suite.add_user(u.clone());
        }
        for a in &assets {
            suite.add_asset(a.clone());
        }

        let report = suite.run();
        assert!(
            report.result.confidence_score >= 0.0,
            "Confidence score should be non-negative"
        );
    }

    #[test]
    fn test_multiple_users_invariant_check() {
        let env = Env::default();
        let config = InvariantTestConfig::default();
        let (users, assets) = setup_test_environment(&env, 10, 3);

        let mut suite = InvariantTestSuite::new(env, config);
        for u in &users {
            suite.add_user(u.clone());
        }
        for a in &assets {
            suite.add_asset(a.clone());
        }

        let report = suite.run();
        assert_eq!(
            report.result.coverage_metrics.users_tested, 10,
            "Should test all 10 users"
        );
        assert_eq!(
            report.result.coverage_metrics.assets_tested, 3,
            "Should test all 3 assets"
        );
    }
}
