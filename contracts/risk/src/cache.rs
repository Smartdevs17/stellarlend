//! Per-block memoization of risk-weight lookups (issue #1774).
//!
//! Health checks, borrow-capacity checks and liquidation sweeps resolve the
//! same asset's risk weights many times inside one ledger. The weights can
//! only change through a governance write, which lands in a later ledger, so
//! a value resolved at height `h` stays valid for every other read at `h`.
//!
//! [`RiskWeightCache`] is a fixed-capacity, `no_std`, allocation-free table
//! keyed by asset id and stamped with the block height it was filled at:
//!
//! * The first read of an asset at a height calls the loader (a *miss*);
//!   later reads at that height return the stored value (a *hit*).
//! * Reading at any other height — forward, or backward after a rewind —
//!   drops every entry before loading (a *reset*). Stale weights are never
//!   served across heights.
//! * Loader errors are returned to the caller and never stored, so a
//!   transient failure is retried on the next read.
//! * When every slot is taken the value is computed and returned without
//!   being stored (a *bypass*); correctness never depends on capacity.
//! * [`RiskWeightCache::invalidate`] drops entries mid-block for callers
//!   that mutate parameters and keep reading in the same height.
//!
//! [`CacheStats`] exposes counters for every outcome so callers can export
//! hit rate and loader round-trips as metrics.

/// Default slot count. Matches the parameter registry's `MAX_ASSETS`.
pub const DEFAULT_CAPACITY: usize = 32;

/// Counters describing how a cache has been used. All counters saturate.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CacheStats {
    /// Reads answered from the cache.
    pub hits: u64,
    /// Reads that called the loader (includes bypasses and failures).
    pub misses: u64,
    /// Loader calls whose result could not be stored because the cache was full.
    pub bypasses: u64,
    /// Loader calls that returned an error (never stored).
    pub failures: u64,
    /// Times the cache was emptied because the block height changed.
    pub resets: u64,
    /// Times the cache was emptied explicitly through `invalidate`.
    pub invalidations: u64,
}

impl CacheStats {
    /// Total reads served, hit or miss.
    pub fn lookups(&self) -> u64 {
        self.hits.saturating_add(self.misses)
    }

    /// Hit rate in basis points (`0..=10_000`); `0` before any lookup.
    pub fn hit_rate_bps(&self) -> u64 {
        let lookups = self.lookups();
        if lookups == 0 {
            return 0;
        }
        // hits <= lookups, so hits * 10_000 / lookups fits once widened.
        ((self.hits as u128 * 10_000) / lookups as u128) as u64
    }
}

/// Fixed-capacity risk-weight memo scoped to a single block height.
///
/// Uses open addressing with linear probing, so a lookup touches one slot in
/// the common case instead of scanning every registered asset. Entries are
/// only ever removed all at once, which keeps probing tombstone-free.
#[derive(Clone, Copy, Debug)]
pub struct RiskWeightCache<V: Copy, const N: usize = DEFAULT_CAPACITY> {
    height: Option<u32>,
    slots: [Option<(u64, V)>; N],
    len: usize,
    stats: CacheStats,
}

impl<V: Copy, const N: usize> Default for RiskWeightCache<V, N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<V: Copy, const N: usize> RiskWeightCache<V, N> {
    /// Create an empty cache not yet bound to any height.
    pub const fn new() -> Self {
        Self {
            height: None,
            slots: [None; N],
            len: 0,
            stats: CacheStats {
                hits: 0,
                misses: 0,
                bypasses: 0,
                failures: 0,
                resets: 0,
                invalidations: 0,
            },
        }
    }

    /// Height the current entries were filled at, if any read happened yet.
    pub fn height(&self) -> Option<u32> {
        self.height
    }

    /// Number of memoized entries for the current height.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Whether no entries are memoized for the current height.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Slot count.
    pub const fn capacity(&self) -> usize {
        N
    }

    /// Snapshot of the usage counters.
    pub fn stats(&self) -> CacheStats {
        self.stats
    }

