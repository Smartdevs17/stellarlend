// ... (existing imports and code remain unchanged until line 156)

pub fn validate_deposit_cap(initialized: bool, amount: i128) -> Result<(), PoolError> {
    if !initialized {
        return Err(PoolError::UninitializedDepositCap);
    }
    // ... (rest of existing validation logic)
    Ok(())
}

// ... (rest of file remains unchanged)
