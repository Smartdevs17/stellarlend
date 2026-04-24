use soroban_sdk::{Address, Env, Vec};

use crate::errors::AccountingError;
use crate::storage::AccountingDataKey;
use crate::types::{
    Recognition, RecognitionMethod, RevenueAnalytics, RevenueRecognitionRule, RevenueSchedule,
    ScheduleEntry, SubscriptionState,
};

// ============================================================================
// Constants
// ============================================================================

const SECONDS_PER_DAY: u64 = 86400;
const BASIS_POINTS: i128 = 10_000;

// ============================================================================
// Initialization
// ============================================================================

pub fn initialize(env: &Env, admin: Address) -> Result<(), AccountingError> {
    if env
        .storage()
        .instance()
        .has(&AccountingDataKey::Initialized)
    {
        return Err(AccountingError::AlreadyInitialized);
    }

    admin.require_auth();

    env.storage()
        .instance()
        .set(&AccountingDataKey::Admin, &admin);
    env.storage()
        .instance()
        .set(&AccountingDataKey::Initialized, &true);
    env.storage()
        .instance()
        .set(&AccountingDataKey::NextSubscriptionId, &1u64);
    env.storage()
        .instance()
        .set(&AccountingDataKey::NextArrangementId, &1u64);

    Ok(())
}

// ============================================================================
// Authorization
// ============================================================================

fn require_admin(env: &Env, caller: &Address) -> Result<(), AccountingError> {
    caller.require_auth();

    let admin: Address = env
        .storage()
        .instance()
        .get(&AccountingDataKey::Admin)
        .ok_or(AccountingError::NotInitialized)?;

    if caller != &admin {
        return Err(AccountingError::Unauthorized);
    }

    Ok(())
}

fn require_merchant(_env: &Env, caller: &Address, merchant_id: &Address) -> Result<(), AccountingError> {
    caller.require_auth();

    if caller != merchant_id {
        return Err(AccountingError::Unauthorized);
    }

    Ok(())
}

// ============================================================================
// Revenue Recognition Configuration
// ============================================================================

pub fn configure_recognition_rule(
    env: &Env,
    caller: Address,
    subscription_id: u64,
    method: u32,
    recognition_period: u64,
) -> Result<(), AccountingError> {
    caller.require_auth();

    if recognition_period == 0 {
        return Err(AccountingError::InvalidPeriod);
    }

    let recognition_method = RecognitionMethod::from_u32(method)
        .ok_or(AccountingError::InvalidRecognitionMethod)?;

    // Get or create subscription state
    let state_key = AccountingDataKey::SubscriptionState(subscription_id);
    let state: SubscriptionState = env
        .storage()
        .persistent()
        .get(&state_key)
        .ok_or(AccountingError::SubscriptionNotFound)?;

    require_merchant(env, &caller, &state.merchant_id)?;

    let current_time = env.ledger().timestamp();
    let end_time = state.start_time + recognition_period;

    let rule = RevenueRecognitionRule {
        subscription_id,
        method: recognition_method,
        recognition_period,
        merchant_id: state.merchant_id.clone(),
        total_amount: state.total_amount,
        start_time: state.start_time,
        end_time,
        created_at: current_time,
    };

    env.storage()
        .persistent()
        .set(&AccountingDataKey::RecognitionRule(subscription_id), &rule);

    // Generate revenue schedule
    generate_revenue_schedule(env, &rule)?;

    Ok(())
}

// ============================================================================
// Revenue Recognition
// ============================================================================

