//! Tests for the manipulation-resistant TWAP oracle (#1033).
//!
//! Three groups, matching the three defences the contract claims:
//!
//! - **Time weighting**: a price that held longer moves the average more.
//! - **Manipulation resistance**: a single spike is rejected and cannot move
//!   the average, whatever the surrounding prices are.
//! - **Bounds and observability**: the ring buffer is bounded, stale feeds are
//!   reported, and governance has an audited escape hatch.

extern crate std;

use super::*;
use soroban_sdk::testutils::{Address as _, Events, Ledger};
use soroban_sdk::xdr::{ContractEventBody, ScVal};
use soroban_sdk::{IntoVal, Symbol, TryFromVal, Val};

struct Fixture {
    env: Env,
    client: TwapOracleClient<'static>,
    admin: Address,
    asset: Address,
}

fn setup() -> Fixture {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let asset = Address::generate(&env);
    let contract_id = env.register(TwapOracle, ());
    let client = TwapOracleClient::new(&env, &contract_id);
    client.initialize(&admin);
    Fixture {
        env,
        client,
        admin,
        asset,
    }
}

/// Record `price` at `timestamp` seconds into the fixture's ledger.
fn at(fx: &Fixture, price: i128, timestamp: u64) {
    fx.env.ledger().set_timestamp(timestamp);
    fx.client.record_price(&fx.asset, &price);
}

/// Record `price` repeatedly until the average covers exactly
/// `coverage_secs` seconds.
///
/// The first observation seeds the accumulator with one second of weight, so
/// the last sample has to land at `coverage_secs - 1` for the total to come
/// out exact.
fn seed_flat(fx: &Fixture, price: i128, coverage_secs: u64) {
    let end = coverage_secs.saturating_sub(1);
    at(fx, price, 0);
    at(fx, price, end / 3);
    at(fx, price, 2 * end / 3);
    at(fx, price, end);
}

fn default_window() -> u64 {
    DEFAULT_WINDOW_SECS
}

// ── Configuration ────────────────────────────────────────────────────────────

#[test]
fn test_initialize_sets_defaults() {
    let fx = setup();
    let config = fx.client.get_config().unwrap();
    assert_eq!(config.admin, fx.admin);
    assert_eq!(config.window_secs, DEFAULT_WINDOW_SECS);
    assert_eq!(config.max_deviation_bps, DEFAULT_MAX_DEVIATION_BPS);
    assert_eq!(config.min_samples, DEFAULT_MIN_SAMPLES);
    // The staleness limit defaults to twice the window.
    assert_eq!(config.max_staleness_secs, DEFAULT_WINDOW_SECS * 2);
}

#[test]
fn test_double_initialize_fails() {
    let fx = setup();
    assert!(fx.client.try_initialize(&fx.admin).is_err());
}

#[test]
fn test_set_config_updates_every_field() {
    let fx = setup();
    fx.client
        .set_config(&fx.admin, &3600u64, &1000i128, &5u32, &7200u64);

    let config = fx.client.get_config().unwrap();
    assert_eq!(config.window_secs, 3600);
    assert_eq!(config.max_deviation_bps, 1000);
    assert_eq!(config.min_samples, 5);
    assert_eq!(config.max_staleness_secs, 7200);
}

#[test]
fn test_set_config_derives_staleness_from_the_window() {
    let fx = setup();
    fx.client
        .set_config(&fx.admin, &600u64, &500i128, &2u32, &0u64);
    assert_eq!(fx.client.get_config().unwrap().max_staleness_secs, 1200);
}

#[test]
fn test_set_config_rejects_nonsense() {
    let fx = setup();
    let bad = [
        (0u64, 500i128, 3u32, 600u64),     // empty window
        (u64::MAX, 500i128, 3u32, 600u64), // window beyond the 24 h bound
        (600, 0, 3, 1200),                 // no deviation band
        (600, 10_001, 3, 1200),            // band beyond 100 %
        (600, 500, 0, 1200),               // no samples
        (600, 500, 33, 1200),              // more samples than the ring holds
        (600, 500, 3, 300),                // staleness below the window
    ];
    for (window, bps, samples, staleness) in bad {
        assert!(
            fx.client
                .try_set_config(&fx.admin, &window, &bps, &samples, &staleness)
                .is_err(),
            "config {window}/{bps}/{samples}/{staleness} should be rejected"
        );
    }
}

