//! In-memory caching layer for storage

use soroban_sdk::{contracttype, Address};

/// Cache eviction policy
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum EvictionPolicy {
    /// Least Recently Used
    LRU,
    /// First In First Out
    FIFO,
    /// Most Recently Used (useful for predictable access patterns)
    MRU,
}

/// Cache entry metadata
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct CacheEntry {
    /// Time-to-live in seconds
    pub ttl: u64,
    /// Creation timestamp
    pub created_at: u64,
    /// Access count for LRU tracking
    pub access_count: u32,
}

/// Storage cache for frequently accessed data
pub struct StorageCache {
    /// Maximum cache entries
    pub max_entries: u32,
    /// Eviction policy
    pub eviction_policy: EvictionPolicy,
    /// Current entry count
    pub current_entries: u32,
}

impl StorageCache {
    /// Create a new storage cache
    pub fn new(max_entries: u32, eviction_policy: EvictionPolicy) -> Self {
        StorageCache {
            max_entries,
            eviction_policy,
            current_entries: 0,
        }
    }

    /// Check if cache is full
    pub fn is_full(&self) -> bool {
        self.current_entries >= self.max_entries
    }

    /// Get cache hit ratio (would be calculated at runtime)
    pub fn hit_ratio(&self) -> f64 {
        // This would be calculated from actual cache operations
        0.0
    }

    /// Get cache size in bytes
    pub fn estimated_size_bytes(&self) -> u32 {
        // Each entry takes approximately 32 bytes base + value size
        self.current_entries * 32
    }

    /// Get eviction policy name
    pub fn policy_name(&self) -> &'static str {
        match self.eviction_policy {
            EvictionPolicy::LRU => "LRU",
            EvictionPolicy::FIFO => "FIFO",
            EvictionPolicy::MRU => "MRU",
        }
    }
}

/// Token balance cache with decimals normalization
#[contracttype]
#[derive(Clone, Debug)]
pub struct TokenBalanceCache {
    pub token: Address,
    pub owner: Address,
    pub balance: i128,
    pub decimals: u32,
    pub timestamp: u64,
}

/// Price cache for asset prices
#[contracttype]
#[derive(Clone, Debug)]
pub struct PriceCache {
    pub asset: Address,
    pub price: i128,
    pub decimals: u32,
    pub timestamp: u64,
    pub source: Address,
}

/// Interest rate index cache
#[contracttype]
#[derive(Clone, Debug)]
pub struct InterestIndexCache {
    pub supply_index: i128,
    pub borrow_index: i128,
    pub update_timestamp: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_creation() {
        let cache = StorageCache::new(100, EvictionPolicy::LRU);
        assert_eq!(cache.max_entries, 100);
        assert_eq!(cache.current_entries, 0);
        assert!(!cache.is_full());
    }

    #[test]
    fn test_cache_full() {
        let mut cache = StorageCache::new(10, EvictionPolicy::FIFO);
        cache.current_entries = 10;
        assert!(cache.is_full());
    }

    #[test]
    fn test_policy_name() {
        let cache_lru = StorageCache::new(100, EvictionPolicy::LRU);
        assert_eq!(cache_lru.policy_name(), "LRU");

        let cache_fifo = StorageCache::new(100, EvictionPolicy::FIFO);
        assert_eq!(cache_fifo.policy_name(), "FIFO");

        let cache_mru = StorageCache::new(100, EvictionPolicy::MRU);
        assert_eq!(cache_mru.policy_name(), "MRU");
    }
}
