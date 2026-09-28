//! Central registry of every error domain in the protocol.
//!
//! Each registered `#[contracterror]` enum gets exactly one constant here, and
//! contracts refer to it from their [`crate::impl_contract_error!`] invocation.
//! Keeping the list in one place is what guarantees global codes never collide:
//! the test below rejects any two enums sharing a domain.
//!
//! To register a new enum, append a constant (next free module index for an
//! existing contract, or a new [`ContractId`]) and add it to [`ALL`].
//! **Never renumber an existing domain** — global codes are public interface.

use crate::domain::{ContractId, ErrorDomain};

/// `core::ProtocolError`
pub const CORE: ErrorDomain = ErrorDomain::new(ContractId::Core, 0);
/// `lending::BorrowError`
pub const LENDING_BORROW: ErrorDomain = ErrorDomain::new(ContractId::Lending, 1);
/// `lending::DepositError`
pub const LENDING_DEPOSIT: ErrorDomain = ErrorDomain::new(ContractId::Lending, 2);
/// `lending::WithdrawError`
pub const LENDING_WITHDRAW: ErrorDomain = ErrorDomain::new(ContractId::Lending, 3);
/// `lending::LiquidationError`
pub const LENDING_LIQUIDATION: ErrorDomain = ErrorDomain::new(ContractId::Lending, 4);
/// `lending::FlashLoanError`
pub const LENDING_FLASH_LOAN: ErrorDomain = ErrorDomain::new(ContractId::Lending, 5);
/// `lending::CrossAssetError`
pub const LENDING_CROSS_ASSET: ErrorDomain = ErrorDomain::new(ContractId::Lending, 6);
/// `lending::InterestRateError`
pub const LENDING_INTEREST_RATE: ErrorDomain = ErrorDomain::new(ContractId::Lending, 7);
/// `lending::InterestCacheError`
pub const LENDING_INTEREST_CACHE: ErrorDomain = ErrorDomain::new(ContractId::Lending, 8);
/// `lending::CalldataError`
pub const LENDING_CALLDATA: ErrorDomain = ErrorDomain::new(ContractId::Lending, 9);
/// `lending::CommitmentError`
pub const LENDING_COMMITMENTS: ErrorDomain = ErrorDomain::new(ContractId::Lending, 10);
/// `lending::DataStoreError`
pub const LENDING_DATA_STORE: ErrorDomain = ErrorDomain::new(ContractId::Lending, 11);
/// `lending::BatchViewError`
pub const LENDING_BATCH_VIEW: ErrorDomain = ErrorDomain::new(ContractId::Lending, 12);
/// `lending::LazyError`
pub const LENDING_LAZY: ErrorDomain = ErrorDomain::new(ContractId::Lending, 13);
/// `lending::MevGuardError`
pub const LENDING_MEV_GUARD: ErrorDomain = ErrorDomain::new(ContractId::Lending, 14);
/// `lending::RiskMonitorError`
pub const LENDING_RISK_MONITOR: ErrorDomain = ErrorDomain::new(ContractId::Lending, 15);
/// `lending::ReentrancyError`
pub const LENDING_REENTRANCY: ErrorDomain = ErrorDomain::new(ContractId::Lending, 16);
/// `lending::RateGuardError`
pub const LENDING_RATE_GUARD: ErrorDomain = ErrorDomain::new(ContractId::Lending, 17);
/// `lending::MetaTxError`
pub const LENDING_META_TX: ErrorDomain = ErrorDomain::new(ContractId::Lending, 18);
/// `lending::SimCacheError`
pub const LENDING_SIM_CACHE: ErrorDomain = ErrorDomain::new(ContractId::Lending, 19);
/// `lending::SandwichError`
pub const LENDING_SANDWICH: ErrorDomain = ErrorDomain::new(ContractId::Lending, 20);
/// `lending::PackError`
pub const LENDING_STORAGE_PACK: ErrorDomain = ErrorDomain::new(ContractId::Lending, 21);
/// `lending::AdapterError`
pub const LENDING_TOKEN_ADAPTER: ErrorDomain = ErrorDomain::new(ContractId::Lending, 22);
/// `lending::YieldCurveError`
pub const LENDING_YIELD_CURVE: ErrorDomain = ErrorDomain::new(ContractId::Lending, 23);
/// `lending::YieldError`
pub const LENDING_YIELD_FARMING: ErrorDomain = ErrorDomain::new(ContractId::Lending, 24);
/// `amm::AmmError`
pub const AMM: ErrorDomain = ErrorDomain::new(ContractId::Amm, 0);
/// `bridge::ContractError`
pub const BRIDGE: ErrorDomain = ErrorDomain::new(ContractId::Bridge, 0);
/// `bridge::LendingBridgeError`
pub const BRIDGE_LENDING: ErrorDomain = ErrorDomain::new(ContractId::Bridge, 1);
/// `delegation-registry::DelegationError`
pub const DELEGATION_REGISTRY: ErrorDomain = ErrorDomain::new(ContractId::DelegationRegistry, 0);
/// `stablecoin::StablecoinError`
pub const STABLECOIN: ErrorDomain = ErrorDomain::new(ContractId::Stablecoin, 0);
/// `institutional-wallet::WalletError`
pub const INSTITUTIONAL_WALLET: ErrorDomain = ErrorDomain::new(ContractId::InstitutionalWallet, 0);
/// `migration-hub::MigrationError`
pub const MIGRATION_HUB: ErrorDomain = ErrorDomain::new(ContractId::MigrationHub, 0);
/// `stealth-address::StealthError`
pub const STEALTH_ADDRESS: ErrorDomain = ErrorDomain::new(ContractId::StealthAddress, 0);
/// `privacy-pool::PrivacyPoolError`
pub const PRIVACY_POOL: ErrorDomain = ErrorDomain::new(ContractId::PrivacyPool, 0);
/// `reputation-system::ReputationError`
pub const REPUTATION_SYSTEM: ErrorDomain = ErrorDomain::new(ContractId::ReputationSystem, 0);
/// `swap-router::SwapRouterError`
pub const SWAP_ROUTER: ErrorDomain = ErrorDomain::new(ContractId::SwapRouter, 0);
/// `oracle-hub::OracleHubError`
pub const ORACLE_HUB: ErrorDomain = ErrorDomain::new(ContractId::OracleHub, 0);
/// `oracle-hub::UpgradeError`
pub const ORACLE_HUB_UPGRADE: ErrorDomain = ErrorDomain::new(ContractId::OracleHub, 1);
/// `sns-integration::SNSError`
pub const SNS_INTEGRATION: ErrorDomain = ErrorDomain::new(ContractId::SnsIntegration, 0);
/// `referral-program::ReferralError`
pub const REFERRAL_PROGRAM: ErrorDomain = ErrorDomain::new(ContractId::ReferralProgram, 0);
/// `insurance-marketplace::Error`
pub const INSURANCE_MARKETPLACE: ErrorDomain = ErrorDomain::new(ContractId::InsuranceMarketplace, 0);
/// `token-adapter::AdapterError`
pub const TOKEN_ADAPTER: ErrorDomain = ErrorDomain::new(ContractId::TokenAdapter, 0);
/// `token::TokenError`
pub const TOKEN: ErrorDomain = ErrorDomain::new(ContractId::Token, 0);
/// `compliance::ComplianceError`
pub const COMPLIANCE: ErrorDomain = ErrorDomain::new(ContractId::Compliance, 0);
/// `leveraged-yield::LeveragedYieldError`
pub const LEVERAGED_YIELD: ErrorDomain = ErrorDomain::new(ContractId::LeveragedYield, 0);
/// `position-manager::PositionError`
pub const POSITION_MANAGER: ErrorDomain = ErrorDomain::new(ContractId::PositionManager, 0);
/// `principal-token::PrincipalTokenError`
pub const PRINCIPAL_TOKEN: ErrorDomain = ErrorDomain::new(ContractId::PrincipalToken, 0);
/// `yield-token::YieldTokenError`
pub const YIELD_TOKEN: ErrorDomain = ErrorDomain::new(ContractId::YieldToken, 0);
/// `yield-splitter::YieldSplitterError`
pub const YIELD_SPLITTER: ErrorDomain = ErrorDomain::new(ContractId::YieldSplitter, 0);
/// `vault-share::VaultShareError`
pub const VAULT_SHARE: ErrorDomain = ErrorDomain::new(ContractId::VaultShare, 0);
/// `auto-compound-vault::VaultError`
pub const AUTO_COMPOUND_VAULT: ErrorDomain = ErrorDomain::new(ContractId::AutoCompoundVault, 0);
/// `risk-scoring::RiskScoringError`
pub const RISK_SCORING: ErrorDomain = ErrorDomain::new(ContractId::RiskScoring, 0);
/// `pool-migration::MigrationError`
pub const POOL_MIGRATION: ErrorDomain = ErrorDomain::new(ContractId::PoolMigration, 0);
/// `yield-router::RouterError`
pub const YIELD_ROUTER: ErrorDomain = ErrorDomain::new(ContractId::YieldRouter, 0);
/// `debt-token::DebtTokenError`
pub const DEBT_TOKEN: ErrorDomain = ErrorDomain::new(ContractId::DebtToken, 0);
/// `shared-math::MathError`
pub const SHARED_MATH: ErrorDomain = ErrorDomain::new(ContractId::SharedMath, 0);
/// `storage-layer::InitError`
pub const STORAGE_LAYER_INIT: ErrorDomain = ErrorDomain::new(ContractId::StorageLayer, 0);
/// `storage-layer::MigrationError`
pub const STORAGE_LAYER_MIGRATION: ErrorDomain = ErrorDomain::new(ContractId::StorageLayer, 1);
/// `earnings-reinvest::ReinvestError`
pub const EARNINGS_REINVEST: ErrorDomain = ErrorDomain::new(ContractId::EarningsReinvest, 0);
/// `twap-oracle::TwapOracleError`
pub const TWAP_ORACLE: ErrorDomain = ErrorDomain::new(ContractId::TwapOracle, 0);
/// `security::ReentrancyError`
pub const SECURITY_REENTRANCY: ErrorDomain = ErrorDomain::new(ContractId::Security, 0);
/// `dca-module::DcaError`
pub const DCA_MODULE: ErrorDomain = ErrorDomain::new(ContractId::DcaModule, 0);
/// `common::UpgradeError`
pub const COMMON_UPGRADE: ErrorDomain = ErrorDomain::new(ContractId::Common, 0);
/// `common::CacheError`
pub const COMMON_CACHE: ErrorDomain = ErrorDomain::new(ContractId::Common, 1);
/// `common::MessageBusError`
pub const COMMON_MESSAGE_BUS: ErrorDomain = ErrorDomain::new(ContractId::Common, 2);
/// `shared-signatures::SignatureError`
pub const SHARED_SIGNATURES: ErrorDomain = ErrorDomain::new(ContractId::SharedSignatures, 0);