pub fn recognize_revenue(
    env: &Env,
    caller: Address,
    subscription_id: u64,
) -> Result<Recognition, AccountingError> {
    caller.require_auth();

    let state_key = AccountingDataKey::SubscriptionState(subscription_id);
    let mut state: SubscriptionState = env
        .storage()
        .persistent()
        .get(&state_key)
        .ok_or(AccountingError::SubscriptionNotFound)?;

    require_merchant(env, &caller, &state.merchant_id)?;

    if !state.is_active {
        return Err(AccountingError::SubscriptionNotActive);
    }

    let rule: RevenueRecognitionRule = env
        .storage()
        .persistent()
        .get(&AccountingDataKey::RecognitionRule(subscription_id))
        .ok_or(AccountingError::SubscriptionNotFound)?;

    let current_time = env.ledger().timestamp();

    // Calculate recognition amount based on method
    let recognized_amount = match rule.method {
        RecognitionMethod::StraightLine => {
            calculate_straight_line_recognition(env, &rule, &state, current_time)?
        }
        RecognitionMethod::UsageBased => {
            calculate_usage_based_recognition(env, &rule, &state, current_time)?
        }
        RecognitionMethod::MilestoneBased => {
            calculate_milestone_based_recognition(env, &rule, &state, current_time)?
        }
    };

    if recognized_amount == 0 {
        return Err(AccountingError::NoRevenueToRecognize);
    }

    // Update state
    state.recognized_amount = state
        .recognized_amount
        .checked_add(recognized_amount)
        .ok_or(AccountingError::ArithmeticOverflow)?;
    state.deferred_amount = state
        .deferred_amount
        .checked_sub(recognized_amount)
        .ok_or(AccountingError::ArithmeticUnderflow)?;
    state.last_recognition_time = current_time;

    env.storage().persistent().set(&state_key, &state);

    // Update merchant totals
    update_merchant_revenue(env, &state.merchant_id, recognized_amount)?;

    let recognition = Recognition {
        subscription_id,
        merchant_id: state.merchant_id.clone(),
        recognized_amount,
        deferred_amount: state.deferred_amount,
        recognition_date: current_time,
        period_start: state.last_recognition_time,
        period_end: current_time,
    };

    Ok(recognition)
}

// ============================================================================
// Recognition Calculation Methods
// ============================================================================

fn calculate_straight_line_recognition(
    _env: &Env,
    rule: &RevenueRecognitionRule,
    state: &SubscriptionState,
    current_time: u64,
) -> Result<i128, AccountingError> {
    if current_time < rule.start_time {
        return Ok(0);
    }

    let total_period = rule.end_time - rule.start_time;
    if total_period == 0 {
        return Err(AccountingError::DivisionByZero);
    }

    let elapsed_time = if current_time >= rule.end_time {
        total_period
    } else {
        current_time - rule.start_time
    };

    let time_since_last = current_time - state.last_recognition_time;
    if time_since_last == 0 {
        return Ok(0);
    }

    // Calculate pro-rata amount for the elapsed period
    let total_recognizable = rule
        .total_amount
        .checked_mul(elapsed_time as i128)
        .ok_or(AccountingError::ArithmeticOverflow)?
        .checked_div(total_period as i128)
        .ok_or(AccountingError::DivisionByZero)?;

    let amount_to_recognize = total_recognizable
        .checked_sub(state.recognized_amount)
        .ok_or(AccountingError::ArithmeticUnderflow)?;

    Ok(amount_to_recognize.max(0))
}

fn calculate_usage_based_recognition(
    _env: &Env,
    _rule: &RevenueRecognitionRule,
    _state: &SubscriptionState,
    _current_time: u64,
) -> Result<i128, AccountingError> {
    // Usage-based recognition would require usage metrics
    // For now, return 0 as placeholder
    // In production, this would query usage data and calculate proportional revenue
    Ok(0)
}

fn calculate_milestone_based_recognition(
    _env: &Env,
    _rule: &RevenueRecognitionRule,
    _state: &SubscriptionState,
    _current_time: u64,
) -> Result<i128, AccountingError> {
    // Milestone-based recognition would require milestone completion data
    // For now, return 0 as placeholder
    // In production, this would check milestone completion and recognize accordingly
    Ok(0)
}

// ============================================================================
// Deferred Revenue
// ============================================================================

