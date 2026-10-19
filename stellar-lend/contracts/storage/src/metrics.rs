//! Storage access metrics and monitoring

use soroban_sdk::contracttype;

/// Storage operation metrics
#[contracttype]
#[derive(Clone, Debug)]
pub struct StorageMetrics {
    /// Total number of read operations
    pub total_reads: u64,
    /// Total number of write operations
    pub total_writes: u64,
    /// Number of cache hits
    pub cache_hits: u64,
    /// Number of cache misses
    pub cache_misses: u64,
    /// Peak storage size in bytes
    pub peak_storage_size: u64,
    /// Last update timestamp
    pub last_updated: u64,
}

impl StorageMetrics {
    /// Create new metrics tracker
    pub fn new() -> Self {
        StorageMetrics {
            total_reads: 0,
            total_writes: 0,
            cache_hits: 0,
            cache_misses: 0,
            peak_storage_size: 0,
            last_updated: 0,
        }
    }

    /// Record a read operation
    pub fn record_read(&mut self, is_cache_hit: bool) {
        self.total_reads = self.total_reads.saturating_add(1);
        if is_cache_hit {
            self.cache_hits = self.cache_hits.saturating_add(1);
        } else {
            self.cache_misses = self.cache_misses.saturating_add(1);
        }
    }

    /// Record a write operation
    pub fn record_write(&mut self) {
        self.total_writes = self.total_writes.saturating_add(1);
    }

    /// Update peak storage size
    pub fn update_peak_size(&mut self, current_size: u64) {
        if current_size > self.peak_storage_size {
            self.peak_storage_size = current_size;
        }
    }

    /// Calculate cache hit ratio (0.0 to 1.0)
    pub fn cache_hit_ratio(&self) -> f64 {
        let total_cache_ops = self.cache_hits.saturating_add(self.cache_misses);
        if total_cache_ops == 0 {
            0.0
        } else {
            self.cache_hits as f64 / total_cache_ops as f64
        }
    }

    /// Calculate average operations per transaction
    pub fn avg_ops_per_tx(&self) -> f64 {
        let total_ops = self.total_reads.saturating_add(self.total_writes);
        if self.total_writes == 0 {
            0.0
        } else {
            total_ops as f64 / self.total_writes as f64
        }
    }

    /// Reset all metrics
    pub fn reset(&mut self) {
        self.total_reads = 0;
        self.total_writes = 0;
        self.cache_hits = 0;
        self.cache_misses = 0;
        self.peak_storage_size = 0;
    }
}

/// Performance tracking for storage operations
#[contracttype]
#[derive(Clone, Debug)]
pub struct PerformanceMetrics {
    /// Average latency for reads in milliseconds
    pub avg_read_latency_ms: u64,
    /// Average latency for writes in milliseconds
    pub avg_write_latency_ms: u64,
    /// Peak read latency in milliseconds
    pub peak_read_latency_ms: u64,
    /// Peak write latency in milliseconds
    pub peak_write_latency_ms: u64,
    /// Number of slow operations (>100ms)
    pub slow_operations: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_creation() {
        let metrics = StorageMetrics::new();
        assert_eq!(metrics.total_reads, 0);
        assert_eq!(metrics.total_writes, 0);
        assert_eq!(metrics.cache_hits, 0);
    }

    #[test]
    fn test_record_operations() {
        let mut metrics = StorageMetrics::new();
        metrics.record_read(true);
        metrics.record_read(false);
        metrics.record_write();

        assert_eq!(metrics.total_reads, 2);
        assert_eq!(metrics.total_writes, 1);
        assert_eq!(metrics.cache_hits, 1);
        assert_eq!(metrics.cache_misses, 1);
    }

    #[test]
    fn test_cache_hit_ratio() {
        let mut metrics = StorageMetrics::new();
        metrics.record_read(true);
        metrics.record_read(true);
        metrics.record_read(false);

        assert!((metrics.cache_hit_ratio() - (2.0 / 3.0)).abs() < 0.01);
    }

    #[test]
    fn test_peak_size_tracking() {
        let mut metrics = StorageMetrics::new();
        metrics.update_peak_size(100);
        metrics.update_peak_size(50);
        metrics.update_peak_size(150);

        assert_eq!(metrics.peak_storage_size, 150);
    }

    #[test]
    fn test_metrics_reset() {
        let mut metrics = StorageMetrics::new();
        metrics.record_read(true);
        metrics.record_write();
        metrics.update_peak_size(200);

        metrics.reset();
        assert_eq!(metrics.total_reads, 0);
        assert_eq!(metrics.total_writes, 0);
        assert_eq!(metrics.peak_storage_size, 0);
    }
}
