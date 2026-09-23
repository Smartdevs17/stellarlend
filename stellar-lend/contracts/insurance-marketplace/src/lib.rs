#![no_std]
use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, token, Address, Env, String, Vec,
};

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotFound = 2,
    Unauthorized = 3,
    InvalidAmount = 4,
    Insolvent = 5,
    Expired = 6,
    TriggerNotCovered = 7,
    AlreadyClaimed = 8,
    NotExpired = 9,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct Provider {
    pub owner: Address,
    pub collateral_token: Address,
    pub collateral: i128,
    pub available: i128,
    pub kyc_approved: bool,
    pub rating: u32,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct Policy {
    pub id: u64,
    pub provider: Address,
    pub coverage: i128,
    pub premium_bps: u32,
    pub duration_ledgers: u32,
    pub terms: String,
    pub triggers: Vec<u32>,
    pub active: bool,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct Coverage {
    pub id: u64,
    pub policy_id: u64,
    pub lender: Address,
    pub position_id: u64,
    pub amount: i128,
    pub expires_at: u32,
    pub claimed: bool,
}

#[contracttype]
#[derive(Clone)]
enum Key {
    Admin,
    Provider(Address),
    Policy(u64),
    Coverage(u64),
    Nonce,
}

#[contract]
pub struct InsuranceMarketplace;

#[contractimpl]
impl InsuranceMarketplace {
    pub fn initialize(env: Env, admin: Address) -> Result<(), Error> {
        if env.storage().instance().has(&Key::Admin) {
            return Err(Error::AlreadyInitialized);
        }
        admin.require_auth();
        env.storage().instance().set(&Key::Admin, &admin);
        env.storage().instance().set(&Key::Nonce, &0u64);
        Ok(())
    }

    pub fn set_kyc_status(env: Env, provider: Address, approved: bool) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&Key::Admin)
            .ok_or(Error::NotFound)?;
        admin.require_auth();
        let mut p: Provider = env
            .storage()
            .persistent()
            .get(&Key::Provider(provider.clone()))
            .ok_or(Error::NotFound)?;
        p.kyc_approved = approved;
        env.storage().persistent().set(&Key::Provider(provider), &p);
        Ok(())
    }

    pub fn onboard_provider(
        env: Env,
        owner: Address,
        token: Address,
        collateral: i128,
        kyc_approved: bool,
    ) -> Result<(), Error> {
        owner.require_auth();
        if env.storage().persistent().has(&Key::Provider(owner.clone())) {
            return Err(Error::AlreadyInitialized);
        }
        if collateral <= 0 {
            return Err(Error::InvalidAmount);
        }
        if kyc_approved {
            let admin: Address = env
                .storage()
                .instance()
                .get(&Key::Admin)
                .ok_or(Error::NotFound)?;
            admin.require_auth();
        }
        token::Client::new(&env, &token).transfer(
            &owner,
            &env.current_contract_address(),
            &collateral,
        );
        env.storage().persistent().set(
            &Key::Provider(owner.clone()),
            &Provider {
                owner,
                collateral_token: token,
                collateral,
                available: collateral,
                kyc_approved,
                rating: 0,
            },
        );
        Ok(())
    }

    pub fn deposit_collateral(env: Env, provider: Address, amount: i128) -> Result<(), Error> {
        provider.require_auth();
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        let mut p: Provider = env
            .storage()
            .persistent()
            .get(&Key::Provider(provider.clone()))
            .ok_or(Error::NotFound)?;
        token::Client::new(&env, &p.collateral_token).transfer(
            &provider,
            &env.current_contract_address(),
            &amount,
        );
        p.collateral = p.collateral.checked_add(amount).ok_or(Error::InvalidAmount)?;
        p.available = p.available.checked_add(amount).ok_or(Error::InvalidAmount)?;
        env.storage().persistent().set(&Key::Provider(provider), &p);
        Ok(())
    }

    pub fn withdraw_collateral(env: Env, provider: Address, amount: i128) -> Result<(), Error> {
        provider.require_auth();
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        let mut p: Provider = env
            .storage()
            .persistent()
            .get(&Key::Provider(provider.clone()))
            .ok_or(Error::NotFound)?;
        if amount > p.available {
            return Err(Error::Insolvent);
        }
        p.available -= amount;
        p.collateral -= amount;
        token::Client::new(&env, &p.collateral_token).transfer(
            &env.current_contract_address(),
            &provider,
            &amount,
        );
        env.storage().persistent().set(&Key::Provider(provider), &p);
        Ok(())
    }

    pub fn create_policy(
        env: Env,
        provider: Address,
        coverage: i128,
        premium_bps: u32,
        duration_ledgers: u32,
        terms: String,
        triggers: Vec<u32>,
    ) -> Result<u64, Error> {
        provider.require_auth();
        let p: Provider = env
            .storage()
            .persistent()
            .get(&Key::Provider(provider.clone()))
            .ok_or(Error::NotFound)?;
        if !p.kyc_approved {
            return Err(Error::Unauthorized);
        }
        if coverage <= 0 || coverage > p.available {
            return Err(Error::Insolvent);
        }
        let id: u64 = env.storage().instance().get(&Key::Nonce).unwrap_or(0);
        env.storage().instance().set(&Key::Nonce, &(id + 1));
        env.storage().persistent().set(
            &Key::Policy(id),
            &Policy {
                id,
                provider,
                coverage,
                premium_bps,
                duration_ledgers,
                terms,
                triggers,
                active: true,
            },
        );
        Ok(id)
    }

    pub fn purchase(
        env: Env,
        policy_id: u64,
        lender: Address,
        position_id: u64,
        amount: i128,
    ) -> Result<u64, Error> {
        lender.require_auth();
        let mut policy: Policy = env
            .storage()
            .persistent()
            .get(&Key::Policy(policy_id))
            .ok_or(Error::NotFound)?;
        let mut provider: Provider = env
            .storage()
            .persistent()
            .get(&Key::Provider(policy.provider.clone()))
            .ok_or(Error::NotFound)?;
        if !policy.active || amount <= 0 || amount > policy.coverage || amount > provider.available
        {
            return Err(Error::Insolvent);
        }
        let premium = amount
            .checked_mul(policy.premium_bps as i128)
            .ok_or(Error::InvalidAmount)?
            / 10_000;
        if premium <= 0 {
            return Err(Error::InvalidAmount);
        }
        token::Client::new(&env, &provider.collateral_token).transfer(
            &lender,
            &policy.provider,
            &premium,
        );
        provider.available -= amount;
        policy.coverage -= amount;
        if policy.coverage == 0 {
            policy.active = false;
        }
        env.storage()
            .persistent()
            .set(&Key::Policy(policy_id), &policy);
        env.storage()
            .persistent()
            .set(&Key::Provider(policy.provider), &provider);
        let id: u64 = env.storage().instance().get(&Key::Nonce).unwrap_or(0);
        env.storage().instance().set(&Key::Nonce, &(id + 1));
        env.storage().persistent().set(
            &Key::Coverage(id),
            &Coverage {
                id,
                policy_id,
                lender,
                position_id,
                amount,
                expires_at: env
                    .ledger()
                    .sequence()
                    .saturating_add(policy.duration_ledgers),
                claimed: false,
            },
        );
        Ok(id)
    }

    pub fn trigger_claim(env: Env, coverage_id: u64, trigger: u32) -> Result<i128, Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&Key::Admin)
            .ok_or(Error::NotFound)?;
        admin.require_auth();
        let mut coverage: Coverage = env
            .storage()
            .persistent()
            .get(&Key::Coverage(coverage_id))
            .ok_or(Error::NotFound)?;
        if coverage.claimed {
            return Err(Error::AlreadyClaimed);
        }
        if env.ledger().sequence() > coverage.expires_at {
            return Err(Error::Expired);
        }
        let policy: Policy = env
            .storage()
            .persistent()
            .get(&Key::Policy(coverage.policy_id))
            .ok_or(Error::NotFound)?;
        if !policy.triggers.iter().any(|item| item == trigger) {
            return Err(Error::TriggerNotCovered);
        }
        let mut provider: Provider = env
            .storage()
            .persistent()
            .get(&Key::Provider(policy.provider.clone()))
            .ok_or(Error::NotFound)?;
        token::Client::new(&env, &provider.collateral_token).transfer(
            &env.current_contract_address(),
            &coverage.lender,
            &coverage.amount,
        );
        coverage.claimed = true;
        provider.collateral -= coverage.amount;
        env.storage()
            .persistent()
            .set(&Key::Coverage(coverage_id), &coverage);
        env.storage()
            .persistent()
            .set(&Key::Provider(policy.provider), &provider);
        Ok(coverage.amount)
    }

    pub fn expire_coverage(env: Env, coverage_id: u64) -> Result<(), Error> {
        let mut coverage: Coverage = env
            .storage()
            .persistent()
            .get(&Key::Coverage(coverage_id))
            .ok_or(Error::NotFound)?;
        if coverage.claimed {
            return Err(Error::AlreadyClaimed);
        }
        if env.ledger().sequence() <= coverage.expires_at {
            return Err(Error::NotExpired);
        }
        coverage.claimed = true;
        let policy: Policy = env
            .storage()
            .persistent()
            .get(&Key::Policy(coverage.policy_id))
            .ok_or(Error::NotFound)?;
        let mut provider: Provider = env
            .storage()
            .persistent()
            .get(&Key::Provider(policy.provider.clone()))
            .ok_or(Error::NotFound)?;
        provider.available = provider
            .available
            .checked_add(coverage.amount)
            .ok_or(Error::InvalidAmount)?;
        env.storage()
            .persistent()
            .set(&Key::Coverage(coverage_id), &coverage);
        env.storage()
            .persistent()
            .set(&Key::Provider(policy.provider), &provider);
        Ok(())
    }

    pub fn get_policy(env: Env, id: u64) -> Option<Policy> {
        env.storage().persistent().get(&Key::Policy(id))
    }

    pub fn get_coverage(env: Env, id: u64) -> Option<Coverage> {
        env.storage().persistent().get(&Key::Coverage(id))
    }

    pub fn get_provider(env: Env, provider: Address) -> Option<Provider> {
        env.storage().persistent().get(&Key::Provider(provider))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, Ledger as _},
        token, vec, Address, Env, String,
    };

    #[test]
    fn purchases_coverage_and_pays_approved_claim() {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let provider = Address::generate(&env);
        let lender = Address::generate(&env);
        let token_admin = Address::generate(&env);
        let asset = env.register_stellar_asset_contract_v2(token_admin);
        let token_address = asset.address();
        let asset_client = token::StellarAssetClient::new(&env, &token_address);
        asset_client.mint(&provider, &10_000);
        asset_client.mint(&lender, &1_000);

        let contract_address = env.register(InsuranceMarketplace, ());
        let client = InsuranceMarketplaceClient::new(&env, &contract_address);
        client.initialize(&admin);
        client.onboard_provider(&provider, &token_address, &10_000, &true);

        let policy_id = client.create_policy(
            &provider,
            &5_000,
            &100,
            &100,
            &String::from_str(&env, "Oracle and contract risk"),
            &vec![&env, 1],
        );
        let coverage_id = client.purchase(&policy_id, &lender, &42, &5_000);

        assert_eq!(asset_client.balance(&provider), 50);
        assert_eq!(asset_client.balance(&contract_address), 10_000);

        assert_eq!(client.trigger_claim(&coverage_id, &1), 5_000);
        assert_eq!(asset_client.balance(&lender), 5_950);
        assert!(client.get_coverage(&coverage_id).unwrap().claimed);
    }

    #[test]
    fn rejects_uncovered_claim_trigger() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let provider = Address::generate(&env);
        let lender = Address::generate(&env);
        let asset = env.register_stellar_asset_contract_v2(Address::generate(&env));
        let token_address = asset.address();
        let asset_client = token::StellarAssetClient::new(&env, &token_address);
        asset_client.mint(&provider, &10_000);
        asset_client.mint(&lender, &1_000);
        let contract_address = env.register(InsuranceMarketplace, ());
        let client = InsuranceMarketplaceClient::new(&env, &contract_address);
        client.initialize(&admin);
        client.onboard_provider(&provider, &token_address, &10_000, &true);
        let policy_id = client.create_policy(
            &provider,
            &5_000,
            &100,
            &100,
            &String::from_str(&env, "Oracle failures only"),
            &vec![&env, 1],
        );
        let coverage_id = client.purchase(&policy_id, &lender, &42, &5_000);

        assert_eq!(
            client.try_trigger_claim(&coverage_id, &2),
            Err(Ok(Error::TriggerNotCovered))
        );
    }

    #[test]
    fn cannot_re_onboard_existing_provider() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let provider = Address::generate(&env);
        let asset = env.register_stellar_asset_contract_v2(Address::generate(&env));
        let token_address = asset.address();
        let asset_client = token::StellarAssetClient::new(&env, &token_address);
        asset_client.mint(&provider, &20_000);

        let contract_address = env.register(InsuranceMarketplace, ());
        let client = InsuranceMarketplaceClient::new(&env, &contract_address);
        client.initialize(&admin);
        client.onboard_provider(&provider, &token_address, &10_000, &true);

        // Re-onboarding the same provider must be rejected to prevent state overwrite
        assert_eq!(
            client.try_onboard_provider(&provider, &token_address, &5_000, &true),
            Err(Ok(Error::AlreadyInitialized))
        );
    }

    #[test]
    fn policy_coverage_decrements_and_enforces_capacity() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let provider = Address::generate(&env);
        let lender1 = Address::generate(&env);
        let lender2 = Address::generate(&env);
        let asset = env.register_stellar_asset_contract_v2(Address::generate(&env));
        let token_address = asset.address();
        let asset_client = token::StellarAssetClient::new(&env, &token_address);
        asset_client.mint(&provider, &10_000);
        asset_client.mint(&lender1, &1_000);
        asset_client.mint(&lender2, &1_000);

        let contract_address = env.register(InsuranceMarketplace, ());
        let client = InsuranceMarketplaceClient::new(&env, &contract_address);
        client.initialize(&admin);
        client.onboard_provider(&provider, &token_address, &10_000, &true);

        // Create policy with 3,000 coverage limit
        let policy_id = client.create_policy(
            &provider,
            &3_000,
            &100,
            &100,
            &String::from_str(&env, "Test coverage"),
            &vec![&env, 1],
        );

        // Lender 1 purchases 2,000 coverage -> 1,000 remaining on policy
        let _cov1 = client.purchase(&policy_id, &lender1, &1, &2_000);
        assert_eq!(client.get_policy(&policy_id).unwrap().coverage, 1_000);
        assert!(client.get_policy(&policy_id).unwrap().active);

        // Lender 2 tries to purchase 1,500 (> 1,000 remaining) -> must fail with Insolvent
        assert_eq!(
            client.try_purchase(&policy_id, &lender2, &2, &1_500),
            Err(Ok(Error::Insolvent))
        );

        // Lender 2 purchases exact remaining 1,000 -> policy becomes inactive (0 coverage left)
        let _cov2 = client.purchase(&policy_id, &lender2, &2, &1_000);
        assert_eq!(client.get_policy(&policy_id).unwrap().coverage, 0);
        assert!(!client.get_policy(&policy_id).unwrap().active);

        // Subsequent purchase must fail because policy is inactive
        assert_eq!(
            client.try_purchase(&policy_id, &lender1, &3, &500),
            Err(Ok(Error::Insolvent))
        );
    }

    #[test]
    fn deposit_and_withdraw_collateral() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let provider = Address::generate(&env);
        let asset = env.register_stellar_asset_contract_v2(Address::generate(&env));
        let token_address = asset.address();
        let asset_client = token::StellarAssetClient::new(&env, &token_address);
        asset_client.mint(&provider, &20_000);

        let contract_address = env.register(InsuranceMarketplace, ());
        let client = InsuranceMarketplaceClient::new(&env, &contract_address);
        client.initialize(&admin);
        client.onboard_provider(&provider, &token_address, &5_000, &true);

        // Deposit additional 3,000 collateral
        client.deposit_collateral(&provider, &3_000);
        let p = client.get_provider(&provider).unwrap();
        assert_eq!(p.collateral, 8_000);
        assert_eq!(p.available, 8_000);

        // Withdraw 4,000 collateral
        client.withdraw_collateral(&provider, &4_000);
        let p = client.get_provider(&provider).unwrap();
        assert_eq!(p.collateral, 4_000);
        assert_eq!(p.available, 4_000);

        // Cannot withdraw more than available
        assert_eq!(
            client.try_withdraw_collateral(&provider, &5_000),
            Err(Ok(Error::Insolvent))
        );
    }

    #[test]
    fn expire_coverage_restores_provider_available_capital() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let provider = Address::generate(&env);
        let lender = Address::generate(&env);
        let asset = env.register_stellar_asset_contract_v2(Address::generate(&env));
        let token_address = asset.address();
        let asset_client = token::StellarAssetClient::new(&env, &token_address);
        asset_client.mint(&provider, &10_000);
        asset_client.mint(&lender, &1_000);

        let contract_address = env.register(InsuranceMarketplace, ());
        let client = InsuranceMarketplaceClient::new(&env, &contract_address);
        client.initialize(&admin);
        client.onboard_provider(&provider, &token_address, &10_000, &true);

        let policy_id = client.create_policy(
            &provider,
            &5_000,
            &100,
            &50, // 50 ledgers duration
            &String::from_str(&env, "Expiry test"),
            &vec![&env, 1],
        );

        let coverage_id = client.purchase(&policy_id, &lender, &1, &4_000);
        let p_before = client.get_provider(&provider).unwrap();
        assert_eq!(p_before.available, 6_000);

        // Attempting to expire before expiration ledger must fail
        assert_eq!(
            client.try_expire_coverage(&coverage_id),
            Err(Ok(Error::NotExpired))
        );

        // Advance ledger past expiration (50 ledgers + current)
        env.ledger().set_sequence_number(100);

        // Expiration succeeds and restores available capital
        client.expire_coverage(&coverage_id);
        let p_after = client.get_provider(&provider).unwrap();
        assert_eq!(p_after.available, 10_000);

        // Cannot claim after expiration
        assert_eq!(
            client.try_trigger_claim(&coverage_id, &1),
            Err(Ok(Error::AlreadyClaimed))
        );
    }

    #[test]
    fn rejects_zero_premium_purchase() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let provider = Address::generate(&env);
        let lender = Address::generate(&env);
        let asset = env.register_stellar_asset_contract_v2(Address::generate(&env));
        let token_address = asset.address();
        let asset_client = token::StellarAssetClient::new(&env, &token_address);
        asset_client.mint(&provider, &10_000);
        asset_client.mint(&lender, &1_000);

        let contract_address = env.register(InsuranceMarketplace, ());
        let client = InsuranceMarketplaceClient::new(&env, &contract_address);
        client.initialize(&admin);
        client.onboard_provider(&provider, &token_address, &10_000, &true);

        // Policy with very low premium bps (e.g. 1 bps)
        let policy_id = client.create_policy(
            &provider,
            &5_000,
            &1,
            &100,
            &String::from_str(&env, "Zero premium test"),
            &vec![&env, 1],
        );

        // Purchasing amount 50 with 1 bps: 50 * 1 / 10,000 = 0 -> rejected
        assert_eq!(
            client.try_purchase(&policy_id, &lender, &1, &50),
            Err(Ok(Error::InvalidAmount))
        );
    }

    #[test]
    fn admin_set_kyc_status() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let provider = Address::generate(&env);
        let asset = env.register_stellar_asset_contract_v2(Address::generate(&env));
        let token_address = asset.address();
        let asset_client = token::StellarAssetClient::new(&env, &token_address);
        asset_client.mint(&provider, &10_000);

        let contract_address = env.register(InsuranceMarketplace, ());
        let client = InsuranceMarketplaceClient::new(&env, &contract_address);
        client.initialize(&admin);

        // Provider onboards with kyc_approved = false
        client.onboard_provider(&provider, &token_address, &5_000, &false);
        assert!(!client.get_provider(&provider).unwrap().kyc_approved);

        // Cannot create policy without KYC
        assert_eq!(
            client.try_create_policy(
                &provider,
                &1_000,
                &100,
                &100,
                &String::from_str(&env, "KYC check"),
                &vec![&env, 1],
            ),
            Err(Ok(Error::Unauthorized))
        );

        // Admin approves KYC
        client.set_kyc_status(&provider, &true);
        assert!(client.get_provider(&provider).unwrap().kyc_approved);

        // Now policy creation succeeds
        let pid = client.create_policy(
            &provider,
            &1_000,
            &100,
            &100,
            &String::from_str(&env, "KYC check"),
            &vec![&env, 1],
        );
        assert_eq!(pid, 0);
    }
}