pub fn get_deferred_revenue(env: &Env, merchant_id: Address) -> Result<i128, AccountingError> {
    let deferred = env
        .storage()
        .persistent()
        .get(&AccountingDataKey::DeferredRevenue(merchant_id))
        .unwrap_or(0i128);

    Ok(deferred)
}

fn update_merchant_revenue(
    env: &Env,
    merchant_id: &Address,
    recognized_amount: i128,
) -> Result<(), AccountingError> {
    // Update recognized revenue
    let recognized_key = AccountingDataKey::RecognizedRevenue(merchant_id.clone());
    let current_recognized: i128 = env.storage().persistent().get(&recognized_key).unwrap_or(0);
    let new_recognized = current_recognized
        .checked_add(recognized_amount)
        .ok_or(AccountingError::ArithmeticOverflow)?;
    env.storage()
        .persistent()
        .set(&recognized_key, &new_recognized);

    // Update deferred revenue
    let deferred_key = AccountingDataKey::DeferredRevenue(merchant_id.clone());
    let current_deferred: i128 = env.storage().persistent().get(&deferred_key).unwrap_or(0);
    let new_deferred = current_deferred
        .checked_sub(recognized_amount)
        .ok_or(AccountingError::ArithmeticUnderflow)?;
    env.storage().persistent().set(&deferred_key, &new_deferred);

    Ok(())
}

// ============================================================================
// Revenue Schedule
// ============================================================================

fn generate_revenue_schedule(
    env: &Env,
    rule: &RevenueRecognitionRule,
) -> Result<(), AccountingError> {
    let mut entries = Vec::new(env);

    match rule.method {
        RecognitionMethod::StraightLine => {
            // Generate monthly schedule entries
            let total_period = rule.end_time - rule.start_time;
            let num_periods = (total_period / (30 * SECONDS_PER_DAY)).max(1);
            let period_duration = total_period / num_periods;
            let amount_per_period = rule
                .total_amount
                .checked_div(num_periods as i128)
                .ok_or(AccountingError::DivisionByZero)?;

            for i in 0..num_periods {
                let period_start = rule.start_time + (i * period_duration);
                let period_end = if i == num_periods - 1 {
                    rule.end_time
                } else {
                    period_start + period_duration
                };

                let entry = ScheduleEntry {
                    period_start,
                    period_end,
                    scheduled_amount: amount_per_period,
                    recognized_amount: 0,
                    is_recognized: false,
                };

                entries.push_back(entry);
            }
        }
        RecognitionMethod::UsageBased | RecognitionMethod::MilestoneBased => {
            // For usage and milestone-based, create a single entry
            let entry = ScheduleEntry {
                period_start: rule.start_time,
                period_end: rule.end_time,
                scheduled_amount: rule.total_amount,
                recognized_amount: 0,
                is_recognized: false,
            };
            entries.push_back(entry);
        }
    }

    let schedule = RevenueSchedule {
        subscription_id: rule.subscription_id,
        merchant_id: rule.merchant_id.clone(),
        total_amount: rule.total_amount,
        total_recognized: 0,
        total_deferred: rule.total_amount,
        entries,
        method: rule.method.clone(),
    };

    env.storage().persistent().set(
        &AccountingDataKey::RevenueSchedule(rule.subscription_id),
        &schedule,
    );

    Ok(())
}

pub fn get_revenue_schedule(
    env: &Env,
    subscription_id: u64,
) -> Result<RevenueSchedule, AccountingError> {
    env.storage()
        .persistent()
        .get(&AccountingDataKey::RevenueSchedule(subscription_id))
        .ok_or(AccountingError::SubscriptionNotFound)
}

// ============================================================================
// Revenue Analytics
// ============================================================================

