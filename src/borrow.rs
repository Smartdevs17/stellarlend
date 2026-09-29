// ... (existing imports and code remain unchanged until line 383)

pub fn validate_debt_ceiling(initialized: bool, amount: i128) -> Result<(), PoolError> {
    if !initialized {
        return Err(PoolError::UninitializedDebtCeiling);
    }
    // ... (rest of existing validation logic)
    Ok(())
}

// ... (rest of file remains unchanged)
