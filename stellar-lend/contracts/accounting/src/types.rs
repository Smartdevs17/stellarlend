use soroban_sdk::{contracttype, Address, Vec};

/// Revenue recognition method
#[derive(Clone, Debug, PartialEq, Copy)]
#[contracttype]
pub enum RecognitionMethod {
    /// Straight-line recognition over the period
    StraightLine = 0,
    /// Usage-based recognition
    UsageBased = 1,
    /// Milestone-based recognition
    MilestoneBased = 2,
}

impl RecognitionMethod {
    pub fn from_u32(value: u32) -> Option<Self> {
        match value {
            0 => Some(RecognitionMethod::StraightLine),
            1 => Some(RecognitionMethod::UsageBased),
            2 => Some(RecognitionMethod::MilestoneBased),
            _ => None,
        }
    }
}

/// Revenue recognition rule configuration
#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub struct RevenueRecognitionRule {
    pub subscription_id: u64,
    pub method: RecognitionMethod,
    pub recognition_period: u64, // in seconds
    pub merchant_id: Address,
    pub total_amount: i128,
    pub start_time: u64,
    pub end_time: u64,
    pub created_at: u64,
}

/// Revenue recognition record
#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub struct Recognition {
    pub subscription_id: u64,
    pub merchant_id: Address,
    pub recognized_amount: i128,
    pub deferred_amount: i128,
    pub recognition_date: u64,
    pub period_start: u64,
    pub period_end: u64,
}

/// Revenue schedule entry
#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub struct ScheduleEntry {
    pub period_start: u64,
    pub period_end: u64,
    pub scheduled_amount: i128,
    pub recognized_amount: i128,
    pub is_recognized: bool,
}

/// Complete revenue schedule for a subscription
#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub struct RevenueSchedule {
    pub subscription_id: u64,
    pub merchant_id: Address,
    pub total_amount: i128,
    pub total_recognized: i128,
    pub total_deferred: i128,
    pub entries: Vec<ScheduleEntry>,
    pub method: RecognitionMethod,
}

/// Revenue analytics for a period
#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub struct RevenueAnalytics {
    pub merchant_id: Address,
    pub period_start: u64,
    pub period_end: u64,
    pub total_revenue: i128,
    pub recognized_revenue: i128,
    pub deferred_revenue: i128,
    pub subscription_count: u32,
    pub average_subscription_value: i128,
}

/// Subscription state for revenue tracking
#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub struct SubscriptionState {
    pub subscription_id: u64,
    pub merchant_id: Address,
    pub total_amount: i128,
    pub recognized_amount: i128,
    pub deferred_amount: i128,
    pub start_time: u64,
    pub end_time: u64,
    pub is_active: bool,
    pub is_cancelled: bool,
    pub cancellation_time: Option<u64>,
    pub last_recognition_time: u64,
}

/// Multi-element arrangement for complex contracts
#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub struct MultiElementArrangement {
    pub arrangement_id: u64,
    pub merchant_id: Address,
    pub elements: Vec<ArrangementElement>,
    pub total_contract_value: i128,
    pub created_at: u64,
}

/// Individual element in a multi-element arrangement
#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub struct ArrangementElement {
    pub element_id: u32,
    pub description: soroban_sdk::String,
    pub standalone_value: i128,
    pub allocated_value: i128,
    pub subscription_id: u64,
    pub is_delivered: bool,
}

/// Contract modification record
#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub struct ContractModification {
    pub subscription_id: u64,
    pub modification_time: u64,
    pub old_amount: i128,
    pub new_amount: i128,
    pub old_end_time: u64,
    pub new_end_time: u64,
    pub reason: soroban_sdk::String,
}
