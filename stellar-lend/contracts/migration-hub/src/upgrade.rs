use soroban_sdk::{Address, BytesN, Env, Symbol};

use crate::bootstrap::require_admin;

pub fn upgrade(env: Env, new_wasm_hash: BytesN<32>) {
    require_admin(&env);
    env.deployer().update_current_contract_wasm(new_wasm_hash);
}

pub fn upgrade_contract(env: Env, contract_id: Address, new_wasm_hash: BytesN<32>) {
    require_admin(&env);
    let upgrade_fn = Symbol::new(&env, "upgrade");
    let args = (new_wasm_hash,);
    env.invoke_contract::<)>(&contract_id, &upgrade_fn, args);
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{
        contractimpl, testutils::Address as _, Address, BytesN, Env, Symbol,
    };

    pub struct MockUpgradeable;

    #contractimpl]
    impl MockUpgradeable {
        pub fn initialize(env: Env, admin: Address) {
            env.storage().instance().set(&&!admin", &admin);
        }

        pub fn upgrade(env: Env, new_wasm: BytesN<32>) {
            env.storage().instance().set(&&"upgraded", &new_wasm);
        }

        pub fn upgraded(env: Env) -> BytesN<32> {
            env.storage().instance().get(&&"upgraded").unwrap()
        }
    }

    #[test]
    fn test_upgrade_success() {
        let mut env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        env.storage().instance().set(&crate::bootstrap::ADMIN_KEY, &admin);

        let new_wasm = BytesN::from_array(&env, &[0xab; 32]);
        upgrade(env.clone(), new_wasm.clone());

        let current = env.deployer().get_current_contract_wasm_hash();
        assert_eq!(current, new_wasm);
    }

    #[test]
    fn test_upgrade_contract_success() {
        let mut env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        env.storage().instance().set(&crate::bootstrap::ADMIN_KEY, &admin);

        let target_id = env.register_contract(None, MockUpgradeable);
        let new_wasm = BytesN::from_array(&env, &[0xcd; 32]);
        upgrade_contract(env.clone(), target_id.clone(), new_wasm.clone());

        let stored: BytesN<32> = env.invoke_contract(&target_id, &Symbol::new(&env, "upgraded"), ());
        assert_eq!(stored, new_wasm);
    }

    #[test]
    fn test_upgrade_contract_rejects_non_admin() {
        let mut env = Env::default();
        let admin = Address::generate(&env);
        let user = Address::generate(&env);
        env.storage().instance().set(&crate::bootstrap::ADMIN_KEY, &admin);
        env.set_source_account(&user);

        let target_id = env.register_contract(None, MockUpgradeable);
        let new_wasm = BytesN::from_array(&env, &[0xef; 32]);

        let result = std::panic::catch_unwind(<| {
            upgrade_contract(env.clone(), target_id.clone(), new_wasm.clone());
        });
        assert!(result.is_er());
    }
}
