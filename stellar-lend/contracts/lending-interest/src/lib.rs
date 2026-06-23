#![no_std]

use soroban_sdk::Env;

pub struct InterestRateModel {
    pub base_rate: i128,
    pub slope1: i128,
    pub slope2: i128,
    pub optimal_utilization: i128,
}

impl InterestRateModel {
    pub fn calculate_borrow_rate(&self, utilization: i128) -> i128 {
        if utilization <= self.optimal_utilization {
            self.base_rate + (utilization * self.slope1 / 10_000)
        } else {
            let excess = utilization - self.optimal_utilization;
            self.base_rate + (self.optimal_utilization * self.slope1 / 10_000) + (excess * self.slope2 / 10_000)
        }
    }

    pub fn calculate_supply_rate(&self, borrow_rate: i128, utilization: i128, reserve_factor: i128) -> i128 {
        let rate_to_pool = borrow_rate * (10_000 - reserve_factor) / 10_000;
        rate_to_pool * utilization / 10_000
    }
}

pub fn calculate_utilization(total_borrows: i128, total_supply: i128) -> i128 {
    if total_supply == 0 {
        return 0;
    }
    total_borrows * 10_000 / total_supply
}

pub fn accrue_interest(principal: i128, rate: i128, time_elapsed: u64) -> i128 {
    if time_elapsed == 0 {
        return 0;
    }
    
    let seconds_per_year = 31_536_000_i128;
    principal
        .checked_mul(rate)
        .and_then(|v| v.checked_mul(time_elapsed as i128))
        .and_then(|v| v.checked_div(seconds_per_year))
        .and_then(|v| v.checked_div(10_000))
        .unwrap_or(0)
}

pub fn compound_interest(principal: i128, rate: i128, periods: u64) -> i128 {
    let mut result = principal;
    for _ in 0..periods {
        let interest = result * rate / 10_000;
        result = result.checked_add(interest).unwrap_or(result);
    }
    result - principal
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_utilization_calculation() {
        assert_eq!(calculate_utilization(50_000, 100_000), 5_000);
        assert_eq!(calculate_utilization(80_000, 100_000), 8_000);
        assert_eq!(calculate_utilization(0, 100_000), 0);
        assert_eq!(calculate_utilization(100_000, 0), 0);
    }

    #[test]
    fn test_interest_rate_model() {
        let model = InterestRateModel {
            base_rate: 200,
            slope1: 400,
            slope2: 6_000,
            optimal_utilization: 8_000,
        };

        let rate_at_50 = model.calculate_borrow_rate(5_000);
        assert!(rate_at_50 > model.base_rate);

        let rate_at_90 = model.calculate_borrow_rate(9_000);
        assert!(rate_at_90 > rate_at_50);
    }

    #[test]
    fn test_accrue_interest() {
        let principal = 100_000;
        let rate = 500;
        let time_elapsed = 31_536_000;
        
        let interest = accrue_interest(principal, rate, time_elapsed);
        assert_eq!(interest, 5_000);
    }
}