#[test]
fn test_non_admin_cannot_reconfigure() {
    let fx = setup();
    let stranger = Address::generate(&fx.env);
    fx.env.mock_all_auths();
    assert!(fx
        .client
        .try_set_config(&stranger, &600u64, &500i128, &3u32, &1200u64)
        .is_err());
}

#[test]
fn test_record_rejects_non_positive_prices() {
    let fx = setup();
    assert!(fx.client.try_record_price(&fx.asset, &0).is_err());
    assert!(fx.client.try_record_price(&fx.asset, &-1).is_err());
}

// ── Time weighting ───────────────────────────────────────────────────────────

#[test]
fn test_first_observation_seeds_the_average() {
    let fx = setup();
    at(&fx, 1000, 0);

    let acc = fx.client.get_accumulator(&fx.asset).unwrap();
    assert_eq!(acc.twap, 1000);
    assert_eq!(acc.total_time, 1);
    assert_eq!(acc.sample_count, 1);
    assert_eq!(acc.last_price, 1000);
}

#[test]
fn test_twap_weights_by_elapsed_time() {
    let fx = setup();
    let window = default_window();
    seed_flat(&fx, 1000, window);

    // 1000 held for the whole window, then 1040 is observed twice, 100 s
    // apart. 4 % is inside the 5 % band, so both are accepted.
    at(&fx, 1_040, window + 100);
    at(&fx, 1_040, window + 200);

    // The 1000 is credited for every second up to its last observation and the
    // 1040 only for the 100 s it was actually in effect.
    let acc = fx.client.get_accumulator(&fx.asset).unwrap();
    assert_eq!(
        acc.price_sum,
        1000i128 * (window + 101) as i128 + 1040i128 * 100
    );
    assert_eq!(acc.total_time, window + 201);
    assert_eq!(acc.twap, acc.price_sum / acc.total_time as i128);
    // The move shifted the average by roughly its 100 s share of the window.
    // An arithmetic mean over samples would have counted the 1040 three times
    // out of six and landed far higher.
    assert!(acc.twap > 1000 && acc.twap < 1030, "twap was {}", acc.twap);
    assert_eq!(acc.rejected_count, 0);
}

#[test]
fn test_a_brief_move_is_weighed_by_its_duration() {
    let fx = setup();
    let window = default_window();
    seed_flat(&fx, 1000, window);

    // One second at +4 %, then straight back.
    at(&fx, 1_040, window);
    at(&fx, 1000, window + 1);

    let acc = fx.client.get_accumulator(&fx.asset).unwrap();
    assert_eq!(acc.total_time, window + 2);
    // The blip moved the average by a fraction of a percent, where a mean over
    // sample counts would have folded in the full 4 %.
    assert!(acc.twap >= 999 && acc.twap <= 1000, "twap was {}", acc.twap);
    assert_eq!(acc.rejected_count, 0);
}

#[test]
fn test_duplicate_observations_in_one_second_do_not_inflate_weight() {
    let fx = setup();
    at(&fx, 1000, 0);
    at(&fx, 1000, 0);
    at(&fx, 1000, 0);

    let acc = fx.client.get_accumulator(&fx.asset).unwrap();
    // Same ledger second: no time elapsed, so no weighting at all.
    assert_eq!(acc.total_time, 1);
    assert_eq!(acc.twap, 1000);
    assert_eq!(acc.sample_count, 3);
}

#[test]
fn test_window_coverage_is_reported() {
    let fx = setup();
    let window = default_window();
    seed_flat(&fx, 1000, window / 2);

    let result = fx.client.get_twap(&fx.asset);
    assert_eq!(result.coverage_secs, window / 2);
    assert_eq!(result.window_secs, window);
    assert_eq!(result.window_coverage_bps, 5_000);
    // Half a window is not enough to be trusted.
    assert!(result.used_fallback);
}

