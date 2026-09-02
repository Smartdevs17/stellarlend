//! Tests for the unified error handling framework bridge in `crate::errors`.
//!
//! These tests live in a dedicated file so they don't depend on the (currently
//! mid-refactor) rest of the contract. They only require the per-module error
//! enums declared in `crate::errors`, all of which are part of the lib's public
//! surface and have stable ABI codes.

#![cfg(test)]

use crate::admin::AdminError;
use crate::analytics::AnalyticsError;
use crate::borrow::BorrowError;
use crate::cross_asset::CrossAssetError;
use crate::debt_token::DebtTokenError;
use crate::deposit::DepositError;
use crate::emergency_withdrawal::EmergencyWithdrawalError;
use crate::flash_loan::FlashLoanError;
use crate::interest_rate::InterestRateError;
use crate::liquidate::LiquidationError;
use crate::mev_protection::MevProtectionError;
use crate::rate_limiter::RateLimitError;
use crate::rebalancing::RebalancingError;
use crate::repay::RepayError;
use crate::reserve::ReserveError;
use crate::risk_management::RiskManagementError;
use crate::risk_params::RiskParamsError;
use crate::treasury::TreasuryError;
use crate::withdraw::WithdrawError;
use crate::errors::{lending_error_to_core, LendingError};
use stellarlend_errors::{lending_code_to_core, CoreError, IntoError, LendingCode};

fn core_eq<T: IntoError>(err: T, expected: CoreError) {
    assert_eq!(err.into_core(), expected, "category mismatch");
}

#[test]
fn every_lending_variant_has_a_core_mapping() {
    let samples = [
        LendingError::Unauthorized,
        LendingError::InvalidAmount,
        LendingError::InvalidAsset,
        LendingError::InvalidParameter,
        LendingError::InsufficientBalance,
        LendingError::InsufficientCollateral,
        LendingError::InsufficientCollateralRatio,
        LendingError::Overflow,
        LendingError::ProtocolPaused,
        LendingError::Reentrancy,
        LendingError::NotInitialized,
        LendingError::AlreadyInitialized,
        LendingError::DataNotFound,
        LendingError::DivisionByZero,
        LendingError::NoDebt,
        LendingError::AssetNotEnabled,
        LendingError::LimitExceeded,
        LendingError::InvalidState,
        LendingError::PriceUnavailable,
        LendingError::InsufficientLiquidity,
        LendingError::InvalidCallback,
        LendingError::CallbackFailed,
        LendingError::NotRepaid,
        LendingError::TreasuryNotSet,
        LendingError::InsufficientReserve,
        LendingError::InvalidFee,
        LendingError::GovernanceRequired,
        LendingError::GovernanceError,
        LendingError::CommitRequired,
        LendingError::CommitNotFound,
        LendingError::CommitNotReady,
        LendingError::CommitExpired,
        LendingError::FeeCapExceeded,
        LendingError::NotFound,
        LendingError::AlreadyExists,
    ];
    for variant in samples {
        let code = variant as u32;
        assert!(
            lending_code_to_core(code).is_some(),
            "LendingError code {} is unmapped",
            code
        );
        assert_eq!(variant.code(), code);
        assert_eq!(lending_error_to_core(variant), lending_code_to_core(code).unwrap());
    }
}

#[test]
fn admin_error_categories() {
    core_eq(AdminError::Unauthorized, CoreError::Unauthorized);
    core_eq(AdminError::InvalidParameter, CoreError::InvalidInput);
    core_eq(AdminError::AdminAlreadySet, CoreError::AlreadyInitialized);
}

#[test]
fn analytics_error_categories() {
    core_eq(AnalyticsError::NotInitialized, CoreError::NotInitialized);
    core_eq(AnalyticsError::InvalidParameter, CoreError::InvalidInput);
    core_eq(AnalyticsError::Overflow, CoreError::Overflow);
    core_eq(AnalyticsError::DataNotFound, CoreError::NotFound);
    core_eq(AnalyticsError::Unauthorized, CoreError::Unauthorized);
}

#[test]
fn borrow_error_categories() {
    core_eq(BorrowError::InvalidAmount, CoreError::InvalidInput);
    core_eq(BorrowError::InvalidAsset, CoreError::InvalidAsset);
    core_eq(BorrowError::InsufficientCollateral, CoreError::Insufficient);
    core_eq(
        BorrowError::BorrowPaused,
        CoreError::Paused,
    );
    core_eq(
        BorrowError::InsufficientCollateralRatio,
        CoreError::GuaranteeViolated,
    );
    core_eq(BorrowError::Overflow, CoreError::Overflow);
    core_eq(BorrowError::Reentrancy, CoreError::Reentrancy);
    core_eq(BorrowError::MaxBorrowExceeded, CoreError::LimitExceeded);
    core_eq(BorrowError::AssetNotEnabled, CoreError::InvalidState);
}

