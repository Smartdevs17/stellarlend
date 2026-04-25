//! Standardized contract events (V1 schema)

use soroban_sdk::contracttype;

pub mod borrow;
mod deposit;
pub mod events;
mod flash_loan;
pub mod invariants;
pub mod pause;
mod token_receiver;
mod withdraw;
pub mod yield_farming;
use soroban_sdk::{contractevent, Address, Symbol};

// ─────────────────────────────────────────
// Shared constants
// ─────────────────────────────────────────

const BORROW: Symbol = Symbol::short("borrow");
const REPAY: Symbol = Symbol::short("repay");
const DEPOSIT: Symbol = Symbol::short("deposit");
const WITHDRAW: Symbol = Symbol::short("withdraw");
const FLASH_LOAN: Symbol = Symbol::short("flash");
const BAD_DEBT: Symbol = Symbol::short("baddebt");
const BAD_DEBT_RECOVER: Symbol = Symbol::short("bdrec");

// ─────────────────────────────────────────
// Lending Events (STANDARDIZED)
// ─────────────────────────────────────────

#[contractevent]
#[derive(Clone, Debug)]
pub struct BorrowEventV1 {
    #[topic]
    pub event: Symbol, // "borrow"

    #[topic]
    pub user: Address,

    pub asset: Address,
    pub amount: i128,
    pub collateral: i128,
    pub timestamp: u64,
}

// Re-export contract types used in the public interface so downstream tooling
// (including fuzzing harnesses) can construct/inspect them without relying on
// private module paths.
pub use borrow::{BorrowCollateral, BorrowError, DebtPosition, StablecoinConfig};
pub use deposit::{DepositCollateral, DepositError};
pub use flash_loan::FlashLoanError;
pub use pause::PauseType;
pub use views::{ProtocolMetrics, ProtocolReport, StablecoinAssetStats, UserPositionSummary};
pub use withdraw::WithdrawError;

use borrow::{
    borrow as borrow_cmd, deposit as borrow_deposit, get_admin as get_borrow_admin,
    get_stablecoin_config as get_stablecoin_config_logic,
    get_user_collateral as get_borrow_collateral, get_user_debt as get_borrow_debt,
    initialize_borrow_settings as initialize_borrow_logic, repay as borrow_repay,
    set_admin as set_borrow_admin,
    set_liquidation_threshold_bps as set_liquidation_threshold_logic,
    set_oracle as set_oracle_logic, set_stablecoin_config as set_stablecoin_config_logic,
};
use deposit::{
    deposit as deposit_logic, get_user_collateral as get_deposit_collateral,
    initialize_deposit_settings as initialize_deposit_logic,
};
use flash_loan::{
    flash_loan as flash_loan_logic, set_flash_loan_fee_bps as set_flash_loan_fee_logic,
};
use pause::{is_paused, set_pause as set_pause_logic};
use token_receiver::receive as receive_logic;

pub mod views;
use views::{
    get_collateral_balance as view_collateral_balance,
    get_collateral_value as view_collateral_value, get_debt_balance as view_debt_balance,
    get_debt_value as view_debt_value, get_health_factor as view_health_factor,
    get_user_position as view_user_position,
};

use withdraw::{
    initialize_withdraw_settings as initialize_withdraw_logic,
    set_withdraw_paused as set_withdraw_paused_logic, withdraw as withdraw_logic,
};

#[derive(Clone)]
#[contracttype]
pub enum BadDebtKey {
    Total,
    User(Address),
}

#[derive(Clone)]
#[contracttype]
pub enum ReserveKey {
    ProtocolReserves,
}

mod data_store;
pub mod upgrade;

#[cfg(test)]
mod borrow_test;
#[cfg(test)]
mod data_store_test;
#[cfg(test)]
mod deposit_test;
#[cfg(test)]
mod flash_loan_test;
#[cfg(test)]
mod math_safety_test;
#[cfg(test)]
mod pause_test;
#[cfg(test)]
mod stablecoin_test;
#[cfg(test)]
mod token_receiver_test;
#[cfg(test)]
mod upgrade_test;
#[cfg(test)]
mod views_test;
#[cfg(test)]
mod withdraw_test;

