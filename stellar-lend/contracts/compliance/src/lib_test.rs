#![cfg(test)]

use soroban_sdk::{testutils::Address as _, testutils::Ledger as _, Address, Env, Symbol};

use crate::{ComplianceContract, ComplianceContractClient, ComplianceError, TransactionLimits};

fn setup() -> (Env, Address, ComplianceContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register(ComplianceContract, ());
    let client = ComplianceContractClient::new(&env, &contract_id);
    (env, admin, client)
}

#[test]
fn test_initialize() {
    let (_env, admin, client) = setup();
    client.initialize(&admin);
}

#[test]
fn test_initialize_cannot_double_init() {
    let (_env, admin, client) = setup();
    client.initialize(&admin);
    let result = client.try_initialize(&admin);
    assert_eq!(result, Err(Ok(ComplianceError::Unauthorized)));
}

#[test]
fn test_add_and_check_sanction() {
    let (env, admin, client) = setup();
    client.initialize(&admin);

    let target = Address::generate(&env);
    let source = Symbol::new(&env, "OFAC");
    let reason = Symbol::new(&env, "sanctioned_entity");

    client.add_sanction(&admin, &target, &source, &reason, &None);
    assert!(client.check_sanctioned(&target));
}

#[test]
fn test_remove_sanction() {
    let (env, admin, client) = setup();
    client.initialize(&admin);

    let target = Address::generate(&env);
    let source = Symbol::new(&env, "OFAC");
    let reason = Symbol::new(&env, "sanctioned_entity");

    client.add_sanction(&admin, &target, &source, &reason, &None);
    assert!(client.check_sanctioned(&target));

    client.remove_sanction(&admin, &target);
    assert!(!client.check_sanctioned(&target));
}

#[test]
fn test_double_sanction_fails() {
    let (env, admin, client) = setup();
    client.initialize(&admin);

    let target = Address::generate(&env);
    let source = Symbol::new(&env, "OFAC");
    let reason = Symbol::new(&env, "sanctioned_entity");

    client.add_sanction(&admin, &target, &source, &reason, &None);
    let result = client.try_add_sanction(&admin, &target, &source, &reason, &None);
    assert_eq!(result, Err(Ok(ComplianceError::AlreadySanctioned)));
}

#[test]
fn test_remove_unsanctioned_fails() {
    let (env, admin, client) = setup();
    client.initialize(&admin);

    let target = Address::generate(&env);
    let result = client.try_remove_sanction(&admin, &target);
    assert_eq!(result, Err(Ok(ComplianceError::AddressNotSanctioned)));
}

#[test]
fn test_set_and_check_kyc() {
    let (env, admin, client) = setup();
    client.initialize(&admin);

    let user = Address::generate(&env);
    let jurisdiction = Symbol::new(&env, "US");
    let provider = Symbol::new(&env, "Jumio");

    client.set_kyc_verification(&admin, &user, &1, &jurisdiction, &provider, &31536000);
    assert!(client.check_kyc(&user));

    let kyc = client.get_kyc(&user).unwrap();
    assert_eq!(kyc.verified, true);
    assert_eq!(kyc.tier, 1);
    assert_eq!(kyc.jurisdiction, jurisdiction);
    assert_eq!(kyc.kyc_provider, provider);
}

#[test]
fn test_revoke_kyc() {
    let (env, admin, client) = setup();
    client.initialize(&admin);

    let user = Address::generate(&env);
    let jurisdiction = Symbol::new(&env, "US");
    let provider = Symbol::new(&env, "Jumio");

    client.set_kyc_verification(&admin, &user, &1, &jurisdiction, &provider, &31536000);
    assert!(client.check_kyc(&user));

    client.revoke_kyc(&admin, &user);
    assert!(!client.check_kyc(&user));
}

#[test]
fn test_unauthorized_admin_fails() {
    let (env, admin, client) = setup();
    client.initialize(&admin);

    let unauthorized = Address::generate(&env);
    let target = Address::generate(&env);
    let source = Symbol::new(&env, "OFAC");
    let reason = Symbol::new(&env, "sanctioned_entity");

    let result = client.try_add_sanction(&unauthorized, &target, &source, &reason, &None);
    assert_eq!(result, Err(Ok(ComplianceError::Unauthorized)));
}

