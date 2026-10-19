mod common;

// A deliberately nonstandard token exercises transfer rollback and callback
// protection; normal payout tests below use the real Stellar Asset Contract.
mod adversarial {
    use soroban_sdk::{contract, contractimpl, symbol_short, Address, Env};
    use stellarlend_lending::LendingContractClient;

    #[contract]
    pub struct ProbeToken;

    #[contractimpl]
    impl ProbeToken {
        pub fn initialize(env: Env, pool: Address) {
            env.storage().instance().set(&symbol_short!("pool"), &pool);
        }

        pub fn set_fee(env: Env, fee: i128) {
            env.storage().instance().set(&symbol_short!("fee"), &fee);
        }

        pub fn mint(env: Env, user: Address, amount: i128) {
            env.storage().persistent().set(&user, &amount);
        }

        pub fn balance(env: Env, user: Address) -> i128 {
            env.storage().persistent().get(&user).unwrap_or(0)
        }

        pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
            let pool: Address = env
                .storage()
                .instance()
                .get(&symbol_short!("pool"))
                .unwrap();
            // The host or shared guard must reject callbacks during transfers.
            assert!(LendingContractClient::new(&env, &pool)
                .try_claim_supplier_rewards(&to)
                .is_err());
            let fee: i128 = env
                .storage()
                .instance()
                .get(&symbol_short!("fee"))
                .unwrap_or(0);
            let from_balance = Self::balance(env.clone(), from.clone());
            let to_balance = Self::balance(env.clone(), to.clone());
            assert!(from_balance >= amount);
            env.storage()
                .persistent()
                .set(&from, &(from_balance - amount));
            env.storage()
                .persistent()
                .set(&to, &(to_balance + amount - fee));
        }
    }
}

#[test]
fn token_callbacks_are_blocked_and_failed_transfers_roll_back_claims_and_funding() {
    let f = common::setup();
    let user = Address::generate(&f.env);
    let asset = f.env.register(adversarial::ProbeToken, ());
    let token = adversarial::ProbeTokenClient::new(&f.env, &asset);
    token.initialize(&f.client.address);
    token.mint(&f.admin, &1_000);
    f.client.configure_supplier_rewards(&f.admin, &asset, &true);
    token.set_fee(&1);
    assert_eq!(
        f.client.try_fund_supplier_rewards(&f.admin, &1_000),
        Err(Ok(Error::UnsupportedTransfer))
    );
    assert_eq!(
        f.client.get_supplier_reward_pool(),
        SupplierRewardPool::default()
    );
    assert_eq!(token.balance(&f.admin), 1_000);
    assert_eq!(token.balance(&f.client.address), 0);

    token.set_fee(&0);
    f.client.fund_supplier_rewards(&f.admin, &1_000);
    f.client.deposit(&user, &f.collateral_asset, &500);
    f.client.set_supplier_reward(&f.admin, &user, &300);
    let before = f.client.get_supplier_reward_pool();
    token.set_fee(&1);
    assert_eq!(
        f.client.try_claim_supplier_rewards(&user),
        Err(Ok(Error::UnsupportedTransfer))
    );
    assert_eq!(f.client.get_supplier_reward_pool(), before);
    assert_eq!(f.client.get_supplier_reward(&user).claimed, 0);
    assert_eq!(token.balance(&user), 0);
    assert_eq!(token.balance(&f.client.address), 1_000);
    token.set_fee(&0);
    assert_eq!(f.client.claim_supplier_rewards(&user), 300);
    assert_eq!(token.balance(&user), 300);
}

use soroban_sdk::{
    testutils::{Address as _, Events as _},
    token, Address,
};
use stellarlend_lending::{SupplierReward, SupplierRewardPool, SupplierRewardsError as Error};

fn funded() -> (common::Fixture<'static>, Address, Address, Address) {
    let f = common::setup();
    let issuer = Address::generate(&f.env);
    let token = f.env.register_stellar_asset_contract_v2(issuer).address();
    let alice = Address::generate(&f.env);
    let bob = Address::generate(&f.env);
    token::StellarAssetClient::new(&f.env, &token).mint(&f.admin, &1_000);
    f.client.configure_supplier_rewards(&f.admin, &token, &true);
    f.client.fund_supplier_rewards(&f.admin, &1_000);
    f.client.deposit(&alice, &f.collateral_asset, &500);
    f.client.deposit(&bob, &f.collateral_asset, &500);
    (f, token, alice, bob)
}

