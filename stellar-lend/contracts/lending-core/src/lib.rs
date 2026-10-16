#![no_std]

use lending_types::{CommonError, Position, ProtocolConfig, UserPosition};
use soroban_sdk::{contract, contractimpl, Address, Env};

#[contract]
pub struct LendingCoreContract;

#[contractimpl]
impl LendingCoreContract {
    pub fn initialize(env: Env, admin: Address, debt_ceiling: i128) -> Result<(), CommonError> {
        if Self::get_admin(env.clone()).is_some() {
            return Err(CommonError::Unauthorized);
        }

        // The first initializer becomes the protocol administrator. Require
        // that address to authorize the bootstrap so an unrelated caller
        // cannot front-run deployment and take over the contract.
        admin.require_auth();

        let config = ProtocolConfig {
            admin: admin.clone(),
            oracle: None,
            debt_ceiling,
            min_borrow_amount: 100,
            liquidation_threshold_bps: 8_000,
        };

        env.storage().instance().set(&"config", &config);
        Ok(())
    }

    pub fn get_admin(env: Env) -> Option<Address> {
        let config: Option<ProtocolConfig> = env.storage().instance().get(&"config");
        config.map(|c| c.admin)
    }

    pub fn get_position(env: Env, user: Address) -> Option<Position> {
        env.storage().persistent().get(&user)
    }

    pub fn update_position(env: Env, user: Address, position: Position) -> Result<(), CommonError> {
        // Positions are protocol accounting state. Only the configured
        // administrator may write them; otherwise any caller could overwrite
        // an arbitrary user's collateral and debt values.
        let admin = Self::get_admin(env.clone()).ok_or(CommonError::Unauthorized)?;
        admin.require_auth();
        env.storage().persistent().set(&user, &position);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Env};
    use test_utils::TestEnv;

    #[test]
    fn test_initialize() {
        let test_env = TestEnv::new();
        let contract_id = test_env.env.register(LendingCoreContract, ());
        let client = LendingCoreContractClient::new(&test_env.env, &contract_id);

        let result = client.initialize(&test_env.admin, &1_000_000_000);
        assert!(result.is_ok());
    }

    #[test]
    fn test_get_admin() {
        let test_env = TestEnv::new();
        let contract_id = test_env.env.register(LendingCoreContract, ());
        let client = LendingCoreContractClient::new(&test_env.env, &contract_id);

        client.initialize(&test_env.admin, &1_000_000_000);
        let admin = client.get_admin();
        assert_eq!(admin, Some(test_env.admin));
    }

    #[test]
    #[should_panic(expected = "HostError")]
    fn test_initialize_requires_admin_auth() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let contract_id = env.register(LendingCoreContract, ());
        let client = LendingCoreContractClient::new(&env, &contract_id);

        // No authorization is mocked, so an unrelated caller cannot choose
        // the first administrator.
        client.initialize(&admin, &1_000_000_000);
    }

    #[test]
    #[should_panic(expected = "HostError")]
    fn test_update_position_requires_admin_auth() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let user = Address::generate(&env);
        let contract_id = env.register(LendingCoreContract, ());

        // Seed an initialized contract without granting the caller auth so
        // this test isolates the authorization check on update_position.
        env.as_contract(&contract_id, || {
            let config = ProtocolConfig {
                admin: admin.clone(),
                oracle: None,
                debt_ceiling: 1_000_000_000,
                min_borrow_amount: 100,
                liquidation_threshold_bps: 8_000,
            };
            env.storage().instance().set(&"config", &config);
        });

        let client = LendingCoreContractClient::new(&env, &contract_id);
        let position = Position {
            collateral_amount: 1_000,
            debt_amount: 500,
            last_updated: 1,
        };

        // An unrelated caller must not be able to write another user's
        // position.
        client.update_position(&user, &position);
    }

    #[test]
    fn test_admin_can_update_position() {
        let test_env = TestEnv::new();
        let contract_id = test_env.env.register(LendingCoreContract, ());
        let client = LendingCoreContractClient::new(&test_env.env, &contract_id);
        let user = Address::generate(&test_env.env);

        client.initialize(&test_env.admin, &1_000_000_000);
        let position = Position {
            collateral_amount: 1_000,
            debt_amount: 500,
            last_updated: 1,
        };

        client.update_position(&user, &position);
        assert_eq!(client.get_position(&user), Some(position));
    }
}