#[test]
fn deposit_error_categories() {
    core_eq(DepositError::InvalidAmount, CoreError::InvalidInput);
    core_eq(DepositError::InvalidAsset, CoreError::InvalidAsset);
    core_eq(DepositError::InsufficientBalance, CoreError::Insufficient);
    core_eq(DepositError::DepositPaused, CoreError::Paused);
    core_eq(DepositError::AssetNotEnabled, CoreError::InvalidState);
    core_eq(DepositError::Overflow, CoreError::Overflow);
    core_eq(DepositError::Reentrancy, CoreError::Reentrancy);
    core_eq(DepositError::Unauthorized, CoreError::Unauthorized);
}

#[test]
fn flash_loan_error_categories() {
    core_eq(FlashLoanError::InvalidAmount, CoreError::InvalidInput);
    core_eq(FlashLoanError::InvalidAsset, CoreError::InvalidAsset);
    core_eq(
        FlashLoanError::InsufficientLiquidity,
        CoreError::Insufficient,
    );
    core_eq(FlashLoanError::FlashLoanPaused, CoreError::Paused);
    core_eq(FlashLoanError::NotRepaid, CoreError::InvalidState);
    core_eq(
        FlashLoanError::InsufficientRepayment,
        CoreError::Insufficient,
    );
    core_eq(FlashLoanError::Overflow, CoreError::Overflow);
    core_eq(FlashLoanError::Reentrancy, CoreError::Reentrancy);
    core_eq(FlashLoanError::InvalidCallback, CoreError::InvalidInput);
    core_eq(FlashLoanError::CallbackFailed, CoreError::Internal);
    core_eq(FlashLoanError::ExceedsLiquidityCap, CoreError::LimitExceeded);
    core_eq(
        FlashLoanError::ExcessivePriceImpact,
        CoreError::LimitExceeded,
    );
    core_eq(FlashLoanError::ConcurrentLoan, CoreError::Reentrancy);
    core_eq(
        FlashLoanError::PriceManipulationDetected,
        CoreError::InvalidState,
    );
    core_eq(FlashLoanError::Expired, CoreError::InvalidState);
    core_eq(FlashLoanError::Unprofitable, CoreError::InvalidState);
    core_eq(FlashLoanError::EmptyLegs, CoreError::InvalidInput);
    core_eq(FlashLoanError::TooManyLegs, CoreError::LimitExceeded);
}

#[test]
fn liquidation_error_categories() {
    core_eq(LiquidationError::InvalidAmount, CoreError::InvalidInput);
    core_eq(LiquidationError::InvalidAsset, CoreError::InvalidAsset);
    core_eq(LiquidationError::NotLiquidatable, CoreError::InvalidState);
    core_eq(LiquidationError::LiquidationPaused, CoreError::Paused);
    core_eq(LiquidationError::ExceedsCloseFactor, CoreError::LimitExceeded);
    core_eq(LiquidationError::InsufficientBalance, CoreError::Insufficient);
    core_eq(LiquidationError::Overflow, CoreError::Overflow);
    core_eq(
        LiquidationError::InvalidCollateralAsset,
        CoreError::InvalidAsset,
    );
    core_eq(LiquidationError::InvalidDebtAsset, CoreError::InvalidAsset);
    core_eq(
        LiquidationError::PriceNotAvailable,
        CoreError::PriceUnavailable,
    );
    core_eq(
        LiquidationError::InsufficientLiquidation,
        CoreError::InvalidState,
    );
    core_eq(LiquidationError::Reentrancy, CoreError::Reentrancy);
    core_eq(
        LiquidationError::UnprofitableLiquidation,
        CoreError::InvalidState,
    );
}