    /// Return the memoized value for `key` at `height` without loading or
    /// touching the counters. A different height always yields `None`.
    pub fn peek(&self, height: u32, key: u64) -> Option<V> {
        if self.height != Some(height) {
            return None;
        }
        match self.find(key) {
            Probe::Found(index) => self.slots[index].map(|(_, value)| value),
            Probe::Vacant(_) | Probe::Full => None,
        }
    }

    /// Drop every entry but stay bound to the current height.
    ///
    /// Call after mutating risk parameters when further reads happen in the
    /// same block.
    pub fn invalidate(&mut self) {
        self.clear();
        self.stats.invalidations = self.stats.invalidations.saturating_add(1);
    }

    /// Infallible variant of [`Self::get_or_try_insert_with`].
    pub fn get_or_insert_with<F: FnOnce() -> V>(&mut self, height: u32, key: u64, load: F) -> V {
        match self.get_or_try_insert_with::<core::convert::Infallible, _>(height, key, || {
            Ok(load())
        }) {
            Ok(value) => value,
            Err(never) => match never {},
        }
    }

    /// Return the value for `key` at `height`, calling `load` at most once
    /// per `(height, key)` pair.
    ///
    /// Errors from `load` are passed through and never stored.
    pub fn get_or_try_insert_with<E, F: FnOnce() -> Result<V, E>>(
        &mut self,
        height: u32,
        key: u64,
        load: F,
    ) -> Result<V, E> {
        self.bind(height);

        let probe = self.find(key);
        if let Probe::Found(index) = probe {
            if let Some((_, value)) = self.slots[index] {
                self.stats.hits = self.stats.hits.saturating_add(1);
                return Ok(value);
            }
        }

        self.stats.misses = self.stats.misses.saturating_add(1);
        let value = match load() {
            Ok(value) => value,
            Err(error) => {
                self.stats.failures = self.stats.failures.saturating_add(1);
                return Err(error);
            }
        };

        match probe {
            Probe::Vacant(index) => {
                self.slots[index] = Some((key, value));
                self.len += 1;
            }
            Probe::Full | Probe::Found(_) => {
                self.stats.bypasses = self.stats.bypasses.saturating_add(1);
            }
        }
        Ok(value)
    }

    /// Bind the cache to `height`, dropping entries from any other height.
    fn bind(&mut self, height: u32) {
        match self.height {
            Some(current) if current == height => {}
            Some(_) => {
                self.clear();
                self.height = Some(height);
                self.stats.resets = self.stats.resets.saturating_add(1);
            }
            None => self.height = Some(height),
        }
    }

    fn clear(&mut self) {
        self.slots = [None; N];
        self.len = 0;
    }

    fn find(&self, key: u64) -> Probe {
        if N == 0 {
            return Probe::Full;
        }
        let start = slot_for(key, N);
        for offset in 0..N {
            let index = (start + offset) % N;
            match self.slots[index] {
                Some((stored, _)) if stored == key => return Probe::Found(index),
                Some(_) => continue,
                None => return Probe::Vacant(index),
            }
        }
        Probe::Full
    }
}

enum Probe {
    Found(usize),
    Vacant(usize),
    Full,
}