/// Every registered domain (used to prove there are no collisions).
pub const ALL: &[ErrorDomain] = &[
    CORE,
    LENDING_BORROW,
    LENDING_DEPOSIT,
    LENDING_WITHDRAW,
    LENDING_LIQUIDATION,
    LENDING_FLASH_LOAN,
    LENDING_CROSS_ASSET,
    LENDING_INTEREST_RATE,
    LENDING_INTEREST_CACHE,
    LENDING_CALLDATA,
    LENDING_COMMITMENTS,
    LENDING_DATA_STORE,
    LENDING_BATCH_VIEW,
    LENDING_LAZY,
    LENDING_MEV_GUARD,
    LENDING_RISK_MONITOR,
    LENDING_REENTRANCY,
    LENDING_RATE_GUARD,
    LENDING_META_TX,
    LENDING_SIM_CACHE,
    LENDING_SANDWICH,
    LENDING_STORAGE_PACK,
    LENDING_TOKEN_ADAPTER,
    LENDING_YIELD_CURVE,
    LENDING_YIELD_FARMING,
    AMM,
    BRIDGE,
    BRIDGE_LENDING,
    DELEGATION_REGISTRY,
    STABLECOIN,
    INSTITUTIONAL_WALLET,
    MIGRATION_HUB,
    STEALTH_ADDRESS,
    PRIVACY_POOL,
    REPUTATION_SYSTEM,
    SWAP_ROUTER,
    ORACLE_HUB,
    ORACLE_HUB_UPGRADE,
    SNS_INTEGRATION,
    REFERRAL_PROGRAM,
    INSURANCE_MARKETPLACE,
    TOKEN_ADAPTER,
    TOKEN,
    COMPLIANCE,
    LEVERAGED_YIELD,
    POSITION_MANAGER,
    PRINCIPAL_TOKEN,
    YIELD_TOKEN,
    YIELD_SPLITTER,
    VAULT_SHARE,
    AUTO_COMPOUND_VAULT,
    RISK_SCORING,
    POOL_MIGRATION,
    YIELD_ROUTER,
    DEBT_TOKEN,
    SHARED_MATH,
    STORAGE_LAYER_INIT,
    STORAGE_LAYER_MIGRATION,
    EARNINGS_REINVEST,
    TWAP_ORACLE,
    SECURITY_REENTRANCY,
    DCA_MODULE,
    COMMON_UPGRADE,
    COMMON_CACHE,
    COMMON_MESSAGE_BUS,
    SHARED_SIGNATURES,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domains_are_unique() {
        for (i, a) in ALL.iter().enumerate() {
            for b in ALL.iter().skip(i + 1) {
                assert_ne!(a, b, "two error enums share domain {:?}", a);
            }
        }
    }

    #[test]
    fn no_domain_uses_the_framework_namespace() {
        assert!(ALL.iter().all(|d| d.contract != ContractId::Framework));
    }
}