#[contract]
pub struct LendingContract;

#[contractimpl]
impl LendingContract {
    /// Initialize the protocol with admin and settings
    pub fn initialize(
        env: Env,
        admin: Address,
        debt_ceiling: i128,
        min_borrow_amount: i128,
    ) -> Result<(), BorrowError> {
        if get_borrow_admin(&env).is_some() {
            return Err(BorrowError::Unauthorized);
        }
        set_borrow_admin(&env, &admin);
        initialize_borrow_logic(&env, debt_ceiling, min_borrow_amount)?;
        Ok(())
    }

    // Bad debt helper functions
    pub fn get_total_bad_debt(env: &Env) -> i128 {
        env.storage()
            .persistent()
            .get(&BadDebtKey::Total)
            .unwrap_or(0)
    }

    pub fn add_bad_debt(env: &Env, user: &Address, amount: i128) {
        let mut total = Self::get_total_bad_debt(env);
        total += amount;

        env.storage().persistent().set(&BadDebtKey::Total, &total);

        let user_key = BadDebtKey::User(user.clone());
        let mut user_debt = env.storage().persistent().get(&user_key).unwrap_or(0);
        user_debt += amount;

        env.storage().persistent().set(&user_key, &user_debt);
    }

    pub fn get_reserves(env: &Env) -> i128 {
        env.storage()
            .persistent()
            .get(&ReserveKey::ProtocolReserves)
            .unwrap_or(0)
    }

    /// Borrow assets against deposited collateral
    pub fn borrow(
        env: Env,
        user: Address,
        asset: Address,
        amount: i128,
        collateral_asset: Address,
        collateral_amount: i128,
    ) -> Result<(), BorrowError> {
        borrow_cmd(
            &env,
            user,
            asset,
            amount,
            collateral_asset,
            collateral_amount,
        )
    }

    /// Set protocol pause state for a specific operation (admin only)
    pub fn set_pause(
        env: Env,
        admin: Address,
        pause_type: PauseType,
        paused: bool,
    ) -> Result<(), BorrowError> {
        let current_admin = get_borrow_admin(&env).ok_or(BorrowError::Unauthorized)?;
        if admin != current_admin {
            return Err(BorrowError::Unauthorized);
        }
        admin.require_auth();
        set_pause_logic(&env, admin, pause_type, paused);
        Ok(())
    }

    /// Repay borrowed assets
    pub fn repay(env: Env, user: Address, asset: Address, amount: i128) -> Result<(), BorrowError> {
        user.require_auth();
        if is_paused(&env, PauseType::Repay) {
            return Err(BorrowError::ProtocolPaused);
        }
        borrow_repay(&env, user, asset, amount)
    }

    /// Deposit collateral into the protocol
    pub fn deposit(
        env: Env,
        user: Address,
        asset: Address,
        amount: i128,
    ) -> Result<i128, DepositError> {
        if is_paused(&env, PauseType::Deposit) {
            return Err(DepositError::DepositPaused);
        }
        deposit_logic(&env, user, asset, amount)
    }

    /// Deposit collateral for a borrow position
    pub fn deposit_collateral(
        env: Env,
        user: Address,
        asset: Address,
        amount: i128,
    ) -> Result<(), BorrowError> {
        user.require_auth();
        if is_paused(&env, PauseType::Deposit) {
            return Err(BorrowError::ProtocolPaused);
        }
        borrow_deposit(&env, user, asset, amount)
    }

    /// Liquidate a position
    pub fn liquidate(
        env: Env,
        liquidator: Address,
        borrower: Address,
        debt_asset: Address,
        collateral_asset: Address,
        repay_amount: i128,
    ) -> Result<(), BorrowError> {
        liquidator.require_auth();

        if is_paused(&env, PauseType::Liquidation) {
            return Err(BorrowError::ProtocolPaused);
        }

        // 1. Get borrower state
        let debt_value = view_debt_value(&env, &borrower.clone());
        let collateral_value = view_collateral_value(&env, &borrower.clone());

        // 2. Ensure liquidatable
        let health = view_health_factor(&env, &borrower.clone());
        if health >= 10000 {
            return Err(BorrowError::PositionHealthy);
        }

        // 3. Perform liquidation logic (simplified)
        let recovered_value = repay_amount; // (you’ll replace with real calc)

        // 4. Detect bad debt
        if recovered_value < debt_value {
            let bad_debt = debt_value - recovered_value;

            Self::add_bad_debt(&env, &borrower, bad_debt);

            // Optional: emit event
            events::emit_bad_debt(&env, &borrower, bad_debt);
        }

        Ok(())
    }
    /// Get user's debt position
    pub fn get_user_debt(env: Env, user: Address) -> DebtPosition {
        get_borrow_debt(&env, &user)
    }
#[contractevent]
#[derive(Clone, Debug)]
pub struct RepayEventV1 {
    #[topic]
    pub event: Symbol, // "repay"

