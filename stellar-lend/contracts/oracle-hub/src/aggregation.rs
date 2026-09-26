//! Feed quote aggregation across multiple sources.
//!
//! Aggregation runs in three stages:
//!
//! 1. **Selection** ([`crate::fallback`]) decides *which* sources are trusted:
//!    a source whose price deviates from the median of the others beyond the
//!    configured band is dropped, and the deviation is reported back to the
//!    caller.
//! 2. **Outlier filtering** ([`filter_quotes`]) repeats the deviation test
//!    inside the selected set. It protects the weighted strategies, where a
//!    single extreme quote moves the mean, and it is the only defence left
//!    when the selection stage was skipped (fewer than three sources).
//! 3. **Combination** ([`median_result`], [`weighted_result`],
//!    [`trimmed_mean_result`]) folds the surviving quotes into one price plus
//!    a confidence score and the newest contributing timestamp.
//!
//! Every stage is bounded by [`MAX_FEEDS_PER_ASSET`], and the sort is a plain
//! insertion sort so the cost is quadratic only in a constant at most five.

use crate::types::{AggregationStrategy, FeedQuote, DEFAULT_FEED_WEIGHT_BPS, MAX_FEEDS_PER_ASSET};
use soroban_sdk::{Env, Vec};

/// Aggregated price plus the metadata the resolver reports on-chain.
#[derive(Clone, Debug, PartialEq)]
pub struct AggregationOutcome {
    /// Combined price in canonical decimals.
    pub price: i128,
    /// Confidence score of the result, in the provider's own 0..=100 scale.
    pub confidence: u32,
    /// Newest timestamp among the surviving quotes.
    pub timestamp: u64,
    /// How many quotes survived filtering.
    pub kept: u32,
    /// How many quotes were dropped as outliers or non-positive.
    pub rejected: u32,
    /// Largest deviation (in basis points) inside the surviving set.
    pub deviation_bps: i128,
    /// Shortest freshness budget among the surviving quotes.
    pub min_stale_threshold: u64,
}

