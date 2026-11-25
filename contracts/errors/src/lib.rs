//! Canonical protocol error codes shared across StellarLend contracts, backend, and SDK.
//!
//! Each Soroban contract keeps its local `#[contracterror]` `u32` discriminants for ABI
//! stability, while [`ProtocolErrorCode`] and [`map_contract_error`] provide a single
//! collision-free canonical code space (`1000..=1999`) for cross-layer diagnostics,
//! observability, and SDK/backend error normalization.

#![no_std]

/// Canonical protocol-wide error codes aligned with `@stellarlend/error-codes` and `@stellarlend/sdk`.
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord, Hash)]
#[repr(u32)]
pub enum ProtocolErrorCode {
    Unauthorized = 1000,
    AlreadyInitialized = 1001,
    NotInitialized = 1002,
    InvalidAddress = 1003,
    InvalidAmount = 1004,
    InvalidConfiguration = 1005,
    InvalidState = 1006,
    InsufficientBalance = 1007,
    InsufficientCollateral = 1008,
    LimitExceeded = 1009,
    ReentrancyDetected = 1010,
    CrossContractFailure = 1011,
    SerializationFailure = 1012,
    Overflow = 1013,
    Underflow = 1014,
    UnsupportedOperation = 1015,
    NotFound = 1016,
    DeadlineExpired = 1017,
    DuplicateRequest = 1018,

    // Domain-specific unified codes (1020..=1030)
    OracleStale = 1020,
    OracleUnavailable = 1021,
    TimelockActive = 1022,
    PauseActive = 1023,
    SlippageExceeded = 1024,
    HealthFactorTooLow = 1025,
    CooldownNotMet = 1026,
    AttestationExpired = 1027,
    InvalidSignature = 1028,
    FlashLoanRepaymentFailed = 1029,
    KeeperSlashed = 1030,

    Unknown = 1999,
}

/// Contract module domains whose local `#[contracterror]` discriminants map to [`ProtocolErrorCode`].
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum ContractDomain {
    Core,
    Attestation,
    Events,
    FlashLoan,
    Health,
    Interest,
    Keeper,
    Migration,
    MockOracle,
    Oracle,
    Params,
    Pause,
    Pool,
    Queue,
    SafeMath,
    Timelock,
}

/// Static metadata describing a canonical [`ProtocolErrorCode`].
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct ErrorMetadata {
    pub code: ProtocolErrorCode,
    pub slug: &'static str,
    pub http_status: u16,
    pub retryable: bool,
    pub message: &'static str,
}

impl ProtocolErrorCode {
    /// Returns the numeric `u32` code.
    #[inline]
    pub const fn as_u32(self) -> u32 {
        self as u32
    }

    /// Converts a numeric `u32` into a [`ProtocolErrorCode`] if known.
    pub const fn from_u32(value: u32) -> Option<Self> {
        match value {
            1000 => Some(Self::Unauthorized),
            1001 => Some(Self::AlreadyInitialized),
            1002 => Some(Self::NotInitialized),
            1003 => Some(Self::InvalidAddress),
            1004 => Some(Self::InvalidAmount),
            1005 => Some(Self::InvalidConfiguration),
            1006 => Some(Self::InvalidState),
            1007 => Some(Self::InsufficientBalance),
            1008 => Some(Self::InsufficientCollateral),
            1009 => Some(Self::LimitExceeded),
            1010 => Some(Self::ReentrancyDetected),
            1011 => Some(Self::CrossContractFailure),
            1012 => Some(Self::SerializationFailure),
            1013 => Some(Self::Overflow),
            1014 => Some(Self::Underflow),
            1015 => Some(Self::UnsupportedOperation),
            1016 => Some(Self::NotFound),
            1017 => Some(Self::DeadlineExpired),
            1018 => Some(Self::DuplicateRequest),
            1020 => Some(Self::OracleStale),
            1021 => Some(Self::OracleUnavailable),
            1022 => Some(Self::TimelockActive),
            1023 => Some(Self::PauseActive),
            1024 => Some(Self::SlippageExceeded),
            1025 => Some(Self::HealthFactorTooLow),
            1026 => Some(Self::CooldownNotMet),
            1027 => Some(Self::AttestationExpired),
            1028 => Some(Self::InvalidSignature),
            1029 => Some(Self::FlashLoanRepaymentFailed),
            1030 => Some(Self::KeeperSlashed),
            1999 => Some(Self::Unknown),
            _ => None,
        }
    }

