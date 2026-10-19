//! Unified token adapter interface and patterns
//!
//! This module provides standardized token handling patterns across all contracts,
//! ensuring consistent behavior for token operations.

use soroban_sdk::{contracttype, Address, Env, Symbol};

/// Unified token operation interface
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct UnifiedTokenInterface {
    /// Token address
    pub token: Address,
    /// Standard interface version (e.g., "1.0")
    pub version: u32,
    /// Supported features
    pub features: u32,
}

/// Token operation result with metadata
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct TokenOperationResult {
    /// Success flag
    pub success: bool,
    /// Amount processed
    pub amount: i128,
    /// Gas used estimation
    pub gas_used: u64,
    /// Error code if failed
    pub error_code: u32,
}

/// Standardized token transfer pattern
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct StandardizedTransfer {
    /// Source address
    pub from: Address,
    /// Destination address
    pub to: Address,
    /// Token address
    pub token: Address,
    /// Amount to transfer
    pub amount: i128,
    /// Optional memo
    pub memo: u32,
    /// Timestamp
    pub timestamp: u64,
}

/// Token metadata with caching info
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct TokenMetadata {
    /// Token address
    pub token: Address,
    /// Token name
    pub name: Symbol,
    /// Token symbol
    pub symbol: Symbol,
    /// Number of decimals
    pub decimals: u32,
    /// Total supply (if available)
    pub total_supply: Option<i128>,
    /// Last update timestamp
    pub updated_at: u64,
    /// Cache validity duration in seconds
    pub cache_ttl: u64,
}

impl TokenMetadata {
    /// Check if metadata cache is still valid
    pub fn is_cache_valid(&self, current_time: u64) -> bool {
        current_time < (self.updated_at.saturating_add(self.cache_ttl))
    }

    /// Get age of metadata in seconds
    pub fn age(&self, current_time: u64) -> u64 {
        current_time.saturating_sub(self.updated_at)
    }
}

/// Validation rules for token operations
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct TokenValidationRules {
    /// Minimum transfer amount
    pub min_transfer: i128,
    /// Maximum transfer amount
    pub max_transfer: i128,
    /// Minimum balance required
    pub min_balance: i128,
    /// Fee percentage (in basis points)
    pub fee_bps: u32,
    /// Whether token is blacklisted
    pub is_blacklisted: bool,
    /// Whether token is whitelisted (if whitelist enabled)
    pub is_whitelisted: bool,
}

impl TokenValidationRules {
    /// Validate transfer amount against rules
    pub fn validate_transfer_amount(&self, amount: i128) -> Result<(), &'static str> {
        if amount < self.min_transfer {
            return Err("Amount below minimum");
        }
        if amount > self.max_transfer {
            return Err("Amount exceeds maximum");
        }
        Ok(())
    }

    /// Check if token operations are allowed
    pub fn is_operation_allowed(&self) -> bool {
        !self.is_blacklisted
    }
}

/// Unified error handling for token operations
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum UnifiedTokenError {
    /// Token operation failed
    OperationFailed = 1,
    /// Invalid token
    InvalidToken = 2,
    /// Validation failed
    ValidationFailed = 3,
    /// Metadata cache stale
    MetadataCacheStale = 4,
    /// Token blacklisted
    TokenBlacklisted = 5,
    /// Insufficient balance
    InsufficientBalance = 6,
    /// Insufficient allowance
    InsufficientAllowance = 7,
}

/// Token adapter module
pub struct UnifiedTokenAdapter;

impl UnifiedTokenAdapter {
    /// Validate token according to standardized rules
    pub fn validate_token(_env: &Env, _token: &Address, rules: &TokenValidationRules) -> Result<(), UnifiedTokenError> {
        if rules.is_blacklisted {
            return Err(UnifiedTokenError::TokenBlacklisted);
        }
        Ok(())
    }

    /// Normalize amount according to token decimals
    pub fn normalize_amount(amount: i128, from_decimals: u32, to_decimals: u32) -> Result<i128, UnifiedTokenError> {
        if from_decimals == to_decimals {
            return Ok(amount);
        }

        if from_decimals > to_decimals {
            let factor = 10i128.pow(from_decimals - to_decimals);
            amount.checked_div(factor).ok_or(UnifiedTokenError::OperationFailed)
        } else {
            let factor = 10i128.pow(to_decimals - from_decimals);
            amount.checked_mul(factor).ok_or(UnifiedTokenError::OperationFailed)
        }
    }

    /// Calculate fee for transfer
    pub fn calculate_fee(amount: i128, fee_bps: u32) -> Result<i128, UnifiedTokenError> {
        // fee = amount * fee_bps / 10000
        amount
            .checked_mul(fee_bps as i128)
            .and_then(|v| v.checked_div(10_000))
            .ok_or(UnifiedTokenError::OperationFailed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metadata_cache_validity() {
        let metadata = TokenMetadata {
            token: unsafe { Address::from_raw_bytes(&[0u8; 32]) },
            name: Symbol::from_str(&unsafe { soroban_sdk::Env::default() }, "TEST"),
            symbol: Symbol::from_str(&unsafe { soroban_sdk::Env::default() }, "TST"),
            decimals: 18,
            total_supply: None,
            updated_at: 1000,
            cache_ttl: 300,
        };

        assert!(metadata.is_cache_valid(1200)); // Within TTL
        assert!(!metadata.is_cache_valid(1400)); // TTL expired
        assert_eq!(metadata.age(1100), 100);
    }

    #[test]
    fn test_validation_rules() {
        let rules = TokenValidationRules {
            min_transfer: 100,
            max_transfer: 1_000_000,
            min_balance: 0,
            fee_bps: 0,
            is_blacklisted: false,
            is_whitelisted: true,
        };

        assert!(rules.validate_transfer_amount(500).is_ok());
        assert!(rules.validate_transfer_amount(50).is_err());
        assert!(rules.validate_transfer_amount(1_000_001).is_err());
        assert!(rules.is_operation_allowed());
    }
}
