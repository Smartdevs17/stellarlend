#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deposit_fails_uninitialized_cap() {
        let pool = create_test_pool();
        let result = pool.deposit(100, &user);
        assert!(matches!(result, Err(PoolError::UninitializedDepositCap)));
    }

    #[test]
    fn test_borrow_fails_uninitialized_ceiling() {
        let pool = create_test_pool();
        let result = pool.borrow(100, &user);
        assert!(matches!(result, Err(PoolError::UninitializedDebtCeiling)));
    }

    // ... (other existing tests)
}