    /// Returns static metadata (slug, HTTP status, retryability, message) for this code.
    pub const fn metadata(self) -> ErrorMetadata {
        match self {
            Self::Unauthorized => ErrorMetadata {
                code: self,
                slug: "UNAUTHORIZED",
                http_status: 403,
                retryable: false,
                message: "Unauthorized action",
            },
            Self::AlreadyInitialized => ErrorMetadata {
                code: self,
                slug: "ALREADY_INITIALIZED",
                http_status: 409,
                retryable: false,
                message: "Contract has already been initialized",
            },
            Self::NotInitialized => ErrorMetadata {
                code: self,
                slug: "NOT_INITIALIZED",
                http_status: 409,
                retryable: false,
                message: "Contract state is not initialized",
            },
            Self::InvalidAddress => ErrorMetadata {
                code: self,
                slug: "INVALID_ADDRESS",
                http_status: 400,
                retryable: false,
                message: "Address is invalid or missing",
            },
            Self::InvalidAmount => ErrorMetadata {
                code: self,
                slug: "INVALID_AMOUNT",
                http_status: 400,
                retryable: false,
                message: "Amount is invalid",
            },
            Self::InvalidConfiguration => ErrorMetadata {
                code: self,
                slug: "INVALID_CONFIGURATION",
                http_status: 400,
                retryable: false,
                message: "Configuration value is invalid",
            },
            Self::InvalidState => ErrorMetadata {
                code: self,
                slug: "INVALID_STATE",
                http_status: 409,
                retryable: false,
                message: "Operation is not valid for the current state",
            },
            Self::InsufficientBalance => ErrorMetadata {
                code: self,
                slug: "INSUFFICIENT_BALANCE",
                http_status: 400,
                retryable: false,
                message: "Insufficient balance",
            },
            Self::InsufficientCollateral => ErrorMetadata {
                code: self,
                slug: "INSUFFICIENT_COLLATERAL",
                http_status: 400,
                retryable: false,
                message: "Insufficient collateral",
            },
            Self::LimitExceeded => ErrorMetadata {
                code: self,
                slug: "LIMIT_EXCEEDED",
                http_status: 429,
                retryable: false,
                message: "Requested value exceeds an allowed limit",
            },
            Self::ReentrancyDetected => ErrorMetadata {
                code: self,
                slug: "REENTRANCY_DETECTED",
                http_status: 409,
                retryable: true,
                message: "Reentrancy was detected and blocked",
            },
            Self::CrossContractFailure => ErrorMetadata {
                code: self,
                slug: "CROSS_CONTRACT_FAILURE",
                http_status: 502,
                retryable: true,
                message: "Cross-contract invocation failed",
            },
            Self::SerializationFailure => ErrorMetadata {
                code: self,
                slug: "SERIALIZATION_FAILURE",
                http_status: 500,
                retryable: false,
                message: "Failed to serialize or deserialize contract data",
            },
            Self::Overflow => ErrorMetadata {
                code: self,
                slug: "OVERFLOW",
                http_status: 400,
                retryable: false,
                message: "Arithmetic overflow occurred",
            },
            Self::Underflow => ErrorMetadata {
                code: self,
                slug: "UNDERFLOW",
                http_status: 400,
                retryable: false,
                message: "Arithmetic underflow occurred",
            },
            Self::UnsupportedOperation => ErrorMetadata {
                code: self,
                slug: "UNSUPPORTED_OPERATION",
                http_status: 400,
                retryable: false,
                message: "Operation is not supported",
            },
            Self::NotFound => ErrorMetadata {
                code: self,
                slug: "NOT_FOUND",
                http_status: 404,
                retryable: false,
                message: "Requested item was not found",
            },
            Self::DeadlineExpired => ErrorMetadata {
                code: self,
                slug: "DEADLINE_EXPIRED",
                http_status: 408,
                retryable: true,
                message: "Deadline has expired",
            },
            Self::DuplicateRequest => ErrorMetadata {
                code: self,
                slug: "DUPLICATE_REQUEST",
                http_status: 409,
                retryable: false,
                message: "Duplicate request was rejected",
            },
            Self::OracleStale => ErrorMetadata {
                code: self,
                slug: "ORACLE_STALE",
                http_status: 503,
                retryable: true,
                message: "Oracle price feed is stale",
            },
            Self::OracleUnavailable => ErrorMetadata {
                code: self,
                slug: "ORACLE_UNAVAILABLE",
                http_status: 503,
                retryable: true,
                message: "Oracle price feed is unavailable",
            },
            Self::TimelockActive => ErrorMetadata {
                code: self,
                slug: "TIMELOCK_ACTIVE",
                http_status: 409,
                retryable: true,
                message: "Timelock delay has not elapsed",
            },
            Self::PauseActive => ErrorMetadata {
                code: self,
                slug: "PAUSE_ACTIVE",
                http_status: 503,
                retryable: true,
                message: "Protocol or module is currently paused",
            },
            Self::SlippageExceeded => ErrorMetadata {
                code: self,
                slug: "SLIPPAGE_EXCEEDED",
                http_status: 409,
                retryable: true,
                message: "Execution slippage exceeded allowed tolerance",
            },
            Self::HealthFactorTooLow => ErrorMetadata {
                code: self,
                slug: "HEALTH_FACTOR_TOO_LOW",
                http_status: 400,
                retryable: false,
                message: "Operation would violate minimum health factor",
            },
            Self::CooldownNotMet => ErrorMetadata {
                code: self,
                slug: "COOLDOWN_NOT_MET",
                http_status: 409,
                retryable: true,
                message: "Cooldown period has not elapsed",
            },
            Self::AttestationExpired => ErrorMetadata {
                code: self,
                slug: "ATTESTATION_EXPIRED",
                http_status: 401,
                retryable: true,
                message: "Attestation has expired",
            },
            Self::InvalidSignature => ErrorMetadata {
                code: self,
                slug: "INVALID_SIGNATURE",
                http_status: 401,
                retryable: false,
                message: "Cryptographic signature or payload hash is invalid",
            },
            Self::FlashLoanRepaymentFailed => ErrorMetadata {
                code: self,
                slug: "FLASH_LOAN_REPAYMENT_FAILED",
                http_status: 400,
                retryable: false,
                message: "Flash loan repayment did not cover principal and fee",
            },
            Self::KeeperSlashed => ErrorMetadata {
                code: self,
                slug: "KEEPER_SLASHED",
                http_status: 409,
                retryable: false,
                message: "Keeper submission was already slashed",
            },
            Self::Unknown => ErrorMetadata {
                code: self,
                slug: "UNKNOWN",
                http_status: 500,
                retryable: false,
                message: "An unknown contract error occurred",
            },
        }
    }