#[test]
fn mev_protection_error_categories() {
    core_eq(MevProtectionError::InvalidConfig, CoreError::InvalidInput);
    core_eq(MevProtectionError::CommitNotFound, CoreError::NotFound);
    core_eq(MevProtectionError::CommitNotReady, CoreError::InvalidState);
    core_eq(MevProtectionError::CommitExpired, CoreError::InvalidState);
    core_eq(MevProtectionError::Unauthorized, CoreError::Unauthorized);
    core_eq(MevProtectionError::FeeCapExceeded, CoreError::LimitExceeded);
    core_eq(MevProtectionError::InvalidAmount, CoreError::InvalidInput);
    core_eq(MevProtectionError::InvalidOperation, CoreError::InvalidState);
    core_eq(MevProtectionError::SlippageExpired, CoreError::InvalidState);
    core_eq(MevProtectionError::SlippageExceeded, CoreError::LimitExceeded);
    core_eq(MevProtectionError::AuctionNotFound, CoreError::NotFound);
    core_eq(MevProtectionError::AuctionNotOpen, CoreError::InvalidState);
    core_eq(MevProtectionError::AuctionNotReady, CoreError::InvalidState);
    core_eq(MevProtectionError::BidNotFound, CoreError::NotFound);
    core_eq(MevProtectionError::BidTooLow, CoreError::LimitExceeded);
    core_eq(
        MevProtectionError::PrivateRouteRequired,
        CoreError::InvalidState,
    );
    core_eq(
        MevProtectionError::PrivateRouteNotFound,
        CoreError::NotFound,
    );
}

#[test]
fn rate_limit_error_categories() {
    core_eq(RateLimitError::RateLimited, CoreError::LimitExceeded);
    core_eq(RateLimitError::InvalidConfig, CoreError::InvalidInput);
    core_eq(RateLimitError::Unauthorized, CoreError::Unauthorized);
    core_eq(RateLimitError::Overflow, CoreError::Overflow);
}

#[test]
fn repay_error_categories() {
    core_eq(RepayError::InvalidAmount, CoreError::InvalidInput);
    core_eq(RepayError::InvalidAsset, CoreError::InvalidAsset);
    core_eq(RepayError::InsufficientBalance, CoreError::Insufficient);
    core_eq(RepayError::RepayPaused, CoreError::Paused);
    core_eq(RepayError::NoDebt, CoreError::InvalidState);
    core_eq(RepayError::Overflow, CoreError::Overflow);
    core_eq(RepayError::Reentrancy, CoreError::Reentrancy);
}

#[test]
fn reserve_error_categories() {
    core_eq(ReserveError::Unauthorized, CoreError::Unauthorized);
    core_eq(ReserveError::InvalidReserveFactor, CoreError::InvalidInput);
    core_eq(ReserveError::InsufficientReserve, CoreError::Insufficient);
    core_eq(ReserveError::InvalidAsset, CoreError::InvalidAsset);
    core_eq(ReserveError::InvalidTreasury, CoreError::InvalidInput);
    core_eq(ReserveError::InvalidAmount, CoreError::InvalidInput);
    core_eq(ReserveError::Overflow, CoreError::Overflow);
    core_eq(ReserveError::TreasuryNotSet, CoreError::NotInitialized);
}

#[test]
fn risk_management_error_categories() {
    core_eq(RiskManagementError::Unauthorized, CoreError::Unauthorized);
    core_eq(
        RiskManagementError::InvalidParameter,
        CoreError::InvalidInput,
    );
    core_eq(
        RiskManagementError::ParameterChangeTooLarge,
        CoreError::LimitExceeded,
    );
    core_eq(
        RiskManagementError::InsufficientCollateralRatio,
        CoreError::GuaranteeViolated,
    );
    core_eq(RiskManagementError::OperationPaused, CoreError::Paused);
    core_eq(RiskManagementError::EmergencyPaused, CoreError::Paused);
    core_eq(
        RiskManagementError::InvalidCollateralRatio,
        CoreError::InvalidInput,
    );
    core_eq(
        RiskManagementError::InvalidLiquidationThreshold,
        CoreError::InvalidInput,
    );
    core_eq(
        RiskManagementError::InvalidCloseFactor,
        CoreError::InvalidInput,
    );
    core_eq(
        RiskManagementError::InvalidLiquidationIncentive,
        CoreError::InvalidInput,
    );
    core_eq(RiskManagementError::Overflow, CoreError::Overflow);
    core_eq(
        RiskManagementError::GovernanceRequired,
        CoreError::InvalidState,
    );
    core_eq(
        RiskManagementError::AlreadyInitialized,
        CoreError::AlreadyInitialized,
    );
}

#[test]
fn risk_params_error_categories() {
    core_eq(RiskParamsError::Unauthorized, CoreError::Unauthorized);
    core_eq(RiskParamsError::InvalidParameter, CoreError::InvalidInput);
    core_eq(
        RiskParamsError::ParameterChangeTooLarge,
        CoreError::LimitExceeded,
    );
    core_eq(
        RiskParamsError::InvalidCollateralRatio,
        CoreError::InvalidInput,
    );
    core_eq(
        RiskParamsError::InvalidLiquidationThreshold,
        CoreError::InvalidInput,
    );
    core_eq(RiskParamsError::InvalidCloseFactor, CoreError::InvalidInput);
    core_eq(
        RiskParamsError::InvalidLiquidationIncentive,
        CoreError::InvalidInput,
    );
}

