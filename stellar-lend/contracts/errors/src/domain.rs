//! Error domains and globally-unique error codes.
//!
//! On-chain, every `#[contracterror]` enum only carries a *local* `u32` code, and
//! those codes overlap heavily: `lending::BorrowError::InsufficientCollateral` and
//! `lending::DepositError::InvalidAmount` are both `1`. The unified framework
//! assigns every error enum an [`ErrorDomain`] (`contract` + `module`) so that each
//! error also has a **global code** that is unique across the whole protocol:
//!
//! ```text
//!   global = contract * 1_000_000 + module * 10_000 + local
//!            └─ ContractId ─┘       └─ 0..=99 ─┘    └ 0..=9_999 ┘
//! ```
//!
//! Example: `lending` (contract `2`), `BorrowError` (module `1`),
//! `InsufficientCollateral` (local `1`) has global code `2_010_001`.
//!
//! Global codes are what off-chain clients, indexers and dashboards should key on.
//! The numbers in [`ContractId`] and every module index are part of the public
//! interface: **never renumber, only append**.

/// Maximum local code an error enum may use inside one domain.
pub const MAX_LOCAL_CODE: u32 = 9_999;
/// Maximum module index inside one contract.
pub const MAX_MODULE: u32 = 99;

const CONTRACT_STRIDE: u32 = 1_000_000;
const MODULE_STRIDE: u32 = 10_000;

/// Stable identifier of every contract (crate) that exposes errors.
///
/// Append new contracts at the end with the next free number.
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum ContractId {
    /// Errors defined by the framework itself ([`crate::CoreError`]).
    Framework = 0,
    /// `stellarlend-core` shared protocol codebook.
    Core = 1,
    Lending = 2,
    Amm = 3,
    Bridge = 4,
    DelegationRegistry = 5,
    Stablecoin = 6,
    InstitutionalWallet = 7,
    MigrationHub = 8,
    StealthAddress = 9,
    PrivacyPool = 10,
    ReputationSystem = 11,
    SwapRouter = 12,
    OracleHub = 13,
    SnsIntegration = 14,
    ReferralProgram = 15,
    InsuranceMarketplace = 16,
    TokenAdapter = 17,
    Token = 18,
    Compliance = 19,
    LeveragedYield = 20,
    PositionManager = 21,
    PrincipalToken = 22,
    YieldToken = 23,
    YieldSplitter = 24,
    VaultShare = 25,
    AutoCompoundVault = 26,
    RiskScoring = 27,
    PoolMigration = 28,
    YieldRouter = 29,
    DebtToken = 30,
    SharedMath = 31,
    StorageLayer = 32,
    EarningsReinvest = 33,
    TwapOracle = 34,
    Security = 35,
    DcaModule = 36,
    Common = 37,
    SharedSignatures = 38,
    /// Legacy monolithic lending contract (`hello-world`).
    LegacyLending = 39,
    PositionNft = 40,
    DynamicFeeEngine = 41,
    AuctionTypes = 42,
}

impl ContractId {
    /// Every registered contract, in numeric order.
    pub const ALL: &'static [ContractId] = &[
        ContractId::Framework,
        ContractId::Core,
        ContractId::Lending,
        ContractId::Amm,
        ContractId::Bridge,
        ContractId::DelegationRegistry,
        ContractId::Stablecoin,
        ContractId::InstitutionalWallet,
        ContractId::MigrationHub,
        ContractId::StealthAddress,
        ContractId::PrivacyPool,
        ContractId::ReputationSystem,
        ContractId::SwapRouter,
        ContractId::OracleHub,
        ContractId::SnsIntegration,
        ContractId::ReferralProgram,
        ContractId::InsuranceMarketplace,
        ContractId::TokenAdapter,
        ContractId::Token,
        ContractId::Compliance,
        ContractId::LeveragedYield,
        ContractId::PositionManager,
        ContractId::PrincipalToken,
        ContractId::YieldToken,
        ContractId::YieldSplitter,
        ContractId::VaultShare,
        ContractId::AutoCompoundVault,
        ContractId::RiskScoring,
        ContractId::PoolMigration,
        ContractId::YieldRouter,
        ContractId::DebtToken,
        ContractId::SharedMath,
        ContractId::StorageLayer,
        ContractId::EarningsReinvest,
        ContractId::TwapOracle,
        ContractId::Security,
        ContractId::DcaModule,
        ContractId::Common,
        ContractId::SharedSignatures,
        ContractId::LegacyLending,
        ContractId::PositionNft,
        ContractId::DynamicFeeEngine,
        ContractId::AuctionTypes,
    ];

    /// Resolves a raw contract number back to a [`ContractId`].
    pub fn from_u32(value: u32) -> Option<ContractId> {
        Self::ALL.iter().copied().find(|c| *c as u32 == value)
    }

    /// Short, stable, human-readable name (matches the crate directory).
    pub const fn name(self) -> &'static str {
        match self {
            ContractId::Framework => "errors",
            ContractId::Core => "core",
            ContractId::Lending => "lending",
            ContractId::Amm => "amm",
            ContractId::Bridge => "bridge",
            ContractId::DelegationRegistry => "delegation-registry",
            ContractId::Stablecoin => "stablecoin",
            ContractId::InstitutionalWallet => "institutional-wallet",
            ContractId::MigrationHub => "migration-hub",
            ContractId::StealthAddress => "stealth-address",
            ContractId::PrivacyPool => "privacy-pool",
            ContractId::ReputationSystem => "reputation-system",
            ContractId::SwapRouter => "swap-router",
            ContractId::OracleHub => "oracle-hub",
            ContractId::SnsIntegration => "sns-integration",
            ContractId::ReferralProgram => "referral-program",
            ContractId::InsuranceMarketplace => "insurance-marketplace",
            ContractId::TokenAdapter => "token-adapter",
            ContractId::Token => "token",
            ContractId::Compliance => "compliance",
            ContractId::LeveragedYield => "leveraged-yield",
            ContractId::PositionManager => "position-manager",
            ContractId::PrincipalToken => "principal-token",
            ContractId::YieldToken => "yield-token",
            ContractId::YieldSplitter => "yield-splitter",
            ContractId::VaultShare => "vault-share",
            ContractId::AutoCompoundVault => "auto-compound-vault",
            ContractId::RiskScoring => "risk-scoring",
            ContractId::PoolMigration => "pool-migration",
            ContractId::YieldRouter => "yield-router",
            ContractId::DebtToken => "debt-token",
            ContractId::SharedMath => "shared-math",
            ContractId::StorageLayer => "storage-layer",
            ContractId::EarningsReinvest => "earnings-reinvest",
            ContractId::TwapOracle => "twap-oracle",
            ContractId::Security => "security",
            ContractId::DcaModule => "dca-module",
            ContractId::Common => "common",
            ContractId::SharedSignatures => "shared-signatures",
            ContractId::LegacyLending => "hello-world",
            ContractId::PositionNft => "position-nft",
            ContractId::DynamicFeeEngine => "dynamic-fee-engine",
            ContractId::AuctionTypes => "auction-types",
        }
    }
}

