// invariant_prop_test.rs
// Property-based fuzz testing for lending protocol invariants.
//
// This module defines property-based tests that verify core lending
// invariants hold under random state transitions.

#![allow(unused_imports)]

extern crate std;
use crate::proptest_helpers::{LARGE_CEILING, MAX_AMOUNT, MIN_AMOUNT};
use crate::invariant_test_suite::{InvariantTestConfig, InvariantTestSuite, setup_test_environment};
use crate::{LendingContract, LendingContractClient};
use proptest::prelude::*;
use soroban_sdk::{testutils::Address as _, Address, Env};

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

#[derive(Clone, Debug)]
enum Action {
    Deposit {
        user: u8,
        amount: i128,
    },
    Withdraw {
        user: u8,
        amount: i128,
    },
    Borrow {
        user: u8,
        borrow: i128,
        collateral: i128,
    },
    Repay {
        user: u8,
        amount: i128,
    },
}

fn action_strategy() -> impl Strategy<Value = Action> {
    prop_oneof![
        (0u8..=2, MIN_AMOUNT..=MAX_AMOUNT / 4)
            .prop_map(|(u, a)| Action::Deposit { user: u, amount: a }),
        (0u8..=2, MIN_AMOUNT..=MAX_AMOUNT / 8)
            .prop_map(|(u, a)| Action::Withdraw { user: u, amount: a }),
        (0u8..=2, MIN_AMOUNT..=MAX_AMOUNT / 4).prop_flat_map(|(u, b)| {
            let min_c = (b * 15_000 + 9_999) / 10_000;
            (Just(u), Just(b), min_c..=MAX_AMOUNT / 2).prop_map(|(u, b, c)| Action::Borrow {
                user: u,
                borrow: b,
                collateral: c,
            })
        }),
        (0u8..=2, MIN_AMOUNT..=MAX_AMOUNT / 4)
            .prop_map(|(u, a)| Action::Repay { user: u, amount: a }),
    ]
}

proptest! {
    /// PROP-INV-01: balances always non-negative across any action sequence
    #[test]
    fn prop_balances_always_non_negative(
        actions in prop::collection::vec(action_strategy(), 1..8)
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(LendingContract, ());
        let client = LendingContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let mut users = std::vec::Vec::new();
        for _ in 0..3 { users.push(Address::generate(&env)); }
        let asset     = Address::generate(&env);
        let col_asset = Address::generate(&env);

        client.initialize(&admin, &LARGE_CEILING, &MIN_AMOUNT);
        client.initialize_deposit_settings(&LARGE_CEILING, &MIN_AMOUNT);
        client.initialize_withdraw_settings(&MIN_AMOUNT);

        for action in &actions {
            let idx = match action {
                Action::Deposit{user,..}|Action::Withdraw{user,..}
                |Action::Borrow{user,..}|Action::Repay{user,..} => (*user as usize) % 3,
            };
            let user = &users[idx];
            match action {
                Action::Deposit{amount,..} => {
                    let _ = client.try_deposit(user, &asset, amount);
                }
                Action::Withdraw{amount,..} => {
                    let _ = client.try_withdraw(user, &asset, amount);
                }
                Action::Borrow{borrow,collateral,..} => {
                    let _ = client.try_borrow(user, &asset, borrow, &col_asset, collateral);
                }
                Action::Repay{amount,..} => {
                    let _ = client.try_repay(user, &asset, amount);
                }
            }
            for u in &users {
                prop_assert!(client.get_collateral_balance(u) >= 0);
                prop_assert!(client.get_debt_balance(u)       >= 0);
            }
        }
    }

    /// PROP-INV-02: deposit + full withdraw = no-op
    #[test]
    fn prop_deposit_withdraw_noop(amount in MIN_AMOUNT..=MAX_AMOUNT) {
        let (_env, client, _admin, user, asset, _col) =
            crate::proptest_helpers::make_harness();
        client.deposit(&user, &asset, &amount);
        client.withdraw(&user, &asset, &amount);
        prop_assert_eq!(client.get_collateral_balance(&user), 0);
    }

    /// PROP-INV-03: user isolation — B's balance unaffected by A's operations
    #[test]
    fn prop_user_isolation(
        dep_a in MIN_AMOUNT..=MAX_AMOUNT/2,
        dep_b in MIN_AMOUNT..=MAX_AMOUNT/2,
    ) {
        let (env, client, _admin, _u, asset, _col) =
            crate::proptest_helpers::make_harness();
        let user_a = Address::generate(&env);
        let user_b = Address::generate(&env);
        client.deposit(&user_a, &asset, &dep_a);
        client.deposit(&user_b, &asset, &dep_b);
        let bal_b = client.get_collateral_balance(&user_b);
        let _ = client.try_withdraw(&user_a, &asset, &(dep_a / 2 + 1));
        prop_assert_eq!(client.get_collateral_balance(&user_b), bal_b);
    }
}
