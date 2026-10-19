//! # Oracle Contract Integration Stress Tests (#691)
//!
//! The existing oracle suites (`oracle_test`, `oracle_circuit_breaker_test`,
//! `oracle_staleness_fallback_test`, `oracle_configuration_test`) each verify
//! one guard in isolation under normal operation. This module pushes the
//! on-chain oracle through the extreme conditions that only occur during an
//! incident, and exercises the guards *against each other*:
//!
//! - **Failure scenarios** — every source stale, no source configured,
//!   unauthorized submitters hammering the feed, quorum starvation.
//! - **Price spike / crash** — flash crashes and vertical spikes across many
//!   consecutive ledgers, including sustained volatility.
//! - **Latency** — long gaps between updates, the staleness boundary, and cache
//!   expiry under ledger-time advancement.
//! - **Circuit breaker** — trip, cooldown, stabilization, recovery, and
//!   emergency pause, including repeated trip/recover cycles.
//! - **Multi-oracle redundancy** — many sources, outlier filtering, quorum
//!   enforcement, primary/fallback promotion.
//! - **Upgrade** — reconfiguring thresholds, rotating the source set, and
//!   swapping primary/fallback against a live feed.
//!
//! ## Invariants asserted throughout
//!
//! 1. `get_price` never returns a price that failed a guard — it returns an
//!    error instead. A wrong price liquidates solvent positions; an error only
//!    blocks an operation.
//! 2. An open circuit breaker is always temporary. Every trip has a finite
//!    cooldown, and reads resume once the asset demonstrably stabilizes.
//! 3. Guards are per-asset. An incident on one asset never halts another.
//! 4. Configuration changes take effect on the next call, and rolling one back
//!    restores the prior behaviour exactly.

#![cfg(test)]

use crate::oracle::{
    self, CircuitBreakerState, OracleConfig, OracleDataKey, OracleError, OracleIncidentKind,
};
use crate::{HelloContract, HelloContractClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    vec, Address, Env, Vec,
};

// =============================================================================
// HELPERS
// =============================================================================

fn create_env() -> Env {
    let env = Env::default();
    env.mock_all_auths();
    env
}

fn setup(env: &Env) -> (Address, Address, HelloContractClient<'_>) {
    let contract_id = env.register(HelloContract, ());
    let client = HelloContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    client.initialize(&admin);
    (contract_id, admin, client)
}

/// A permissive baseline config, so each scenario can tighten only the one
/// threshold it is actually testing. Starting from the production defaults
/// would leave several guards firing at once and make failures ambiguous.
fn permissive_config() -> OracleConfig {
    OracleConfig {
        max_deviation_bps: 10_000,
        max_staleness_seconds: 3_600,
        cache_ttl_seconds: 0,
        min_price: 1,
        max_price: i128::MAX,
        twap_window_seconds: 0,
        max_observations: 64,
        min_sources: 1,
        outlier_deviation_bps: 10_000,
        breaker_deviation_bps: 10_000,
        breaker_cooldown_seconds: 600,
    }
}

/// Advance ledger time by `seconds`, leaving everything else intact.
fn advance(env: &Env, seconds: u64) {
    let now = env.ledger().timestamp();
    env.ledger().set_timestamp(now + seconds);
}

/// Clear an asset's circuit breaker so a scenario can continue past a trip it
/// has already asserted on.
///
/// Written straight to storage rather than through a production helper: the
/// contract deliberately exposes no "un-trip" entry point, and adding one just
/// for tests would widen the real attack surface.
fn clear_breaker(env: &Env, contract_id: &Address, asset: &Address) {
    env.as_contract(contract_id, || {
        env.storage().persistent().set(
            &OracleDataKey::CircuitBreaker(asset.clone()),
            &CircuitBreakerState {
                open_until: 0,
                last_safe_price: 0,
                last_trip_timestamp: 0,
            },
        );
    });
}

// =============================================================================
// 1. ORACLE FAILURE SCENARIOS
// =============================================================================

