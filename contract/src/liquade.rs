pub fn liquidate(
    env: Env,
    liquidator: Address,
    borrower: Address,
    debt_asset: Address,
    collateral_asset: Address,
    amount: i128,
) -> Result<(), LiquidationError> {
    // 1. GUARD: Lock the contract
    let _guard = ReentrancyGuard::new(&env);
    
    // 2. CHECKS
    liquidator.require_auth();
    let health_factor = get_health_factor(&env, &borrower);
    if health_factor >= 1_0000000 { // Assuming 1.0 is the threshold
        return Err(LiquidationError::NotLiquidatable);
    }

    // 3. EFFECTS: Update internal ledgers
    // Calculate how much collateral the liquidator gets + bonus
    let collateral_to_receive = calculate_liquidation_collateral(&env, &debt_asset, &collateral_asset, amount);
    
    update_debt(&env, &borrower, &debt_asset, -amount);
    update_collateral(&env, &borrower, &collateral_asset, -collateral_to_receive);

    // 4. INTERACTIONS: Physical token transfers
    let debt_token = token::Client::new(&env, &debt_asset);
    let coll_token = token::Client::new(&env, &collateral_asset);

    // Pull debt from liquidator to protocol
    debt_token.transfer(&liquidator, &env.current_contract_address(), &amount);
    // Push collateral from protocol to liquidator
    coll_token.transfer(&env.current_contract_address(), &liquidator, &coll_to_receive);

    Ok(())
}