#[test]
fn test_set_and_get_tx_limits() {
    let (env, admin, client) = setup();
    client.initialize(&admin);

    let user = Address::generate(&env);
    let limits = TransactionLimits {
        daily_limit: 500_000_000_000,
        weekly_limit: 2_000_000_000_000,
        max_single_tx: 100_000_000_000,
    };

    client.set_tx_limits(&admin, &user, &limits);
    let result = client.get_tx_limits(&user);
    assert_eq!(result.daily_limit, 500_000_000_000);
    assert_eq!(result.weekly_limit, 2_000_000_000_000);
    assert_eq!(result.max_single_tx, 100_000_000_000);
}

#[test]
fn test_check_transaction_passes() {
    let (env, admin, client) = setup();
    client.initialize(&admin);

    let from = Address::generate(&env);
    let to = Address::generate(&env);
    let asset = Address::generate(&env);

    client.check_transaction(&from, &to, &100_000_000, &asset);
}

#[test]
fn test_check_transaction_sanctioned_fails() {
    let (env, admin, client) = setup();
    client.initialize(&admin);

    let from = Address::generate(&env);
    let to = Address::generate(&env);
    let asset = Address::generate(&env);

    let source = Symbol::new(&env, "OFAC");
    let reason = Symbol::new(&env, "sanctioned");
    client.add_sanction(&admin, &from, &source, &reason, &None);

    let result = client.try_check_transaction(&from, &to, &100_000_000, &asset);
    assert_eq!(result, Err(Ok(ComplianceError::AddressSanctioned)));
}

#[test]
fn test_check_transaction_too_large_fails() {
    let (env, admin, client) = setup();
    client.initialize(&admin);

    let from = Address::generate(&env);
    let to = Address::generate(&env);
    let asset = Address::generate(&env);

    let limits = TransactionLimits {
        daily_limit: 1_000_000_000_000,
        weekly_limit: 5_000_000_000_000,
        max_single_tx: 100,
    };
    client.set_tx_limits(&admin, &from, &limits);

    let result = client.try_check_transaction(&from, &to, &200, &asset);
    assert_eq!(result, Err(Ok(ComplianceError::TransactionTooLarge)));
}

#[test]
fn test_file_and_get_sar() {
    let (env, admin, client) = setup();
    client.initialize(&admin);

    let target = Address::generate(&env);
    let asset = Address::generate(&env);
    let reason = Symbol::new(&env, "unusual_activity");

    let sar_id = client.file_sar(&admin, &target, &reason, &500_000, &asset);
    assert_eq!(sar_id, 0);

    let sar = client.get_sar(&0).unwrap();
    assert_eq!(sar.address, target);
    assert_eq!(sar.amount, 500_000);
    assert_eq!(sar.sar_id, 0);

    let sar_id2 = client.file_sar(&admin, &target, &reason, &1_000_000, &asset);
    assert_eq!(sar_id2, 1);
}

#[test]
fn test_add_and_remove_restricted_jurisdiction() {
    let (env, admin, client) = setup();
    client.initialize(&admin);

    let jurisdiction = Symbol::new(&env, "KP");
    client.add_restricted_jurisdiction(&admin, &jurisdiction);

    let config = client.get_config();
    assert!(config.restricted_jurisdictions.contains(&jurisdiction));

    client.remove_restricted_jurisdiction(&admin, &jurisdiction);
    let config = client.get_config();
    assert!(!config.restricted_jurisdictions.contains(&jurisdiction));
}

#[test]
fn test_pause_unpause() {
    let (env, admin, client) = setup();
    client.initialize(&admin);

    client.pause(&admin);
    let config = client.get_config();
    assert!(config.paused);

    let from = Address::generate(&env);
    let to = Address::generate(&env);
    let asset = Address::generate(&env);
    let result = client.try_check_transaction(&from, &to, &100, &asset);
    assert_eq!(result, Err(Ok(ComplianceError::CompliancePaused)));

    client.unpause(&admin);
    let config = client.get_config();
    assert!(!config.paused);
}

#[test]
fn test_geographic_restriction() {
    let (env, admin, client) = setup();
    client.initialize(&admin);

    let user = Address::generate(&env);
    let jurisdiction = Symbol::new(&env, "KP");
    let provider = Symbol::new(&env, "Jumio");

    client.add_restricted_jurisdiction(&admin, &jurisdiction);
    client.set_kyc_verification(&admin, &user, &1, &jurisdiction, &provider, &31536000);

    let to = Address::generate(&env);
    let asset = Address::generate(&env);
    let result = client.try_check_transaction(&user, &to, &100, &asset);
    assert_eq!(result, Err(Ok(ComplianceError::GeographicRestricted)));
}