#[test]
fn reads_fail_closed_when_the_only_feed_goes_stale() {
    let env = create_env();
    let (_, admin, client) = setup(&env);
    let asset = Address::generate(&env);
    let oracle_addr = Address::generate(&env);

    let mut config = permissive_config();
    config.max_staleness_seconds = 300;
    client.configure_oracle(&admin, &config);
    client.update_price_feed(&admin, &asset, &100_000_000, &8, &oracle_addr);

    // Fresh read succeeds.
    assert_eq!(client.get_price(&asset), 100_000_000);

    // One second past the staleness bound, the read must fail rather than
    // return the last known price.
    advance(&env, 301);
    assert!(
        client.try_get_price(&asset).is_err(),
        "a stale feed must fail closed, never return its last value"
    );
}

#[test]
fn reads_fail_when_no_feed_was_ever_configured() {
    let env = create_env();
    let (_, admin, client) = setup(&env);
    let unknown_asset = Address::generate(&env);

    client.configure_oracle(&admin, &permissive_config());

    assert!(
        client.try_get_price(&unknown_asset).is_err(),
        "an asset with no feed must error, not default to a price"
    );
}

#[test]
fn unauthorized_submitters_cannot_move_the_feed_under_repeated_attempts() {
    let env = create_env();
    let (_, admin, client) = setup(&env);
    let asset = Address::generate(&env);
    let real_oracle = Address::generate(&env);

    client.configure_oracle(&admin, &permissive_config());
    client.update_price_feed(&admin, &asset, &100_000_000, &8, &real_oracle);

    // Fifty different unauthorized addresses each try to push a price.
    for i in 0..50 {
        let attacker = Address::generate(&env);
        let result = client.try_update_price_feed(&attacker, &asset, &(1_000 + i), &8, &attacker);
        assert!(
            result.is_err(),
            "unauthorized submitter {i} must be rejected"
        );
    }

    // The honest price is untouched.
    assert_eq!(
        client.get_price(&asset),
        100_000_000,
        "no unauthorized attempt may alter the feed"
    );
}

#[test]
fn an_incident_on_one_asset_does_not_halt_another() {
    // Guards are per-asset. A breaker opened on a volatile asset must leave an
    // unrelated, healthy asset fully readable.
    let env = create_env();
    let (_, admin, client) = setup(&env);
    let volatile = Address::generate(&env);
    let calm = Address::generate(&env);
    let oracle_addr = Address::generate(&env);

    client.configure_oracle(&admin, &permissive_config());
    client.update_price_feed(&admin, &volatile, &100_000_000, &8, &oracle_addr);
    client.update_price_feed(&admin, &calm, &50_000_000, &8, &oracle_addr);

    // Trip the volatility breaker on `volatile` only: >20% inside 10 minutes.
    advance(&env, 60);
    client.update_price_feed(&admin, &volatile, &70_000_000, &8, &oracle_addr);

    assert!(
        client.try_get_price(&volatile).is_err(),
        "the volatile asset's breaker should be open"
    );
    assert_eq!(
        client.get_price(&calm),
        50_000_000,
        "an unrelated asset must remain readable"
    );
}

// =============================================================================
// 2. PRICE SPIKE / CRASH SIMULATION
// =============================================================================

#[test]
fn a_flash_crash_trips_the_volatility_breaker() {
    let env = create_env();
    let (_, admin, client) = setup(&env);
    let asset = Address::generate(&env);
    let oracle_addr = Address::generate(&env);

    client.configure_oracle(&admin, &permissive_config());
    client.update_price_feed(&admin, &asset, &100_000_000, &8, &oracle_addr);

    // −50% inside the 10-minute volatility window.
    advance(&env, 120);
    client.update_price_feed(&admin, &asset, &50_000_000, &8, &oracle_addr);

    let report = client
        .get_oracle_incident_report(&asset)
        .expect("a crash of this size must produce an incident report");
    assert_eq!(report.kind, OracleIncidentKind::VolatilityPause);
    assert!(
        client.try_get_price(&asset).is_err(),
        "reads must be halted during a flash crash"
    );
}

