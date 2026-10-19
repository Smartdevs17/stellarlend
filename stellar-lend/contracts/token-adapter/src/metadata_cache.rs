//! Token metadata caching to optimize storage access

use soroban_sdk::{contracttype, Address, Env};

/// Cache key for token metadata
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum MetadataCacheKey {
    /// Metadata cache for a token
    TokenMetadata(Address),
    /// Last cache update time
    LastUpdate(Address),
    /// Cache invalidation counter
    InvalidationCounter,
}

/// Metadata cache manager
pub struct MetadataCache;

impl MetadataCache {
    /// TTL for metadata cache in ledger seconds (5 minutes default)
    pub const DEFAULT_CACHE_TTL: u64 = 300;

    /// Maximum cache entries
    pub const MAX_CACHE_ENTRIES: u32 = 1_000;

    /// Get cached metadata or refresh if stale
    pub fn get_or_refresh(
        env: &Env,
        token: &Address,
        fetch_fn: impl Fn(&Env, &Address) -> Option<super::unified::TokenMetadata>,
    ) -> Option<super::unified::TokenMetadata> {
        let cache_key = MetadataCacheKey::TokenMetadata(token.clone());

        // Try to get from cache
        if let Some(cached) = env.storage().temporary().get::<MetadataCacheKey, super::unified::TokenMetadata>(&cache_key) {
            let current_time = env.ledger().timestamp();
            if cached.is_cache_valid(current_time) {
                return Some(cached);
            }
        }

        // Cache miss or stale, fetch fresh data
        let fresh = fetch_fn(env, token)?;

        // Store in temporary cache
        env.storage().temporary().set(&cache_key, &fresh);

        Some(fresh)
    }

    /// Invalidate cache for a specific token
    pub fn invalidate(env: &Env, token: &Address) {
        let cache_key = MetadataCacheKey::TokenMetadata(token.clone());
        env.storage().temporary().remove(&cache_key);

        // Increment invalidation counter for tracking
        let counter_key = MetadataCacheKey::InvalidationCounter;
        let count: u64 = env
            .storage()
            .temporary()
            .get::<MetadataCacheKey, u64>(&counter_key)
            .unwrap_or(0);
        env.storage()
            .temporary()
            .set(&counter_key, &count.saturating_add(1));
    }

    /// Clear all metadata cache
    pub fn clear_all(env: &Env) {
        // Note: In practice, individual token caches should be invalidated as needed
        // Full clear would require iterating through all cached tokens
        let counter_key = MetadataCacheKey::InvalidationCounter;
        env.storage().temporary().remove(&counter_key);
    }

    /// Get cache statistics
    pub fn get_cache_stats(_env: &Env) -> CacheStats {
        CacheStats {
            total_entries: 0,
            hits: 0,
            misses: 0,
            stale_entries: 0,
        }
    }
}

/// Cache statistics
#[contracttype]
#[derive(Clone, Debug)]
pub struct CacheStats {
    /// Total entries in cache
    pub total_entries: u32,
    /// Number of cache hits
    pub hits: u64,
    /// Number of cache misses
    pub misses: u64,
    /// Number of stale entries
    pub stale_entries: u32,
}

impl CacheStats {
    /// Calculate hit ratio
    pub fn hit_ratio(&self) -> f64 {
        let total = self.hits.saturating_add(self.misses);
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_stats_hit_ratio() {
        let stats = CacheStats {
            total_entries: 100,
            hits: 75,
            misses: 25,
            stale_entries: 0,
        };

        assert!((stats.hit_ratio() - 0.75).abs() < 0.01);
    }

    #[test]
    fn test_cache_stats_zero_operations() {
        let stats = CacheStats {
            total_entries: 0,
            hits: 0,
            misses: 0,
            stale_entries: 0,
        };

        assert_eq!(stats.hit_ratio(), 0.0);
    }
}