#[test]
fn funded_allocations_are_claimable_once_and_isolated_per_supplier() {
    let (f, asset, alice, bob) = funded();
    let token = token::Client::new(&f.env, &asset);
    f.client.set_supplier_reward(&f.admin, &alice, &300);
    f.client.set_supplier_reward(&f.admin, &bob, &200);
    let events_before = f.env.events().all();
    assert_eq!(f.client.claim_supplier_rewards(&alice), 300);
    assert_ne!(f.env.events().all(), events_before);
    assert_eq!(token.balance(&alice), 300);
    assert_eq!(token.balance(&bob), 0);
    assert_eq!(token.balance(&f.client.address), 700);
    assert_eq!(
        f.client.get_supplier_reward(&alice),
        SupplierReward {
            allocated: 300,
            claimed: 300
        }
    );
    let events_after = f.env.events().all();
    assert_eq!(f.client.claim_supplier_rewards(&alice), 0);
    assert_eq!(f.env.events().all(), events_after);
    assert_eq!(token.balance(&alice), 300);
    assert_eq!(f.client.claim_supplier_rewards(&bob), 200);
    assert_eq!(
        f.client.get_supplier_reward_pool(),
        SupplierRewardPool {
            available: 500,
            reserved: 0,
            paid: 500
        }
    );
}

#[test]
fn cumulative_allocations_are_retry_safe_before_and_after_claiming() {
    let (f, asset, alice, _) = funded();
    f.client.set_supplier_reward(&f.admin, &alice, &300);
    let pool = f.client.get_supplier_reward_pool();
    f.client.set_supplier_reward(&f.admin, &alice, &300);
    assert_eq!(f.client.get_supplier_reward_pool(), pool);
    assert_eq!(f.client.claim_supplier_rewards(&alice), 300);
    f.client.set_supplier_reward(&f.admin, &alice, &300);
    assert_eq!(f.client.claim_supplier_rewards(&alice), 0);
    f.client.set_supplier_reward(&f.admin, &alice, &450);
    assert_eq!(f.client.claim_supplier_rewards(&alice), 150);
    assert_eq!(token::Client::new(&f.env, &asset).balance(&alice), 450);
    assert_eq!(
        f.client.try_set_supplier_reward(&f.admin, &alice, &449),
        Err(Ok(Error::InvalidAmount))
    );
}

#[test]
fn unallocated_deposits_do_not_earn_and_funding_caps_allocations() {
    let (f, _, alice, bob) = funded();
    assert_eq!(f.client.claim_supplier_rewards(&alice), 0);
    f.client.set_supplier_reward(&f.admin, &alice, &800);
    assert_eq!(
        f.client.try_set_supplier_reward(&f.admin, &bob, &201),
        Err(Ok(Error::InsufficientBudget))
    );
    assert_eq!(
        f.client.get_supplier_reward(&bob),
        SupplierReward::default()
    );
    assert_eq!(
        f.client.get_supplier_reward_pool(),
        SupplierRewardPool {
            available: 200,
            reserved: 800,
            paid: 0
        }
    );
    f.client.set_supplier_reward(&f.admin, &bob, &200);
    assert_eq!(f.client.claim_supplier_rewards(&alice), 800);
    assert_eq!(f.client.claim_supplier_rewards(&bob), 200);
}

#[test]
fn disabling_allocations_and_withdrawing_do_not_confiscate_vested_rewards() {
    let (f, asset, alice, _) = funded();
    f.client.set_supplier_reward(&f.admin, &alice, &300);
    f.client
        .configure_supplier_rewards(&f.admin, &asset, &false);
    assert_eq!(
        f.client.try_set_supplier_reward(&f.admin, &alice, &400),
        Err(Ok(Error::Disabled))
    );
    f.client.withdraw(&alice, &f.collateral_asset, &500);
    f.client.set_supplier_reward(&f.admin, &alice, &300);
    assert_eq!(f.client.claim_supplier_rewards(&alice), 300);
    f.client.configure_supplier_rewards(&f.admin, &asset, &true);
    assert_eq!(
        f.client.try_set_supplier_reward(&f.admin, &alice, &400),
        Err(Ok(Error::NotSupplier))
    );
}

