//! Gas benchmarks for the shared fixtures themselves.
//!
//! Every contract suite pays for fixture setup, so token and oracle calls are
//! metered here with the same `GasMeter` the contract journeys use. Budgets
//! are local to this suite; the report lands in
//! `target/test-benchmarks/test-utils-fixtures.json` (override with
//! `BENCH_REPORT_DIR`).

use test_utils::bench::report_dir;
use test_utils::{BenchReport, GasBudgets, GasMeter, ProtocolFixture, ORACLE_PRICE_SCALE};

fn budgets() -> GasBudgets {
    GasBudgets::from_pairs(&[
        ("sac::mint", 1_000_000),
        ("sac::transfer", 1_000_000),
        ("sac::get_balance", 500_000),
        ("oracle::set_price", 500_000),
        ("oracle::get_price", 500_000),
    ])
}

#[test]
fn fixture_operations_within_gas_budgets() {
    let f = ProtocolFixture::builder().users(2).build();
    let budgets = budgets();
    let token = f.token(0);
    let (a, b) = (f.user(0), f.user(1));
    let mut meter = GasMeter::new("fixture_operations");

    meter.measure(&f.env, "mint", "sac::mint", || token.mint(a, 10_000));
    meter.measure(&f.env, "transfer", "sac::transfer", || {
        token.client().transfer(a, b, &2_500)
    });
    let balance = meter.measure(&f.env, "balance", "sac::get_balance", || token.balance(b));
    assert_eq!(balance, 2_500);
    meter.measure(&f.env, "set_price", "oracle::set_price", || {
        f.set_price(0, 3 * ORACLE_PRICE_SCALE)
    });
    let price = meter.measure(&f.env, "price", "oracle::get_price", || f.price(0));
    assert_eq!(price, 3 * ORACLE_PRICE_SCALE);

    assert_eq!(meter.steps.len(), 5);
    assert!(
        meter.steps.iter().all(|s| s.cpu > 0 && s.mem > 0),
        "every step must be metered"
    );
    budgets.assert_within(&meter);

    let dir = report_dir(
        "BENCH_REPORT_DIR",
        format!(
            "{}/../../target/test-benchmarks",
            env!("CARGO_MANIFEST_DIR")
        ),
    );
    let path = BenchReport::new(&[meter], &budgets).write(&dir, "test-utils-fixtures.json");
    let written: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(written["journeys"][0]["steps"].as_array().unwrap().len(), 5);
}

#[test]
fn repeated_operations_cost_the_same() {
    // Cost must not depend on how many users already hold the token.
    let f = ProtocolFixture::builder().users(8).build();
    let token = f.token(0);
    let mut meter = GasMeter::new("repeated_mint");
    for user in &f.users {
        meter.measure(&f.env, "mint", "sac::mint", || token.mint(user, 1_000));
    }
    let costs = meter.cpu_of("mint");
    let (min, max) = (*costs.iter().min().unwrap(), *costs.iter().max().unwrap());
    assert!(max <= min * 2, "mint cost varies {min}..{max} across users");
}