#[test]
fn treasury_error_categories() {
    core_eq(TreasuryError::Unauthorized, CoreError::Unauthorized);
    core_eq(TreasuryError::InvalidAmount, CoreError::InvalidInput);
    core_eq(TreasuryError::InsufficientReserve, CoreError::Insufficient);
    core_eq(TreasuryError::Overflow, CoreError::Overflow);
    core_eq(TreasuryError::TreasuryNotSet, CoreError::NotInitialized);
    core_eq(TreasuryError::InvalidFee, CoreError::InvalidInput);
}

#[test]
fn withdraw_error_categories() {
    core_eq(WithdrawError::InvalidAmount, CoreError::InvalidInput);
    core_eq(WithdrawError::InvalidAsset, CoreError::InvalidAsset);
    core_eq(
        WithdrawError::InsufficientCollateral,
        CoreError::Insufficient,
    );
    core_eq(WithdrawError::WithdrawPaused, CoreError::Paused);
    core_eq(
        WithdrawError::InsufficientCollateralRatio,
        CoreError::GuaranteeViolated,
    );
    core_eq(WithdrawError::Overflow, CoreError::Overflow);
    core_eq(WithdrawError::Reentrancy, CoreError::Reentrancy);
    core_eq(
        WithdrawError::Undercollateralized,
        CoreError::InvalidState,
    );
}

#[test]
fn rebalancing_error_categories() {
    core_eq(RebalancingError::Unauthorized, CoreError::Unauthorized);
    core_eq(RebalancingError::InvalidConfig, CoreError::InvalidInput);
    core_eq(RebalancingError::AlreadyHealthy, CoreError::InvalidState);
    core_eq(RebalancingError::GasCostTooHigh, CoreError::LimitExceeded);
    core_eq(RebalancingError::SlippageTooHigh, CoreError::LimitExceeded);
    core_eq(RebalancingError::SwapTooSmall, CoreError::InvalidInput);
    core_eq(RebalancingError::CooldownActive, CoreError::LimitExceeded);
    core_eq(
        RebalancingError::Undercollateralized,
        CoreError::GuaranteeViolated,
    );
    core_eq(RebalancingError::AmmFailed, CoreError::InvalidState);
    core_eq(
        RebalancingError::InsufficientLiquidity,
        CoreError::Insufficient,
    );
    core_eq(RebalancingError::Overflow, CoreError::Overflow);
}

#[test]
fn interest_rate_error_categories() {
    core_eq(InterestRateError::Unauthorized, CoreError::Unauthorized);
    core_eq(
        InterestRateError::InvalidParameter,
        CoreError::InvalidInput,
    );
    core_eq(
        InterestRateError::ParameterChangeTooLarge,
        CoreError::LimitExceeded,
    );
    core_eq(InterestRateError::Overflow, CoreError::Overflow);
    core_eq(InterestRateError::DivisionByZero, CoreError::DivisionByZero);
    core_eq(
        InterestRateError::AlreadyInitialized,
        CoreError::AlreadyInitialized,
    );
}

#[test]
fn emergency_withdrawal_error_categories() {
    core_eq(
        EmergencyWithdrawalError::NotActive,
        CoreError::InvalidState,
    );
    core_eq(
        EmergencyWithdrawalError::AlreadyActive,
        CoreError::AlreadyExists,
    );
    core_eq(
        EmergencyWithdrawalError::WindowNotOpen,
        CoreError::InvalidState,
    );
    core_eq(
        EmergencyWithdrawalError::NotAuthorized,
        CoreError::Unauthorized,
    );
    core_eq(
        EmergencyWithdrawalError::InsufficientBalance,
        CoreError::Insufficient,
    );
    core_eq(
        EmergencyWithdrawalError::ExceedsWithdrawalCap,
        CoreError::LimitExceeded,
    );
    core_eq(
        EmergencyWithdrawalError::InvalidParameter,
        CoreError::InvalidInput,
    );
    core_eq(
        EmergencyWithdrawalError::AlreadyWithdrawn,
        CoreError::AlreadyExists,
    );
}