#[test]
fn configuration_rejects_non_admin_and_token_replacement() {
    let (f, asset, alice, _) = funded();
    assert_eq!(
        f.client
            .try_configure_supplier_rewards(&alice, &asset, &false),
        Err(Ok(Error::Unauthorized))
    );
    let other = Address::generate(&f.env);
    assert_eq!(
        f.client
            .try_configure_supplier_rewards(&f.admin, &other, &true),
        Err(Ok(Error::TokenImmutable))
    );
    assert_eq!(
        f.client
            .try_configure_supplier_rewards(&f.admin, &f.client.address, &true),
        Err(Ok(Error::InvalidAddress))
    );
    assert_eq!(f.client.get_supplier_rewards_config().unwrap().token, asset);
}

#[test]
fn only_the_admin_can_allocate_and_only_suppliers_are_eligible() {
    let (f, _, alice, _) = funded();
    assert_eq!(
        f.client.try_set_supplier_reward(&alice, &alice, &100),
        Err(Ok(Error::Unauthorized))
    );
    let stranger = Address::generate(&f.env);
    assert_eq!(
        f.client.try_set_supplier_reward(&f.admin, &stranger, &100),
        Err(Ok(Error::NotSupplier))
    );
    assert_eq!(
        f.client.try_set_supplier_reward(&f.admin, &alice, &-1),
        Err(Ok(Error::InvalidAmount))
    );
    assert_eq!(f.client.get_supplier_reward_pool().available, 1_000);
}

#[test]
fn every_mutation_requires_the_actual_signer() {
    let (f, asset, alice, _) = funded();
    f.client.set_supplier_reward(&f.admin, &alice, &100);
    f.env.set_auths(&[]);
    assert!(f
        .client
        .try_configure_supplier_rewards(&f.admin, &asset, &false)
        .is_err());
    assert!(f
        .client
        .try_set_supplier_reward(&f.admin, &alice, &200)
        .is_err());
    assert!(f.client.try_fund_supplier_rewards(&f.admin, &100).is_err());
    assert!(f.client.try_claim_supplier_rewards(&alice).is_err());
    assert_eq!(f.client.get_supplier_reward(&alice).claimed, 0);
}

#[test]
fn failed_funding_does_not_credit_the_budget() {
    let (f, asset, alice, _) = funded();
    let before = f.client.get_supplier_reward_pool();
    assert!(f.client.try_fund_supplier_rewards(&alice, &100).is_err());
    assert_eq!(f.client.get_supplier_reward_pool(), before);
    assert_eq!(
        token::Client::new(&f.env, &asset).balance(&f.client.address),
        1_000
    );
    assert_eq!(
        f.client.try_fund_supplier_rewards(&f.admin, &0),
        Err(Ok(Error::InvalidAmount))
    );
    assert_eq!(
        f.client.try_fund_supplier_rewards(&f.admin, &-1),
        Err(Ok(Error::InvalidAmount))
    );
}

#[test]
fn claims_require_configuration_and_do_not_require_a_current_deposit() {
    let f = common::setup();
    let user = Address::generate(&f.env);
    assert_eq!(
        f.client.try_claim_supplier_rewards(&user),
        Err(Ok(Error::NotConfigured))
    );
    assert_eq!(
        f.client.try_fund_supplier_rewards(&f.admin, &100),
        Err(Ok(Error::NotConfigured))
    );
    assert_eq!(
        f.client.try_set_supplier_reward(&f.admin, &user, &100),
        Err(Ok(Error::NotConfigured))
    );
    assert_eq!(f.client.get_supplier_rewards_config(), None);
    assert_eq!(
        f.client.get_supplier_reward(&user),
        SupplierReward::default()
    );
}
