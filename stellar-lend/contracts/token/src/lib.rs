#![no_std]

extern crate alloc;

use soroban_sdk::{contracterror, contracttype, Address, Env, Vec};
use soroban_token_sdk::token::{Client as TokenClient, StellarAssetClient};

// ========================================================================
// Unified Error Type
// ========================================================================

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum TokenError {
    /// Insufficient balance for the requested operation.
    InsufficientBalance = 1,
    /// Insufficient allowance for the spender.
    InsufficientAllowance = 2,
    /// Token transfer failed.
    TransferFailed = 3,
    /// Token approval failed.
    ApproveFailed = 4,
    /// Invalid amount (zero, negative, or exceeds bounds).
    InvalidAmount = 5,
    /// Decimal mismatch between expected and actual token decimals.
    DecimalMismatch = 6,
    /// Amount is below the dust threshold.
    DustAmount = 7,
    /// Token address is not registered with the adapter.
    TokenNotRegistered = 8,
    /// Caller is not authorized for this operation.
    Unauthorized = 9,
    /// Arithmetic overflow occurred.
    Overflow = 10,
    /// Adapter verification failed.
    VerificationFailed = 11,
    /// Adapter type does not match the expected type.
    TypeMismatch = 12,
}

// ========================================================================
// Token Types & Configuration
// ========================================================================

/// Supported token standard variants.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum TokenType {
    /// Standard Soroban SEP-41 token contract.
    SorobanToken,
    /// Native Stellar asset (via SAC).
    NativeAsset,
    /// Wrapped representation of a native or external asset.
    WrappedAsset,
}

/// Metadata for a registered token.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct TokenInfo {
    pub address: Address,
    pub token_type: TokenType,
    pub decimals: u32,
    pub min_transfer: i128,
}

/// Storage keys for the adapter.
#[contracttype]
#[derive(Clone, Debug)]
pub enum DataKey {
    Admin,
    TokenInfo(Address),
    AdapterConfig(Address),
    RegisteredTokens,
    GlobalDecimals,
}

/// Result of a transfer operation.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct TransferResult {
    pub from: Address,
    pub to: Address,
    pub amount: i128,
    pub actual_amount: i128,
    pub dust_remaining: i128,
}

/// Result of a batch approve-and-transfer operation.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct BatchResult {
    pub approve_result: bool,
    pub transfer_from_result: bool,
    pub total_moved: i128,
}

/// Verification result for an adapter.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct VerificationResult {
    pub is_valid: bool,
    pub token_type: TokenType,
    pub verified_at: u64,
}

// ========================================================================
// Core Token Adapter Trait
// ========================================================================

/// Unified trait for all token operations.
///
/// Implementors must support the full SEP-41 interface (transfer, approve,
/// balance_of, allowance, total_supply) plus dust filtering, decimal
/// normalization, and batch operations.
pub trait TokenAdapter {
    /// Returns the type of token this adapter handles.
    fn token_type(env: &Env, token: &Address) -> Result<TokenType, TokenError>;

    /// Returns the registered token info, or `TokenNotRegistered`.
    fn get_token_info(env: &Env, token: &Address) -> Result<TokenInfo, TokenError>;

    /// Transfer tokens between addresses.
    fn transfer(
        env: &Env,
        token: &Address,
        from: &Address,
        to: &Address,
        amount: i128,
    ) -> Result<TransferResult, TokenError>;

    /// Transfer tokens using allowance (approve + transfer_from pattern).
    fn transfer_from(
        env: &Env,
        token: &Address,
        from: &Address,
        to: &Address,
        amount: i128,
    ) -> Result<TransferResult, TokenError>;

    /// Approve a spender to spend tokens on behalf of the owner.
    fn approve(
        env: &Env,
        token: &Address,
        owner: &Address,
        spender: &Address,
        amount: i128,
    ) -> Result<(), TokenError>;

    /// Query the token balance of an address.
    fn balance_of(env: &Env, token: &Address, address: &Address) -> Result<i128, TokenError>;

    /// Query the allowance granted by owner to spender.
    fn allowance(
        env: &Env,
        token: &Address,
        owner: &Address,
        spender: &Address,
    ) -> Result<i128, TokenError>;