#[test]
fn test_full_window_is_trusted() {
    let fx = setup();
    let window = default_window();
    seed_flat(&fx, 1000, window);

    let result = fx.client.get_twap(&fx.asset);
    assert_eq!(result.twap, 1000);
    assert_eq!(result.sample_count, 4);
    assert_eq!(result.window_coverage_bps, 10_000);
    assert!(result.fresh);
    assert!(!result.manipulation_detected);
    assert!(!result.used_fallback);
}

#[test]
fn test_too_few_samples_still_falls_back() {
    let fx = setup();
    seed_flat(&fx, 1000, 60);
    // Four accepted samples are short of the nine governance now demands.
    fx.client
        .set_config(&fx.admin, &60u64, &500i128, &9u32, &600u64);

    let result = fx.client.get_twap(&fx.asset);
    assert!(result.used_fallback);
    assert_eq!(result.sample_count, 4);
    assert_eq!(result.coverage_secs, 60);
    // The liquidation path still gives the caller something to work with.
    assert_eq!(fx.client.get_liquidation_price(&fx.asset).twap, 1000);
}

// ── Manipulation resistance ──────────────────────────────────────────────────

#[test]
fn test_spike_is_rejected_and_does_not_move_the_average() {
    let fx = setup();
    let window = default_window();
    seed_flat(&fx, 1000, window);

    let accepted = fx.client.record_price(&fx.asset, &10_000i128);
    assert!(!accepted);

    let result = fx.client.get_twap(&fx.asset);
    // A 10x print leaves the average exactly where it was.
    assert_eq!(result.twap, 1000);
    assert!(result.manipulation_detected);
    assert!(result.used_fallback);
    assert_eq!(result.rejected_count, 1);
    assert_eq!(result.spot_price, 1000);
}

#[test]
fn test_rejection_keeps_the_window_filling() {
    let fx = setup();
    fx.client
        .set_config(&fx.admin, &600u64, &500i128, &1u32, &1_200u64);
    seed_flat(&fx, 1000, 300);

    // A burst of garbage must not stop the average from maturing: the time is
    // still covered, by the price that was already accepted.
    for i in 1..=5u64 {
        fx.env.ledger().set_timestamp(300 + 100 * i);
        fx.client.record_price(&fx.asset, &100_000i128);
    }

    let acc = fx.client.get_accumulator(&fx.asset).unwrap();
    assert!(acc.manipulation_pending);
    assert_eq!(acc.rejected_count, 5);
    assert!(acc.total_time >= 600, "covered {} s", acc.total_time);
    // The average still tracks the honest price.
    assert_eq!(acc.twap, 1000);
}

#[test]
fn test_dip_is_rejected_too() {
    let fx = setup();
    let window = default_window();
    seed_flat(&fx, 1000, window);

    // Cheapening the asset is the attack that matters for borrowing.
    let accepted = fx.client.record_price(&fx.asset, &10i128);
    assert!(!accepted);
    assert_eq!(fx.client.get_twap(&fx.asset).twap, 1000);
}

#[test]
fn test_accepted_reprice_clears_the_manipulation_flag() {
    let fx = setup();
    let window = default_window();
    seed_flat(&fx, 1000, window);

    fx.client.record_price(&fx.asset, &10_000i128);
    assert!(fx.client.get_twap(&fx.asset).manipulation_detected);

    // A price inside the band clears the flag: the attack has stopped.
    at(&fx, 1010, window + 10);
    let result = fx.client.get_twap(&fx.asset);
    assert!(!result.manipulation_detected);
    assert!(!result.used_fallback);
    // The rejected observation is still on the record.
    assert_eq!(result.rejected_count, 1);
}

#[test]
fn test_governance_can_widen_the_band() {
    let fx = setup();
    let window = default_window();
    seed_flat(&fx, 1000, window);
    fx.client
        .set_config(&fx.admin, &window, &2_000i128, &3u32, &(window * 2));

    // 10 % is outside the 5 % default and inside a 20 % band.
    let accepted = fx.client.record_price(&fx.asset, &1_100i128);
    assert!(accepted);
    assert_eq!(fx.client.get_twap(&fx.asset).rejected_count, 0);
}

