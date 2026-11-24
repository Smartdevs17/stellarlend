//! # Fee-Switch Mechanism with Revenue-Share Distribution
//!
//! A governance-controlled fee switch that can be toggled on/off. When enabled,
//! a configurable fee is charged on protocol revenue and distributed to
//! designated revenue-share recipients according to their configured shares.
//!
//! ## Design
//! - Admin controls the fee switch (on/off) and the fee rate (in basis points).
//! - Revenue-share recipients are registered with a share percentage.
//! - When the fee switch is on, `distribute_revenue` splits an incoming amount
//!   among recipients pro-rata and returns the net amount to the caller.
//! - Shares must sum to <= 10000 (100%).
//! - Every state change emits an event for observability.

use soroban_sdk::{contracterror, contracttype, Address, Env, IntoVal, Symbol, Vec};

use crate::admin;

// ── Errors ──────────────────────────────────────────────────────────────────

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum FeeSwitchError {
    Unauthorized = 1,
    InvalidFeeRate = 2,
    InvalidShare = 3,
    SharesExceed100 = 4,
    RecipientNotFound = 5,
    RecipientAlreadyExists = 6,
    NoRecipients = 7,
    Overflow = 8,
    FeeSwitchOff = 9,
    InvalidAmount = 10,
}

// ── Types ───────────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone)]
pub enum FeeSwitchDataKey {
    /// Whether the fee switch is enabled.
    SwitchEnabled,
    /// Fee rate in basis points (e.g., 500 = 5%).
    FeeRateBps,
    /// Total accumulated fees (for observability).
    AccumulatedFees,
    /// Revenue-share recipient.
    Recipient(Address),
    /// All registered recipients.
    RecipientsList,
}

/// A revenue-share recipient with their share in basis points.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RevenueShareRecipient {
    pub address: Address,
    pub share_bps: i128,
}

/// Result of a revenue distribution.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DistributionResult {
    pub total_amount: i128,
    pub fee_amount: i128,
    pub net_amount: i128,
    pub recipient_count: u32,
}

// ── Admin: Fee switch control ───────────────────────────────────────────────

/// Enable or disable the fee switch.
pub fn set_fee_switch(env: &Env, admin: Address, enabled: bool) -> Result<(), FeeSwitchError> {
    admin::require_admin(env, &admin).map_err(|_| FeeSwitchError::Unauthorized)?;
    env.storage()
        .persistent()
        .set(&FeeSwitchDataKey::SwitchEnabled, &enabled);
    env.events().publish(
        (Symbol::new(env, "fee_switch_toggled"), admin),
        enabled,
    );
    Ok(())
}

/// Set the fee rate in basis points (e.g., 500 = 5%).
pub fn set_fee_rate(env: &Env, admin: Address, fee_rate_bps: i128) -> Result<(), FeeSwitchError> {
    admin::require_admin(env, &admin).map_err(|_| FeeSwitchError::Unauthorized)?;
    if fee_rate_bps < 0 || fee_rate_bps > 10000 {
        return Err(FeeSwitchError::InvalidFeeRate);
    }
    env.storage()
        .persistent()
        .set(&FeeSwitchDataKey::FeeRateBps, &fee_rate_bps);
    env.events().publish(
        (Symbol::new(env, "fee_rate_set"), admin),
        fee_rate_bps,
    );
    Ok(())
}

/// Check whether the fee switch is enabled.
pub fn is_fee_switch_on(env: &Env) -> bool {
    env.storage()
        .persistent()
        .get::<_, bool>(&FeeSwitchDataKey::SwitchEnabled)
        .unwrap_or(false)
}

/// Get the current fee rate in basis points.
pub fn get_fee_rate(env: &Env) -> i128 {
    env.storage()
        .persistent()
        .get::<_, i128>(&FeeSwitchDataKey::FeeRateBps)
        .unwrap_or(0)
}

/// Get total accumulated fees.
pub fn get_accumulated_fees(env: &Env) -> i128 {
    env.storage()
        .persistent()
        .get::<_, i128>(&FeeSwitchDataKey::AccumulatedFees)
        .unwrap_or(0)
}

// ── Admin: Revenue-share recipient management ───────────────────────────────

