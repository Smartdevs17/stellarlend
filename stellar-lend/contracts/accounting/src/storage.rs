use soroban_sdk::{contracttype, Address};

#[derive(Clone)]
#[contracttype]
pub enum AccountingDataKey {
    /// Admin address
    Admin,
    /// Initialized flag
    Initialized,
    /// Revenue recognition rule for a subscription
    RecognitionRule(u64),
    /// Subscription state
    SubscriptionState(u64),
    /// Deferred revenue for a merchant
    DeferredRevenue(Address),
    /// Total recognized revenue for a merchant
    RecognizedRevenue(Address),
    /// Revenue schedule for a subscription
    RevenueSchedule(u64),
    /// Multi-element arrangement
    MultiElementArrangement(u64),
    /// Contract modification history
    ContractModification(u64, u64), // (subscription_id, modification_index)
    /// Merchant subscription list
    MerchantSubscriptions(Address),
    /// Next subscription ID
    NextSubscriptionId,
    /// Next arrangement ID
    NextArrangementId,
}
