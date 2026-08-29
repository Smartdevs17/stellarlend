use soroban_sdk::{Address, BytesN, Env, Symbol, Val, Vec};

pub(crate) const ADMIN_KEY: &str = "Admin";

pub(crate) fn get_admin(env: &Env) -> Address {
    env.storage()
        .instance()
        .get(&ADMIN_KEY)
        .expect("migration-hub not initialized")
}

pub(crate) fn require_admin(env: &Env) {
    let admin = get_admin(env);
    admin.require_auth();
}

pub fn bootstrap(
    env: Env,
    salt: BytesN<32>,
    wasm_hash: BytesN<32>,
    init_fn: Symbol,
    init_args: Vec<Val>,
) -> Address {
    require_admin(&env);
    let deployed_address = env
        .deployer()
        .with_address(env.current_contract_address(), salt)
        .deploy(wasm_hash);
    env.invoke_contract::<)>(&deployed_address, &init_fn, init_args);
    deployed_address
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{
        contractimpl, testutils::Address as _, Address, BytesN, Env, Symbol, Vec, vec,
    };

    pub struct MockChild;

    #contractimpl]
    impl MockChild {
        pub fn initialize(env: Env, admin: Address, value: u32) {
            env.storage().instance().set(&&"admin", &admin);
            env.storage().instance().set(&&"value", &value);
        }

        pub fn value(env: Env) -> u32 {
            env.storage().instance().get(&&"value").unwrap()
        }
    }

    #[test]
    fn test_bootstrap_deploys_and_initializes() {
        let mut env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        env.storage().instance().set(&ADMIN_KEY, &admin);

        let mock_id = env.register_contract(None, MockChild);
        let wasm_hash = env.get_contract_wasm_hash(&mock_id);

        let salt = BytesN::from_array(&env, &[7u8; 32]);
        let init_fn = Symbol::new(&env, "initialize");
        let init_args = vec!&env, admin.clone(), 42u32);

        let new_contract = bootstrap(env.clone(), salt, wasm_hash, init_fn, init_args);

        let value: u32 = env.invoke_contract(&new_contract, &Symbol::new(&env, "value"), ());
        assert_eq!(value, 42);
    }

    #[test]
    fn test_bootstrap_rejects_non_admin() {
        let mut env = Env::default();
        let admin = Address::generate(&env);
        let user = Address::generate(&env);
        env.storage().instance().set(&ADMIN_KEY, &admin);
        env.set_source_account(&user);

        let mock_id = env.register_contract(None, MockChild);
        let wasm_hash = env.get_contract_wasm_hash(&mock_id);
        let salt = BytesN::from_array(&env, &[10u8; 32]);
        let init_fn = Symbol::new(&env, "initialize");
        let init_args = vec!&env, admin.clone(), 42u32);

        let result = std::panic::catch_unwind(<| {
            let _ = bootstrap(env.clone(), salt, wasm_hash, init_fn, init_args);
        });
        assert!(result.is_err());
    }

    #[test]
    fn test_get_admin_panics_when_uninitialized() {
        let env = Env::default();
        let result = std::panic::catch_unwind(<< {
            let _ = get_admin(&env);
        });
        assert!(result.is_err());
    }
}