#[test]
fn test_forced_record_reseeds_and_is_auditable() {
    let fx = setup();
    let window = default_window();
    seed_flat(&fx, 1000, window);

    fx.env.ledger().set_timestamp(window + 10);
    fx.client.force_record_price(&fx.asset, &5_000i128);
    // The event log only survives the call that produced it.
    assert!(has_event(&fx, "price_force_recorded_event"));
    assert!(has_event(&fx, "accumulator_reseeded_event"));

    let result = fx.client.get_twap(&fx.asset);
    // History is not blended: the forced price starts a fresh accumulator.
    assert_eq!(result.twap, 5_000);
    assert_eq!(result.coverage_secs, 1);
    assert!(!result.manipulation_detected);
}

#[test]
fn test_deviation_check_rejects_an_off_band_spot_price() {
    let fx = setup();
    let window = default_window();
    seed_flat(&fx, 1000, window);

    assert!(fx.client.try_check_deviation(&fx.asset, &1010i128).is_ok());
    assert!(fx
        .client
        .try_check_deviation(&fx.asset, &2_000i128)
        .is_err());
}

#[test]
fn test_deviation_check_refuses_a_stale_average() {
    let fx = setup();
    let window = default_window();
    seed_flat(&fx, 1000, window);

    // Past the staleness limit the average no longer says anything about the
    // current market, so it must not vouch for a spot price.
    fx.env.ledger().set_timestamp(window * 3 + 1);
    assert!(fx.client.try_check_deviation(&fx.asset, &1000i128).is_err());
}

#[test]
fn test_deviation_check_needs_enough_samples() {
    let fx = setup();
    at(&fx, 1000, 0);
    assert!(fx.client.try_check_deviation(&fx.asset, &1000i128).is_err());
}

// ── Staleness and lifecycle ──────────────────────────────────────────────────

#[test]
fn test_stale_feed_is_reported() {
    let fx = setup();
    let window = default_window();
    seed_flat(&fx, 1000, window);

    // The staleness limit is exactly twice the window, measured from the last
    // observation.
    let last_update = window - 1;
    fx.env.ledger().set_timestamp(last_update + window * 2);
    assert!(fx.client.get_twap(&fx.asset).fresh);

    fx.env.ledger().set_timestamp(last_update + window * 2 + 1);
    let result = fx.client.get_twap(&fx.asset);
    assert!(!result.fresh);
    assert!(result.used_fallback);
    assert_eq!(
        fx.client.get_health(&fx.asset).unwrap().age_secs,
        window * 2 + 1
    );
}

#[test]
fn test_long_silence_reseeds_instead_of_crediting_the_stale_price() {
    let fx = setup();
    let window = default_window();
    seed_flat(&fx, 1000, window);

    // The feed goes quiet well past the staleness limit and comes back with a
    // new price inside the band. The old price must not be credited for the
    // silence, otherwise the new price would look like a huge deviation.
    fx.env.ledger().set_timestamp(window * 10);
    let accepted = fx.client.record_price(&fx.asset, &1_020i128);
    assert!(accepted);
    assert!(has_event(&fx, "accumulator_reseeded_event"));

    let acc = fx.client.get_accumulator(&fx.asset).unwrap();
    assert_eq!(acc.twap, 1_020);
    assert_eq!(acc.total_time, 1);
    assert_eq!(acc.sample_count, 1);
}

#[test]
fn test_reset_clears_history() {
    let fx = setup();
    seed_flat(&fx, 1000, default_window());
    assert!(fx.client.get_health(&fx.asset).is_some());

    fx.client.reset_asset(&fx.asset);

    assert!(fx.client.get_accumulator(&fx.asset).is_none());
    assert!(fx.client.get_health(&fx.asset).is_none());
    assert!(fx.client.get_observations(&fx.asset).is_empty());
    assert!(fx.client.get_twap(&fx.asset).used_fallback);
}

#[test]
fn test_unknown_asset_reports_a_fallback() {
    let fx = setup();
    let other = Address::generate(&fx.env);
    let result = fx.client.get_twap(&other);
    assert!(result.used_fallback);
    assert_eq!(result.twap, 0);
    assert_eq!(result.sample_count, 0);
    assert!(!result.fresh);
}

// ── Ring buffer ──────────────────────────────────────────────────────────────