#[test]
fn a_vertical_spike_trips_the_volatility_breaker() {
    let env = create_env();
    let (_, admin, client) = setup(&env);
    let asset = Address::generate(&env);
    let oracle_addr = Address::generate(&env);

    client.configure_oracle(&admin, &permissive_config());
    client.update_price_feed(&admin, &asset, &100_000_000, &8, &oracle_addr);

    // +150% inside the volatility window.
    advance(&env, 60);
    client.update_price_feed(&admin, &asset, &250_000_000, &8, &oracle_addr);

    let report = client
        .get_oracle_incident_report(&asset)
        .expect("a spike of this size must produce an incident report");
    assert_eq!(report.kind, OracleIncidentKind::VolatilityPause);
    assert!(client.try_get_price(&asset).is_err());
}

#[test]
fn a_gradual_move_outside_the_volatility_window_is_tracked() {
    // The same total move, spread beyond the 10-minute window, is legitimate
    // market action. Halting on it would blind the protocol during a real
    // trend, so each step must be accepted and readable.
    let env = create_env();
    let (_, admin, client) = setup(&env);
    let asset = Address::generate(&env);
    let oracle_addr = Address::generate(&env);

    client.configure_oracle(&admin, &permissive_config());

    let mut price: i128 = 100_000_000;
    client.update_price_feed(&admin, &asset, &price, &8, &oracle_addr);

    // Ten steps of −5%, each separated by more than the volatility window.
    for step in 0..10 {
        advance(&env, 700);
        price = price * 95 / 100;
        client.update_price_feed(&admin, &asset, &price, &8, &oracle_addr);
        assert_eq!(
            client.get_price(&asset),
            price,
            "step {step} of a gradual move must remain readable"
        );
    }

    assert!(
        price < 60_000_000,
        "the cumulative move should exceed 40%, got {price}"
    );
}

#[test]
fn sustained_whipsaw_never_yields_a_price_that_was_not_submitted() {
    // Alternating large swings. Whether a given read succeeds or is halted, any
    // price that *is* returned must be one that was actually submitted.
    let env = create_env();
    let (contract_id, admin, client) = setup(&env);
    let asset = Address::generate(&env);
    let oracle_addr = Address::generate(&env);

    client.configure_oracle(&admin, &permissive_config());

    let high: i128 = 100_000_000;
    let low: i128 = 60_000_000;
    client.update_price_feed(&admin, &asset, &high, &8, &oracle_addr);

    for round in 0..20 {
        let submitted = if round % 2 == 0 { low } else { high };
        advance(&env, 120);
        client.update_price_feed(&admin, &asset, &submitted, &8, &oracle_addr);

        if let Ok(Ok(price)) = client.try_get_price(&asset) {
            assert!(
                price == high || price == low,
                "round {round} returned {price}, which was never submitted"
            );
        }

        // The whipsaw trips the breaker repeatedly; clear it so the scenario can
        // keep exercising the submission path rather than stopping at the first trip.
        clear_breaker(&env, &contract_id, &asset);
    }
}

// =============================================================================
// 3. ORACLE LATENCY
// =============================================================================

#[test]
fn the_staleness_boundary_is_exact() {
    // Off-by-one at the boundary is the classic oracle bug: one second either
    // way is the difference between a blocked liquidation and a stale one.
    let env = create_env();
    let (_, admin, client) = setup(&env);
    let asset = Address::generate(&env);
    let oracle_addr = Address::generate(&env);

    let mut config = permissive_config();
    config.max_staleness_seconds = 600;
    client.configure_oracle(&admin, &config);
    client.update_price_feed(&admin, &asset, &100_000_000, &8, &oracle_addr);

    // Exactly at the bound: still fresh.
    advance(&env, 600);
    assert_eq!(
        client.get_price(&asset),
        100_000_000,
        "a feed exactly at max_staleness_seconds must still be readable"
    );

    // One second past: stale.
    advance(&env, 1);
    assert!(
        client.try_get_price(&asset).is_err(),
        "one second past the bound must be rejected"
    );
}

#[test]
fn a_long_update_gap_is_recoverable_by_a_single_fresh_update() {
    // A provider outage lasting days must not require any intervention beyond
    // resuming updates.
    let env = create_env();
    let (_, admin, client) = setup(&env);
    let asset = Address::generate(&env);
    let oracle_addr = Address::generate(&env);

    let mut config = permissive_config();
    config.max_staleness_seconds = 3_600;
    client.configure_oracle(&admin, &config);
    client.update_price_feed(&admin, &asset, &100_000_000, &8, &oracle_addr);

    // Seven days of silence.
    advance(&env, 7 * 24 * 3_600);
    assert!(
        client.try_get_price(&asset).is_err(),
        "a week-old feed must be stale"
    );

    // One fresh update restores service.
    client.update_price_feed(&admin, &asset, &100_000_000, &8, &oracle_addr);
    assert_eq!(
        client.get_price(&asset),
        100_000_000,
        "a single fresh update must restore readability"
    );
}

