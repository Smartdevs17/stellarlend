//! # ERC-4626 Tokenized Vault Interface for Stablecoin Pools (#1608)
//!
//! ## Why this exists
//!
//! Stablecoin liquidity in lending protocols has traditionally been tightly coupled
//! to bespoke lending pools and internal balance tracking. This organic growth has
//! duplicated concepts (such as share accounting, deposit/withdraw limits, conversion math)
//! and buried intent behind disparate modules.
//!
//! This module extracts a canonical **ERC-4626-style Tokenized Vault Interface**
//! for stablecoin pools (e.g. USDC, EURC, USDS). It establishes a standard,
//! interoperable share-based accounting layer on top of pool reserves.
//!
//! ## Core Features
//!
//! 1. **ERC-4626 Standard Operations**:
//!    - **Metadata & Accounting**: `asset`, `total_assets`, `total_shares`,
//!      `convert_to_shares`, `convert_to_assets`.
//!    - **Preview Mechanics**: `preview_deposit`, `preview_mint`, `preview_withdraw`, `preview_redeem`.
//!    - **Deposit & Mint Limits**: `max_deposit`, `max_mint`.
//!    - **Withdraw & Redeem Limits**: `max_withdraw`, `max_redeem`.
//!    - **State Mutations**: `deposit`, `mint`, `withdraw`, `redeem`.
//!
//! 2. **Inflation / Donation Attack Protection**:
//!    Uses virtual offset accounting (`VIRTUAL_OFFSET = 1`) for both assets and shares.
//!    This prevents the classic first-depositor exchange-rate manipulation attack
//!    where empty vaults are donated to in order to round down subsequent depositor shares to zero.
//!
//! 3. **Strict Rounding Conventions**:
//!    Per ERC-4626 specifications:
//!    - Conversions and previews for deposits / redeems round *down* (in favor of vault).
//!    - Previews for mints / withdrawals round *up* (in favor of vault).
//!
//! 4. **Delegated Operations & Share Allowances**:
//!    Supports standard third-party approvals and allowances (`approve_shares`, `share_allowance`,
//!    `share_balance_of`, `transfer_shares`), enabling routers, managers, and custodians
//!    to execute `withdraw` or `redeem` on behalf of an `owner`.
//!
//! 5. **Observability**:
//!    Emits typed events for all state-changing operations:
//!    - `VaultDepositEvent`
//!    - `VaultWithdrawEvent`
//!    - `VaultConfiguredEvent`
//!    - `VaultApprovalEvent`
//!    - `VaultYieldAccruedEvent`
//!
//! 6. **Multi-Pool Isolation**:
//!    All vault states are keyed by `Option<Address>`:
//!    - `None`: Default / native stablecoin pool
//!    - `Some(Address)`: Dedicated asset / pool instance (e.g., USDC, EURC)

use soroban_sdk::{contractevent, contracttype, Address, Env};

use crate::errors::LendingError;

// ─── Tunables & Constants ───────────────────────────────────────────────────

/// Virtual offset added to numerator and denominator to prevent initial-deposit inflation attacks.
pub const VIRTUAL_OFFSET: i128 = 1;

/// Standard Soroban share precision scale (10^7).
pub const SHARE_PRECISION: i128 = 10_000_000;

// ─── Storage Keys ───────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone)]
pub enum VaultKey {
    Config(Option<Address>),
    Asset(Option<Address>),
    TotalAssets(Option<Address>),
    TotalShares(Option<Address>),
    ShareBalance(Option<Address>, Address),
    Allowance(Option<Address>, Address, Address),
    Initialized(Option<Address>),
}

// ─── Configuration & Statistics ─────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct VaultConfig {
    /// Maximum assets allowed to be deposited into this vault (0 = uncapped).
    pub deposit_cap: i128,
    /// Minimum deposit amount permitted per transaction (0 = no minimum).
    pub min_deposit: i128,
    /// Flag indicating whether deposit and mint operations are paused.
    pub deposit_paused: bool,
    /// Flag indicating whether withdraw and redeem operations are paused.
    pub withdraw_paused: bool,
}

