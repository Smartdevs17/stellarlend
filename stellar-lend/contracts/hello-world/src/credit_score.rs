use soroban_sdk::{contracttype, symbol_short, Address, Env, Symbol, Vec};

use crate::errors::LendingError;
use crate::storage;

/// Event emitted whenever a borrower's credit score changes.
///
/// Topics: `("credit", "score_chg")`
/// Data: `(user, old_score, new_score, reason)`
pub const CREDIT_SCORE_CHANGED: Symbol = symbol_short!("score_chg");

/// Maximum number of snapshots retained per borrower. Older entries are
/// dropped from the front so storage stays bounded on-chain.
pub const MAX_HISTORY_ENTRIES: u32 = 50;

/// Credit score range: 0-1000
pub const MIN_CREDIT_SCORE: i128 = 0;
pub const MAX_CREDIT_SCORE: i128 = 1000;
pub const DEFAULT_CREDIT_SCORE: i128 = 500;

/// Score impact factors (in basis points)
pub const ON_TIME_REPAYMENT_BONUS: i128 = 10; // +1% per on-time repayment
pub const LATE_REPAYMENT_PENALTY: i128 = 50; // -5% per late repayment
pub const LIQUIDATION_PENALTY: i128 = 100; // -10% per liquidation
pub const HEALTHY_POSITION_BONUS: i128 = 5; // +0.5% for maintaining healthy position

/// LTV adjustments based on credit score (in basis points)
pub const BASE_LTV_BPS: i128 = 7000; // 70% base LTV
pub const MAX_LTV_BONUS_BPS: i128 = 1500; // Up to +15% for excellent credit
pub const MAX_LTV_PENALTY_BPS: i128 = 2000; // Up to -20% for poor credit

/// Interest rate adjustments (in basis points)
pub const BASE_RATE_ADJUSTMENT_BPS: i128 = 500; // ±5% max rate adjustment

#[derive(Clone, Debug)]
#[contracttype]
pub struct CreditScore {
    pub user: Address,
    pub score: i128,
    pub on_time_repayments: u32,
    pub late_repayments: u32,
    pub liquidations: u32,
    pub total_borrowed: i128,
    pub total_repaid: i128,
    pub last_updated: u64,
    pub history: Vec<CreditScoreSnapshot>,
}

#[derive(Clone, Debug)]
#[contracttype]
pub struct CreditScoreSnapshot {
    pub score: i128,
    pub timestamp: u64,
    pub reason: CreditScoreChangeReason,
}

#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub enum CreditScoreChangeReason {
    OnTimeRepayment,
    LateRepayment,
    Liquidation,
    HealthyPosition,
    NewUser,
}

/// Clamp a raw score into the valid `[MIN_CREDIT_SCORE, MAX_CREDIT_SCORE]` range.
fn clamp_score(score: i128) -> i128 {
    score.max(MIN_CREDIT_SCORE).min(MAX_CREDIT_SCORE)
}

/// Append a snapshot to the borrower's history, trimming the oldest entries
/// once `MAX_HISTORY_ENTRIES` is exceeded.
fn push_history(env: &Env, credit_score: &mut CreditScore, reason: CreditScoreChangeReason) {
    credit_score.history.push_back(CreditScoreSnapshot {
        score: credit_score.score,
        timestamp: env.ledger().timestamp(),
        reason,
    });

    while credit_score.history.len() > MAX_HISTORY_ENTRIES {
        credit_score.history.remove(0);
    }
}

/// Emit the score-change event and persist the updated record.
fn commit_score(
    env: &Env,
    key: &storage::DataKey,
    credit_score: &mut CreditScore,
    old_score: i128,
    reason: CreditScoreChangeReason,
) {
    credit_score.score = clamp_score(credit_score.score);
    credit_score.last_updated = env.ledger().timestamp();
    push_history(env, credit_score, reason.clone());

    env.storage().persistent().set(key, credit_score);

    env.events().publish(
        (symbol_short!("credit"), CREDIT_SCORE_CHANGED),
        (
            credit_score.user.clone(),
            old_score,
            credit_score.score,
            reason,
        ),
    );
}

/// Load a borrower's credit record, initializing it if absent.
fn load_or_init(env: &Env, user: &Address) -> Result<CreditScore, LendingError> {
    initialize_credit_score(env, user)?;
    get_credit_score(env, user)
}