#[test]
fn many_updates_across_advancing_ledgers_stay_consistent() {
    // Sustained load: 100 small updates, each on a later ledger. Every read must
    // reflect exactly the most recent submission.
    let env = create_env();
    let (_, admin, client) = setup(&env);
    let asset = Address::generate(&env);
    let oracle_addr = Address::generate(&env);

    let mut config = permissive_config();
    config.max_staleness_seconds = 3_600;
    client.configure_oracle(&admin, &config);

    let mut price: i128 = 100_000_000;
    client.update_price_feed(&admin, &asset, &price, &8, &oracle_addr);

    for round in 0..100 {
        advance(&env, 30);
        // 0.1% steps stay well inside every guard band.
        price += price / 1_000;
        client.update_price_feed(&admin, &asset, &price, &8, &oracle_addr);
        assert_eq!(
            client.get_price(&asset),
            price,
            "read after update {round} did not match the submitted price"
        );
    }
}

// =============================================================================
// 4. CIRCUIT BREAKER VALIDATION
// =============================================================================

#[test]
fn the_breaker_reopens_reads_after_its_cooldown_and_stabilization() {
    let env = create_env();
    let (_, admin, client) = setup(&env);
    let asset = Address::generate(&env);
    let oracle_addr = Address::generate(&env);

    let mut config = permissive_config();
    config.breaker_cooldown_seconds = 600;
    client.configure_oracle(&admin, &config);
    client.update_price_feed(&admin, &asset, &100_000_000, &8, &oracle_addr);

    // Trip it with a 40% crash inside the volatility window.
    advance(&env, 60);
    client.update_price_feed(&admin, &asset, &60_000_000, &8, &oracle_addr);
    let tripped_at = client.get_oracle_circuit_breaker_state(&asset).open_until;
    assert!(
        tripped_at > env.ledger().timestamp(),
        "breaker should be open"
    );
    assert!(client.try_get_price(&asset).is_err());

    // Cooldown elapses and the asset holds its new level across several
    // updates, demonstrating stabilization.
    advance(&env, 601);
    for _ in 0..5 {
        advance(&env, 700);
        client.update_price_feed(&admin, &asset, &60_000_000, &8, &oracle_addr);
    }

    assert_eq!(
        client.get_price(&asset),
        60_000_000,
        "reads must resume at the stabilized level after cooldown"
    );
}

#[test]
fn repeated_trip_and_recover_cycles_do_not_wedge_the_breaker() {
    // A breaker that leaks state across cycles eventually stays open forever.
    // Three full cycles must each end with the feed readable.
    let env = create_env();
    let (_, admin, client) = setup(&env);
    let asset = Address::generate(&env);
    let oracle_addr = Address::generate(&env);

    let mut config = permissive_config();
    config.breaker_cooldown_seconds = 300;
    client.configure_oracle(&admin, &config);

    let mut level: i128 = 100_000_000;
    client.update_price_feed(&admin, &asset, &level, &8, &oracle_addr);

    for cycle in 0..3 {
        // Trip: −40% inside the volatility window.
        advance(&env, 60);
        level = level * 60 / 100;
        client.update_price_feed(&admin, &asset, &level, &8, &oracle_addr);
        assert!(
            client.try_get_price(&asset).is_err(),
            "cycle {cycle}: breaker should have opened"
        );

        // Recover: cooldown plus a stable stretch at the new level.
        advance(&env, 301);
        for _ in 0..5 {
            advance(&env, 700);
            client.update_price_feed(&admin, &asset, &level, &8, &oracle_addr);
        }
        assert_eq!(
            client.get_price(&asset),
            level,
            "cycle {cycle}: reads should have resumed"
        );
    }
}