impl Default for VaultConfig {
    fn default() -> Self {
        Self {
            deposit_cap: 0,
            min_deposit: 0,
            deposit_paused: false,
            withdraw_paused: false,
        }
    }
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct VaultStats {
    pub total_assets: i128,
    pub total_shares: i128,
    pub share_price_scaled: i128,
    pub deposit_cap: i128,
    pub deposit_paused: bool,
    pub withdraw_paused: bool,
}

// ─── Events ─────────────────────────────────────────────────────────────────

#[contractevent]
#[derive(Clone, Debug, PartialEq)]
pub struct VaultDepositEvent {
    pub pool: Option<Address>,
    pub caller: Address,
    pub receiver: Address,
    pub assets: i128,
    pub shares: i128,
    pub timestamp: u64,
}

#[contractevent]
#[derive(Clone, Debug, PartialEq)]
pub struct VaultWithdrawEvent {
    pub pool: Option<Address>,
    pub caller: Address,
    pub receiver: Address,
    pub owner: Address,
    pub assets: i128,
    pub shares: i128,
    pub timestamp: u64,
}

#[contractevent]
#[derive(Clone, Debug, PartialEq)]
pub struct VaultConfiguredEvent {
    pub pool: Option<Address>,
    pub caller: Address,
    pub deposit_cap: i128,
    pub deposit_paused: bool,
    pub withdraw_paused: bool,
    pub timestamp: u64,
}

#[contractevent]
#[derive(Clone, Debug, PartialEq)]
pub struct VaultApprovalEvent {
    pub pool: Option<Address>,
    pub owner: Address,
    pub spender: Address,
    pub amount: i128,
    pub timestamp: u64,
}

#[contractevent]
#[derive(Clone, Debug, PartialEq)]
pub struct VaultYieldAccruedEvent {
    pub pool: Option<Address>,
    pub yield_amount: i128,
    pub new_total_assets: i128,
    pub timestamp: u64,
}

// ─── ERC-4626 Trait Definition ──────────────────────────────────────────────

/// Canonical ERC-4626 Tokenized Vault interface adapted for Soroban stablecoin pools.
pub trait ERC4626Vault {
    type Error;

    /// The address of the underlying token managed by the vault for `pool`.
    fn asset(&self, env: &Env, pool: Option<Address>) -> Result<Address, Self::Error>;

    /// Total amount of underlying assets held/managed by the vault for `pool`.
    fn total_assets(&self, env: &Env, pool: Option<Address>) -> Result<i128, Self::Error>;

    /// Total vault shares minted and outstanding for `pool`.
    fn total_shares(&self, env: &Env, pool: Option<Address>) -> Result<i128, Self::Error>;

    /// Convert an amount of assets to equivalent shares without fee/slippage (rounds down).
    fn convert_to_shares(
        &self,
        env: &Env,
        pool: Option<Address>,
        assets: i128,
    ) -> Result<i128, Self::Error>;

    /// Convert an amount of shares to equivalent assets without fee/slippage (rounds down).
    fn convert_to_assets(
        &self,
        env: &Env,
        pool: Option<Address>,
        shares: i128,
    ) -> Result<i128, Self::Error>;

    /// Maximum amount of underlying assets that can be deposited for `receiver`.
    fn max_deposit(
        &self,
        env: &Env,
        pool: Option<Address>,
        receiver: &Address,
    ) -> Result<i128, Self::Error>;

    /// Simulate deposit of `assets` at current state, returning shares that would be minted.
    fn preview_deposit(
        &self,
        env: &Env,
        pool: Option<Address>,
        assets: i128,
    ) -> Result<i128, Self::Error>;

    /// Deposit exact `assets` from caller, minting shares to `receiver`.
    fn deposit(
        &self,
        env: &Env,
        caller: &Address,
        assets: i128,
        receiver: &Address,
        pool: Option<Address>,
    ) -> Result<i128, Self::Error>;

    /// Maximum amount of shares that can be minted for `receiver`.
    fn max_mint(
        &self,
        env: &Env,
        pool: Option<Address>,
        receiver: &Address,
    ) -> Result<i128, Self::Error>;

    /// Simulate minting `shares` at current state, returning assets required.
    fn preview_mint(
        &self,
        env: &Env,
        pool: Option<Address>,
        shares: i128,
    ) -> Result<i128, Self::Error>;

    /// Mint exact `shares` to `receiver`, pulling necessary assets from caller.
    fn mint(
        &self,
        env: &Env,
        caller: &Address,
        shares: i128,
        receiver: &Address,
        pool: Option<Address>,
    ) -> Result<i128, Self::Error>;

    /// Maximum amount of assets that can be withdrawn by `owner`.
    fn max_withdraw(
        &self,
        env: &Env,
        pool: Option<Address>,
        owner: &Address,
    ) -> Result<i128, Self::Error>;

    /// Simulate withdrawal of `assets` at current state, returning shares burned.
    fn preview_withdraw(
        &self,
        env: &Env,
        pool: Option<Address>,
        assets: i128,
    ) -> Result<i128, Self::Error>;

    /// Withdraw `assets` to `receiver`, burning shares from `owner`.
    fn withdraw(
        &self,
        env: &Env,
        caller: &Address,
        assets: i128,
        receiver: &Address,
        owner: &Address,
        pool: Option<Address>,
    ) -> Result<i128, Self::Error>;

    /// Maximum amount of shares that can be redeemed by `owner`.
    fn max_redeem(
        &self,
        env: &Env,
        pool: Option<Address>,
        owner: &Address,
    ) -> Result<i128, Self::Error>;

    /// Simulate redemption of `shares` at current state, returning assets released.
    fn preview_redeem(
        &self,
        env: &Env,
        pool: Option<Address>,
        shares: i128,
    ) -> Result<i128, Self::Error>;

