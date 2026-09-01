//! Feed quote aggregation strategies.
//!
//! Two strategies are supported:
//! - [`AggregationStrategy::Median`]: returns the median price across the
//!   non-outlier quotes (robust to individual corrupt feeds).
//! - [`AggregationStrategy::Weighted`]: returns a confidence- and weight-
//!   adjusted mean across the non-outlier quotes.
//!
//! Both strategies first reject outliers that deviate from the median by more
//! than [`OUTLIER_DEVIATION_BPS`].

use crate::types::{
    AggregationStrategy, FeedQuote, DEFAULT_FEED_WEIGHT_BPS, MAX_FEEDS_PER_ASSET,
    OUTLIER_DEVIATION_BPS,
};
use soroban_sdk::{Env, Vec};

/// Sorted copy of the quote prices (ascending), using insertion sort.
fn sorted_prices(env: &Env, quotes: &Vec<FeedQuote>) -> Vec<i128> {
    let n = quotes.len();
    let mut sorted = Vec::new(env);
    for i in 0..n {
        sorted.push_back(quotes.get(i).unwrap().price);
    }
    let mut i = 1;
    while i < n {
        let key = sorted.get(i).unwrap();
        let mut j = i;
        while j > 0 {
            let prev = sorted.get(j - 1).unwrap();
            if prev <= key {
                break;
            }
            sorted.set(j, prev);
            j -= 1;
        }
        sorted.set(j, key);
        i += 1;
    }
    sorted
}

/// Median price of the sorted list (upper median for even counts).
fn median_of(sorted: &Vec<i128>) -> i128 {
    sorted.get(sorted.len() / 2).unwrap()
}

/// Deviation in basis points of `price` from `reference`.
fn deviation_bps(price: i128, reference: i128) -> i128 {
    if reference <= 0 || price <= 0 {
        return i128::MAX;
    }
    let diff = if price > reference {
        price - reference
    } else {
        reference - price
    };
    diff.saturating_mul(10_000) / reference
}

/// Quotes that stay within [`OUTLIER_DEVIATION_BPS`] of `reference`.
fn filter_quotes(env: &Env, quotes: &Vec<FeedQuote>) -> Vec<FeedQuote> {
    let mut kept = Vec::new(env);
    if quotes.is_empty() {
        return kept;
    }
    let mut prices = Vec::new(env);
    for q in quotes.iter() {
        prices.push_back(q.price);
    }
    let reference = median_of(&sorted_prices(env, quotes));
    for q in quotes.iter() {
        if q.price <= 0 {
            continue;
        }
        if deviation_bps(q.price, reference) <= OUTLIER_DEVIATION_BPS {
            kept.push_back(q);
        }
    }
    kept
}

/// Median result: price = median of kept quotes, confidence = average
/// confidence of kept quotes, timestamp = latest timestamp of kept quotes.
fn median_result(env: &Env, kept: &Vec<FeedQuote>) -> (i128, u32, u64) {
    let price = {
        let mut prices = Vec::new(env);
        for q in kept.iter() {
            prices.push_back(q.price);
        }
        let mut sorted = Vec::new(env);
        for i in 0..prices.len() {
            sorted.push_back(prices.get(i).unwrap());
        }
        let mut i = 1;
        while i < sorted.len() {
            let key = sorted.get(i).unwrap();
            let mut j = i;
            while j > 0 {
                let prev = sorted.get(j - 1).unwrap();
                if prev <= key {
                    break;
                }
                sorted.set(j, prev);
                j -= 1;
            }
            sorted.set(j, key);
            i += 1;
        }
        sorted.get(sorted.len() / 2).unwrap()
    };

    let mut total_conf: u64 = 0;
    let mut latest_ts = 0u64;
    let count = kept.len();
    for q in kept.iter() {
        total_conf += q.confidence as u64;
        if q.timestamp > latest_ts {
            latest_ts = q.timestamp;
        }
    }
    let avg_conf = if count > 0 {
        (total_conf / count as u64) as u32
    } else {
        0
    };
    (price, avg_conf, latest_ts)
}

/// Weighted result over kept quotes. Effective weight is the feed-configured
/// `weight_bps` (default 10_000) scaled by confidence (0 falls back to 1) so
/// low-confidence sources are down-weighted.
fn weighted_result(kept: &Vec<FeedQuote>) -> (i128, u32, u64) {
    let mut weighted_sum: i128 = 0;
    let mut conf_weighted_sum: i128 = 0;
    let mut total_weight: i128 = 0;
    let mut latest_ts = 0u64;

    for q in kept.iter() {
        let feed_weight = if q.weight_bps == 0 {
            DEFAULT_FEED_WEIGHT_BPS as i128
        } else {
            q.weight_bps as i128
        };
        let conf = if q.confidence == 0 { 1 } else { q.confidence };
        let weight = feed_weight.saturating_mul(conf as i128);
        weighted_sum = weighted_sum.saturating_add(q.price.saturating_mul(weight));
        conf_weighted_sum =
            conf_weighted_sum.saturating_add((q.confidence as i128).saturating_mul(weight));
        total_weight = total_weight.saturating_add(weight);
        if q.timestamp > latest_ts {
            latest_ts = q.timestamp;
        }
    }

    if total_weight <= 0 {
        // No usable weights: fall back to a plain average.
        let mut sum: i128 = 0;
        let mut conf_sum: u64 = 0;
        let count = kept.len();
        for q in kept.iter() {
            sum = sum.saturating_add(q.price);
            conf_sum += q.confidence as u64;
        }
        let avg_price = if count > 0 { sum / count as i128 } else { 0 };
        let avg_conf = if count > 0 {
            (conf_sum / count as u64) as u32
        } else {
            0
        };
        return (avg_price, avg_conf, latest_ts);
    }

    let price = weighted_sum / total_weight;
    let conf = (conf_weighted_sum / total_weight) as u32;
    (price, conf, latest_ts)
}

/// Aggregate `quotes` using `strategy`.
///
/// Returns `(price, confidence, timestamp)`. A single quote short-circuits the
/// aggregation to avoid unnecessary work.
pub fn aggregate(
    env: &Env,
    quotes: Vec<FeedQuote>,
    strategy: AggregationStrategy,
) -> Result<(i128, u32, u64), &'static str> {
    let n = quotes.len();
    if n == 0 {
        return Err("No feeds to aggregate");
    }
    if n == 1 {
        let q = quotes.get(0).unwrap();
        return Ok((q.price, q.confidence, q.timestamp));
    }
    if n > MAX_FEEDS_PER_ASSET {
        return Err("Too many feeds to aggregate");
    }

    let mut kept = filter_quotes(env, &quotes);
    if kept.is_empty() {
        // Defensive: every quote was an outlier; degrade to the full set so
        // the system never bricks on pathological inputs.
        for q in quotes.iter() {
            kept.push_back(q);
        }
    }

    let result = match strategy {
        AggregationStrategy::Median => median_result(env, &kept),
        AggregationStrategy::Weighted => weighted_result(&kept),
    };
    Ok(result)
}