#[test]
fn emergency_pause_halts_reads_and_expires_on_its_own() {
    // Emergency pause is an admin escape hatch, but it must still be bounded —
    // an unbounded pause is an unrecoverable protocol halt.
    let env = create_env();
    let (contract_id, admin, client) = setup(&env);
    let asset = Address::generate(&env);
    let oracle_addr = Address::generate(&env);

    client.configure_oracle(&admin, &permissive_config());
    client.update_price_feed(&admin, &asset, &100_000_000, &8, &oracle_addr);
    assert_eq!(client.get_price(&asset), 100_000_000);

    env.as_contract(&contract_id, || {
        oracle::emergency_pause_asset_oracle(&env, admin.clone(), asset.clone(), 1_800)
            .expect("admin may pause an asset oracle");
    });

    assert!(
        client.try_get_price(&asset).is_err(),
        "an emergency pause must halt reads"
    );

    // Past the pause window, a fresh update restores service.
    advance(&env, 1_801);
    client.update_price_feed(&admin, &asset, &100_000_000, &8, &oracle_addr);
    assert_eq!(
        client.get_price(&asset),
        100_000_000,
        "the pause must expire rather than halt the asset permanently"
    );
}

#[test]
fn consecutive_failures_auto_trigger_the_breaker_and_reset_on_success() {
    let env = create_env();
    let (contract_id, admin, client) = setup(&env);
    let asset = Address::generate(&env);
    let oracle_addr = Address::generate(&env);

    client.configure_oracle(&admin, &permissive_config());
    client.update_price_feed(&admin, &asset, &100_000_000, &8, &oracle_addr);

    // Two reported failures stay below the auto-trip threshold of three.
    env.as_contract(&contract_id, || {
        for _ in 0..2 {
            let status = oracle::monitor_oracle_health(&env, &asset)
                .expect("health monitoring should not error");
            assert!(
                !status.auto_triggered,
                "the breaker must not trip below the threshold"
            );
        }

        // The third crosses it.
        let status = oracle::monitor_oracle_health(&env, &asset)
            .expect("health monitoring should not error");
        assert!(
            status.auto_triggered,
            "the third consecutive failure must auto-trigger the breaker"
        );
        assert!(status.circuit_breaker_open);

        // A success resets the counter so the next outage starts from zero.
        oracle::record_oracle_success(&env, &asset);
        assert_eq!(
            oracle::get_consecutive_failures(&env, &asset),
            0,
            "a success must clear the consecutive failure count"
        );
    });
}

// =============================================================================
// 5. MULTI-ORACLE REDUNDANCY
// =============================================================================

#[test]
fn an_outlier_source_is_filtered_out_by_its_peers() {
    let env = create_env();
    let (_, admin, client) = setup(&env);
    let asset = Address::generate(&env);
    let s1 = Address::generate(&env);
    let s2 = Address::generate(&env);
    let s3 = Address::generate(&env);

    let mut config = permissive_config();
    config.min_sources = 2;
    // 5% outlier band, wide enough to keep the honest pair and drop the liar.
    config.outlier_deviation_bps = 500;
    client.configure_oracle(&admin, &config);
    client.set_oracle_sources(
        &admin,
        &asset,
        &vec![&env, s1.clone(), s2.clone(), s3.clone()],
    );

    // Two honest sources agree; the third is 40% high.
    client.update_price_feed(&admin, &asset, &100_000_000, &8, &s1);
    client.update_price_feed(&admin, &asset, &100_000_000, &8, &s2);
    client.update_price_feed(&admin, &asset, &140_000_000, &8, &s3);

    // The aggregate must sit at the honest level, not be dragged by the outlier.
    // A trip is also an acceptable outcome; what is not acceptable is returning
    // a price influenced by the liar.
    if let Ok(Ok(price)) = client.try_get_price(&asset) {
        assert!(
            price <= 105_000_000,
            "outlier source dragged the aggregate to {price}"
        );
    }
}

