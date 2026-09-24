//! Gas benchmarks for the shared math hot paths (issue #694).
//!
//! These are regression tests, not micro-optimizations for their own sake: the
//! rate curves and liquidation sizing run on every borrow, accrual and
//! liquidation, so a change that quietly routes them back through the `I256`
//! host object would cost real fees on every call.
//!
//! Each benchmark measures CPU instructions with the Soroban budget and asserts
//! a ceiling. Run with `cargo test -p stellarlend-math --test gas_benchmarks
//! -- --nocapture` to print the measurements.

use soroban_sdk::Env;
use stellarlend_math::checked::checked_mul_div;
use stellarlend_math::liquidation::{dynamic_penalty_bps, seize_amount, split_proceeds};
use stellarlend_math::mul_div::mul_div;
use stellarlend_math::rates::{kink_rate, simple_interest, RateCurve, RateModelKind};

/// Number of iterations per measurement, to keep per-call noise small.
const ITERATIONS: u32 = 100;

fn curve() -> RateCurve {
    RateCurve {
        kind: RateModelKind::Kink,
        base_rate_bps: 100,
        kink_utilization_bps: 8_000,
        multiplier_bps: 2_000,
        jump_multiplier_bps: 10_000,
    }
}

/// Runs `body` under a fresh budget and returns the CPU instructions consumed.
fn measure<F: FnMut()>(label: &str, mut body: F) -> u64 {
    let env = Env::default();
    env.cost_estimate().budget().reset_unlimited();
    body();
    let cpu = env.cost_estimate().budget().cpu_instruction_cost();
    println!("{label}: {cpu} CPU instructions for {ITERATIONS} iterations");
    cpu
}

#[test]
fn kink_rate_stays_off_the_host() {
    let c = curve();
    let cpu = measure("kink_rate", || {
        for i in 0..ITERATIONS {
            let utilization = (i as i128 * 100) % 10_001;
            let _ = kink_rate(utilization, &c).unwrap();
        }
    });
    // Pure i128 arithmetic bills no host work at all. A non-zero reading means
    // the curve has started allocating host objects again.
    assert_eq!(cpu, 0, "kink_rate consumed host CPU budget");
}

#[test]
fn liquidation_sizing_stays_off_the_host() {
    let cpu = measure("liquidation sizing", || {
        for i in 0..ITERATIONS {
            let collateral = 1_000_000 + i as i128;
            let penalty = dynamic_penalty_bps(collateral, 1_000_000, 1_000, 10_500).unwrap();
            let seize = seize_amount(collateral, penalty).unwrap();
            let _ = split_proceeds(seize, collateral * 2, 1_000_000, penalty, 2_000).unwrap();
        }
    });
    assert_eq!(cpu, 0, "liquidation sizing consumed host CPU budget");
}

#[test]
fn interest_accrual_stays_off_the_host() {
    let cpu = measure("simple_interest", || {
        for i in 0..ITERATIONS {
            let _ = simple_interest(1_000_000_000, 500, 86_400 * (i as i128 + 1)).unwrap();
        }
    });
    assert_eq!(cpu, 0, "simple_interest consumed host CPU budget");
}

/// The point of [`checked_mul_div`]: same answer as the `I256` path for
/// protocol-scale operands, without the host calls.
#[test]
fn checked_mul_div_is_cheaper_than_the_i256_path() {
    let env = Env::default();

    env.cost_estimate().budget().reset_unlimited();
    for i in 0..ITERATIONS {
        let _ = checked_mul_div(1_000_000 + i as i128, 2_500, 10_000).unwrap();
    }
    let checked_cpu = env.cost_estimate().budget().cpu_instruction_cost();

    env.cost_estimate().budget().reset_unlimited();
    for i in 0..ITERATIONS {
        let _ = mul_div(&env, 1_000_000 + i as i128, 2_500, 10_000).unwrap();
    }
    let i256_cpu = env.cost_estimate().budget().cpu_instruction_cost();

    println!("checked_mul_div: {checked_cpu} CPU / mul_div (I256): {i256_cpu} CPU");
    assert!(
        checked_cpu < i256_cpu,
        "checked_mul_div ({checked_cpu}) is not cheaper than mul_div ({i256_cpu})"
    );
}

/// Both paths must agree wherever the operands fit in `i128`, otherwise the
/// cheaper one is not a drop-in replacement.
#[test]
fn checked_and_i256_paths_agree() {
    let env = Env::default();
    let cases: [(i128, i128, i128); 8] = [
        (1_000_000, 2_500, 10_000),
        (1, 1, 1),
        (-1_000, 500, 10_000),
        (1_000, -500, 10_000),
        (i64::MAX as i128, 10_000, 10_000),
        (999_999_999, 7, 3),
        (0, 12_345, 10_000),
        (7, 3, 2),
    ];
    for (a, b, d) in cases {
        assert_eq!(
            checked_mul_div(a, b, d).unwrap(),
            mul_div(&env, a, b, d).unwrap(),
            "paths disagree for ({a}, {b}, {d})"
        );
    }
}