/// Initialize credit score for a new user
pub fn initialize_credit_score(env: &Env, user: &Address) -> Result<(), LendingError> {
    let key = storage::DataKey::CreditScore(user.clone());

    if env.storage().persistent().has(&key) {
        return Ok(()); // Already initialized
    }

    let mut history = Vec::new(env);
    history.push_back(CreditScoreSnapshot {
        score: DEFAULT_CREDIT_SCORE,
        timestamp: env.ledger().timestamp(),
        reason: CreditScoreChangeReason::NewUser,
    });

    let credit_score = CreditScore {
        user: user.clone(),
        score: DEFAULT_CREDIT_SCORE,
        on_time_repayments: 0,
        late_repayments: 0,
        liquidations: 0,
        total_borrowed: 0,
        total_repaid: 0,
        last_updated: env.ledger().timestamp(),
        history,
    };

    env.storage().persistent().set(&key, &credit_score);
    Ok(())
}

/// Get credit score for a user
pub fn get_credit_score(env: &Env, user: &Address) -> Result<CreditScore, LendingError> {
    let key = storage::DataKey::CreditScore(user.clone());
    env.storage()
        .persistent()
        .get(&key)
        .ok_or(LendingError::NotFound)
}

/// Get the score-change history for a user (most recent last).
pub fn get_credit_score_history(
    env: &Env,
    user: &Address,
) -> Result<Vec<CreditScoreSnapshot>, LendingError> {
    Ok(get_credit_score(env, user)?.history)
}

/// Update credit score after repayment
pub fn update_score_on_repayment(
    env: &Env,
    user: &Address,
    amount: i128,
    is_on_time: bool,
) -> Result<(), LendingError> {
    if amount < 0 {
        return Err(LendingError::InvalidAmount);
    }

    let key = storage::DataKey::CreditScore(user.clone());
    let mut credit_score = load_or_init(env, user)?;
    let old_score = credit_score.score;

    credit_score.total_repaid += amount;

    let (score_change, reason) = if is_on_time {
        credit_score.on_time_repayments += 1;
        (
            ON_TIME_REPAYMENT_BONUS,
            CreditScoreChangeReason::OnTimeRepayment,
        )
    } else {
        credit_score.late_repayments += 1;
        (
            -LATE_REPAYMENT_PENALTY,
            CreditScoreChangeReason::LateRepayment,
        )
    };

    credit_score.score += score_change;
    commit_score(env, &key, &mut credit_score, old_score, reason);
    Ok(())
}

/// Update credit score after liquidation
pub fn update_score_on_liquidation(env: &Env, user: &Address) -> Result<(), LendingError> {
    let key = storage::DataKey::CreditScore(user.clone());
    let mut credit_score = load_or_init(env, user)?;
    let old_score = credit_score.score;

    credit_score.liquidations += 1;
    credit_score.score -= LIQUIDATION_PENALTY;
    commit_score(
        env,
        &key,
        &mut credit_score,
        old_score,
        CreditScoreChangeReason::Liquidation,
    );
    Ok(())
}

/// Update credit score for maintaining healthy position
pub fn update_score_on_healthy_position(env: &Env, user: &Address) -> Result<(), LendingError> {
    let key = storage::DataKey::CreditScore(user.clone());
    let mut credit_score = load_or_init(env, user)?;
    let old_score = credit_score.score;

    credit_score.score += HEALTHY_POSITION_BONUS;
    commit_score(
        env,
        &key,
        &mut credit_score,
        old_score,
        CreditScoreChangeReason::HealthyPosition,
    );
    Ok(())
}