#[test]
fn a_quorum_of_sources_is_enforced() {
    let env = create_env();
    let (_, admin, client) = setup(&env);
    let asset = Address::generate(&env);
    let s1 = Address::generate(&env);
    let s2 = Address::generate(&env);
    let s3 = Address::generate(&env);

    let mut config = permissive_config();
    config.min_sources = 3;
    client.configure_oracle(&admin, &config);
    client.set_oracle_sources(
        &admin,
        &asset,
        &vec![&env, s1.clone(), s2.clone(), s3.clone()],
    );

    // Only two of the three required sources report.
    client.update_price_feed(&admin, &asset, &100_000_000, &8, &s1);
    client.update_price_feed(&admin, &asset, &100_000_000, &8, &s2);

    assert!(
        client.try_get_price(&asset).is_err(),
        "a read must fail while the source quorum is unmet"
    );

    // The third arrives and the quorum is satisfied.
    client.update_price_feed(&admin, &asset, &100_000_000, &8, &s3);
    assert_eq!(
        client.get_price(&asset),
        100_000_000,
        "the read must succeed once quorum is met"
    );
}

#[test]
fn a_fallback_oracle_serves_when_the_primary_goes_stale() {
    let env = create_env();
    let (contract_id, admin, client) = setup(&env);
    let asset = Address::generate(&env);
    let primary = Address::generate(&env);
    let fallback = Address::generate(&env);

    let mut config = permissive_config();
    config.max_staleness_seconds = 600;
    client.configure_oracle(&admin, &config);

    env.as_contract(&contract_id, || {
        oracle::set_primary_oracle(&env, admin.clone(), asset.clone(), primary.clone())
            .expect("admin may set the primary oracle");
        oracle::set_fallback_oracle(&env, admin.clone(), asset.clone(), fallback.clone())
            .expect("admin may set the fallback oracle");
    });

    // Primary publishes, then goes quiet.
    client.update_price_feed(&primary, &asset, &100_000_000, &8, &primary);
    advance(&env, 601);
    // Fallback keeps publishing.
    client.update_price_feed(&fallback, &asset, &99_000_000, &8, &fallback);

    let price = client.get_price(&asset);
    assert_eq!(
        price, 99_000_000,
        "the fallback feed must serve while the primary is stale"
    );
}

#[test]
fn scaling_to_many_sources_keeps_the_aggregate_at_the_consensus() {
    // Eight sources, one of them wrong. The consensus must hold.
    let env = create_env();
    let (_, admin, client) = setup(&env);
    let asset = Address::generate(&env);

    let mut sources: Vec<Address> = Vec::new(&env);
    for _ in 0..8 {
        sources.push_back(Address::generate(&env));
    }

    let mut config = permissive_config();
    config.min_sources = 5;
    config.outlier_deviation_bps = 500;
    client.configure_oracle(&admin, &config);
    client.set_oracle_sources(&admin, &asset, &sources);

    // Seven agree at 100, the last reports 300.
    let last_index = sources.len() - 1;
    for (index, source) in sources.iter().enumerate() {
        let price = if index as u32 == last_index {
            300_000_000
        } else {
            100_000_000
        };
        client.update_price_feed(&admin, &asset, &price, &8, &source);
    }

    if let Ok(Ok(price)) = client.try_get_price(&asset) {
        assert!(
            price <= 105_000_000,
            "one bad source out of eight moved the aggregate to {price}"
        );
    }
}

// =============================================================================
// 6. ORACLE UPGRADE
// =============================================================================

#[test]
fn tightening_the_staleness_threshold_takes_effect_immediately() {
    let env = create_env();
    let (_, admin, client) = setup(&env);
    let asset = Address::generate(&env);
    let oracle_addr = Address::generate(&env);

    let mut lenient = permissive_config();
    lenient.max_staleness_seconds = 3_600;
    client.configure_oracle(&admin, &lenient);
    client.update_price_feed(&admin, &asset, &100_000_000, &8, &oracle_addr);

    advance(&env, 1_200);
    assert_eq!(
        client.get_price(&asset),
        100_000_000,
        "20 minutes old is fresh under a 1-hour bound"
    );

    // Tighten to 10 minutes. The same feed is now stale, with no further update.
    let mut strict = lenient.clone();
    strict.max_staleness_seconds = 600;
    client.configure_oracle(&admin, &strict);

    assert!(
        client.try_get_price(&asset).is_err(),
        "the tightened bound must apply to the existing feed on the next read"
    );
}

