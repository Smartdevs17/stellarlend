use soroban_sdk::{contracterror, contracttype, Address, BytesN, Env, Symbol};

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum UpgradeError {
    Unauthorized = 1,
    HashNotRegistered = 2,
    InvalidHash = 3,
}

#[contracttype]
#[derive(Clone)]
pub enum UpgradeDataKey {
    RegisteredHash(BytesN<32>),
    RegistryAdmin,
}

pub fn set_registry_admin(env: &Env, caller: Address, admin: Address) -> Result<(), UpgradeError> {
    if let Some(current) = env
        .storage()
        .persistent()
        .get::<_, Address>(&UpgradeDataKey::RegistryAdmin)
    {
        caller.require_auth();
        if caller != current {
            return Err(UpgradeError::Unauthorized);
        }
    } else {
        caller.require_auth();
    }
    env.storage()
        .persistent()
        .set(&UpgradeDataKey::RegistryAdmin, &admin);
    Ok(())
}

pub fn register_hash(
    env: &Env,
    caller: Address,
    wasm_hash: BytesN<32>,
) -> Result<(), UpgradeError> {
    let admin = env
        .storage()
        .persistent()
        .get::<_, Address>(&UpgradeDataKey::RegistryAdmin)
        .ok_or(UpgradeError::Unauthorized)?;
    if caller != admin {
        return Err(UpgradeError::Unauthorized);
    }
    env.storage()
        .persistent()
        .set(&UpgradeDataKey::RegisteredHash(wasm_hash.clone()), &true);
    env.events()
        .publish((Symbol::new(env, "upgrade_hash_registered"),), wasm_hash);
    Ok(())
}

pub fn require_registered_hash(env: &Env, wasm_hash: &BytesN<32>) -> Result<(), UpgradeError> {
    if env
        .storage()
        .persistent()
        .get::<_, bool>(&UpgradeDataKey::RegisteredHash(wasm_hash.clone()))
        .unwrap_or(false)
    {
        Ok(())
    } else {
        Err(UpgradeError::HashNotRegistered)
    }
}

pub fn is_registered_hash(env: &Env, wasm_hash: &BytesN<32>) -> bool {
    require_registered_hash(env, wasm_hash).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{contract, contractimpl, testutils::Address as _};

    #[contract]
    struct UpgradeTestContract;

    #[contractimpl]
    impl UpgradeTestContract {}

    #[test]
    fn upgrade_hash_must_be_registered() {
        let env = Env::default();
        env.mock_all_auths();
        let contract = env.register_contract(None, UpgradeTestContract);
        let hash = BytesN::from_array(&env, &[3u8; 32]);
        env.as_contract(&contract, || {
            assert_eq!(
                require_registered_hash(&env, &hash),
                Err(UpgradeError::HashNotRegistered)
            );
        });
    }

    #[test]
    fn upgrade_hash_registry_accepts_admin_registered_hash() {
        let env = Env::default();
        env.mock_all_auths();
        let contract = env.register_contract(None, UpgradeTestContract);
        let admin = Address::generate(&env);
        let hash = BytesN::from_array(&env, &[7u8; 32]);
        env.as_contract(&contract, || {
            set_registry_admin(&env, admin.clone(), admin.clone()).unwrap();
            register_hash(&env, admin, hash.clone()).unwrap();
            assert!(is_registered_hash(&env, &hash));
        });
    }
}