/// Calculate adjusted LTV based on credit score
pub fn calculate_adjusted_ltv(env: &Env, user: &Address) -> Result<i128, LendingError> {
    let credit_score = get_credit_score(env, user).unwrap_or(CreditScore {
        user: user.clone(),
        score: DEFAULT_CREDIT_SCORE,
        on_time_repayments: 0,
        late_repayments: 0,
        liquidations: 0,
        total_borrowed: 0,
        total_repaid: 0,
        last_updated: env.ledger().timestamp(),
        history: Vec::new(env),
    });

    // Score ranges: 0-300 (poor), 301-600 (fair), 601-800 (good), 801-1000 (excellent)
    let adjustment = if credit_score.score >= 800 {
        // Excellent: +15% LTV
        MAX_LTV_BONUS_BPS
    } else if credit_score.score >= 600 {
        // Good: +5% to +15% LTV (linear)
        ((credit_score.score - 600) * MAX_LTV_BONUS_BPS) / 200
    } else if credit_score.score >= 300 {
        // Fair: -5% to +5% LTV (linear)
        ((credit_score.score - 450) * MAX_LTV_BONUS_BPS) / 150
    } else {
        // Poor: -20% to -5% LTV
        -MAX_LTV_PENALTY_BPS + ((credit_score.score * (MAX_LTV_PENALTY_BPS - 500)) / 300)
    };

    Ok(BASE_LTV_BPS + adjustment)
}

/// Calculate adjusted interest rate based on credit score
pub fn calculate_adjusted_interest_rate(
    env: &Env,
    user: &Address,
    base_rate_bps: i128,
) -> Result<i128, LendingError> {
    let credit_score = get_credit_score(env, user).unwrap_or(CreditScore {
        user: user.clone(),
        score: DEFAULT_CREDIT_SCORE,
        on_time_repayments: 0,
        late_repayments: 0,
        liquidations: 0,
        total_borrowed: 0,
        total_repaid: 0,
        last_updated: env.ledger().timestamp(),
        history: Vec::new(env),
    });

    // Better credit score = lower interest rate
    // Score 1000 = -5% rate, Score 500 = 0% adjustment, Score 0 = +5% rate
    let adjustment = ((500 - credit_score.score) * BASE_RATE_ADJUSTMENT_BPS) / 500;

    Ok(base_rate_bps + adjustment)
}

