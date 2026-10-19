//! Multi-source fallback with deviation checks.
//!
//! Aggregation alone protects against a corrupted source only when the
//! majority of sources are honest. This module adds the second, explicit
//! layer: before the surviving quotes are folded into a price, the *leading*
//! source is checked against the median of the others and demoted when it
//! strays further than the configured band.
//!
//! Demotion is deliberately conservative:
//!
//! - It needs a quorum. With fewer than three candidate sources there is no
//!   independent reference to compare against, so the leading source is left
//!   alone and the aggregation stage decides on its own.
//! - It can only *remove* sources, never invent a price. A demoted source is
//!   dropped from the set; the remaining quotes are aggregated as usual.
//! - If demotion would leave fewer sources than the asset requires, the read
//!   fails closed instead of quietly serving a thinner consensus.
//!
//! The full decision is emitted on-chain ([`DeviationRejectedEvent`] and
//! [`FallbackActivatedEvent`]) so a keeper or a monitor can prove which
//! source was demoted and why.

use crate::aggregation::{deviation_bps, median_of, sorted_prices};
use crate::types::{DeviationRejectedEvent, FallbackActivatedEvent, FeedQuote};
use soroban_sdk::{Bytes, Env, Vec};

/// Number of candidates below which no source may be demoted.
const MIN_DEMOTION_QUORUM: u32 = 3;

/// Outcome of the deviation check.
#[derive(Clone, Debug, PartialEq)]
pub struct Selection {
    /// Quotes that survived the check, in the order they were collected.
    pub accepted: Vec<FeedQuote>,
    /// Number of candidates that were demoted.
    pub rejected: u32,
    /// Slot of the leading candidate.
    pub leading_priority: u32,
    /// Deviation (bps) of the leading candidate from the reference, or 0 when
    /// fewer than three sources were available.
    pub deviation_bps: i128,
    /// True when the leading source was demoted and a lower slot took over.
    pub demoted_leading: bool,
}

/// Median of the quotes that are not the leading candidate.
fn reference_excluding_leading(
    env: &Env,
    quotes: &Vec<FeedQuote>,
    leading_priority: u32,
) -> (i128, u32) {
    let mut others = Vec::new(env);
    for q in quotes.iter() {
        if q.priority != leading_priority {
            others.push_back(q);
        }
    }
    if others.is_empty() {
        return (0, 0);
    }
    let reference = median_of(&sorted_prices(env, &others));
    (reference, others.len())
}

/// Price of the highest-priority candidate, and the slot it came from.
fn leading_quote(quotes: &Vec<FeedQuote>) -> Option<FeedQuote> {
    let mut leading: Option<FeedQuote> = None;
    for q in quotes.iter() {
        match &leading {
            None => leading = Some(q),
            Some(current) => {
                if q.priority < current.priority {
                    leading = Some(q);
                }
            }
        }
    }
    leading
}

/// Reject the leading source when it deviates from the rest by more than
/// `max_deviation_bps`.
///
/// `max_deviation_bps` of `0` disables the check entirely.
pub fn select_sources(
    env: &Env,
    asset: &Bytes,
    quotes: &Vec<FeedQuote>,
    max_deviation_bps: i128,
) -> Selection {
    let candidates = quotes.len();
    let leading = match leading_quote(quotes) {
        Some(q) => q,
        None => {
            return Selection {
                accepted: Vec::new(env),
                rejected: 0,
                leading_priority: 0,
                deviation_bps: 0,
                demoted_leading: false,
            }
        }
    };

    if max_deviation_bps == 0 || candidates < MIN_DEMOTION_QUORUM {
        // Not enough independent sources to challenge the leader.
        return Selection {
            accepted: quotes.clone(),
            rejected: 0,
            leading_priority: leading.priority,
            deviation_bps: 0,
            demoted_leading: false,
        };
    }

    let (reference, others) = reference_excluding_leading(env, quotes, leading.priority);
    if reference <= 0 || others == 0 {
        return Selection {
            accepted: quotes.clone(),
            rejected: 0,
            leading_priority: leading.priority,
            deviation_bps: 0,
            demoted_leading: false,
        };
    }

    let deviation = deviation_bps(leading.price, reference);
    if deviation <= max_deviation_bps {
        return Selection {
            accepted: quotes.clone(),
            rejected: 0,
            leading_priority: leading.priority,
            deviation_bps: deviation,
            demoted_leading: false,
        };
    }

    let mut accepted = Vec::new(env);
    for q in quotes.iter() {
        if q.priority != leading.priority {
            accepted.push_back(q);
        }
    }

    DeviationRejectedEvent {
        asset: asset.clone(),
        priority: leading.priority,
        price: leading.price,
        reference,
        deviation_bps: deviation,
    }
    .publish(env);

    FallbackActivatedEvent {
        asset: asset.clone(),
        rejected_priority: leading.priority,
        serving_priority: lowest_priority(&accepted),
        remaining_sources: accepted.len(),
    }
    .publish(env);

    Selection {
        accepted,
        rejected: 1,
        leading_priority: leading.priority,
        deviation_bps: deviation,
        demoted_leading: true,
    }
}

/// Slot of the first accepted quote in ascending priority order.
fn lowest_priority(quotes: &Vec<FeedQuote>) -> u32 {
    let mut lowest = u32::MAX;
    for q in quotes.iter() {
        if q.priority < lowest {
            lowest = q.priority;
        }
    }
    if lowest == u32::MAX {
        0
    } else {
        lowest
    }
}
