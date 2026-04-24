use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum AccountingError {
    /// Unauthorized access
    Unauthorized = 1,
    /// Already initialized
    AlreadyInitialized = 2,
    /// Not initialized
    NotInitialized = 3,
    /// Invalid subscription ID
    InvalidSubscriptionId = 4,
    /// Invalid recognition method
    InvalidRecognitionMethod = 5,
    /// Invalid period
    InvalidPeriod = 6,
    /// Invalid amount
    InvalidAmount = 7,
    /// Subscription not found
    SubscriptionNotFound = 8,
    /// Subscription already exists
    SubscriptionAlreadyExists = 9,
    /// Subscription not active
    SubscriptionNotActive = 10,
    /// Subscription already cancelled
    SubscriptionAlreadyCancelled = 11,
    /// No revenue to recognize
    NoRevenueToRecognize = 12,
    /// Recognition period not elapsed
    RecognitionPeriodNotElapsed = 13,
    /// Invalid merchant
    InvalidMerchant = 14,
    /// Arithmetic overflow
    ArithmeticOverflow = 15,
    /// Arithmetic underflow
    ArithmeticUnderflow = 16,
    /// Division by zero
    DivisionByZero = 17,
}