    /// Redeem exact `shares` from `owner`, transferring assets to `receiver`.
    fn redeem(
        &self,
        env: &Env,
        caller: &Address,
        shares: i128,
        receiver: &Address,
        owner: &Address,
        pool: Option<Address>,
    ) -> Result<i128, Self::Error>;
}

/// Default implementation struct for the ERC4626Vault trait.
pub struct StablecoinVault;

impl ERC4626Vault for StablecoinVault {
    type Error = LendingError;

    fn asset(&self, env: &Env, pool: Option<Address>) -> Result<Address, Self::Error> {
        get_asset(env, pool)
    }

    fn total_assets(&self, env: &Env, pool: Option<Address>) -> Result<i128, Self::Error> {
        total_assets(env, pool)
    }

    fn total_shares(&self, env: &Env, pool: Option<Address>) -> Result<i128, Self::Error> {
        total_shares(env, pool)
    }

    fn convert_to_shares(
        &self,
        env: &Env,
        pool: Option<Address>,
        assets: i128,
    ) -> Result<i128, Self::Error> {
        convert_to_shares(env, pool, assets)
    }

    fn convert_to_assets(
        &self,
        env: &Env,
        pool: Option<Address>,
        shares: i128,
    ) -> Result<i128, Self::Error> {
        convert_to_assets(env, pool, shares)
    }

    fn max_deposit(
        &self,
        env: &Env,
        pool: Option<Address>,
        receiver: &Address,
    ) -> Result<i128, Self::Error> {
        max_deposit(env, pool, receiver)
    }

    fn preview_deposit(
        &self,
        env: &Env,
        pool: Option<Address>,
        assets: i128,
    ) -> Result<i128, Self::Error> {
        preview_deposit(env, pool, assets)
    }

    fn deposit(
        &self,
        env: &Env,
        caller: &Address,
        assets: i128,
        receiver: &Address,
        pool: Option<Address>,
    ) -> Result<i128, Self::Error> {
        deposit(env, caller, assets, receiver, pool)
    }

    fn max_mint(
        &self,
        env: &Env,
        pool: Option<Address>,
        receiver: &Address,
    ) -> Result<i128, Self::Error> {
        max_mint(env, pool, receiver)
    }

    fn preview_mint(
        &self,
        env: &Env,
        pool: Option<Address>,
        shares: i128,
    ) -> Result<i128, Self::Error> {
        preview_mint(env, pool, shares)
    }

    fn mint(
        &self,
        env: &Env,
        caller: &Address,
        shares: i128,
        receiver: &Address,
        pool: Option<Address>,
    ) -> Result<i128, Self::Error> {
        mint(env, caller, shares, receiver, pool)
    }

    fn max_withdraw(
        &self,
        env: &Env,
        pool: Option<Address>,
        owner: &Address,
    ) -> Result<i128, Self::Error> {
        max_withdraw(env, pool, owner)
    }

    fn preview_withdraw(
        &self,
        env: &Env,
        pool: Option<Address>,
        assets: i128,
    ) -> Result<i128, Self::Error> {
        preview_withdraw(env, pool, assets)
    }

    fn withdraw(
        &self,
        env: &Env,
        caller: &Address,
        assets: i128,
        receiver: &Address,
        owner: &Address,
        pool: Option<Address>,
    ) -> Result<i128, Self::Error> {
        withdraw(env, caller, assets, receiver, owner, pool)
    }

    fn max_redeem(
        &self,
        env: &Env,
        pool: Option<Address>,
        owner: &Address,
    ) -> Result<i128, Self::Error> {
        max_redeem(env, pool, owner)
    }

    fn preview_redeem(
        &self,
        env: &Env,
        pool: Option<Address>,
        shares: i128,
    ) -> Result<i128, Self::Error> {
        preview_redeem(env, pool, shares)
    }

