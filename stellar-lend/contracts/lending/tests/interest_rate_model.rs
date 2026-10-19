//! Configurable interest rate model (issue #1194).
//!
//! The borrow rate is a kinked curve over pool utilization
//! (`total_debt / debt_ceiling`):
//!
//! - up to the kink: `base + util * slope / kink`
//! - above the kink: `base + slope + (util - kink) * jump_slope / (100% - kink)`
//!
//! clamped to `[rate_floor, rate_ceiling]`. The supply rate is the borrow rate
//! minus the spread, floored at `rate_floor`. Every parameter is set through
//! the admin-gated `set_interest_rate_config`.

mod common;

use common::*;
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::Address;
use stellarlend_lending::{BorrowError, InterestRateConfigUpdate};

const YEAR: u64 = 31_536_000;

fn no_change() -> InterestRateConfigUpdate {
    InterestRateConfigUpdate {
        base_rate_bps: None,
        kink_utilization_bps: None,
        slope_bps: None,
        jump_slope_bps: None,
        rate_floor_bps: None,
        rate_ceiling_bps: None,
        spread_bps: None,
    }
}

/// Borrow from a fresh account until pool utilization reaches `target_bps`.
fn raise_utilization_to(f: &Fixture, target_bps: i128) {
    let current = f.client.get_utilization_bps();
    let amount = (target_bps - current) * DEBT_CEILING / HF_SCALE;
    let user = Address::generate(&f.env);
    f.client.borrow(
        &user,
        &f.debt_asset,
        &amount,
        &f.collateral_asset,
        &(amount * COLLATERAL_RATIO_BPS / HF_SCALE),
    );
    assert_eq!(f.client.get_utilization_bps(), target_bps);
}

/// (utilization, borrow rate, supply rate), walking utilization upwards.
fn assert_curve(f: &Fixture, points: &[(i128, i128, i128)]) {
    for &(util, borrow, supply) in points {
        if util > 0 {
            raise_utilization_to(f, util);
        }
        assert_eq!(
            f.client.get_borrow_rate_bps(),
            borrow,
            "borrow rate at {util}"
        );
        assert_eq!(
            f.client.get_supply_rate_bps(),
            supply,
            "supply rate at {util}"
        );
    }
}

#[test]
fn default_model_outputs() {
    let f = setup();
    let cfg = f.client.get_interest_rate_config();
    assert_eq!(
        (
            cfg.base_rate_bps,
            cfg.kink_utilization_bps,
            cfg.slope_bps,
            cfg.jump_slope_bps,
            cfg.rate_floor_bps,
            cfg.rate_ceiling_bps,
            cfg.spread_bps,
        ),
        (500, 8_000, 2_000, 10_000, 0, 10_000, 200)
    );

    assert_curve(
        &f,
        &[
            (0, 500, 300),
            (4_000, 1_500, 1_300),   // 500 + 4000 * 2000 / 8000
            (8_000, 2_500, 2_300),   // at the kink: 500 + 2000
            (9_000, 7_500, 7_300),   // 2500 + 1000 * 10000 / 2000
            (10_000, 10_000, 9_800), // 12,500 clamped to the 100% ceiling
        ],
    );
}

#[test]
fn configured_model_outputs() {
    let f = setup();
    let next = f.client.set_interest_rate_config(
        &f.admin,
        &InterestRateConfigUpdate {
            base_rate_bps: Some(200),
            kink_utilization_bps: Some(5_000),
            slope_bps: Some(1_000),
            jump_slope_bps: Some(3_000),
            rate_floor_bps: Some(300),
            rate_ceiling_bps: Some(4_000),
            spread_bps: Some(100),
        },
    );
    assert_eq!(next, f.client.get_interest_rate_config());
    assert_eq!(next.kink_utilization_bps, 5_000);

    assert_curve(
        &f,
        &[
            (0, 300, 300),          // base 200 lifted to the floor; supply floored too
            (2_500, 700, 600),      // 200 + 2500 * 1000 / 5000
            (5_000, 1_200, 1_100),  // at the kink: 200 + 1000
            (7_500, 2_700, 2_600),  // 1200 + 2500 * 3000 / 5000
            (10_000, 4_000, 3_900), // 4,200 clamped to the 40% ceiling
        ],
    );
}

