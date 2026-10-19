use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum MathError {
    Overflow = 1,
    DivisionByZero = 2,
    InvalidParameter = 3,
    NegativeValue = 4,
    ExceedsMax = 5,
    InsufficientCollateral = 6,
    InvalidHealthFactor = 7,
    RoundingError = 8,
}

// Unified error registry: protocol-wide global codes, messages and recovery
// suggestions for every variant (see `stellarlend_errors` and docs/ERROR_HANDLING.md).
stellarlend_errors::impl_contract_error! {
    MathError => stellarlend_errors::domains::SHARED_MATH;
    Overflow => Overflow, "Arithmetic overflow or underflow";
    DivisionByZero => DivisionByZero, "Division by zero";
    InvalidParameter => InvalidInput, "Parameter is invalid or out of range";
    NegativeValue => InvalidInput, "Negative value";
    ExceedsMax => LimitExceeded, "Exceeds max", ReduceAmount;
    InsufficientCollateral => Insufficient, "Collateral is insufficient for this position", AddCollateral;
    InvalidHealthFactor => InvalidInput, "Invalid health factor";
    RoundingError => Internal, "Rounding error", ReportBug;
}