#[test]
fn test_get_config_defaults() {
    let (_env, admin, client) = setup();
    client.initialize(&admin);

    let config = client.get_config();
    assert_eq!(config.admin, admin);
    assert!(!config.paused);
    assert_eq!(config.default_limits.daily_limit, 1_000_000_000_000);
    assert_eq!(config.default_limits.weekly_limit, 5_000_000_000_000);
    assert_eq!(config.default_limits.max_single_tx, 500_000_000_000);
}

#[test]
fn test_reject_zero_or_negative_amounts() {
    let (env, admin, client) = setup();
    client.initialize(&admin);

    let from = Address::generate(&env);
    let to = Address::generate(&env);
    let asset = Address::generate(&env);

    // Negative transaction amount is rejected
    let res_neg = client.try_check_transaction(&from, &to, &-500, &asset);
    assert_eq!(res_neg, Err(Ok(ComplianceError::InvalidAmount)));

    // Zero transaction amount is rejected
    let res_zero = client.try_check_transaction(&from, &to, &0, &asset);
    assert_eq!(res_zero, Err(Ok(ComplianceError::InvalidAmount)));

    // Negative SAR amount is rejected
    let reason = Symbol::new(&env, "test");
    let sar_neg = client.try_file_sar(&admin, &from, &reason, &-100, &asset);
    assert_eq!(sar_neg, Err(Ok(ComplianceError::InvalidAmount)));
}

#[test]
fn test_failed_transaction_does_not_corrupt_volume() {
    let (env, admin, client) = setup();
    client.initialize(&admin);

    let from = Address::generate(&env);
    let to = Address::generate(&env);
    let asset = Address::generate(&env);

    let limits = TransactionLimits {
        daily_limit: 1_000,
        weekly_limit: 5_000,
        max_single_tx: 1_000,
    };
    client.set_tx_limits(&admin, &from, &limits);

    // Initial tx of 600 passes
    client.check_transaction(&from, &to, &600, &asset);
    let vol = client.get_tx_volume(&from);
    assert_eq!(vol.daily_volume, 600);

    // Second tx of 500 would exceed daily limit (600 + 500 = 1100 > 1000)
    let err = client.try_check_transaction(&from, &to, &500, &asset);
    assert_eq!(err, Err(Ok(ComplianceError::DailyLimitExceeded)));

    // Daily volume must NOT be updated or corrupted by the failed tx!
    let vol_after = client.get_tx_volume(&from);
    assert_eq!(vol_after.daily_volume, 600);
    assert_eq!(vol_after.weekly_volume, 600);

    // A valid tx of 300 can still succeed (600 + 300 = 900 <= 1000)
    client.check_transaction(&from, &to, &300, &asset);
    let vol_final = client.get_tx_volume(&from);
    assert_eq!(vol_final.daily_volume, 900);
}

#[test]
fn test_daily_and_weekly_window_resets() {
    let (env, admin, client) = setup();
    client.initialize(&admin);

    let from = Address::generate(&env);
    let to = Address::generate(&env);
    let asset = Address::generate(&env);

    let limits = TransactionLimits {
        daily_limit: 1_000,
        weekly_limit: 2_000,
        max_single_tx: 1_000,
    };
    client.set_tx_limits(&admin, &from, &limits);

    // Day 1: tx of 800
    env.ledger().set_timestamp(10_000);
    client.check_transaction(&from, &to, &800, &asset);
    let vol1 = client.get_tx_volume(&from);
    assert_eq!(vol1.daily_volume, 800);
    assert_eq!(vol1.weekly_volume, 800);

    // Later on Day 1 (timestamp 50,000): another tx of 300 fails because 800 + 300 > 1000
    env.ledger().set_timestamp(50_000);
    let err_day1 = client.try_check_transaction(&from, &to, &300, &asset);
    assert_eq!(err_day1, Err(Ok(ComplianceError::DailyLimitExceeded)));

    // Day 2 (timestamp 90,000, which is in next day bucket: 90000 / 86400 = 1):
    // Daily volume must reset to 0, allowing new transactions!
    env.ledger().set_timestamp(90_000);
    client.check_transaction(&from, &to, &800, &asset);
    let vol2 = client.get_tx_volume(&from);
    assert_eq!(vol2.daily_volume, 800);
    // Weekly volume accumulates within the same week: 800 + 800 = 1600
    assert_eq!(vol2.weekly_volume, 1600);

    // Week 2 (timestamp 700_000, which is in next week bucket: 700000 / 604800 = 1):
    // Both daily and weekly volumes reset!
    env.ledger().set_timestamp(700_000);
    client.check_transaction(&from, &to, &800, &asset);
    let vol3 = client.get_tx_volume(&from);
    assert_eq!(vol3.daily_volume, 800);
    assert_eq!(vol3.weekly_volume, 800);
}