    /// Query the total supply of the token.
    fn total_supply(env: &Env, token: &Address) -> Result<i128, TokenError>;

    /// Normalize an amount from one decimal precision to the token's precision.
    fn normalize_amount(
        env: &Env,
        token: &Address,
        amount: i128,
        from_decimals: u32,
    ) -> Result<i128, TokenError>;

    /// Convert a human-readable amount to the token's minimal unit.
    fn to_minimal_unit(
        env: &Env,
        token: &Address,
        amount: i128,
    ) -> Result<i128, TokenError>;

    /// Convert a minimal unit amount to human-readable format.
    fn from_minimal_unit(
        env: &Env,
        token: &Address,
        minimal: i128,
    ) -> Result<i128, TokenError>;

    /// Perform an approve + transfer_from in a single call.
    fn batch_approve_and_transfer(
        env: &Env,
        token: &Address,
        owner: &Address,
        spender: &Address,
        to: &Address,
        amount: i128,
    ) -> Result<BatchResult, TokenError>;
}

// ========================================================================
// Unified Adapter Implementation
// ========================================================================

/// The unified token adapter that dispatches to the correct underlying
/// token client based on the registered token type.
pub struct UnifiedTokenAdapter;

impl UnifiedTokenAdapter {
    /// Register a token with the adapter.
    pub fn register_token(
        env: &Env,
        admin: &Address,
        token_address: &Address,
        token_type: TokenType,
        decimals: u32,
        min_transfer: i128,
    ) -> Result<(), TokenError> {
        Self::require_admin(env, admin)?;

        let info = TokenInfo {
            address: token_address.clone(),
            token_type,
            decimals,
            min_transfer,
        };
        env.storage()
            .persistent()
            .set(&DataKey::TokenInfo(token_address.clone()), &info);
        Ok(())
    }

    /// Verify that a registered token can perform core operations.
    pub fn verify_token(
        env: &Env,
        token: &Address,
    ) -> Result<VerificationResult, TokenError> {
        let info = Self::get_token_info_stored(env, token)?;

        // Try a balance_of call to verify the token is functional
        let _balance = Self::do_balance_of(env, &info, &env.current_contract_address())?;

        Ok(VerificationResult {
            is_valid: true,
            token_type: info.token_type,
            verified_at: env.ledger().timestamp(),
        })
    }

    /// Get the admin address.
    pub fn get_admin(env: &Env) -> Option<Address> {
        env.storage().persistent().get(&DataKey::Admin)
    }

    /// Initialize the adapter with an admin.
    pub fn initialize(env: &Env, admin: &Address) -> Result<(), TokenError> {
        if env.storage().persistent().has(&DataKey::Admin) {
            return Ok(()); // Already initialized
        }
        admin.require_auth();
        env.storage().persistent().set(&DataKey::Admin, admin);
        env.storage()
            .persistent()
            .set(&DataKey::GlobalDecimals, &18u32);
        Ok(())
    }

    // -- Internal helpers --

    fn require_admin(env: &Env, admin: &Address) -> Result<(), TokenError> {
        let stored: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .ok_or(TokenError::Unauthorized)?;
        if admin != &stored {
            return Err(TokenError::Unauthorized);
        }
        admin.require_auth();
        Ok(())
    }

    fn get_token_info_stored(
        env: &Env,
        token: &Address,
    ) -> Result<TokenInfo, TokenError> {
        env.storage()
            .persistent()
            .get(&DataKey::TokenInfo(token.clone()))
            .ok_or(TokenError::TokenNotRegistered)
    }

    fn apply_dust_filter(amount: i128, min_transfer: i128) -> i128 {
        if min_transfer <= 0 || amount < min_transfer {
            0
        } else {
            amount
        }
    }

    fn do_transfer(
        env: &Env,
        info: &TokenInfo,
        from: &Address,
        to: &Address,
        amount: i128,
    ) -> Result<(), TokenError> {
        match info.token_type {
            TokenType::SorobanToken => {
                let client = TokenClient::new(env, &info.address);
                client.transfer(from, to, &amount);
                Ok(())
            }
            TokenType::NativeAsset | TokenType::WrappedAsset => {
                let client = StellarAssetClient::new(env, &info.address);
                client.transfer(from, to, &amount);
                Ok(())
            }
        }
    }