pub fn get_revenue_analytics(
    env: &Env,
    merchant_id: Address,
    start_time: u64,
    end_time: u64,
) -> Result<RevenueAnalytics, AccountingError> {
    if start_time >= end_time {
        return Err(AccountingError::InvalidPeriod);
    }

    let recognized_revenue: i128 = env
        .storage()
        .persistent()
        .get(&AccountingDataKey::RecognizedRevenue(merchant_id.clone()))
        .unwrap_or(0);

    let deferred_revenue: i128 = env
        .storage()
        .persistent()
        .get(&AccountingDataKey::DeferredRevenue(merchant_id.clone()))
        .unwrap_or(0);

    let total_revenue = recognized_revenue
        .checked_add(deferred_revenue)
        .ok_or(AccountingError::ArithmeticOverflow)?;

    // Get subscription count for merchant
    let subscription_ids: Vec<u64> = env
        .storage()
        .persistent()
        .get(&AccountingDataKey::MerchantSubscriptions(merchant_id.clone()))
        .unwrap_or(Vec::new(env));

    let subscription_count = subscription_ids.len();
    let average_subscription_value = if subscription_count > 0 {
        total_revenue
            .checked_div(subscription_count as i128)
            .unwrap_or(0)
    } else {
        0
    };

    Ok(RevenueAnalytics {
        merchant_id,
        period_start: start_time,
        period_end: end_time,
        total_revenue,
        recognized_revenue,
        deferred_revenue,
        subscription_count,
        average_subscription_value,
    })
}

// ============================================================================
// Contract Modifications
// ============================================================================

pub fn handle_contract_modification(
    env: &Env,
    caller: Address,
    subscription_id: u64,
    new_amount: i128,
) -> Result<(), AccountingError> {
    caller.require_auth();

    if new_amount <= 0 {
        return Err(AccountingError::InvalidAmount);
    }

    let state_key = AccountingDataKey::SubscriptionState(subscription_id);
    let mut state: SubscriptionState = env
        .storage()
        .persistent()
        .get(&state_key)
        .ok_or(AccountingError::SubscriptionNotFound)?;

    require_merchant(env, &caller, &state.merchant_id)?;

    if !state.is_active {
        return Err(AccountingError::SubscriptionNotActive);
    }

    let old_amount = state.total_amount;
    let amount_diff = new_amount
        .checked_sub(old_amount)
        .ok_or(AccountingError::ArithmeticUnderflow)?;

    // Update subscription state
    state.total_amount = new_amount;
    state.deferred_amount = state
        .deferred_amount
        .checked_add(amount_diff)
        .ok_or(AccountingError::ArithmeticOverflow)?;

    env.storage().persistent().set(&state_key, &state);

    // Update merchant deferred revenue
    let deferred_key = AccountingDataKey::DeferredRevenue(state.merchant_id.clone());
    let current_deferred: i128 = env.storage().persistent().get(&deferred_key).unwrap_or(0);
    let new_deferred = current_deferred
        .checked_add(amount_diff)
        .ok_or(AccountingError::ArithmeticOverflow)?;
    env.storage().persistent().set(&deferred_key, &new_deferred);

    // Regenerate revenue schedule
    let rule: RevenueRecognitionRule = env
        .storage()
        .persistent()
        .get(&AccountingDataKey::RecognitionRule(subscription_id))
        .ok_or(AccountingError::SubscriptionNotFound)?;

    let updated_rule = RevenueRecognitionRule {
        total_amount: new_amount,
        ..rule
    };

    env.storage().persistent().set(
        &AccountingDataKey::RecognitionRule(subscription_id),
        &updated_rule,
    );

    generate_revenue_schedule(env, &updated_rule)?;

    Ok(())
}

// ============================================================================
// Cancellations
// ============================================================================