/// Register a revenue-share recipient with a share in basis points.
///
/// All recipients' shares combined must not exceed 10000 (100%).
pub fn add_recipient(
    env: &Env,
    admin: Address,
    recipient: Address,
    share_bps: i128,
) -> Result<(), FeeSwitchError> {
    admin::require_admin(env, &admin).map_err(|_| FeeSwitchError::Unauthorized)?;
    if share_bps <= 0 || share_bps > 10000 {
        return Err(FeeSwitchError::InvalidShare);
    }
    let key = FeeSwitchDataKey::Recipient(recipient.clone());
    if env.storage().persistent().get::<_, i128>(&key).is_some() {
        return Err(FeeSwitchError::RecipientAlreadyExists);
    }

    // Validate total shares.
    let mut recipients: Vec<Address> = env
        .storage()
        .persistent()
        .get(&FeeSwitchDataKey::RecipientsList)
        .unwrap_or(Vec::new(env));
    let mut total: i128 = 0;
    for addr in recipients.iter() {
        let share: i128 = env
            .storage()
            .persistent()
            .get(&FeeSwitchDataKey::Recipient(addr.clone()))
            .unwrap_or(0);
        total = total.checked_add(share).ok_or(FeeSwitchError::Overflow)?;
    }
    total = total.checked_add(share_bps).ok_or(FeeSwitchError::Overflow)?;
    if total > 10000 {
        return Err(FeeSwitchError::SharesExceed100);
    }

    env.storage().persistent().set(&key, &share_bps);
    recipients.push_back(recipient.clone());
    env.storage()
        .persistent()
        .set(&FeeSwitchDataKey::RecipientsList, &recipients);

    env.events().publish(
        (Symbol::new(env, "revenue_recipient_added"), admin, recipient),
        share_bps,
    );
    Ok(())
}

/// Remove a revenue-share recipient.
pub fn remove_recipient(
    env: &Env,
    admin: Address,
    recipient: Address,
) -> Result<(), FeeSwitchError> {
    admin::require_admin(env, &admin).map_err(|_| FeeSwitchError::Unauthorized)?;
    let key = FeeSwitchDataKey::Recipient(recipient.clone());
    if env.storage().persistent().get::<_, i128>(&key).is_none() {
        return Err(FeeSwitchError::RecipientNotFound);
    }
    env.storage().persistent().remove(&key);

    let recipients: Vec<Address> = env
        .storage()
        .persistent()
        .get(&FeeSwitchDataKey::RecipientsList)
        .unwrap_or(Vec::new(env));
    let mut updated = Vec::new(env);
    for addr in recipients.iter() {
        if addr != recipient {
            updated.push_back(addr);
        }
    }
    env.storage()
        .persistent()
        .set(&FeeSwitchDataKey::RecipientsList, &updated);

    env.events().publish(
        (Symbol::new(env, "revenue_recipient_removed"), admin, recipient),
        (),
    );
    Ok(())
}

/// List all registered revenue-share recipients.
pub fn list_recipients(env: &Env) -> Vec<RevenueShareRecipient> {
    let addresses: Vec<Address> = env
        .storage()
        .persistent()
        .get(&FeeSwitchDataKey::RecipientsList)
        .unwrap_or(Vec::new(env));
    let mut result = Vec::new(env);
    for addr in addresses.iter() {
        let share: i128 = env
            .storage()
            .persistent()
            .get(&FeeSwitchDataKey::Recipient(addr.clone()))
            .unwrap_or(0);
        result.push_back(RevenueShareRecipient {
            address: addr,
            share_bps: share,
        });
    }
    result
}

// ── Revenue distribution ────────────────────────────────────────────────────

