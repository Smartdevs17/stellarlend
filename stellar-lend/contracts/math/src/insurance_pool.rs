//! Opt-in lender insurance pool accounting.
//!
//! Lenders deposit into the pool and receive shares; borrower defaults are
//! paid out of pool assets, which reduces every share's value pro rata. A
//! claim larger than the pool is only partially covered — the uncovered
//! remainder is reported, never silently dropped.

use crate::checked::checked_mul_div;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum InsuranceError {
    NonPositiveAmount,
    InsufficientShares,
    /// Pool has shares but no assets left (fully drained by claims).
    PoolInsolvent,
    Overflow,
}

/// Result of covering a default.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct Coverage {
    pub paid: i128,
    pub uncovered: i128,
}

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub struct InsurancePool {
    pub total_assets: i128,
    pub total_shares: i128,
}

impl InsurancePool {
    /// Deposits `amount`; returns shares minted (1:1 for the first deposit).
    pub fn deposit(&mut self, amount: i128) -> Result<i128, InsuranceError> {
        if amount <= 0 {
            return Err(InsuranceError::NonPositiveAmount);
        }
        let shares = if self.total_shares == 0 {
            amount
        } else {
            if self.total_assets == 0 {
                return Err(InsuranceError::PoolInsolvent);
            }
            checked_mul_div(amount, self.total_shares, self.total_assets)
                .map_err(|_| InsuranceError::Overflow)?
        };
        self.total_assets = self
            .total_assets
            .checked_add(amount)
            .ok_or(InsuranceError::Overflow)?;
        self.total_shares = self
            .total_shares
            .checked_add(shares)
            .ok_or(InsuranceError::Overflow)?;
        Ok(shares)
    }

    /// Burns `shares`; returns assets paid out (rounded down, in the pool's favour).
    pub fn withdraw(&mut self, shares: i128, holder_shares: i128) -> Result<i128, InsuranceError> {
        if shares <= 0 {
            return Err(InsuranceError::NonPositiveAmount);
        }
        if shares > holder_shares || shares > self.total_shares {
            return Err(InsuranceError::InsufficientShares);
        }
        let assets = checked_mul_div(shares, self.total_assets, self.total_shares)
            .map_err(|_| InsuranceError::Overflow)?;
        self.total_assets -= assets;
        self.total_shares -= shares;
        Ok(assets)
    }

    /// Pays a borrower-default `loss` from pool assets, up to what the pool holds.
    pub fn cover_default(&mut self, loss: i128) -> Result<Coverage, InsuranceError> {
        if loss <= 0 {
            return Err(InsuranceError::NonPositiveAmount);
        }
        let paid = loss.min(self.total_assets);
        self.total_assets -= paid;
        Ok(Coverage {
            paid,
            uncovered: loss - paid,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deposits_mint_proportional_shares() {
        let mut pool = InsurancePool::default();
        assert_eq!(pool.deposit(1_000), Ok(1_000));
        assert_eq!(pool.deposit(500), Ok(500));
        assert_eq!(
            pool,
            InsurancePool {
                total_assets: 1_500,
                total_shares: 1_500
            }
        );
    }

    #[test]
    fn default_is_shared_pro_rata() {
        let mut pool = InsurancePool::default();
        let alice = pool.deposit(1_000).unwrap();
        let bob = pool.deposit(1_000).unwrap();

        assert_eq!(
            pool.cover_default(500),
            Ok(Coverage {
                paid: 500,
                uncovered: 0
            })
        );
        assert_eq!(pool.withdraw(alice, alice), Ok(750));
        assert_eq!(pool.withdraw(bob, bob), Ok(750));
        assert_eq!(pool, InsurancePool::default());
    }

    #[test]
    fn oversized_claim_reports_uncovered_remainder() {
        let mut pool = InsurancePool::default();
        pool.deposit(300).unwrap();
        assert_eq!(
            pool.cover_default(1_000),
            Ok(Coverage {
                paid: 300,
                uncovered: 700
            })
        );
        assert_eq!(pool.deposit(100), Err(InsuranceError::PoolInsolvent));
    }

    #[test]
    fn rejects_invalid_amounts_and_share_overdraw() {
        let mut pool = InsurancePool::default();
        assert_eq!(pool.deposit(0), Err(InsuranceError::NonPositiveAmount));
        assert_eq!(
            pool.cover_default(-1),
            Err(InsuranceError::NonPositiveAmount)
        );
        let shares = pool.deposit(100).unwrap();
        assert_eq!(
            pool.withdraw(shares + 1, shares),
            Err(InsuranceError::InsufficientShares)
        );
        assert_eq!(
            pool.withdraw(0, shares),
            Err(InsuranceError::NonPositiveAmount)
        );
    }
}