#[test]
fn partial_update_changes_only_the_given_fields() {
    let f = setup();
    let before = f.client.get_interest_rate_config();
    f.client.set_interest_rate_config(
        &f.admin,
        &InterestRateConfigUpdate {
            slope_bps: Some(4_000),
            ..no_change()
        },
    );
    let after = f.client.get_interest_rate_config();
    assert_eq!(after.slope_bps, 4_000);
    assert_eq!(after.base_rate_bps, before.base_rate_bps);
    assert_eq!(after.kink_utilization_bps, before.kink_utilization_bps);
    assert_eq!(after.jump_slope_bps, before.jump_slope_bps);
    assert_eq!(after.spread_bps, before.spread_bps);

    raise_utilization_to(&f, 4_000);
    assert_eq!(f.client.get_borrow_rate_bps(), 2_500); // 500 + 4000 * 4000 / 8000
}

#[test]
fn only_the_admin_can_change_the_model() {
    let f = setup();
    let update = InterestRateConfigUpdate {
        base_rate_bps: Some(900),
        ..no_change()
    };
    assert_eq!(
        f.client
            .try_set_interest_rate_config(&Address::generate(&f.env), &update),
        Err(Ok(BorrowError::Unauthorized))
    );

    f.env.set_auths(&[]);
    assert!(f
        .client
        .try_set_interest_rate_config(&f.admin, &update)
        .is_err());
    assert_eq!(f.client.get_interest_rate_config().base_rate_bps, 500);
}

#[test]
fn invalid_parameters_are_rejected_and_nothing_changes() {
    let f = setup();
    let before = f.client.get_interest_rate_config();
    let invalid = [
        InterestRateConfigUpdate {
            base_rate_bps: Some(-1),
            ..no_change()
        },
        InterestRateConfigUpdate {
            base_rate_bps: Some(10_001),
            ..no_change()
        },
        InterestRateConfigUpdate {
            kink_utilization_bps: Some(0),
            ..no_change()
        },
        InterestRateConfigUpdate {
            kink_utilization_bps: Some(10_000),
            ..no_change()
        },
        InterestRateConfigUpdate {
            slope_bps: Some(-1),
            ..no_change()
        },
        InterestRateConfigUpdate {
            jump_slope_bps: Some(-1),
            ..no_change()
        },
        InterestRateConfigUpdate {
            rate_ceiling_bps: Some(10_001),
            ..no_change()
        },
        InterestRateConfigUpdate {
            spread_bps: Some(10_001),
            ..no_change()
        },
        // Floor above ceiling, including when set in the same update.
        InterestRateConfigUpdate {
            rate_floor_bps: Some(10_000),
            rate_ceiling_bps: Some(5_000),
            ..no_change()
        },
        // A valid field does not rescue an invalid one in the same update.
        InterestRateConfigUpdate {
            base_rate_bps: Some(100),
            slope_bps: Some(-5),
            ..no_change()
        },
    ];
    for update in invalid.iter() {
        assert_eq!(
            f.client.try_set_interest_rate_config(&f.admin, update),
            Err(Ok(BorrowError::InvalidAmount)),
            "{update:?}"
        );
    }
    assert_eq!(f.client.get_interest_rate_config(), before);
}

#[test]
fn configured_rate_drives_interest_accrual() {
    // 400M of a 1B ceiling = 40% utilization.
    let principal = 400_000_000;

    let accrued_after_a_year = |base: Option<i128>| {
        let f = setup();
        if base.is_some() {
            f.client.set_interest_rate_config(
                &f.admin,
                &InterestRateConfigUpdate {
                    base_rate_bps: base,
                    ..no_change()
                },
            );
        }
        let user = Address::generate(&f.env);
        f.client.borrow(
            &user,
            &f.debt_asset,
            &principal,
            &f.collateral_asset,
            &(principal * COLLATERAL_RATIO_BPS / HF_SCALE),
        );
        let rate = f.client.get_borrow_rate_bps();
        f.env.ledger().with_mut(|l| l.timestamp += YEAR);
        (rate, f.client.get_debt_balance(&user) - principal)
    };

    // Default: 500 + 4000 * 2000 / 8000 = 15% a year.
    assert_eq!(
        accrued_after_a_year(None),
        (1_500, principal * 1_500 / 10_000)
    );
    // Base raised to 10%: 20% a year.
    assert_eq!(
        accrued_after_a_year(Some(1_000)),
        (2_000, principal * 2_000 / 10_000)
    );
}