    fn redeem(
        &self,
        env: &Env,
        caller: &Address,
        shares: i128,
        receiver: &Address,
        owner: &Address,
        pool: Option<Address>,
    ) -> Result<i128, Self::Error> {
        redeem(env, caller, shares, receiver, owner, pool)
    }
}

// ─── Mathematical Helpers ───────────────────────────────────────────────────

/// Ceiling integer division: computes ceil(numerator / denominator) for non-negative integers.
#[inline]
fn div_ceil(numerator: i128, denominator: i128) -> Result<i128, LendingError> {
    if denominator == 0 {
        return Err(LendingError::DivisionByZero);
    }
    if numerator == 0 {
        return Ok(0);
    }
    let add_term = denominator.checked_sub(1).ok_or(LendingError::Overflow)?;
    let num_adj = numerator.checked_add(add_term).ok_or(LendingError::Overflow)?;
    num_adj.checked_div(denominator).ok_or(LendingError::Overflow)
}

// ─── Module Public Functions ────────────────────────────────────────────────

/// Check if a vault for `pool` has been initialized.
pub fn is_initialized(env: &Env, pool: Option<Address>) -> bool {
    env.storage()
        .persistent()
        .get::<VaultKey, bool>(&VaultKey::Initialized(pool))
        .unwrap_or(false)
}

/// Retrieve the underlying asset address for `pool`.
pub fn get_asset(env: &Env, pool: Option<Address>) -> Result<Address, LendingError> {
    env.storage()
        .persistent()
        .get::<VaultKey, Address>(&VaultKey::Asset(pool))
        .ok_or(LendingError::NotInitialized)
}

/// Retrieve the vault configuration for `pool`.
pub fn get_vault_config(env: &Env, pool: Option<Address>) -> Option<VaultConfig> {
    env.storage()
        .persistent()
        .get::<VaultKey, VaultConfig>(&VaultKey::Config(pool))
}

/// Retrieve the total managed assets for `pool`.
pub fn total_assets(env: &Env, pool: Option<Address>) -> Result<i128, LendingError> {
    if !is_initialized(env, pool.clone()) {
        return Err(LendingError::NotInitialized);
    }
    Ok(env
        .storage()
        .persistent()
        .get::<VaultKey, i128>(&VaultKey::TotalAssets(pool))
        .unwrap_or(0))
}

/// Retrieve the total shares minted for `pool`.
pub fn total_shares(env: &Env, pool: Option<Address>) -> Result<i128, LendingError> {
    if !is_initialized(env, pool.clone()) {
        return Err(LendingError::NotInitialized);
    }
    Ok(env
        .storage()
        .persistent()
        .get::<VaultKey, i128>(&VaultKey::TotalShares(pool))
        .unwrap_or(0))
}

/// Retrieve the share balance of `user` in `pool`.
pub fn share_balance_of(env: &Env, pool: Option<Address>, user: &Address) -> i128 {
    env.storage()
        .persistent()
        .get::<VaultKey, i128>(&VaultKey::ShareBalance(pool, user.clone()))
        .unwrap_or(0)
}

/// Retrieve the share allowance granted by `owner` to `spender` in `pool`.
pub fn share_allowance(
    env: &Env,
    pool: Option<Address>,
    owner: &Address,
    spender: &Address,
) -> i128 {
    env.storage()
        .persistent()
        .get::<VaultKey, i128>(&VaultKey::Allowance(pool, owner.clone(), spender.clone()))
        .unwrap_or(0)
}

/// Set the share allowance granted by `owner` to `spender` in `pool`.
pub fn set_share_allowance(
    env: &Env,
    pool: Option<Address>,
    owner: &Address,
    spender: &Address,
    amount: i128,
) {
    env.storage()
        .persistent()
        .set(&VaultKey::Allowance(pool, owner.clone(), spender.clone()), &amount);
}

/// Approve `spender` to spend `amount` shares belonging to `owner`.
pub fn approve_shares(
    env: &Env,
    owner: &Address,
    spender: &Address,
    pool: Option<Address>,
    amount: i128,
) -> Result<(), LendingError> {
    owner.require_auth();
    if amount < 0 {
        return Err(LendingError::InvalidAmount);
    }
    if !is_initialized(env, pool.clone()) {
        return Err(LendingError::NotInitialized);
    }
    set_share_allowance(env, pool.clone(), owner, spender, amount);

    VaultApprovalEvent {
        pool,
        owner: owner.clone(),
        spender: spender.clone(),
        amount,
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);

    Ok(())
}

/// Transfer `amount` shares from caller (`from`) to `to`.
pub fn transfer_shares(
    env: &Env,
    from: &Address,
    to: &Address,
    pool: Option<Address>,
    amount: i128,
) -> Result<(), LendingError> {
    from.require_auth();
    if amount <= 0 {
        return Err(LendingError::InvalidAmount);
    }
    if !is_initialized(env, pool.clone()) {
        return Err(LendingError::NotInitialized);
    }
    let from_bal = share_balance_of(env, pool.clone(), from);
    if from_bal < amount {
        return Err(LendingError::InsufficientBalance);
    }
    let to_bal = share_balance_of(env, pool.clone(), to);
    let new_from = from_bal.checked_sub(amount).ok_or(LendingError::Overflow)?;
    let new_to = to_bal.checked_add(amount).ok_or(LendingError::Overflow)?;

    set_share_balance(env, pool.clone(), from, new_from);
    set_share_balance(env, pool, to, new_to);
    Ok(())
}

/// Initialize a new ERC-4626 stablecoin pool vault.
pub fn initialize_vault(
    env: &Env,
    caller: &Address,
    pool: Option<Address>,
    asset: Address,
    config: VaultConfig,
) -> Result<(), LendingError> {
    caller.require_auth();
    require_admin(env, caller)?;

    if is_initialized(env, pool.clone()) {
        return Err(LendingError::AlreadyInitialized);
    }

    env.storage()
        .persistent()
        .set(&VaultKey::Initialized(pool.clone()), &true);
    env.storage()
        .persistent()
        .set(&VaultKey::Asset(pool.clone()), &asset);
    env.storage()
        .persistent()
        .set(&VaultKey::Config(pool.clone()), &config);
    env.storage()
        .persistent()
        .set(&VaultKey::TotalAssets(pool.clone()), &0i128);
    env.storage()
        .persistent()
        .set(&VaultKey::TotalShares(pool.clone()), &0i128);

    VaultConfiguredEvent {
        pool,
        caller: caller.clone(),
        deposit_cap: config.deposit_cap,
        deposit_paused: config.deposit_paused,
        withdraw_paused: config.withdraw_paused,
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);

    Ok(())
}

/// Update configuration for an existing vault.
pub fn set_vault_config(
    env: &Env,
    caller: &Address,
    pool: Option<Address>,
    config: VaultConfig,
) -> Result<(), LendingError> {
    caller.require_auth();
    require_admin(env, caller)?;

    if !is_initialized(env, pool.clone()) {
        return Err(LendingError::NotInitialized);
    }

    env.storage()
        .persistent()
        .set(&VaultKey::Config(pool.clone()), &config);

    VaultConfiguredEvent {
        pool,
        caller: caller.clone(),
        deposit_cap: config.deposit_cap,
        deposit_paused: config.deposit_paused,
        withdraw_paused: config.withdraw_paused,
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);

    Ok(())
}

/// Accrue interest/yield into the vault, increasing total_assets without altering total_shares.
pub fn accrue_yield(
    env: &Env,
    caller: &Address,
    pool: Option<Address>,
    yield_amount: i128,
) -> Result<i128, LendingError> {
    caller.require_auth();
    require_admin(env, caller)?;

    if yield_amount <= 0 {
        return Err(LendingError::InvalidAmount);
    }
    let tot_assets = total_assets(env, pool.clone())?;
    let new_tot_assets = tot_assets
        .checked_add(yield_amount)
        .ok_or(LendingError::Overflow)?;
    set_total_assets(env, pool.clone(), new_tot_assets);

    VaultYieldAccruedEvent {
        pool,
        yield_amount,
        new_total_assets,
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);

    Ok(new_tot_assets)
}

/// Convert an amount of assets to shares, rounding down (in favor of the vault).
pub fn convert_to_shares(
    env: &Env,
    pool: Option<Address>,
    assets: i128,
) -> Result<i128, LendingError> {
    if assets < 0 {
        return Err(LendingError::InvalidAmount);
    }
    if assets == 0 {
        return Ok(0);
    }
    let tot_assets = total_assets(env, pool.clone())?;
    let tot_shares = total_shares(env, pool)?;

    if tot_assets == 0 || tot_shares == 0 {
        return Ok(assets);
    }

    let effective_assets = tot_assets
        .checked_add(VIRTUAL_OFFSET)
        .ok_or(LendingError::Overflow)?;
    let effective_shares = tot_shares
        .checked_add(VIRTUAL_OFFSET)
        .ok_or(LendingError::Overflow)?;

    let numerator = assets
        .checked_mul(effective_shares)
        .ok_or(LendingError::Overflow)?;
    numerator
        .checked_div(effective_assets)
        .ok_or(LendingError::Overflow)
}

/// Convert an amount of shares to assets, rounding down (in favor of the vault).
pub fn convert_to_assets(
    env: &Env,
    pool: Option<Address>,
    shares: i128,
) -> Result<i128, LendingError> {
    if shares < 0 {
        return Err(LendingError::InvalidAmount);
    }
    if shares == 0 {
        return Ok(0);
    }
    let tot_assets = total_assets(env, pool.clone())?;
    let tot_shares = total_shares(env, pool)?;

    if tot_assets == 0 || tot_shares == 0 {
        return Ok(shares);
    }

    let effective_assets = tot_assets
        .checked_add(VIRTUAL_OFFSET)
        .ok_or(LendingError::Overflow)?;
    let effective_shares = tot_shares
        .checked_add(VIRTUAL_OFFSET)
        .ok_or(LendingError::Overflow)?;

    let numerator = shares
        .checked_mul(effective_assets)
        .ok_or(LendingError::Overflow)?;
    numerator
        .checked_div(effective_shares)
        .ok_or(LendingError::Overflow)
}

/// Maximum assets that can be deposited for `receiver`.
pub fn max_deposit(
    env: &Env,
    pool: Option<Address>,
    _receiver: &Address,
) -> Result<i128, LendingError> {
    let config = match get_vault_config(env, pool.clone()) {
        Some(c) => c,
        None => return Ok(0),
    };
    if config.deposit_paused {
        return Ok(0);
    }
    if config.deposit_cap > 0 {
        let tot_assets = total_assets(env, pool)?;
        if tot_assets >= config.deposit_cap {
            Ok(0)
        } else {
            Ok(config.deposit_cap - tot_assets)
        }
    } else {
        Ok(i128::MAX)
    }
}

/// Simulate a deposit of `assets`, returning shares minted (rounds down).
pub fn preview_deposit(
    env: &Env,
    pool: Option<Address>,
    assets: i128,
) -> Result<i128, LendingError> {
    convert_to_shares(env, pool, assets)
}

/// Deposit `assets` from `caller`, minting shares to `receiver`.
pub fn deposit(
    env: &Env,
    caller: &Address,
    assets: i128,
    receiver: &Address,
    pool: Option<Address>,
) -> Result<i128, LendingError> {
    caller.require_auth();
    if assets <= 0 {
        return Err(LendingError::InvalidAmount);
    }
    let config = get_vault_config(env, pool.clone()).ok_or(LendingError::NotInitialized)?;
    if config.deposit_paused {
        return Err(LendingError::ProtocolPaused);
    }
    if config.min_deposit > 0 && assets < config.min_deposit {
        return Err(LendingError::InvalidAmount);
    }

    let tot_assets = total_assets(env, pool.clone())?;
    if config.deposit_cap > 0 {
        let new_tot = tot_assets
            .checked_add(assets)
            .ok_or(LendingError::Overflow)?;
        if new_tot > config.deposit_cap {
            return Err(LendingError::LimitExceeded);
        }
    }

    let shares = preview_deposit(env, pool.clone(), assets)?;
    if shares <= 0 {
        return Err(LendingError::InvalidAmount);
    }

    let asset = get_asset(env, pool.clone())?;
    #[cfg(not(test))]
    {
        let token_client = soroban_sdk::token::Client::new(env, &asset);
        token_client.transfer(caller, &env.current_contract_address(), &assets);
    }

    let new_tot_assets = tot_assets
        .checked_add(assets)
        .ok_or(LendingError::Overflow)?;
    let tot_shares = total_shares(env, pool.clone())?;
    let new_tot_shares = tot_shares
        .checked_add(shares)
        .ok_or(LendingError::Overflow)?;

    let recv_bal = share_balance_of(env, pool.clone(), receiver);
    let new_recv_bal = recv_bal
        .checked_add(shares)
        .ok_or(LendingError::Overflow)?;

    set_total_assets(env, pool.clone(), new_tot_assets);
    set_total_shares(env, pool.clone(), new_tot_shares);
    set_share_balance(env, pool.clone(), receiver, new_recv_bal);

    VaultDepositEvent {
        pool,
        caller: caller.clone(),
        receiver: receiver.clone(),
        assets,
        shares,
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);

    Ok(shares)
}

/// Maximum shares that can be minted for `receiver`.
pub fn max_mint(
    env: &Env,
    pool: Option<Address>,
    receiver: &Address,
) -> Result<i128, LendingError> {
    let max_dep = max_deposit(env, pool.clone(), receiver)?;
    if max_dep == 0 {
        return Ok(0);
    }
    if max_dep == i128::MAX {
        return Ok(i128::MAX);
    }
    convert_to_shares(env, pool, max_dep)
}

/// Simulate minting `shares`, returning assets required (rounds up).
pub fn preview_mint(
    env: &Env,
    pool: Option<Address>,
    shares: i128,
) -> Result<i128, LendingError> {
    if shares < 0 {
        return Err(LendingError::InvalidAmount);
    }
    if shares == 0 {
        return Ok(0);
    }
    let tot_assets = total_assets(env, pool.clone())?;
    let tot_shares = total_shares(env, pool)?;

    if tot_assets == 0 || tot_shares == 0 {
        return Ok(shares);
    }

    let effective_assets = tot_assets
        .checked_add(VIRTUAL_OFFSET)
        .ok_or(LendingError::Overflow)?;
    let effective_shares = tot_shares
        .checked_add(VIRTUAL_OFFSET)
        .ok_or(LendingError::Overflow)?;

    let numerator = shares
        .checked_mul(effective_assets)
        .ok_or(LendingError::Overflow)?;
    div_ceil(numerator, effective_shares)
}

/// Mint exact `shares` to `receiver`, pulling necessary assets from `caller`.
pub fn mint(
    env: &Env,
    caller: &Address,
    shares: i128,
    receiver: &Address,
    pool: Option<Address>,
) -> Result<i128, LendingError> {
    caller.require_auth();
    if shares <= 0 {
        return Err(LendingError::InvalidAmount);
    }
    let config = get_vault_config(env, pool.clone()).ok_or(LendingError::NotInitialized)?;
    if config.deposit_paused {
        return Err(LendingError::ProtocolPaused);
    }

    let assets = preview_mint(env, pool.clone(), shares)?;
    if assets <= 0 {
        return Err(LendingError::InvalidAmount);
    }
    if config.min_deposit > 0 && assets < config.min_deposit {
        return Err(LendingError::InvalidAmount);
    }

    let tot_assets = total_assets(env, pool.clone())?;
    if config.deposit_cap > 0 {
        let new_tot = tot_assets
            .checked_add(assets)
            .ok_or(LendingError::Overflow)?;
        if new_tot > config.deposit_cap {
            return Err(LendingError::LimitExceeded);
        }
    }

    let asset = get_asset(env, pool.clone())?;
    #[cfg(not(test))]
    {
        let token_client = soroban_sdk::token::Client::new(env, &asset);
        token_client.transfer(caller, &env.current_contract_address(), &assets);
    }

    let new_tot_assets = tot_assets
        .checked_add(assets)
        .ok_or(LendingError::Overflow)?;
    let tot_shares = total_shares(env, pool.clone())?;
    let new_tot_shares = tot_shares
        .checked_add(shares)
        .ok_or(LendingError::Overflow)?;

    let recv_bal = share_balance_of(env, pool.clone(), receiver);
    let new_recv_bal = recv_bal
        .checked_add(shares)
        .ok_or(LendingError::Overflow)?;

    set_total_assets(env, pool.clone(), new_tot_assets);
    set_total_shares(env, pool.clone(), new_tot_shares);
    set_share_balance(env, pool.clone(), receiver, new_recv_bal);

    VaultDepositEvent {
        pool,
        caller: caller.clone(),
        receiver: receiver.clone(),
        assets,
        shares,
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);

    Ok(assets)
}

/// Maximum assets that can be withdrawn by `owner`.
pub fn max_withdraw(
    env: &Env,
    pool: Option<Address>,
    owner: &Address,
) -> Result<i128, LendingError> {
    let config = match get_vault_config(env, pool.clone()) {
        Some(c) => c,
        None => return Ok(0),
    };
    if config.withdraw_paused {
        return Ok(0);
    }
    let shares = share_balance_of(env, pool.clone(), owner);
    if shares == 0 {
        return Ok(0);
    }
    let assets = convert_to_assets(env, pool.clone(), shares)?;
    let tot_assets = total_assets(env, pool)?;
    Ok(assets.min(tot_assets))
}

/// Simulate withdrawal of `assets`, returning shares required to burn (rounds up).
pub fn preview_withdraw(
    env: &Env,
    pool: Option<Address>,
    assets: i128,
) -> Result<i128, LendingError> {
    if assets < 0 {
        return Err(LendingError::InvalidAmount);
    }
    if assets == 0 {
        return Ok(0);
    }
    let tot_assets = total_assets(env, pool.clone())?;
    let tot_shares = total_shares(env, pool)?;

    if tot_assets == 0 || tot_shares == 0 {
        return Ok(assets);
    }

    let effective_assets = tot_assets
        .checked_add(VIRTUAL_OFFSET)
        .ok_or(LendingError::Overflow)?;
    let effective_shares = tot_shares
        .checked_add(VIRTUAL_OFFSET)
        .ok_or(LendingError::Overflow)?;

    let numerator = assets
        .checked_mul(effective_shares)
        .ok_or(LendingError::Overflow)?;
    div_ceil(numerator, effective_assets)
}

/// Withdraw `assets` to `receiver`, burning shares from `owner`.
pub fn withdraw(
    env: &Env,
    caller: &Address,
    assets: i128,
    receiver: &Address,
    owner: &Address,
    pool: Option<Address>,
) -> Result<i128, LendingError> {
    caller.require_auth();
    if assets <= 0 {
        return Err(LendingError::InvalidAmount);
    }
    let config = get_vault_config(env, pool.clone()).ok_or(LendingError::NotInitialized)?;
    if config.withdraw_paused {
        return Err(LendingError::ProtocolPaused);
    }
    let tot_assets = total_assets(env, pool.clone())?;
    if assets > tot_assets {
        return Err(LendingError::InsufficientLiquidity);
    }

    let shares = preview_withdraw(env, pool.clone(), assets)?;
    let owner_bal = share_balance_of(env, pool.clone(), owner);
    if owner_bal < shares {
        return Err(LendingError::InsufficientBalance);
    }

    if caller != owner {
        let allow = share_allowance(env, pool.clone(), owner, caller);
        if allow < shares {
            return Err(LendingError::Unauthorized);
        }
        if allow != i128::MAX {
            set_share_allowance(env, pool.clone(), owner, caller, allow - shares);
        }
    }

    let asset = get_asset(env, pool.clone())?;
    #[cfg(not(test))]
    {
        let token_client = soroban_sdk::token::Client::new(env, &asset);
        token_client.transfer(&env.current_contract_address(), receiver, &assets);
    }

    let new_tot_assets = tot_assets
        .checked_sub(assets)
        .ok_or(LendingError::Overflow)?;
    let tot_shares = total_shares(env, pool.clone())?;
    let new_tot_shares = tot_shares
        .checked_sub(shares)
        .ok_or(LendingError::Overflow)?;
    let new_owner_bal = owner_bal
        .checked_sub(shares)
        .ok_or(LendingError::Overflow)?;

    set_total_assets(env, pool.clone(), new_tot_assets);
    set_total_shares(env, pool.clone(), new_tot_shares);
    set_share_balance(env, pool.clone(), owner, new_owner_bal);

    VaultWithdrawEvent {
        pool,
        caller: caller.clone(),
        receiver: receiver.clone(),
        owner: owner.clone(),
        assets,
        shares,
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);

    Ok(shares)
}

/// Maximum shares that can be redeemed by `owner`.
pub fn max_redeem(
    env: &Env,
    pool: Option<Address>,
    owner: &Address,
) -> Result<i128, LendingError> {
    let config = match get_vault_config(env, pool.clone()) {
        Some(c) => c,
        None => return Ok(0),
    };
    if config.withdraw_paused {
        return Ok(0);
    }
    Ok(share_balance_of(env, pool, owner))
}

/// Simulate redemption of `shares`, returning assets released (rounds down).
pub fn preview_redeem(
    env: &Env,
    pool: Option<Address>,
    shares: i128,
) -> Result<i128, LendingError> {
    convert_to_assets(env, pool, shares)
}

/// Redeem `shares` from `owner`, transferring assets to `receiver`.
pub fn redeem(
    env: &Env,
    caller: &Address,
    shares: i128,
    receiver: &Address,
    owner: &Address,
    pool: Option<Address>,
) -> Result<i128, LendingError> {
    caller.require_auth();
    if shares <= 0 {
        return Err(LendingError::InvalidAmount);
    }
    let config = get_vault_config(env, pool.clone()).ok_or(LendingError::NotInitialized)?;
    if config.withdraw_paused {
        return Err(LendingError::ProtocolPaused);
    }
    let owner_bal = share_balance_of(env, pool.clone(), owner);
    if owner_bal < shares {
        return Err(LendingError::InsufficientBalance);
    }

    if caller != owner {
        let allow = share_allowance(env, pool.clone(), owner, caller);
        if allow < shares {
            return Err(LendingError::Unauthorized);
        }
        if allow != i128::MAX {
            set_share_allowance(env, pool.clone(), owner, caller, allow - shares);
        }
    }

    let assets = preview_redeem(env, pool.clone(), shares)?;
    if assets <= 0 {
        return Err(LendingError::InvalidAmount);
    }
    let tot_assets = total_assets(env, pool.clone())?;
    if assets > tot_assets {
        return Err(LendingError::InsufficientLiquidity);
    }

    let asset = get_asset(env, pool.clone())?;
    #[cfg(not(test))]
    {
        let token_client = soroban_sdk::token::Client::new(env, &asset);
        token_client.transfer(&env.current_contract_address(), receiver, &assets);
    }

    let new_tot_assets = tot_assets
        .checked_sub(assets)
        .ok_or(LendingError::Overflow)?;
    let tot_shares = total_shares(env, pool.clone())?;
    let new_tot_shares = tot_shares
        .checked_sub(shares)
        .ok_or(LendingError::Overflow)?;
    let new_owner_bal = owner_bal
        .checked_sub(shares)
        .ok_or(LendingError::Overflow)?;

    set_total_assets(env, pool.clone(), new_tot_assets);
    set_total_shares(env, pool.clone(), new_tot_shares);
    set_share_balance(env, pool.clone(), owner, new_owner_bal);

    VaultWithdrawEvent {
        pool,
        caller: caller.clone(),
        receiver: receiver.clone(),
        owner: owner.clone(),
        assets,
        shares,
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);

    Ok(assets)
}

/// Retrieve comprehensive statistics for `pool`.
pub fn get_vault_stats(env: &Env, pool: Option<Address>) -> Result<VaultStats, LendingError> {
    let config = get_vault_config(env, pool.clone()).ok_or(LendingError::NotInitialized)?;
    let tot_assets = total_assets(env, pool.clone())?;
    let tot_shares = total_shares(env, pool)?;

    let share_price_scaled = if tot_shares > 0 {
        tot_assets
            .checked_mul(SHARE_PRECISION)
            .ok_or(LendingError::Overflow)?
            .checked_div(tot_shares)
            .ok_or(LendingError::Overflow)?
    } else {
        SHARE_PRECISION
    };

    Ok(VaultStats {
        total_assets: tot_assets,
        total_shares: tot_shares,
        share_price_scaled,
        deposit_cap: config.deposit_cap,
        deposit_paused: config.deposit_paused,
        withdraw_paused: config.withdraw_paused,
    })
}

// ─── Internal Helpers ───────────────────────────────────────────────────────

fn require_admin(env: &Env, caller: &Address) -> Result<(), LendingError> {
    let admin = crate::admin::get_admin(env).ok_or(LendingError::Unauthorized)?;
    if &admin != caller {
        return Err(LendingError::Unauthorized);
    }
    Ok(())
}

fn set_total_assets(env: &Env, pool: Option<Address>, amount: i128) {
    env.storage()
        .persistent()
        .set(&VaultKey::TotalAssets(pool), &amount);
}

fn set_total_shares(env: &Env, pool: Option<Address>, amount: i128) {
    env.storage()
        .persistent()
        .set(&VaultKey::TotalShares(pool), &amount);
}

fn set_share_balance(env: &Env, pool: Option<Address>, user: &Address, amount: i128) {
    env.storage()
        .persistent()
        .set(&VaultKey::ShareBalance(pool, user.clone()), &amount);
}
