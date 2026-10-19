//! Strategy performance tracking and analytics

use soroban_sdk::contracttype;

/// Strategy performance metrics
#[contracttype]
#[derive(Clone, Debug)]
pub struct StrategyPerformance {
    /// Total number of liquidations using this strategy
    pub total_liquidations: u64,
    /// Total value liquidated
    pub total_value_liquidated: i128,
    /// Average discount applied
    pub average_discount_bps: u32,
    /// Success rate (percentage)
    pub success_rate: u32,
    /// Average gas cost per liquidation
    pub avg_gas_cost: u64,
    /// Last update timestamp
    pub last_updated: u64,
}

impl StrategyPerformance {
    /// Create new performance tracker
    pub fn new() -> Self {
        StrategyPerformance {
            total_liquidations: 0,
            total_value_liquidated: 0,
            average_discount_bps: 0,
            success_rate: 0,
            avg_gas_cost: 0,
            last_updated: 0,
        }
    }

    /// Record a successful liquidation
    pub fn record_liquidation(&mut self, value: i128, discount_bps: u32, gas_used: u64) {
        self.total_liquidations = self.total_liquidations.saturating_add(1);
        self.total_value_liquidated = self.total_value_liquidated.saturating_add(value);

        // Update average discount (simple moving average)
        if self.total_liquidations == 1 {
            self.average_discount_bps = discount_bps;
        } else {
            self.average_discount_bps = (self.average_discount_bps as u64).saturating_add(discount_bps as u64)
                .checked_div(2)
                .unwrap_or(0) as u32;
        }

        // Update average gas cost
        if self.total_liquidations == 1 {
            self.avg_gas_cost = gas_used;
        } else {
            self.avg_gas_cost = (self.avg_gas_cost as u64).saturating_add(gas_used)
                .checked_div(2)
                .unwrap_or(0);
        }
    }

    /// Update success rate
    pub fn update_success_rate(&mut self, successes: u64, total: u64) {
        if total > 0 {
            self.success_rate = ((successes * 100) / total) as u32;
        }
    }

    /// Get average value per liquidation
    pub fn average_value_per_liquidation(&self) -> i128 {
        if self.total_liquidations == 0 {
            0
        } else {
            self.total_value_liquidated / (self.total_liquidations as i128)
        }
    }
}

/// Strategy comparison metrics
#[contracttype]
#[derive(Clone, Debug)]
pub struct StrategyComparison {
    /// Strategy ID 1
    pub strategy_id_1: u64,
    /// Strategy ID 2
    pub strategy_id_2: u64,
    /// Which strategy has better performance (0 = same, 1 = strategy_1, 2 = strategy_2)
    pub winner: u32,
    /// Comparison score (percentage advantage)
    pub advantage_score: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_performance_tracking() {
        let mut perf = StrategyPerformance::new();
        assert_eq!(perf.total_liquidations, 0);

        perf.record_liquidation(1000, 500, 100000);
        assert_eq!(perf.total_liquidations, 1);
        assert_eq!(perf.total_value_liquidated, 1000);

        perf.record_liquidation(2000, 600, 120000);
        assert_eq!(perf.total_liquidations, 2);
        assert_eq!(perf.total_value_liquidated, 3000);
    }

    #[test]
    fn test_average_value_per_liquidation() {
        let mut perf = StrategyPerformance::new();
        perf.record_liquidation(1000, 500, 100000);
        perf.record_liquidation(3000, 600, 120000);

        assert_eq!(perf.average_value_per_liquidation(), 2000);
    }

    #[test]
    fn test_success_rate_update() {
        let mut perf = StrategyPerformance::new();
        perf.update_success_rate(80, 100);
        assert_eq!(perf.success_rate, 80);
    }
}