/// The namespace an error enum lives in: which contract and which module inside it.
///
/// Module `0` is conventionally the contract's primary error enum.
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub struct ErrorDomain {
    pub contract: ContractId,
    pub module: u32,
}

impl ErrorDomain {
    /// Domain of a contract's primary error enum (module `0`).
    pub const fn primary(contract: ContractId) -> Self {
        Self::new(contract, 0)
    }

    /// Domain of a secondary error enum inside `contract`.
    pub const fn new(contract: ContractId, module: u32) -> Self {
        assert!(module <= MAX_MODULE, "error module index out of range");
        Self { contract, module }
    }

    /// Numeric prefix shared by every global code in this domain.
    pub const fn base(self) -> u32 {
        self.contract as u32 * CONTRACT_STRIDE + self.module * MODULE_STRIDE
    }

    /// Builds the globally-unique code for `local` inside this domain.
    ///
    /// Local codes above [`MAX_LOCAL_CODE`] cannot be represented; they are
    /// clamped so the result still decodes to this domain (the registry tests
    /// in every contract crate guarantee this never happens in practice).
    pub const fn global_code(self, local: u32) -> u32 {
        let local = if local > MAX_LOCAL_CODE {
            MAX_LOCAL_CODE
        } else {
            local
        };
        self.base() + local
    }
}

/// Splits a global code back into its domain and local code.
///
/// Returns `None` if the contract number is not registered.
pub fn decode_global_code(global: u32) -> Option<(ErrorDomain, u32)> {
    let contract = ContractId::from_u32(global / CONTRACT_STRIDE)?;
    let module = (global % CONTRACT_STRIDE) / MODULE_STRIDE;
    let local = global % MODULE_STRIDE;
    Some((ErrorDomain { contract, module }, local))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contract_ids_are_dense_and_unique() {
        for (i, c) in ContractId::ALL.iter().enumerate() {
            assert_eq!(*c as u32, i as u32, "ContractId::ALL out of order at {}", i);
            assert_eq!(ContractId::from_u32(i as u32), Some(*c));
        }
        assert_eq!(ContractId::from_u32(ContractId::ALL.len() as u32), None);
    }

    #[test]
    fn global_code_layout() {
        let d = ErrorDomain::new(ContractId::Lending, 1);
        assert_eq!(d.global_code(1), 2_010_001);
        assert_eq!(ErrorDomain::primary(ContractId::Amm).global_code(7), 3_000_007);
    }

    #[test]
    fn global_code_round_trips() {
        let d = ErrorDomain::new(ContractId::OracleHub, 3);
        let (decoded, local) = decode_global_code(d.global_code(42)).unwrap();
        assert_eq!(decoded, d);
        assert_eq!(local, 42);
    }

    #[test]
    fn unknown_contract_does_not_decode() {
        assert!(decode_global_code(4_000_000_000).is_none());
    }

    #[test]
    fn oversized_local_code_stays_in_domain() {
        let d = ErrorDomain::primary(ContractId::Core);
        let (decoded, _) = decode_global_code(d.global_code(50_000)).unwrap();
        assert_eq!(decoded, d);
    }
}