/// Distribute revenue through the fee switch.
///
/// When the fee switch is on, `amount` is split:
/// - `fee_amount` = amount * fee_rate_bps / 10000 (accumulated)
/// - `net_amount` = amount - fee_amount (returned to caller)
///
/// The fee is then distributed to recipients pro-rata by their share.
/// Returns a `DistributionResult` with the breakdown.
///
/// When the fee switch is off, the full amount is returned as net with zero fee.
pub fn distribute_revenue(
    env: &Env,
    amount: i128,
) -> Result<DistributionResult, FeeSwitchError> {
    if amount < 0 {
        return Err(FeeSwitchError::InvalidAmount);
    }
    if !is_fee_switch_on(env) {
        return Ok(DistributionResult {
            total_amount: amount,
            fee_amount: 0,
            net_amount: amount,
            recipient_count: 0,
        });
    }

    let fee_rate = get_fee_rate(env);
    let fee_amount = amount
        .checked_mul(fee_rate)
        .and_then(|v| v.checked_div(10000))
        .ok_or(FeeSwitchError::Overflow)?;
    let net_amount = amount.checked_sub(fee_amount).ok_or(FeeSwitchError::Overflow)?;

    // Accumulate fees.
    let accumulated = get_accumulated_fees(env);
    let new_accumulated = accumulated
        .checked_add(fee_amount)
        .ok_or(FeeSwitchError::Overflow)?;
    env.storage()
        .persistent()
        .set(&FeeSwitchDataKey::AccumulatedFees, &new_accumulated);

    // Distribute fee to recipients.
    let recipients = list_recipients(env);
    if fee_amount > 0 && recipients.len() > 0 {
        let mut distributed: i128 = 0;
        for recipient in recipients.iter() {
            let share_amount = fee_amount
                .checked_mul(recipient.share_bps)
                .and_then(|v| v.checked_div(10000))
                .ok_or(FeeSwitchError::Overflow)?;
            distributed = distributed
                .checked_add(share_amount)
                .ok_or(FeeSwitchError::Overflow)?;
            env.events().publish(
                (
                    Symbol::new(env, "revenue_distributed"),
                    recipient.address.clone(),
                ),
                share_amount,
            );
        }
        // Any remainder from rounding goes to the first recipient.
        let remainder = fee_amount.checked_sub(distributed).unwrap_or(0);
        if remainder > 0 {
            if let Some(first) = recipients.first() {
                env.events().publish(
                    (Symbol::new(env, "revenue_distributed"), first.address.clone()),
                    remainder,
                );
            }
        }
    }

    env.events().publish(
        (Symbol::new(env, "fee_switch_distributed"),),
        (amount, fee_amount, net_amount),
    );

    Ok(DistributionResult {
        total_amount: amount,
        fee_amount,
        net_amount,
        recipient_count: recipients.len(),
    })
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::{contract, contractimpl};

    #[contract]
    struct FeeSwitchTestContract;

    #[contractimpl]
    impl FeeSwitchTestContract {}

    fn setup() -> (Env, Address, Address) {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 100);
        let admin = Address::generate(&env);
        let contract_id = env.register_contract(None, FeeSwitchTestContract);
        env.as_contract(&contract_id, || {
            crate::admin::set_admin(&env, admin.clone(), None).unwrap();
        });
        (env, admin, contract_id)
    }

    #[test]
    fn fee_switch_off_returns_full_amount() {
        let (env, _admin, contract_id) = setup();
        env.as_contract(&contract_id, || {
            let result = distribute_revenue(&env, 1000).unwrap();
            assert_eq!(result.fee_amount, 0);
            assert_eq!(result.net_amount, 1000);
            assert_eq!(result.recipient_count, 0);
        });
    }

    #[test]
    fn fee_switch_on_distributes_to_recipients() {
        let (env, admin, contract_id) = setup();
        env.as_contract(&contract_id, || {
            set_fee_switch(&env, admin.clone(), true).unwrap();
            set_fee_rate(&env, admin.clone(), 500).unwrap(); // 5%

            let r1 = Address::generate(&env);
            let r2 = Address::generate(&env);
            add_recipient(&env, admin.clone(), r1.clone(), 6000).unwrap(); // 60%
            add_recipient(&env, admin.clone(), r2.clone(), 4000).unwrap(); // 40%

            let result = distribute_revenue(&env, 10000).unwrap();
            assert_eq!(result.total_amount, 10000);
            assert_eq!(result.fee_amount, 500); // 5% of 10000
            assert_eq!(result.net_amount, 9500);
            assert_eq!(result.recipient_count, 2);
            assert_eq!(get_accumulated_fees(&env), 500);
        });
    }

    #[test]
    fn shares_cannot_exceed_100_percent() {
        let (env, admin, contract_id) = setup();
        env.as_contract(&contract_id, || {
            let r1 = Address::generate(&env);
            let r2 = Address::generate(&env);
            add_recipient(&env, admin.clone(), r1.clone(), 6000).unwrap();
            assert_eq!(
                add_recipient(&env, admin.clone(), r2.clone(), 5000),
                Err(FeeSwitchError::SharesExceed100)
            );
        });
    }

    #[test]
    fn duplicate_recipient_rejected() {
        let (env, admin, contract_id) = setup();
        env.as_contract(&contract_id, || {
            let r1 = Address::generate(&env);
            add_recipient(&env, admin.clone(), r1.clone(), 5000).unwrap();
            assert_eq!(
                add_recipient(&env, admin, r1, 3000),
                Err(FeeSwitchError::RecipientAlreadyExists)
            );
        });
    }

    #[test]
    fn remove_recipient_frees_share() {
        let (env, admin, contract_id) = setup();
        env.as_contract(&contract_id, || {
            let r1 = Address::generate(&env);
            let r2 = Address::generate(&env);
            add_recipient(&env, admin.clone(), r1.clone(), 6000).unwrap();
            remove_recipient(&env, admin.clone(), r1).unwrap();
            // Now r2 can take up to 10000.
            add_recipient(&env, admin, r2, 10000).unwrap();
        });
    }

    #[test]
    fn invalid_fee_rate_rejected() {
        let (env, admin, contract_id) = setup();
        env.as_contract(&contract_id, || {
            assert_eq!(
                set_fee_rate(&env, admin.clone(), 10001),
                Err(FeeSwitchError::InvalidFeeRate)
            );
            assert_eq!(
                set_fee_rate(&env, admin, -1),
                Err(FeeSwitchError::InvalidFeeRate)
            );
        });
    }

    #[test]
    fn zero_fee_rate_distributes_nothing() {
        let (env, admin, contract_id) = setup();
        env.as_contract(&contract_id, || {
            set_fee_switch(&env, admin.clone(), true).unwrap();
            set_fee_rate(&env, admin, 0).unwrap();

            let result = distribute_revenue(&env, 5000).unwrap();
            assert_eq!(result.fee_amount, 0);
            assert_eq!(result.net_amount, 5000);
        });
    }
}