pub fn handle_cancellation(
    env: &Env,
    caller: Address,
    subscription_id: u64,
) -> Result<i128, AccountingError> {
    caller.require_auth();

    let state_key = AccountingDataKey::SubscriptionState(subscription_id);
    let mut state: SubscriptionState = env
        .storage()
        .persistent()
        .get(&state_key)
        .ok_or(AccountingError::SubscriptionNotFound)?;

    require_merchant(env, &caller, &state.merchant_id)?;

    if !state.is_active {
        return Err(AccountingError::SubscriptionNotActive);
    }

    if state.is_cancelled {
        return Err(AccountingError::SubscriptionAlreadyCancelled);
    }

    let current_time = env.ledger().timestamp();

    // Calculate pro-rata refund for unused period
    let rule: RevenueRecognitionRule = env
        .storage()
        .persistent()
        .get(&AccountingDataKey::RecognitionRule(subscription_id))
        .ok_or(AccountingError::SubscriptionNotFound)?;

    let total_period = rule.end_time - rule.start_time;
    let _elapsed_period = current_time - rule.start_time;
    let remaining_period = if current_time < rule.end_time {
        rule.end_time - current_time
    } else {
        0
    };

    let refund_amount = if total_period > 0 {
        rule.total_amount
            .checked_mul(remaining_period as i128)
            .ok_or(AccountingError::ArithmeticOverflow)?
            .checked_div(total_period as i128)
            .ok_or(AccountingError::DivisionByZero)?
    } else {
        0
    };

    // Update state
    state.is_active = false;
    state.is_cancelled = true;
    state.cancellation_time = Some(current_time);
    state.deferred_amount = state
        .deferred_amount
        .checked_sub(refund_amount)
        .ok_or(AccountingError::ArithmeticUnderflow)?;

    env.storage().persistent().set(&state_key, &state);

    // Update merchant deferred revenue
    let deferred_key = AccountingDataKey::DeferredRevenue(state.merchant_id.clone());
    let current_deferred: i128 = env.storage().persistent().get(&deferred_key).unwrap_or(0);
    let new_deferred = current_deferred
        .checked_sub(refund_amount)
        .ok_or(AccountingError::ArithmeticUnderflow)?;
    env.storage().persistent().set(&deferred_key, &new_deferred);

    Ok(refund_amount)
}

// ============================================================================
// Helper Functions
// ============================================================================

pub fn create_subscription(
    env: &Env,
    merchant_id: Address,
    total_amount: i128,
    start_time: u64,
) -> Result<u64, AccountingError> {
    merchant_id.require_auth();

    if total_amount <= 0 {
        return Err(AccountingError::InvalidAmount);
    }

    let subscription_id: u64 = env
        .storage()
        .instance()
        .get(&AccountingDataKey::NextSubscriptionId)
        .unwrap_or(1);

    let state = SubscriptionState {
        subscription_id,
        merchant_id: merchant_id.clone(),
        total_amount,
        recognized_amount: 0,
        deferred_amount: total_amount,
        start_time,
        end_time: start_time,
        is_active: true,
        is_cancelled: false,
        cancellation_time: None,
        last_recognition_time: start_time,
    };

    env.storage().persistent().set(
        &AccountingDataKey::SubscriptionState(subscription_id),
        &state,
    );

    // Update merchant subscriptions list
    let mut subscription_ids: Vec<u64> = env
        .storage()
        .persistent()
        .get(&AccountingDataKey::MerchantSubscriptions(merchant_id.clone()))
        .unwrap_or(Vec::new(env));
    subscription_ids.push_back(subscription_id);
    env.storage().persistent().set(
        &AccountingDataKey::MerchantSubscriptions(merchant_id.clone()),
        &subscription_ids,
    );

    // Update merchant deferred revenue
    let deferred_key = AccountingDataKey::DeferredRevenue(merchant_id);
    let current_deferred: i128 = env.storage().persistent().get(&deferred_key).unwrap_or(0);
    let new_deferred = current_deferred
        .checked_add(total_amount)
        .ok_or(AccountingError::ArithmeticOverflow)?;
    env.storage().persistent().set(&deferred_key, &new_deferred);

    // Increment next subscription ID
    env.storage()
        .instance()
        .set(&AccountingDataKey::NextSubscriptionId, &(subscription_id + 1));

    Ok(subscription_id)
}
