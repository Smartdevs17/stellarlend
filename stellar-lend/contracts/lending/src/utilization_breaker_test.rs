use super::*;
use crate::borrow::BorrowError;
use crate::utilization_breaker::{UtilizationBreakerConfig, UtilizationBreakerError};
use soroban_sdk::{
    testutils::{Address as _, Events as _},
    xdr, Address, Env,
};

const CEILING: i128 = 1_000_000;

struct Ctx<'a> {
    env: Env,
    client: LendingContractClient<'a>,
    admin: Address,
    asset: Address,
    collateral: Address,
}

fn setup<'a>() -> Ctx<'a> {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(LendingContract, ());
    let client = LendingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin, &CEILING, &100);
    let asset = Address::generate(&env);
    let collateral = Address::generate(&env);
    Ctx {
        env,
        client,
        admin,
        asset,
        collateral,
    }
}

fn cfg(trip: u32, reset: u32, max_while_tripped: i128) -> UtilizationBreakerConfig {
    UtilizationBreakerConfig {
        enabled: true,
        trip_utilization_bps: trip,
        reset_utilization_bps: reset,
        max_borrow_while_tripped: max_while_tripped,
    }
}

impl Ctx<'_> {
    /// Borrow `amount` for a fresh user (collateralised at 200%), returning the user.
    fn borrow(&self, amount: i128) -> Address {
        let user = Address::generate(&self.env);
        self.client
            .borrow(&user, &self.asset, &amount, &self.collateral, &(amount * 2));
        user
    }

    fn try_borrow(&self, amount: i128) -> Result<(), BorrowError> {
        let user = Address::generate(&self.env);
        match self
            .client
            .try_borrow(&user, &self.asset, &amount, &self.collateral, &(amount * 2))
        {
            Ok(_) => Ok(()),
            Err(Ok(e)) => Err(e),
            Err(Err(e)) => panic!("unexpected invoke error: {e:?}"),
        }
    }

    fn repay(&self, user: &Address, amount: i128) {
        self.client.repay(user, &self.asset, &amount);
    }

    fn tripped(&self) -> bool {
        self.client.get_utilization_breaker().tripped
    }
}

/// Data payload of the most recent event published under `topic` in the last invocation.
fn event_data(env: &Env, topic: &str) -> Option<xdr::ScVal> {
    env.events().all().events().iter().rev().find_map(|e| {
        let xdr::ContractEventBody::V0(body) = &e.body;
        match body.topics.first() {
            Some(xdr::ScVal::Symbol(s)) if s.0.as_slice() == topic.as_bytes() => {
                Some(body.data.clone())
            }
            _ => None,
        }
    })
}

fn field(data: &xdr::ScVal, name: &str) -> xdr::ScVal {
    let xdr::ScVal::Map(Some(map)) = data else {
        panic!("event data is not a map: {data:?}");
    };
    map.iter()
        .find(|e| matches!(&e.key, xdr::ScVal::Symbol(s) if s.0.as_slice() == name.as_bytes()))
        .map(|e| e.val.clone())
        .unwrap_or_else(|| panic!("field {name} missing"))
}

#[test]
fn breaker_is_inert_by_default() {
    let ctx = setup();
    let status = ctx.client.get_utilization_breaker();
    assert!(!status.config.enabled);
    assert!(!status.tripped);

    // Without configuration the pool can still be borrowed to the ceiling.
    ctx.borrow(CEILING);
    let status = ctx.client.get_utilization_breaker();
    assert_eq!(status.utilization_bps, 10_000);
    assert!(!status.tripped);
}

#[test]
fn threshold_is_configurable_and_emits_configured_event() {
    let ctx = setup();
    ctx.client
        .set_utilization_breaker(&ctx.admin, &cfg(8_000, 6_000, 50_000));

    let data = event_data(&ctx.env, "util_breaker_configured_event").expect("configured event");
    assert_eq!(field(&data, "trip_utilization_bps"), xdr::ScVal::U32(8_000));
    assert_eq!(
        field(&data, "reset_utilization_bps"),
        xdr::ScVal::U32(6_000)
    );
    assert_eq!(
        ctx.client.get_utilization_breaker().config,
        cfg(8_000, 6_000, 50_000)
    );

    // Reconfiguring takes effect for the next borrow.
    ctx.client
        .set_utilization_breaker(&ctx.admin, &cfg(5_000, 4_000, 0));
    assert_eq!(
        ctx.try_borrow(500_000),
        Err(BorrowError::UtilizationThrottled)
    );
    ctx.borrow(499_000);
}