    fn do_transfer_from(
        env: &Env,
        info: &TokenInfo,
        from: &Address,
        to: &Address,
        amount: i128,
    ) -> Result<(), TokenError> {
        match info.token_type {
            TokenType::SorobanToken => {
                let client = TokenClient::new(env, &info.address);
                let contract_addr = env.current_contract_address();
                client.transfer_from(from, &contract_addr, &amount);
                client.transfer(&contract_addr, to, &amount);
                Ok(())
            }
            TokenType::NativeAsset | TokenType::WrappedAsset => {
                let client = StellarAssetClient::new(env, &info.address);
                let contract_addr = env.current_contract_address();
                client.transfer(from, &contract_addr, &amount);
                client.transfer(&contract_addr, to, &amount);
                Ok(())
            }
        }
    }

    fn do_approve(
        env: &Env,
        info: &TokenInfo,
        owner: &Address,
        spender: &Address,
        amount: i128,
    ) -> Result<(), TokenError> {
        match info.token_type {
            TokenType::SorobanToken => {
                let client = TokenClient::new(env, &info.address);
                let ledger = env.ledger().sequence();
                client.approve(owner, spender, &amount, &(ledger + 100));
                Ok(())
            }
            TokenType::NativeAsset | TokenType::WrappedAsset => {
                let client = StellarAssetClient::new(env, &info.address);
                let ledger = env.ledger().sequence();
                client.approve(owner, spender, &amount, &(ledger + 100));
                Ok(())
            }
        }
    }

    fn do_balance_of(
        env: &Env,
        info: &TokenInfo,
        address: &Address,
    ) -> Result<i128, TokenError> {
        match info.token_type {
            TokenType::SorobanToken => {
                let client = TokenClient::new(env, &info.address);
                Ok(client.balance(address))
            }
            TokenType::NativeAsset | TokenType::WrappedAsset => {
                let client = StellarAssetClient::new(env, &info.address);
                Ok(client.balance(address))
            }
        }
    }

    fn do_allowance(
        env: &Env,
        info: &TokenInfo,
        owner: &Address,
        spender: &Address,
    ) -> Result<i128, TokenError> {
        match info.token_type {
            TokenType::SorobanToken => {
                let client = TokenClient::new(env, &info.address);
                Ok(client.allowance(owner, spender))
            }
            TokenType::NativeAsset | TokenType::WrappedAsset => {
                let client = StellarAssetClient::new(env, &info.address);
                Ok(client.allowance(owner, spender))
            }
        }
    }

    fn do_total_supply(
        env: &Env,
        info: &TokenInfo,
    ) -> Result<i128, TokenError> {
        match info.token_type {
            TokenType::SorobanToken => {
                let client = TokenClient::new(env, &info.address);
                Ok(client.total_supply())
            }
            TokenType::NativeAsset | TokenType::WrappedAsset => {
                let client = StellarAssetClient::new(env, &info.address);
                Ok(client.balance(&env.current_contract_address()))
            }
        }
    }
}

impl TokenAdapter for UnifiedTokenAdapter {
    fn token_type(env: &Env, token: &Address) -> Result<TokenType, TokenError> {
        let info = Self::get_token_info_stored(env, token)?;
        Ok(info.token_type)
    }

    fn get_token_info(env: &Env, token: &Address) -> Result<TokenInfo, TokenError> {
        Self::get_token_info_stored(env, token)
    }

    fn transfer(
        env: &Env,
        token: &Address,
        from: &Address,
        to: &Address,
        amount: i128,
    ) -> Result<TransferResult, TokenError> {
        if amount <= 0 {
            return Err(TokenError::InvalidAmount);
        }
        let info = Self::get_token_info_stored(env, token)?;
        let actual_amount = Self::apply_dust_filter(amount, info.min_transfer);
        if actual_amount <= 0 {
            return Err(TokenError::DustAmount);
        }
        let balance = Self::do_balance_of(env, &info, from)?;
        if balance < actual_amount {
            return Err(TokenError::InsufficientBalance);
        }
        Self::do_transfer(env, &info, from, to, actual_amount)?;
        Ok(TransferResult {
            from: from.clone(),
            to: to.clone(),
            amount,
            actual_amount,
            dust_remaining: amount - actual_amount,
        })
    }