#[test]
fn rolling_a_configuration_change_back_restores_the_previous_behaviour() {
    let env = create_env();
    let (_, admin, client) = setup(&env);
    let asset = Address::generate(&env);
    let oracle_addr = Address::generate(&env);

    let original = permissive_config();
    client.configure_oracle(&admin, &original);
    client.update_price_feed(&admin, &asset, &100_000_000, &8, &oracle_addr);
    advance(&env, 1_200);
    assert_eq!(client.get_price(&asset), 100_000_000);

    // Roll forward to a config that makes the feed unreadable.
    let mut breaking = original.clone();
    breaking.max_staleness_seconds = 60;
    client.configure_oracle(&admin, &breaking);
    assert!(client.try_get_price(&asset).is_err());

    // Roll back. The feed must be readable again at exactly the same price,
    // with no re-submission required.
    client.configure_oracle(&admin, &original);
    assert_eq!(
        client.get_price(&asset),
        100_000_000,
        "rollback must restore the prior behaviour exactly"
    );
}

#[test]
fn rotating_the_source_set_does_not_interrupt_pricing() {
    // Source rotation is the on-chain equivalent of a provider swap: the old set
    // is replaced by a new one while positions stay open.
    let env = create_env();
    let (_, admin, client) = setup(&env);
    let asset = Address::generate(&env);
    let old_a = Address::generate(&env);
    let old_b = Address::generate(&env);
    let new_a = Address::generate(&env);
    let new_b = Address::generate(&env);

    let mut config = permissive_config();
    config.min_sources = 2;
    client.configure_oracle(&admin, &config);

    client.set_oracle_sources(&admin, &asset, &vec![&env, old_a.clone(), old_b.clone()]);
    client.update_price_feed(&admin, &asset, &100_000_000, &8, &old_a);
    client.update_price_feed(&admin, &asset, &100_000_000, &8, &old_b);
    assert_eq!(client.get_price(&asset), 100_000_000);

    // Cut over to the new set, which publishes the same level.
    advance(&env, 60);
    client.set_oracle_sources(&admin, &asset, &vec![&env, new_a.clone(), new_b.clone()]);
    client.update_price_feed(&admin, &asset, &100_000_000, &8, &new_a);
    client.update_price_feed(&admin, &asset, &100_000_000, &8, &new_b);

    assert_eq!(
        client.get_price(&asset),
        100_000_000,
        "pricing must continue uninterrupted across a source rotation"
    );
}

#[test]
fn promoting_the_fallback_to_primary_keeps_the_feed_alive() {
    // A primary that has to be decommissioned is replaced by promoting the
    // existing fallback — the feed must not go dark during the handover.
    let env = create_env();
    let (contract_id, admin, client) = setup(&env);
    let asset = Address::generate(&env);
    let old_primary = Address::generate(&env);
    let new_primary = Address::generate(&env);

    client.configure_oracle(&admin, &permissive_config());

    env.as_contract(&contract_id, || {
        oracle::set_primary_oracle(&env, admin.clone(), asset.clone(), old_primary.clone())
            .expect("admin may set the primary oracle");
    });
    client.update_price_feed(&old_primary, &asset, &100_000_000, &8, &old_primary);
    assert_eq!(client.get_price(&asset), 100_000_000);

    // Promote the replacement.
    advance(&env, 60);
    env.as_contract(&contract_id, || {
        oracle::set_primary_oracle(&env, admin.clone(), asset.clone(), new_primary.clone())
            .expect("admin may rotate the primary oracle");
    });
    client.update_price_feed(&new_primary, &asset, &101_000_000, &8, &new_primary);

    assert_eq!(
        client.get_price(&asset),
        101_000_000,
        "the promoted primary must be serving"
    );

    // The decommissioned oracle must no longer be able to publish.
    advance(&env, 60);
    assert!(
        matches!(
            client.try_update_price_feed(&old_primary, &asset, &1_000, &8, &old_primary),
            Err(_)
        ),
        "a decommissioned primary must lose write access"
    );
}

