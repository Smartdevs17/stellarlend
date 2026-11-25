//! Per-block memoization for oracle price reads (issue #1769).
//!
//! A Soroban transaction can ask for the same asset many times through
//! different adapters and guards. Fetching the quote for each call repeats a
//! cross-contract read inside one ledger. This module memoizes quotes for the
//! lifetime of a single ledger sequence and drops the cache when the block
//! changes, so freshness can never be weaker than one ledger.

use crate::PriceQuote;
use soroban_sdk::{contracttype, symbol_short, Env, Map, Symbol};

const CACHE_KEY: Symbol = symbol_short!("pxcache");
const MAX_ENTRIES: u32 = 64;

/// A quote tagged with the ledger sequence that produced it.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CachedPrice {
    pub block: u32,
    pub quote: PriceQuote,
}

fn load_cache(env: &Env) -> Map<(Symbol, u32), CachedPrice> {
    env.storage()
        .temporary()
        .get(&CACHE_KEY)
        .unwrap_or_else(|| Map::new(env))
}

fn store_cache(env: &Env, cache: &Map<(Symbol, u32), CachedPrice>) {
    env.storage().temporary().set(&CACHE_KEY, cache);
}

/// Returns the cached quote for the current ledger, or calls `fetch` once.
///
/// `fetch` is deliberately `FnOnce`: callers cannot accidentally reuse a
/// stale closure result across assets. Only quotes fetched in this ledger are
/// returned; entries from older ledgers are discarded on first access.
pub fn get_or_fetch<F>(env: &Env, asset: &Symbol, fetch: F) -> PriceQuote
where
    F: FnOnce() -> PriceQuote,
{
    let block = env.ledger().sequence();
    let mut cache = load_cache(env);
    let key = (asset.clone(), block);

    if let Some(entry) = cache.get(key.clone()) {
        if entry.block == block {
            return entry.quote;
        }
        cache.remove(key.clone());
    }

    let quote = fetch();
    if cache.len() >= MAX_ENTRIES {
        evict_oldest(&mut cache);
    }

    cache.set(key, CachedPrice { block, quote: quote.clone() });
    store_cache(env, &cache);

    // Observability: one event per miss so operators can trace cache ratios.
    env.events().publish(
        (symbol_short!("pxmiss"), asset.clone()),
        (block, cache.len()),
    );
    quote
}

/// Removes the oldest block's entries until the cache fits again.
fn evict_oldest(cache: &mut Map<(Symbol, u32), CachedPrice>) {
    let mut oldest_block: Option<u32> = None;
    for key in cache.keys().iter() {
        let block = key.1;
        if oldest_block.map_or(true, |current| block < current) {
            oldest_block = Some(block);
        }
    }

    if let Some(block) = oldest_block {
        for key in cache.keys().iter() {
            if key.1 == block {
                cache.remove(key);
            }
        }
    }
}

/// Drops every memoized quote, e.g. after an oracle upgrade.
pub fn invalidate(env: &Env) {
    env.storage().temporary().remove(&CACHE_KEY);
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use soroban_sdk::{symbol_short, Env};
    use std::cell::Cell;

    fn quote(env: &Env, asset: &Symbol, price: i128) -> PriceQuote {
        PriceQuote {
            asset: asset.clone(),
            price,
            decimals: 7,
            timestamp: 1_700_000_000,
            confidence: 100,
        }
    }

    #[test]
    fn repeated_reads_in_one_block_share_a_fetch() {
        let env = Env::default();
        env.ledger().with_mut(|li| li.sequence_number = 10);
        let asset = symbol_short!("XLM");
        let calls = Cell::new(0);

        let first = get_or_fetch(&env, &asset, || {
            calls.set(calls.get() + 1);
            quote(&env, &asset, 100)
        });
        let second = get_or_fetch(&env, &asset, || {
            calls.set(calls.get() + 1);
            quote(&env, &asset, 999)
        });

        assert_eq!(first.price, 100);
        assert_eq!(second.price, 100);
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn new_block_refetches_the_quote() {
        let env = Env::default();
        env.ledger().with_mut(|li| li.sequence_number = 10);
        let asset = symbol_short!("XLM");

        let first = get_or_fetch(&env, &asset, || quote(&env, &asset, 100));
        env.ledger().with_mut(|li| li.sequence_number = 11);
        let second = get_or_fetch(&env, &asset, || quote(&env, &asset, 120));

        assert_eq!(first.price, 100);
        assert_eq!(second.price, 120);
    }

    #[test]
    fn invalidate_forces_a_refetch() {
        let env = Env::default();
        env.ledger().with_mut(|li| li.sequence_number = 10);
        let asset = symbol_short!("XLM");

        get_or_fetch(&env, &asset, || quote(&env, &asset, 100));
        invalidate(&env);
        let second = get_or_fetch(&env, &asset, || quote(&env, &asset, 130));

        assert_eq!(second.price, 130);
    }

    #[test]
    fn cache_is_bounded() {
        let env = Env::default();
        env.ledger().with_mut(|li| li.sequence_number = 10);

        for index in 0..(MAX_ENTRIES + 5) {
            let asset = Symbol::new(&env, &std::format!("A{index}"));
            get_or_fetch(&env, &asset, || quote(&env, &asset, index as i128));
        }

        let cache = load_cache(&env);
        assert!(cache.len() <= MAX_ENTRIES);
    }
}