#[test]
fn borrows_crossing_threshold_are_throttled_and_breaker_trips() {
    let ctx = setup();
    ctx.client
        .set_utilization_breaker(&ctx.admin, &cfg(8_000, 6_000, 50_000));

    ctx.borrow(700_000); // 70%
    assert!(!ctx.tripped());

    // Would land at 85%: larger than the throttle allowance, rejected without tripping.
    assert_eq!(
        ctx.try_borrow(150_000),
        Err(BorrowError::UtilizationThrottled)
    );
    assert!(!ctx.tripped());

    ctx.borrow(50_000); // 75%, below threshold
    assert!(event_data(&ctx.env, "util_breaker_tripped_event").is_none());
    assert!(!ctx.tripped());

    // Lands exactly on 80%: allowed (within allowance) and trips the breaker.
    ctx.borrow(50_000);
    let data = event_data(&ctx.env, "util_breaker_tripped_event").expect("tripped event");
    assert_eq!(field(&data, "utilization_bps"), xdr::ScVal::U32(8_000));
    assert_eq!(field(&data, "trip_utilization_bps"), xdr::ScVal::U32(8_000));
    assert!(ctx.tripped());

    // While tripped only borrows within the allowance go through.
    assert_eq!(
        ctx.try_borrow(50_001),
        Err(BorrowError::UtilizationThrottled)
    );
    ctx.borrow(50_000); // 85%
                        // Already tripped: no duplicate trip event.
    assert!(event_data(&ctx.env, "util_breaker_tripped_event").is_none());
}

#[test]
fn zero_allowance_is_a_hard_stop_past_threshold() {
    let ctx = setup();
    ctx.client
        .set_utilization_breaker(&ctx.admin, &cfg(9_000, 9_000, 0));
    ctx.borrow(899_900);
    assert_eq!(ctx.try_borrow(100), Err(BorrowError::UtilizationThrottled));
}

#[test]
fn breaker_auto_resets_below_reset_threshold_with_hysteresis() {
    let ctx = setup();
    ctx.client
        .set_utilization_breaker(&ctx.admin, &cfg(8_000, 6_000, 10_000));
    let whale = ctx.borrow(790_000);
    ctx.borrow(10_000); // 80% -> trips
    assert!(ctx.tripped());

    // Repay down to 70%: below trip but above reset, so the breaker stays tripped.
    ctx.repay(&whale, 100_000);
    assert!(event_data(&ctx.env, "util_breaker_reset_event").is_none());
    assert!(ctx.tripped());
    // A borrow that would stay under the trip line is still throttled while latched.
    assert_eq!(
        ctx.try_borrow(20_000),
        Err(BorrowError::UtilizationThrottled)
    );

    // Repay down to 55%: breaker re-arms automatically.
    ctx.repay(&whale, 150_000);
    let data = event_data(&ctx.env, "util_breaker_reset_event").expect("reset event");
    assert_eq!(field(&data, "manual"), xdr::ScVal::Bool(false));
    assert_eq!(field(&data, "utilization_bps"), xdr::ScVal::U32(5_500));
    assert!(!ctx.tripped());

    ctx.borrow(200_000); // 75%, unthrottled again
}

#[test]
fn repayments_are_never_throttled() {
    let ctx = setup();
    let user = ctx.borrow(900_000);
    // Tighten past current utilization: trips immediately.
    ctx.client
        .set_utilization_breaker(&ctx.admin, &cfg(5_000, 1_000, 0));
    assert!(ctx.tripped());
    ctx.repay(&user, 10_000);
    ctx.repay(&user, 890_000);
    assert!(!ctx.tripped());
}