#[test]
fn test_observations_are_kept_in_order() {
    let fx = setup();
    fx.client
        .set_config(&fx.admin, &600u64, &5_000i128, &1u32, &1_200u64);
    for i in 1..=4u64 {
        at(&fx, 1_000 + i as i128, i * 10);
    }

    let observations = fx.client.get_observations(&fx.asset);
    assert_eq!(observations.len(), 4);
    let timestamps: std::vec::Vec<u64> = observations.iter().map(|o| o.timestamp).collect();
    assert_eq!(timestamps, [10, 20, 30, 40]);
}

#[test]
fn test_observation_ring_is_bounded() {
    let fx = setup();
    // A wide band so nothing is rejected while flooding the buffer.
    fx.client
        .set_config(&fx.admin, &600u64, &10_000i128, &1u32, &1_200u64);

    for i in 0..(MAX_OBSERVATIONS as u64 * 3) {
        at(&fx, 1_000, i);
    }

    let observations = fx.client.get_observations(&fx.asset);
    assert_eq!(observations.len(), MAX_OBSERVATIONS);
    // The oldest entries were overwritten, the newest ones survived.
    let last = observations.get(observations.len() - 1).unwrap();
    assert_eq!(last.timestamp, MAX_OBSERVATIONS as u64 * 3 - 1);
    let first = observations.get(0).unwrap();
    assert_eq!(first.timestamp, MAX_OBSERVATIONS as u64 * 2);
}

#[test]
fn test_rejected_observations_are_auditable() {
    let fx = setup();
    let window = default_window();
    seed_flat(&fx, 1000, window);

    at(&fx, 9_999, window + 5);
    assert!(has_event(&fx, "price_rejected_event"));
    assert!(!has_event(&fx, "price_recorded_event"));

    let observations = fx.client.get_observations(&fx.asset);
    let last = observations.get(observations.len() - 1).unwrap();
    assert_eq!(last.price, 9_999);
    assert!(!last.accepted);

    at(&fx, 1_005, window + 10);
    assert!(has_event(&fx, "price_recorded_event"));
}

#[test]
fn test_observation_carries_its_weight() {
    let fx = setup();
    at(&fx, 1_000, 0);
    at(&fx, 1_100, 60);

    let observations = fx.client.get_observations(&fx.asset);
    let last = observations.get(observations.len() - 1).unwrap();
    // The 1100 observation is what the 1000 was weighted by.
    assert_eq!(last.weighted_seconds, 60);
}

#[test]
fn test_liquidation_price_refuses_a_manipulated_average() {
    let fx = setup();
    let window = default_window();
    seed_flat(&fx, 1000, window);

    let result = fx.client.get_liquidation_price(&fx.asset);
    assert_eq!(result.twap, 1000);
    assert!(!result.manipulation_detected);

    // Under attack the caller is told, and the raw TWAP is still the average
    // rather than the manipulated spot price.
    fx.client.record_price(&fx.asset, &2_000i128);
    let attacked = fx.client.get_liquidation_price(&fx.asset);
    assert!(attacked.manipulation_detected);
    assert_ne!(attacked.twap, 2_000);
    assert_eq!(attacked.twap, 1000);
}

#[test]
fn test_health_snapshot_matches_the_result() {
    let fx = setup();
    let window = default_window();
    seed_flat(&fx, 1000, window);
    fx.client.record_price(&fx.asset, &5_000i128);
    fx.env.ledger().set_timestamp(window + 10);

    let health = fx.client.get_health(&fx.asset).unwrap();
    let result = fx.client.get_twap(&fx.asset);
    assert_eq!(health.twap, result.twap);
    assert_eq!(health.coverage_secs, result.coverage_secs);
    assert_eq!(health.manipulation_pending, result.manipulation_detected);
    assert!(health.manipulation_pending);
    assert_eq!(health.rejected_count, 1);
}

/// Whether the most recent contract call published an event with this name.
///
/// The test host resets the event log on every top-level call, so this has to
/// be read straight after the call that emits the event.
fn has_event(fx: &Fixture, name: &str) -> bool {
    let name_val: Val = Symbol::new(&fx.env, name).into_val(&fx.env);
    let expected = ScVal::try_from_val(&fx.env, &name_val).unwrap();
    fx.env.events().all().events().iter().any(|event| {
        let ContractEventBody::V0(body) = &event.body;
        body.topics.first() == Some(&expected)
    })
}