    fn transfer_from(
        env: &Env,
        token: &Address,
        from: &Address,
        to: &Address,
        amount: i128,
    ) -> Result<TransferResult, TokenError> {
        if amount <= 0 {
            return Err(TokenError::InvalidAmount);
        }
        let info = Self::get_token_info_stored(env, token)?;
        let actual_amount = Self::apply_dust_filter(amount, info.min_transfer);
        if actual_amount <= 0 {
            return Err(TokenError::DustAmount);
        }
        let allowance = Self::do_allowance(env, &info, from, &env.current_contract_address())?;
        if allowance < actual_amount {
            return Err(TokenError::InsufficientAllowance);
        }
        Self::do_transfer_from(env, &info, from, to, actual_amount)?;
        Ok(TransferResult {
            from: from.clone(),
            to: to.clone(),
            amount,
            actual_amount,
            dust_remaining: amount - actual_amount,
        })
    }

    fn approve(
        env: &Env,
        token: &Address,
        owner: &Address,
        spender: &Address,
        amount: i128,
    ) -> Result<(), TokenError> {
        if amount < 0 {
            return Err(TokenError::InvalidAmount);
        }
        let info = Self::get_token_info_stored(env, token)?;
        Self::do_approve(env, &info, owner, spender, amount)
    }

    fn balance_of(env: &Env, token: &Address, address: &Address) -> Result<i128, TokenError> {
        let info = Self::get_token_info_stored(env, token)?;
        Self::do_balance_of(env, &info, address)
    }

    fn allowance(
        env: &Env,
        token: &Address,
        owner: &Address,
        spender: &Address,
    ) -> Result<i128, TokenError> {
        let info = Self::get_token_info_stored(env, token)?;
        Self::do_allowance(env, &info, owner, spender)
    }

    fn total_supply(env: &Env, token: &Address) -> Result<i128, TokenError> {
        let info = Self::get_token_info_stored(env, token)?;
        Self::do_total_supply(env, &info)
    }

    fn normalize_amount(
        env: &Env,
        token: &Address,
        amount: i128,
        from_decimals: u32,
    ) -> Result<i128, TokenError> {
        let info = Self::get_token_info_stored(env, token)?;
        if from_decimals == info.decimals {
            return Ok(amount);
        }
        if from_decimals > info.decimals {
            let diff = from_decimals - info.decimals;
            let divisor = 10i128.pow(diff);
            Ok(amount / divisor)
        } else {
            let diff = info.decimals - from_decimals;
            let multiplier = 10i128.pow(diff);
            amount.checked_mul(multiplier).ok_or(TokenError::Overflow)
        }
    }

    fn to_minimal_unit(
        env: &Env,
        token: &Address,
        amount: i128,
    ) -> Result<i128, TokenError> {
        let info = Self::get_token_info_stored(env, token)?;
        let scale = 10i128.pow(info.decimals);
        amount.checked_mul(scale).ok_or(TokenError::Overflow)
    }

    fn from_minimal_unit(
        env: &Env,
        token: &Address,
        minimal: i128,
    ) -> Result<i128, TokenError> {
        let info = Self::get_token_info_stored(env, token)?;
        let scale = 10i128.pow(info.decimals);
        Ok(minimal / scale)
    }

    fn batch_approve_and_transfer(
        env: &Env,
        token: &Address,
        owner: &Address,
        spender: &Address,
        to: &Address,
        amount: i128,
    ) -> Result<BatchResult, TokenError> {
        Self::approve(env, token, owner, spender, amount)?;
        let transfer_result = Self::transfer_from(env, token, spender, to, amount)?;
        Ok(BatchResult {
            approve_result: true,
            transfer_from_result: true,
            total_moved: transfer_result.actual_amount,
        })
    }
}