#[test]
fn lowering_threshold_below_current_utilization_trips_immediately() {
    let ctx = setup();
    ctx.borrow(700_000);
    ctx.client
        .set_utilization_breaker(&ctx.admin, &cfg(6_000, 5_000, 0));
    assert!(event_data(&ctx.env, "util_breaker_tripped_event").is_some());
    assert!(ctx.tripped());
}

#[test]
fn disabling_reverses_throttle_and_emits_manual_reset() {
    let ctx = setup();
    ctx.client
        .set_utilization_breaker(&ctx.admin, &cfg(8_000, 6_000, 0));
    ctx.borrow(700_000);
    ctx.client
        .set_utilization_breaker(&ctx.admin, &cfg(7_000, 6_000, 0));
    assert!(ctx.tripped());

    let mut off = cfg(7_000, 6_000, 0);
    off.enabled = false;
    ctx.client.set_utilization_breaker(&ctx.admin, &off);
    let data = event_data(&ctx.env, "util_breaker_reset_event").expect("reset event");
    assert_eq!(field(&data, "manual"), xdr::ScVal::Bool(true));
    assert!(!ctx.tripped());

    // Throttle lifted: the pool can be borrowed to the ceiling again.
    ctx.borrow(300_000);

    // ...and re-enabling restores it.
    ctx.client
        .set_utilization_breaker(&ctx.admin, &cfg(7_000, 6_000, 0));
    assert!(ctx.tripped());
}

#[test]
fn raising_threshold_above_utilization_re_arms() {
    let ctx = setup();
    ctx.borrow(700_000);
    ctx.client
        .set_utilization_breaker(&ctx.admin, &cfg(7_000, 6_000, 0));
    assert!(ctx.tripped());
    ctx.client
        .set_utilization_breaker(&ctx.admin, &cfg(9_000, 8_000, 0));
    assert!(!ctx.tripped());
    ctx.borrow(100_000);
}

#[test]
fn manual_reset_clears_latch_but_re_trips_if_still_hot() {
    let ctx = setup();
    ctx.client
        .set_utilization_breaker(&ctx.admin, &cfg(8_000, 6_000, 10_000));
    ctx.borrow(790_000);
    ctx.borrow(10_000); // 80% -> trips
    assert!(ctx.tripped());

    ctx.client.reset_utilization_breaker(&ctx.admin);
    let data = event_data(&ctx.env, "util_breaker_reset_event").expect("reset event");
    assert_eq!(field(&data, "manual"), xdr::ScVal::Bool(true));
    assert!(!ctx.tripped());

    // Still at 80%: the next borrow is throttled by post-utilization and re-latches.
    assert_eq!(
        ctx.try_borrow(10_001),
        Err(BorrowError::UtilizationThrottled)
    );
    ctx.borrow(10_000);
    assert!(ctx.tripped());

    // Resetting an armed breaker is a no-op.
    ctx.client
        .set_utilization_breaker(&ctx.admin, &cfg(9_500, 9_000, 0));
    ctx.client.reset_utilization_breaker(&ctx.admin);
    assert!(event_data(&ctx.env, "util_breaker_reset_event").is_none());
}

#[test]
fn invalid_configs_are_rejected() {
    let ctx = setup();
    for bad in [
        cfg(0, 0, 0),
        cfg(10_001, 5_000, 0),
        cfg(8_000, 8_001, 0),
        cfg(8_000, 6_000, -1),
    ] {
        assert_eq!(
            ctx.client.try_set_utilization_breaker(&ctx.admin, &bad),
            Err(Ok(UtilizationBreakerError::InvalidConfig))
        );
    }
    assert!(!ctx.client.get_utilization_breaker().config.enabled);
}

#[test]
fn only_admin_can_configure_or_reset() {
    let ctx = setup();
    let stranger = Address::generate(&ctx.env);
    assert_eq!(
        ctx.client
            .try_set_utilization_breaker(&stranger, &cfg(8_000, 6_000, 0)),
        Err(Ok(UtilizationBreakerError::Unauthorized))
    );
    assert_eq!(
        ctx.client.try_reset_utilization_breaker(&stranger),
        Err(Ok(UtilizationBreakerError::Unauthorized))
    );
}