/// Sorted copy of the quote prices (ascending), using insertion sort.
pub(crate) fn sorted_prices(env: &Env, quotes: &Vec<FeedQuote>) -> Vec<i128> {
    let n = quotes.len();
    let mut sorted = Vec::new(env);
    for i in 0..n {
        if let Some(quote) = quotes.get(i) {
            sorted.push_back(quote.price);
        }
    }
    let mut i = 1u32;
    while i < n {
        let key = sorted.get(i).unwrap_or(0);
        let mut j = i;
        while j > 0 {
            let prev = sorted.get(j - 1).unwrap_or(0);
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

/// Median price of the sorted list (lower median for even counts).
pub(crate) fn median_of(sorted: &Vec<i128>) -> i128 {
    sorted.get((sorted.len() - 1) / 2).unwrap_or(0)
}

/// Deviation in basis points of `price` from `reference`.
///
/// Returns `i128::MAX` when either side is non-positive so that a zero or
/// negative quote can never be treated as agreeing with the reference.
pub fn deviation_bps(price: i128, reference: i128) -> i128 {
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

/// Quotes that stay within `max_deviation_bps` of `reference`.
///
/// Quotes with a non-positive price are always dropped.
pub fn filter_quotes(
    env: &Env,
    quotes: &Vec<FeedQuote>,
    reference: i128,
    max_deviation_bps: i128,
) -> Vec<FeedQuote> {
    let mut kept = Vec::new(env);
    for q in quotes.iter() {
        if q.price <= 0 {
            continue;
        }
        if deviation_bps(q.price, reference) <= max_deviation_bps {
            kept.push_back(q);
        }
    }
    kept
}

/// Median result: price = median of kept quotes, confidence = average
/// confidence of kept quotes, timestamp = latest timestamp of kept quotes.
fn median_result(env: &Env, kept: &Vec<FeedQuote>) -> (i128, u32, u64) {
    let sorted = sorted_prices(env, kept);
    let price = median_of(&sorted);

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

/// Trimmed-mean result: the highest and lowest quotes are dropped and the rest
/// is averaged. With three or more sources this removes one manipulated quote
/// from either tail, which a plain mean would absorb.
///
/// Falls back to the median of the kept quotes for the degenerate inputs (no
/// quotes, or a single quote where trimming would leave nothing).
fn trimmed_mean_result(env: &Env, kept: &Vec<FeedQuote>) -> (i128, u32, u64) {
    let count = kept.len();
    if count < 3 {
        return median_result(env, kept);
    }

    let sorted = sorted_prices(env, kept);
    let mut sum: i128 = 0;
    let mut included: u32 = 0;
    // Drop the lowest and the highest price, then average the middle.
    let mut i = 1u32;
    while i + 1 < count {
        sum = sum.saturating_add(sorted.get(i).unwrap_or(0));
        included += 1;
        i += 1;
    }

    let mut total_conf: u64 = 0;
    let mut latest_ts = 0u64;
    for q in kept.iter() {
        total_conf += q.confidence as u64;
        if q.timestamp > latest_ts {
            latest_ts = q.timestamp;
        }
    }

    let price = if included > 0 {
        sum / included as i128
    } else {
        median_of(&sorted)
    };
    let conf = if included > 0 {
        (total_conf / count as u64) as u32
    } else {
        0
    };
    (price, conf, latest_ts)
}

/// Aggregate `quotes` using `strategy`, rejecting outliers that deviate from
/// the median by more than `max_deviation_bps`.
///
/// Returns the combined price with its metadata, or a reason string when the
/// quote set cannot produce a trustworthy price. Callers must treat any `Err`
/// as "no price": the hub never publishes a partially validated aggregate.
pub fn aggregate(
    env: &Env,
    quotes: &Vec<FeedQuote>,
    strategy: AggregationStrategy,
    max_deviation_bps: i128,
) -> Result<AggregationOutcome, &'static str> {
    let n = quotes.len();
    if n == 0 {
        return Err("No feeds to aggregate");
    }
    if n > MAX_FEEDS_PER_ASSET {
        return Err("Too many feeds to aggregate");
    }
    if n == 1 {
        let q = match quotes.get(0) {
            Some(q) => q,
            None => return Err("No feeds to aggregate"),
        };
        if q.price <= 0 {
            return Err("Non-positive price");
        }
        return Ok(AggregationOutcome {
            price: q.price,
            confidence: q.confidence,
            timestamp: q.timestamp,
            kept: 1,
            rejected: 0,
            deviation_bps: 0,
            min_stale_threshold: q.stale_threshold_seconds,
        });
    }

    let reference = median_of(&sorted_prices(env, quotes));
    let kept = filter_quotes(env, quotes, reference, max_deviation_bps);
    if kept.len() < 2 {
        // A single surviving quote is not enough to distinguish an honest
        // source from a corrupt one when the feed set has multiple sources.
        // Returning an error is safer than allowing the unvalidated quote to
        // control the aggregate price.
        return Err("Insufficient feeds after outlier filtering");
    }

    let (price, confidence, timestamp) = match strategy {
        AggregationStrategy::Median => median_result(env, &kept),
        AggregationStrategy::Weighted => weighted_result(&kept),
        AggregationStrategy::TrimmedMean => trimmed_mean_result(env, &kept),
    };

    // Re-anchor the deviation report on the surviving set so callers learn how
    // far the sources they actually used disagree with each other.
    let kept_reference = median_of(&sorted_prices(env, &kept));
    let mut spread = 0i128;
    let mut min_stale = u64::MAX;
    for q in kept.iter() {
        let d = deviation_bps(q.price, kept_reference);
        if d > spread {
            spread = d;
        }
        if q.stale_threshold_seconds < min_stale {
            min_stale = q.stale_threshold_seconds;
        }
    }

    Ok(AggregationOutcome {
        price,
        confidence,
        timestamp,
        kept: kept.len(),
        rejected: n - kept.len(),
        deviation_bps: spread,
        min_stale_threshold: if min_stale == u64::MAX { 0 } else { min_stale },
    })
}
