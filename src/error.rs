// ... (existing error enum)

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum PoolError {
    // ... (existing variants)
    #[error("Deposit cap not initialized")]
    UninitializedDepositCap,
    #[error("Debt ceiling not initialized")]
    UninitializedDebtCeiling,
    // ... (rest of variants)
}
