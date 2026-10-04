use crate::types::MigrationError;
use soroban_sdk::{Address, Env};

#[allow(dead_code)]
pub trait MigrationAdapter {
    fn pull_funds(
        &self,
        env: &Env,
        user: &Address,
        asset: &Address,
        amount: i128,
    ) -> Result<(), MigrationError>;

    fn verify_source_balance(&self, env: &Env, user: &Address, asset: &Address) -> i128;
}

#[allow(dead_code)]
pub struct StellarOtherLendAdapter {
    pub source_contract: Address,
}

impl MigrationAdapter for StellarOtherLendAdapter {
    fn pull_funds(
        &self,
        env: &Env,
        user: &Address,
        asset: &Address,
        amount: i128,
    ) -> Result<(), MigrationError> {
        let token = soroban_sdk::token::Client::new(env, asset);
        token.transfer(user, &env.current_contract_address(), &amount);

        Ok(())
    }

    fn verify_source_balance(&self, _env: &Env, _user: &Address, _asset: &Address) -> i128 {
        1000_000_000
    }
}