    #[inline]
    pub const fn is_retryable(self) -> bool {
        self.metadata().retryable
    }

    #[inline]
    pub const fn http_status(self) -> u16 {
        self.metadata().http_status
    }
}

/// Maps a contract-local `#[contracterror]` `u32` discriminant in a given [`ContractDomain`]
/// to its canonical [`ProtocolErrorCode`].
pub const fn map_contract_error(domain: ContractDomain, local_code: u32) -> ProtocolErrorCode {
    match domain {
        ContractDomain::Core => match ProtocolErrorCode::from_u32(local_code) {
            Some(code) => code,
            None => ProtocolErrorCode::Unknown,
        },
        ContractDomain::Attestation => match local_code {
            1 => ProtocolErrorCode::Unauthorized,       // InvalidSigner
            2 => ProtocolErrorCode::AttestationExpired, // AttestationExpired
            3 => ProtocolErrorCode::InvalidSignature,   // InvalidPayloadHash
            _ => ProtocolErrorCode::Unknown,
        },
        ContractDomain::Events => match local_code {
            1 => ProtocolErrorCode::InvalidAmount, // HealthFactorOutOfRange
            _ => ProtocolErrorCode::Unknown,
        },
        ContractDomain::FlashLoan => match local_code {
            1 => ProtocolErrorCode::Unauthorized,         // NotAuthorized
            2 => ProtocolErrorCode::Unauthorized,         // CallbackNotSafelisted
            3 => ProtocolErrorCode::CrossContractFailure, // ExecutionFailed
            4 => ProtocolErrorCode::FlashLoanRepaymentFailed, // InsufficientRepayment
            5 => ProtocolErrorCode::InvalidConfiguration, // InvalidFee
            _ => ProtocolErrorCode::Unknown,
        },
        ContractDomain::Health => match local_code {
            1 => ProtocolErrorCode::HealthFactorTooLow,
            2 => ProtocolErrorCode::InvalidAmount,
            _ => ProtocolErrorCode::Unknown,
        },
        ContractDomain::Interest => match local_code {
            1 => ProtocolErrorCode::InvalidConfiguration, // InvalidBlockWindow
            2 => ProtocolErrorCode::DuplicateRequest,     // CheckpointAlreadyExists
            3 => ProtocolErrorCode::Overflow,             // ArithmeticOverflow
            _ => ProtocolErrorCode::Unknown,
        },
        ContractDomain::Keeper => match local_code {
            1 => ProtocolErrorCode::Unauthorized,         // NotAuthorized
            2 => ProtocolErrorCode::NotInitialized,       // NotInitialized
            3 => ProtocolErrorCode::AlreadyInitialized,   // AlreadyInitialized
            4 => ProtocolErrorCode::InvalidConfiguration, // InvalidSlashBps
            5 => ProtocolErrorCode::NotFound,             // SubmissionNotFound
            6 => ProtocolErrorCode::KeeperSlashed,        // AlreadySlashed
            7 => ProtocolErrorCode::InsufficientBalance,  // NoStake
            8 => ProtocolErrorCode::InvalidAmount,        // InvalidAmount
            9 => ProtocolErrorCode::InvalidConfiguration, // InvalidReason
            _ => ProtocolErrorCode::Unknown,
        },
        ContractDomain::Migration => match local_code {
            1 => ProtocolErrorCode::InvalidAddress, // InvalidSourcePool
            2 => ProtocolErrorCode::InvalidAddress, // InvalidDestinationPool
            3 => ProtocolErrorCode::InsufficientCollateral, // InsufficientSourceCollateral
            4 => ProtocolErrorCode::SlippageExceeded, // SlippageExceeded
            5 => ProtocolErrorCode::HealthFactorTooLow, // HealthFactorUnderflow
            _ => ProtocolErrorCode::Unknown,
        },
        ContractDomain::MockOracle => match local_code {
            1 => ProtocolErrorCode::OracleUnavailable, // InjectedFailure
            2 => ProtocolErrorCode::NotFound,          // UnknownAsset
            _ => ProtocolErrorCode::Unknown,
        },
        ContractDomain::Oracle => match local_code {
            1 => ProtocolErrorCode::OracleStale,       // PriceStale
            2 => ProtocolErrorCode::InvalidAmount,     // PriceZeroOrNegative
            3 => ProtocolErrorCode::InvalidState,      // TimestampInFuture
            4 => ProtocolErrorCode::OracleUnavailable, // OracleNotFound
            _ => ProtocolErrorCode::Unknown,
        },
        ContractDomain::Params => match local_code {
            1 => ProtocolErrorCode::InvalidConfiguration,
            2 => ProtocolErrorCode::LimitExceeded,
            _ => ProtocolErrorCode::Unknown,
        },
        ContractDomain::Pause => match local_code {
            1 => ProtocolErrorCode::Unauthorized,     // NotAdmin
            2 => ProtocolErrorCode::TimelockActive,   // TimelockActive
            3 => ProtocolErrorCode::InvalidSignature, // InsufficientSignatures
            4 => ProtocolErrorCode::NotFound,         // ProposalNotFound
            5 => ProtocolErrorCode::InvalidState,     // AlreadyExecuted
            _ => ProtocolErrorCode::Unknown,
        },
        ContractDomain::Pool => match local_code {
            1 => ProtocolErrorCode::InvalidConfiguration,
            2 => ProtocolErrorCode::InvalidState,
            3 => ProtocolErrorCode::PauseActive,
            _ => ProtocolErrorCode::Unknown,
        },
        ContractDomain::Queue => match local_code {
            1 => ProtocolErrorCode::CooldownNotMet, // CooldownNotMet
            2 => ProtocolErrorCode::LimitExceeded,  // LimitExceeded
            3 => ProtocolErrorCode::InvalidState,   // QueueEmpty
            4 => ProtocolErrorCode::InvalidAmount,  // InvalidAmount
            _ => ProtocolErrorCode::Unknown,
        },
        ContractDomain::SafeMath => match local_code {
            1 => ProtocolErrorCode::Overflow,
            2 => ProtocolErrorCode::Underflow,
            3 => ProtocolErrorCode::InvalidAmount,
            _ => ProtocolErrorCode::Unknown,
        },
        ContractDomain::Timelock => match local_code {
            1 => ProtocolErrorCode::AlreadyInitialized, // AlreadyInitialized
            2 => ProtocolErrorCode::NotInitialized,     // NotInitialized
            3 => ProtocolErrorCode::Unauthorized,       // NotAuthorized
            4 => ProtocolErrorCode::NotFound,           // OperationNotFound
            5 => ProtocolErrorCode::InvalidConfiguration, // EtaTooSoon
            6 => ProtocolErrorCode::InvalidConfiguration, // EtaTooLate
            7 => ProtocolErrorCode::TimelockActive,     // TimelockNotElapsed
            8 => ProtocolErrorCode::InvalidState,       // AlreadyExecuted
            9 => ProtocolErrorCode::InvalidState,       // AlreadyCancelled
            10 => ProtocolErrorCode::InvalidConfiguration, // InvalidDelay
            11 => ProtocolErrorCode::InvalidSignature,  // ArgsMismatch
            _ => ProtocolErrorCode::Unknown,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_CODES: [ProtocolErrorCode; 31] = [
        ProtocolErrorCode::Unauthorized,
        ProtocolErrorCode::AlreadyInitialized,
        ProtocolErrorCode::NotInitialized,
        ProtocolErrorCode::InvalidAddress,
        ProtocolErrorCode::InvalidAmount,
        ProtocolErrorCode::InvalidConfiguration,
        ProtocolErrorCode::InvalidState,
        ProtocolErrorCode::InsufficientBalance,
        ProtocolErrorCode::InsufficientCollateral,
        ProtocolErrorCode::LimitExceeded,
        ProtocolErrorCode::ReentrancyDetected,
        ProtocolErrorCode::CrossContractFailure,
        ProtocolErrorCode::SerializationFailure,
        ProtocolErrorCode::Overflow,
        ProtocolErrorCode::Underflow,
        ProtocolErrorCode::UnsupportedOperation,
        ProtocolErrorCode::NotFound,
        ProtocolErrorCode::DeadlineExpired,
        ProtocolErrorCode::DuplicateRequest,
        ProtocolErrorCode::OracleStale,
        ProtocolErrorCode::OracleUnavailable,
        ProtocolErrorCode::TimelockActive,
        ProtocolErrorCode::PauseActive,
        ProtocolErrorCode::SlippageExceeded,
        ProtocolErrorCode::HealthFactorTooLow,
        ProtocolErrorCode::CooldownNotMet,
        ProtocolErrorCode::AttestationExpired,
        ProtocolErrorCode::InvalidSignature,
        ProtocolErrorCode::FlashLoanRepaymentFailed,
        ProtocolErrorCode::KeeperSlashed,
        ProtocolErrorCode::Unknown,
    ];

    #[test]
    fn round_trip_and_uniqueness_of_all_codes() {
        for (i, &code) in ALL_CODES.iter().enumerate() {
            let num = code.as_u32();
            assert_eq!(ProtocolErrorCode::from_u32(num), Some(code));
            let meta = code.metadata();
            assert_eq!(meta.code, code);
            assert!(!meta.slug.is_empty());
            assert!(!meta.message.is_empty());
            assert!(meta.http_status >= 400);

            for &other in &ALL_CODES[i + 1..] {
                assert_ne!(num, other.as_u32(), "duplicate numeric error code");
                assert_ne!(meta.slug, other.metadata().slug, "duplicate slug");
            }
        }
        assert_eq!(ProtocolErrorCode::from_u32(9999), None);
    }

    #[test]
    fn maps_contract_domain_discriminants_accurately() {
        assert_eq!(
            map_contract_error(ContractDomain::Attestation, 2),
            ProtocolErrorCode::AttestationExpired
        );
        assert_eq!(
            map_contract_error(ContractDomain::FlashLoan, 4),
            ProtocolErrorCode::FlashLoanRepaymentFailed
        );
        assert_eq!(
            map_contract_error(ContractDomain::Interest, 3),
            ProtocolErrorCode::Overflow
        );
        assert_eq!(
            map_contract_error(ContractDomain::Keeper, 6),
            ProtocolErrorCode::KeeperSlashed
        );
        assert_eq!(
            map_contract_error(ContractDomain::Migration, 4),
            ProtocolErrorCode::SlippageExceeded
        );
        assert_eq!(
            map_contract_error(ContractDomain::MockOracle, 1),
            ProtocolErrorCode::OracleUnavailable
        );
        assert_eq!(
            map_contract_error(ContractDomain::Oracle, 1),
            ProtocolErrorCode::OracleStale
        );
        assert_eq!(
            map_contract_error(ContractDomain::Pause, 2),
            ProtocolErrorCode::TimelockActive
        );
        assert_eq!(
            map_contract_error(ContractDomain::Queue, 1),
            ProtocolErrorCode::CooldownNotMet
        );
        assert_eq!(
            map_contract_error(ContractDomain::Timelock, 7),
            ProtocolErrorCode::TimelockActive
        );
        assert_eq!(
            map_contract_error(ContractDomain::Timelock, 999),
            ProtocolErrorCode::Unknown
        );
    }

    #[test]
    fn core_domain_preserves_canonical_codes() {
        assert_eq!(
            map_contract_error(ContractDomain::Core, 1008),
            ProtocolErrorCode::InsufficientCollateral
        );
        assert_eq!(
            map_contract_error(ContractDomain::Core, 42),
            ProtocolErrorCode::Unknown
        );
    }
}