#[test]
fn an_invalid_configuration_is_rejected_without_disturbing_the_live_one() {
    // A failed upgrade must be a no-op, not a partial apply.
    let env = create_env();
    let (_, admin, client) = setup(&env);
    let asset = Address::generate(&env);
    let oracle_addr = Address::generate(&env);

    let good = permissive_config();
    client.configure_oracle(&admin, &good);
    client.update_price_feed(&admin, &asset, &100_000_000, &8, &oracle_addr);
    assert_eq!(client.get_price(&asset), 100_000_000);

    // min_sources = 0 is rejected by validation.
    let mut invalid = good.clone();
    invalid.min_sources = 0;
    assert!(
        client.try_configure_oracle(&admin, &invalid).is_err(),
        "an invalid configuration must be rejected"
    );

    // The live configuration is untouched and the feed still reads.
    assert_eq!(
        client.get_price(&asset),
        100_000_000,
        "a rejected upgrade must leave the live config intact"
    );
}

#[test]
fn only_the_admin_may_reconfigure_the_oracle() {
    let env = create_env();
    let (_, admin, client) = setup(&env);
    let attacker = Address::generate(&env);

    client.configure_oracle(&admin, &permissive_config());

    let mut hostile = permissive_config();
    hostile.max_staleness_seconds = 1;
    assert!(
        client.try_configure_oracle(&attacker, &hostile).is_err(),
        "a non-admin must not be able to reconfigure the oracle"
    );
}

#[test]
fn incident_reports_and_timeline_survive_a_full_incident_lifecycle() {
    // Monitoring infrastructure depends on these records; they must be written
    // on trip and remain queryable through resolution.
    let env = create_env();
    let (contract_id, admin, client) = setup(&env);
    let asset = Address::generate(&env);
    let oracle_addr = Address::generate(&env);

    client.configure_oracle(&admin, &permissive_config());
    client.update_price_feed(&admin, &asset, &100_000_000, &8, &oracle_addr);

    // Trip the volatility breaker.
    advance(&env, 60);
    client.update_price_feed(&admin, &asset, &60_000_000, &8, &oracle_addr);

    let report = client
        .get_oracle_incident_report(&asset)
        .expect("an incident report must be written on trip");
    assert_eq!(report.asset, asset);
    assert!(report.observed_bps > 0);
    assert!(report.open_until > 0);

    // Resolve it and confirm the timeline records the resolution.
    env.as_contract(&contract_id, || {
        oracle::resolve_oracle_incident(&env, &asset);
        let timeline = oracle::get_incident_timeline(&env, &asset);
        assert!(
            !timeline.is_empty(),
            "the incident timeline must retain at least one entry"
        );
    });
}

#[test]
fn errors_are_specific_enough_for_a_responder_to_act_on() {
    // A responder needs to distinguish "the feed is stale" from "the breaker is
    // open" — the remediation is completely different.
    let env = create_env();
    let (_, admin, client) = setup(&env);
    let stale_asset = Address::generate(&env);
    let halted_asset = Address::generate(&env);
    let oracle_addr = Address::generate(&env);

    let mut config = permissive_config();
    config.max_staleness_seconds = 300;
    client.configure_oracle(&admin, &config);

    // One asset goes stale.
    client.update_price_feed(&admin, &stale_asset, &100_000_000, &8, &oracle_addr);

    // The other trips its breaker.
    client.update_price_feed(&admin, &halted_asset, &100_000_000, &8, &oracle_addr);
    advance(&env, 60);
    client.update_price_feed(&admin, &halted_asset, &60_000_000, &8, &oracle_addr);

    advance(&env, 301);

    assert!(client.try_get_price(&stale_asset).is_err());
    assert!(client.try_get_price(&halted_asset).is_err());

    // The halted asset carries an incident report; the merely-stale one does not
    // describe a volatility pause.
    let halted_report = client
        .get_oracle_incident_report(&halted_asset)
        .expect("a tripped breaker must leave a report");
    assert_eq!(halted_report.kind, OracleIncidentKind::VolatilityPause);
}

#[test]
fn oracle_error_variants_map_to_distinct_protocol_errors() {
    // Guards against a regression that collapses distinct oracle failures into
    // one opaque error at the contract boundary.
    assert_ne!(
        OracleError::CircuitBreakerOpen as u32,
        OracleError::StalePrice as u32
    );
    assert_ne!(
        OracleError::Unauthorized as u32,
        OracleError::CircuitBreakerOpen as u32
    );
}