/// Fibonacci hashing: spreads sequential asset ids across the table.
fn slot_for(key: u64, slots: usize) -> usize {
    (key.wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 32) as usize % slots
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::cell::Cell;

    #[test]
    fn loads_once_per_height_and_key() {
        let mut cache: RiskWeightCache<i128> = RiskWeightCache::new();
        let calls = Cell::new(0);
        let load = || {
            calls.set(calls.get() + 1);
            7_500
        };

        assert_eq!(cache.get_or_insert_with(10, 1, load), 7_500);
        assert_eq!(cache.get_or_insert_with(10, 1, load), 7_500);
        assert_eq!(cache.get_or_insert_with(10, 1, load), 7_500);

        assert_eq!(calls.get(), 1);
        assert_eq!(cache.len(), 1);
        assert_eq!(cache.height(), Some(10));
        let stats = cache.stats();
        assert_eq!((stats.hits, stats.misses), (2, 1));
        assert_eq!(stats.hit_rate_bps(), 6_666);
    }

    #[test]
    fn new_height_drops_stale_values() {
        let mut cache: RiskWeightCache<i128> = RiskWeightCache::new();
        cache.get_or_insert_with(10, 1, || 7_500);

        // Governance changed the weight; the next ledger must see it.
        assert_eq!(cache.get_or_insert_with(11, 1, || 7_000), 7_000);
        assert_eq!(cache.peek(10, 1), None);
        assert_eq!(cache.peek(11, 1), Some(7_000));
        assert_eq!(cache.stats().resets, 1);
    }

    #[test]
    fn rewound_height_also_resets() {
        let mut cache: RiskWeightCache<i128> = RiskWeightCache::new();
        cache.get_or_insert_with(11, 1, || 7_000);
        assert_eq!(cache.get_or_insert_with(10, 1, || 7_500), 7_500);
        assert_eq!(cache.stats().resets, 1);
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn errors_are_returned_and_not_stored() {
        let mut cache: RiskWeightCache<i128> = RiskWeightCache::new();
        assert_eq!(
            cache.get_or_try_insert_with(5, 1, || Err::<i128, _>("oracle down")),
            Err("oracle down")
        );
        assert!(cache.is_empty());
        assert_eq!(cache.get_or_try_insert_with::<&str, _>(5, 1, || Ok(42)), Ok(42));
        assert_eq!(cache.get_or_try_insert_with(5, 1, || Err("unused")), Ok(42));

        let stats = cache.stats();
        assert_eq!((stats.failures, stats.misses, stats.hits), (1, 2, 1));
    }

    #[test]
    fn full_cache_bypasses_without_losing_correctness() {
        let mut cache: RiskWeightCache<u64, 4> = RiskWeightCache::new();
        for key in 0..4 {
            cache.get_or_insert_with(1, key, || key * 10);
        }
        assert_eq!(cache.len(), 4);

        let calls = Cell::new(0);
        for _ in 0..3 {
            let value = cache.get_or_insert_with(1, 99, || {
                calls.set(calls.get() + 1);
                990
            });
            assert_eq!(value, 990);
        }
        assert_eq!(calls.get(), 3);
        assert_eq!(cache.stats().bypasses, 3);

        // Existing entries are still served.
        for key in 0..4 {
            assert_eq!(cache.get_or_insert_with(1, key, || unreachable!()), key * 10);
        }
    }

    #[test]
    fn colliding_keys_probe_to_distinct_slots() {
        let mut cache: RiskWeightCache<u64, 8> = RiskWeightCache::new();
        // Multiples of the capacity are the worst case for naive modulo.
        for key in (0..8).map(|i| i * 8) {
            cache.get_or_insert_with(3, key, || key + 1);
        }
        for key in (0..8).map(|i| i * 8) {
            assert_eq!(cache.peek(3, key), Some(key + 1));
        }
        assert_eq!(cache.stats().bypasses, 0);
    }

    #[test]
    fn invalidate_keeps_height_but_forces_reload() {
        let mut cache: RiskWeightCache<i128> = RiskWeightCache::new();
        cache.get_or_insert_with(7, 1, || 7_500);
        cache.invalidate();
        assert_eq!(cache.height(), Some(7));
        assert!(cache.is_empty());
        assert_eq!(cache.get_or_insert_with(7, 1, || 7_000), 7_000);
        assert_eq!(cache.stats().invalidations, 1);
        assert_eq!(cache.stats().resets, 0);
    }

    #[test]
    fn zero_capacity_always_loads() {
        let mut cache: RiskWeightCache<i128, 0> = RiskWeightCache::new();
        assert_eq!(cache.get_or_insert_with(1, 1, || 3), 3);
        assert_eq!(cache.get_or_insert_with(1, 1, || 4), 4);
        assert_eq!(cache.stats().bypasses, 2);
    }

    #[test]
    fn empty_stats_report_zero_hit_rate() {
        assert_eq!(CacheStats::default().hit_rate_bps(), 0);
    }
}