    #[topic]
    pub user: Address,

    pub asset: Address,
    pub amount: i128,
    pub timestamp: u64,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct DepositEventV1 {
    #[topic]
    pub event: Symbol, // "deposit"

    #[topic]
    pub user: Address,

    pub asset: Address,
    pub amount: i128,
    pub new_balance: i128,
    pub timestamp: u64,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct WithdrawEventV1 {
    #[topic]
    pub event: Symbol, // "withdraw"

    #[topic]
    pub user: Address,

    pub asset: Address,
    pub amount: i128,
    pub remaining_balance: i128,
    pub timestamp: u64,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct FlashLoanEventV1 {
    #[topic]
    pub event: Symbol, // "flash"

    #[topic]
    pub receiver: Address,

    pub asset: Address,
    pub amount: i128,
    pub fee: i128,
    pub timestamp: u64,
}

// ─────────────────────────────────────────
// Bad Debt Events (NEW)
// ─────────────────────────────────────────

#[contractevent]
#[derive(Clone, Debug)]
pub struct BadDebtEventV1 {
    #[topic]
    pub event: Symbol, // "baddebt"

    #[topic]
    pub user: Address,

    pub amount: i128,
    pub timestamp: u64,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct BadDebtRecoveredEventV1 {
    #[topic]
    pub event: Symbol, // "bdrec"

    pub amount: i128,
    pub timestamp: u64,
}

use soroban_sdk::Env;

pub fn emit_borrow(env: &Env, user: Address, asset: Address, amount: i128, collateral: i128) {
    BorrowEventV1 {
        event: BORROW,
        user,
        asset,
        amount,
        collateral,
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

pub fn emit_deposit(env: &Env, user: Address, asset: Address, amount: i128, balance: i128) {
    DepositEventV1 {
        event: DEPOSIT,
        user,
        asset,
        amount,
        new_balance: balance,
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

pub fn emit_bad_debt(env: &Env, user: &Address, amount: i128) {
    BadDebtEventV1 {
        event: BAD_DEBT,
        user: user.clone(),
        amount,
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

    /// Get protocol report including stablecoin stats
    pub fn get_protocol_report(env: Env, stablecoin_assets: Vec<Address>) -> ProtocolReport {
        views::get_protocol_report(&env, stablecoin_assets)
    }

    pub fn recover_bad_debt(env: Env, admin: Address, amount: i128) -> Result<(), BorrowError> {
        let current_admin = get_borrow_admin(&env).ok_or(BorrowError::Unauthorized)?;
        if admin != current_admin {
            return Err(BorrowError::Unauthorized);
        }
        admin.require_auth();

        let mut reserves = Self::get_reserves(&env);
        let mut bad_debt = Self::get_total_bad_debt(&env);

        if reserves < amount {
            return Err(BorrowError::InsufficientReserves);
        }

        let repay_amount = if amount > bad_debt { bad_debt } else { amount };

        reserves -= repay_amount;
        bad_debt -= repay_amount;

        env.storage()
            .persistent()
            .set(&ReserveKey::ProtocolReserves, &reserves);
        env.storage()
            .persistent()
            .set(&BadDebtKey::Total, &bad_debt);

        events::emit_bad_debt_recovered(&env, repay_amount);

        Ok(())
    }
pub fn emit_bad_debt_recovered(env: &Env, amount: i128) {
    BadDebtRecoveredEventV1 {
        event: BAD_DEBT_RECOVER,
        amount,
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}
