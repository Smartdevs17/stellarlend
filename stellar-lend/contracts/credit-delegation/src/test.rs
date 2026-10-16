use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, Env,
};

fn setup_test(env: &Env) -> CreditDelegationContractClient<'_> {
    let contract_id = env.register(CreditDelegationContract, ());
    CreditDelegationContractClient::new(env, &contract_id)
}

#[test]
fn test_create_and_draw_credit_line() {
    let env = Env::default();
    env.mock_all_auths();
    let client = setup_test(&env);

    let delegator = Address::generate(&env);
    let delegate = Address::generate(&env);

    let id = client.create_credit_line(
        &delegator,
        &delegate,
        &10_000,
        &500, // 5%
        &100, // maturity at ledger 100
        &Some(2_000),
    );
    assert_eq!(id, 1);

    let line = client.get_credit_line(&id).unwrap();
    assert_eq!(line.status, CreditStatus::Active);
    assert_eq!(line.max_amount, 10_000);
    assert_eq!(line.drawn_amount, 0);

    // Draw 4,000
    client.draw(&id, &delegate, &4_000);
    let updated = client.get_credit_line(&id).unwrap();
    assert_eq!(updated.status, CreditStatus::Drawn);
    assert_eq!(updated.drawn_amount, 4_000);

    let draw_rec = client.get_draw_record(&id).unwrap();
    assert_eq!(draw_rec.amount, 4_000);
}

#[test]
#[should_panic(expected = "draw amount must be positive")]
fn test_cannot_draw_negative_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let client = setup_test(&env);

    let delegator = Address::generate(&env);
    let delegate = Address::generate(&env);

    let id = client.create_credit_line(&delegator, &delegate, &10_000, &500, &100, &None);
    client.draw(&id, &delegate, &5_000);

    // Attempt to draw negative amount to erase debt -> must panic
    client.draw(&id, &delegate, &-2_000);
}

#[test]
#[should_panic(expected = "repayment amount must be positive")]
fn test_cannot_repay_zero_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let client = setup_test(&env);

    let delegator = Address::generate(&env);
    let delegate = Address::generate(&env);

    let id = client.create_credit_line(&delegator, &delegate, &10_000, &500, &100, &None);

    // Repay 0 on undrawn credit line must panic, not set status to Repaid
    client.repay(&id, &delegate, &0);
}

#[test]
#[should_panic(expected = "no debt to repay")]
fn test_cannot_repay_when_no_debt_drawn() {
    let env = Env::default();
    env.mock_all_auths();
    let client = setup_test(&env);

    let delegator = Address::generate(&env);
    let delegate = Address::generate(&env);

    let id = client.create_credit_line(&delegator, &delegate, &10_000, &500, &100, &None);

    // Attempting to repay 1,000 when drawn_amount == 0 must panic
    client.repay(&id, &delegate, &1_000);
}

#[test]
fn test_partial_and_full_repayment() {
    let env = Env::default();
    env.mock_all_auths();
    let client = setup_test(&env);

    let delegator = Address::generate(&env);
    let delegate = Address::generate(&env);

    let id = client.create_credit_line(&delegator, &delegate, &10_000, &500, &100, &None);
    client.draw(&id, &delegate, &6_000);

    // Partial repay 2,000
    client.repay(&id, &delegate, &2_000);
    let line = client.get_credit_line(&id).unwrap();
    assert_eq!(line.repaid_amount, 2_000);
    assert_eq!(line.status, CreditStatus::Drawn);

    // Full remaining repay 4,000
    client.repay(&id, &delegate, &4_000);
    let line_full = client.get_credit_line(&id).unwrap();
    assert_eq!(line_full.repaid_amount, 6_000);
    assert_eq!(line_full.status, CreditStatus::Repaid);

    let rep_rec = client.get_repayment_record(&id).unwrap();
    assert_eq!(rep_rec.amount, 4_000);
}

#[test]
#[should_panic(expected = "cannot delegate to self")]
fn test_cannot_delegate_to_self() {
    let env = Env::default();
    env.mock_all_auths();
    let client = setup_test(&env);

    let user = Address::generate(&env);
    client.create_credit_line(&user, &user, &10_000, &500, &100, &None);
}

#[test]
#[should_panic(expected = "maturity must be in the future")]
fn test_cannot_create_expired_credit_line() {
    let env = Env::default();
    env.mock_all_auths();
    let client = setup_test(&env);

    let delegator = Address::generate(&env);
    let delegate = Address::generate(&env);

    env.ledger().set_sequence_number(50);
    // maturity 40 is in the past -> must panic
    client.create_credit_line(&delegator, &delegate, &10_000, &500, &40, &None);
}

#[test]
#[should_panic(expected = "cannot transfer to delegate")]
fn test_cannot_transfer_to_delegate() {
    let env = Env::default();
    env.mock_all_auths();
    let client = setup_test(&env);

    let delegator = Address::generate(&env);
    let delegate = Address::generate(&env);

    let id = client.create_credit_line(&delegator, &delegate, &10_000, &500, &100, &None);

    // Delegator cannot transfer credit ownership to the borrower (delegate)
    client.transfer(&id, &delegator, &delegate);
}

#[test]
#[should_panic(expected = "credit line has not matured yet")]
fn test_cannot_claim_default_before_maturity() {
    let env = Env::default();
    env.mock_all_auths();
    let client = setup_test(&env);

    let delegator = Address::generate(&env);
    let delegate = Address::generate(&env);

    let id = client.create_credit_line(&delegator, &delegate, &10_000, &500, &50, &None);
    client.draw(&id, &delegate, &5_000);

    // Ledger sequence is 0 <= maturity 50 -> must panic
    client.claim_default(&id, &delegator);
}

#[test]
fn test_claim_default_after_maturity() {
    let env = Env::default();
    env.mock_all_auths();
    let client = setup_test(&env);

    let delegator = Address::generate(&env);
    let delegate = Address::generate(&env);

    let id = client.create_credit_line(&delegator, &delegate, &10_000, &500, &50, &None);
    client.draw(&id, &delegate, &5_000);

    // Advance past maturity
    env.ledger().set_sequence_number(100);

    client.claim_default(&id, &delegator);
    let line = client.get_credit_line(&id).unwrap();
    assert_eq!(line.status, CreditStatus::Defaulted);
}

#[test]
fn test_adjust_limit() {
    let env = Env::default();
    env.mock_all_auths();
    let client = setup_test(&env);

    let delegator = Address::generate(&env);
    let delegate = Address::generate(&env);

    let id = client.create_credit_line(&delegator, &delegate, &10_000, &500, &100, &None);
    client.draw(&id, &delegate, &3_000);

    // Adjust limit to 15,000
    client.adjust_limit(&id, &delegator, &15_000);
    assert_eq!(client.get_credit_line(&id).unwrap().max_amount, 15_000);
}