/// Record borrow for credit tracking
pub fn record_borrow(env: &Env, user: &Address, amount: i128) -> Result<(), LendingError> {
    if amount < 0 {
        return Err(LendingError::InvalidAmount);
    }

    let key = storage::DataKey::CreditScore(user.clone());
    let mut credit_score = load_or_init(env, user)?;

    credit_score.total_borrowed += amount;
    env.storage().persistent().set(&key, &credit_score);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;

    #[test]
    fn test_initialize_credit_score() {
        let env = Env::default();
        let user = Address::generate(&env);

        initialize_credit_score(&env, &user).unwrap();
        let score = get_credit_score(&env, &user).unwrap();

        assert_eq!(score.score, DEFAULT_CREDIT_SCORE);
        assert_eq!(score.on_time_repayments, 0);
        assert_eq!(score.late_repayments, 0);
        assert_eq!(score.liquidations, 0);
    }

    #[test]
    fn test_on_time_repayment_increases_score() {
        let env = Env::default();
        let user = Address::generate(&env);

        initialize_credit_score(&env, &user).unwrap();
        update_score_on_repayment(&env, &user, 1000, true).unwrap();

        let score = get_credit_score(&env, &user).unwrap();
        assert_eq!(score.score, DEFAULT_CREDIT_SCORE + ON_TIME_REPAYMENT_BONUS);
        assert_eq!(score.on_time_repayments, 1);
    }

    #[test]
    fn test_late_repayment_decreases_score() {
        let env = Env::default();
        let user = Address::generate(&env);

        initialize_credit_score(&env, &user).unwrap();
        update_score_on_repayment(&env, &user, 1000, false).unwrap();

        let score = get_credit_score(&env, &user).unwrap();
        assert_eq!(score.score, DEFAULT_CREDIT_SCORE - LATE_REPAYMENT_PENALTY);
        assert_eq!(score.late_repayments, 1);
    }

    #[test]
    fn test_liquidation_decreases_score() {
        let env = Env::default();
        let user = Address::generate(&env);

        initialize_credit_score(&env, &user).unwrap();
        update_score_on_liquidation(&env, &user).unwrap();

        let score = get_credit_score(&env, &user).unwrap();
        assert_eq!(score.score, DEFAULT_CREDIT_SCORE - LIQUIDATION_PENALTY);
        assert_eq!(score.liquidations, 1);
    }

    #[test]
    fn test_score_bounds() {
        let env = Env::default();
        let user = Address::generate(&env);

        initialize_credit_score(&env, &user).unwrap();

        // Test max score
        for _ in 0..100 {
            update_score_on_repayment(&env, &user, 1000, true).unwrap();
        }
        let score = get_credit_score(&env, &user).unwrap();
        assert_eq!(score.score, MAX_CREDIT_SCORE);

        // Test min score
        for _ in 0..100 {
            update_score_on_liquidation(&env, &user).unwrap();
        }
        let score = get_credit_score(&env, &user).unwrap();
        assert_eq!(score.score, MIN_CREDIT_SCORE);
    }

    #[test]
    fn test_adjusted_ltv() {
        let env = Env::default();
        let user = Address::generate(&env);

        initialize_credit_score(&env, &user).unwrap();

        // Default score should give base LTV
        let ltv = calculate_adjusted_ltv(&env, &user).unwrap();
        assert!(ltv >= BASE_LTV_BPS - 1000 && ltv <= BASE_LTV_BPS + 1000);

        // Improve score to excellent
        for _ in 0..50 {
            update_score_on_repayment(&env, &user, 1000, true).unwrap();
        }

        let ltv = calculate_adjusted_ltv(&env, &user).unwrap();
        assert!(ltv > BASE_LTV_BPS);
    }

    #[test]
    fn test_empty_history_defaults() {
        let env = Env::default();
        let user = Address::generate(&env);

        // Querying an uninitialized user must not panic; LTV/rate fall back to defaults.
        let ltv = calculate_adjusted_ltv(&env, &user).unwrap();
        assert_eq!(ltv, BASE_LTV_BPS);

        let rate = calculate_adjusted_interest_rate(&env, &user, 1000).unwrap();
        assert_eq!(rate, 1000);

        // History is empty until the record is initialized.
        assert!(get_credit_score_history(&env, &user).is_err());
    }

    #[test]
    fn test_history_records_each_change() {
        let env = Env::default();
        let user = Address::generate(&env);

        initialize_credit_score(&env, &user).unwrap();
        update_score_on_repayment(&env, &user, 500, true).unwrap();
        update_score_on_repayment(&env, &user, 500, false).unwrap();
        update_score_on_liquidation(&env, &user).unwrap();

        let history = get_credit_score_history(&env, &user).unwrap();
        // NewUser + 3 changes
        assert_eq!(history.len(), 4);
        assert_eq!(
            history.get(1).unwrap().reason,
            CreditScoreChangeReason::OnTimeRepayment
        );
        assert_eq!(
            history.get(2).unwrap().reason,
            CreditScoreChangeReason::LateRepayment
        );
        assert_eq!(
            history.get(3).unwrap().reason,
            CreditScoreChangeReason::Liquidation
        );
    }

    #[test]
    fn test_history_is_bounded() {
        let env = Env::default();
        let user = Address::generate(&env);

        initialize_credit_score(&env, &user).unwrap();
        for _ in 0..(MAX_HISTORY_ENTRIES + 10) {
            update_score_on_repayment(&env, &user, 1, true).unwrap();
        }

        let history = get_credit_score_history(&env, &user).unwrap();
        assert_eq!(history.len(), MAX_HISTORY_ENTRIES);
    }

    #[test]
    fn test_late_repayment_clamps_at_min() {
        let env = Env::default();
        let user = Address::generate(&env);

        initialize_credit_score(&env, &user).unwrap();
        for _ in 0..100 {
            update_score_on_repayment(&env, &user, 1, false).unwrap();
        }

        let score = get_credit_score(&env, &user).unwrap();
        assert_eq!(score.score, MIN_CREDIT_SCORE);
        assert_eq!(score.late_repayments, 100);
    }

    #[test]
    fn test_negative_amount_rejected() {
        let env = Env::default();
        let user = Address::generate(&env);

        assert!(update_score_on_repayment(&env, &user, -1, true).is_err());
        assert!(record_borrow(&env, &user, -1).is_err());
    }

    #[test]
    fn test_score_change_event_emitted() {
        use soroban_sdk::testutils::Events as _;

        let env = Env::default();
        let user = Address::generate(&env);

        initialize_credit_score(&env, &user).unwrap();
        update_score_on_repayment(&env, &user, 100, true).unwrap();

        let events = env.events().all();
        assert!(!events.is_empty());
    }
}