#[test]
fn debt_token_error_categories() {
    core_eq(DebtTokenError::TokenNotFound, CoreError::NotFound);
    core_eq(DebtTokenError::Unauthorized, CoreError::Unauthorized);
    core_eq(DebtTokenError::TransferPaused, CoreError::Paused);
    core_eq(DebtTokenError::TransferBlocked, CoreError::Unauthorized);
    core_eq(
        DebtTokenError::LiquidationInProgress,
        CoreError::InvalidState,
    );
    core_eq(DebtTokenError::InvalidTokenId, CoreError::InvalidInput);
    core_eq(
        DebtTokenError::Undercollateralized,
        CoreError::GuaranteeViolated,
    );
    core_eq(DebtTokenError::Overflow, CoreError::Overflow);
    core_eq(DebtTokenError::ZeroAddress, CoreError::InvalidInput);
    core_eq(DebtTokenError::AlreadyTokenized, CoreError::AlreadyExists);
    core_eq(DebtTokenError::PositionNotFound, CoreError::NotFound);
    core_eq(DebtTokenError::NotListed, CoreError::NotFound);
    core_eq(DebtTokenError::AlreadyListed, CoreError::AlreadyExists);
    core_eq(DebtTokenError::NotSeller, CoreError::Unauthorized);
    core_eq(DebtTokenError::InvalidPrice, CoreError::InvalidInput);
}

#[test]
fn cross_asset_error_categories() {
    core_eq(
        CrossAssetError::AssetNotConfigured,
        CoreError::NotFound,
    );
    core_eq(CrossAssetError::AssetDisabled, CoreError::InvalidState);
    core_eq(
        CrossAssetError::InsufficientCollateral,
        CoreError::Insufficient,
    );
    core_eq(
        CrossAssetError::ExceedsBorrowCapacity,
        CoreError::GuaranteeViolated,
    );
    core_eq(
        CrossAssetError::UnhealthyPosition,
        CoreError::GuaranteeViolated,
    );
    core_eq(CrossAssetError::SupplyCapExceeded, CoreError::LimitExceeded);
    core_eq(CrossAssetError::BorrowCapExceeded, CoreError::LimitExceeded);
    core_eq(CrossAssetError::InvalidPrice, CoreError::PriceUnavailable);
    core_eq(CrossAssetError::PriceStale, CoreError::PriceUnavailable);
    core_eq(CrossAssetError::NotAuthorized, CoreError::Unauthorized);
    core_eq(CrossAssetError::InvalidCorrelation, CoreError::InvalidInput);
    core_eq(
        CrossAssetError::VolatilityUnavailable,
        CoreError::InvalidInput,
    );
    core_eq(CrossAssetError::Reentrancy, CoreError::Reentrancy);
}

/// The `?` operator chain (module error -> LendingError -> CoreError) must be
/// total. Any module error used in a fallible function must not panic when
/// normalized.
#[test]
fn no_module_error_panics_on_normalization() {
    let _: CoreError = AdminError::Unauthorized.into_core();
    let _: CoreError = BorrowError::Overflow.into_core();
    let _: CoreError = FlashLoanError::CallbackFailed.into_core();
    let _: CoreError = LiquidationError::Reentrancy.into_core();
    let _: CoreError = MevProtectionError::SlippageExceeded.into_core();
    let _: CoreError = RebalancingError::AmmFailed.into_core();
    let _: CoreError = ReserveError::TreasuryNotSet.into_core();
    let _: CoreError = RiskManagementError::GovernanceRequired.into_core();
    let _: CoreError = TreasuryError::InvalidFee.into_core();
    let _: CoreError = WithdrawError::Undercollateralized.into_core();
    let _: CoreError = CrossAssetError::Reentrancy.into_core();
    let _: CoreError = DebtTokenError::LiquidationInProgress.into_core();
    let _: CoreError = EmergencyWithdrawalError::AlreadyActive.into_core();
    let _: CoreError = AnalyticsError::DataNotFound.into_core();
    let _: CoreError = DepositError::Reentrancy.into_core();
    let _: CoreError = InterestRateError::DivisionByZero.into_core();
    let _: CoreError = RateLimitError::RateLimited.into_core();
    let _: CoreError = RepayError::NoDebt.into_core();
    let _: CoreError = RiskParamsError::ParameterChangeTooLarge.into_core();
}

/// Code-based normalization works through the public `LendingCode` trait.
#[test]
fn code_based_normalization_works() {
    assert_eq!(LendingError::Overflow.code(), 8);
    assert_eq!(LendingError::Overflow.into_core(), CoreError::Overflow);
    assert_eq!(
        LendingError::ProtocolPaused.into_core(),
        CoreError::Paused
    );
    assert_eq!(LendingError::Reentrancy.into_core(), CoreError::Reentrancy);
    assert_eq!(
        LendingError::InsufficientCollateralRatio.into_core(),
        CoreError::GuaranteeViolated
    );
    assert_eq!(
        LendingError::PriceUnavailable.into_core(),
        CoreError::PriceUnavailable
    );
}
