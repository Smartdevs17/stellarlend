use soroban_sdk::{contracterror, contracttype, Address, Env, IntoVal, Symbol, Val, Vec};

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum PauseGuardianError {
    Unauthorized = 1,
    GuardianNotFound = 2,
    InvalidOverride = 3,
    BadNonce = 4,
    Expired = 5,
}

#[contracttype]
#[derive(Clone)]
pub enum PauseDataKey {
    Guardian(Address),
    OverrideNonce(Address),
}

pub fn add_guardian(
    env: &Env,
    admin: Address,
    guardian: Address,
) -> Result<(), PauseGuardianError> {
    crate::admin::require_admin(env, &admin).map_err(|_| PauseGuardianError::Unauthorized)?;
    env.storage()
        .persistent()
        .set(&PauseDataKey::Guardian(guardian), &true);
    env.events()
        .publish((Symbol::new(env, "pause_guardian_added"),), admin);
    Ok(())
}

pub fn remove_guardian(
    env: &Env,
    admin: Address,
    guardian: Address,
) -> Result<(), PauseGuardianError> {
    crate::admin::require_admin(env, &admin).map_err(|_| PauseGuardianError::Unauthorized)?;
    env.storage()
        .persistent()
        .remove(&PauseDataKey::Guardian(guardian));
    Ok(())
}

pub fn is_guardian(env: &Env, guardian: &Address) -> bool {
    env.storage()
        .persistent()
        .get::<_, bool>(&PauseDataKey::Guardian(guardian.clone()))
        .unwrap_or(false)
}

pub fn require_guardian_override(
    env: &Env,
    guardian: &Address,
    paused: bool,
    nonce: u64,
    expires_at: u64,
    args: Vec<Val>,
) -> Result<(), PauseGuardianError> {
    if expires_at <= env.ledger().timestamp() {
        return Err(PauseGuardianError::Expired);
    }
    if !is_guardian(env, guardian) {
        return Err(PauseGuardianError::GuardianNotFound);
    }
    let nonce_key = PauseDataKey::OverrideNonce(guardian.clone());
    let expected = env
        .storage()
        .persistent()
        .get::<_, u64>(&nonce_key)
        .unwrap_or(0);
    if nonce != expected {
        return Err(PauseGuardianError::BadNonce);
    }

    let mut auth_args = Vec::new(env);
    auth_args.push_back(env.current_contract_address().into_val(env));
    auth_args.push_back(Symbol::new(env, "emergency_override").into_val(env));
    auth_args.push_back(paused.into_val(env));
    auth_args.push_back(nonce.into_val(env));
    auth_args.push_back(expires_at.into_val(env));
    for arg in args.iter() {
        auth_args.push_back(arg);
    }
    guardian.require_auth_for_args(auth_args);
    env.storage()
        .persistent()
        .set(&nonce_key, &nonce.saturating_add(1));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{contract, contractimpl, testutils::Address as _, testutils::Ledger};

    #[contract]
    struct PauseTestContract;

    #[contractimpl]
    impl PauseTestContract {}

    #[test]
    fn guardian_override_is_nonce_bound_and_expiry_checked() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 100);
        let contract = env.register_contract(None, PauseTestContract);
        let admin = Address::generate(&env);
        let guardian = Address::generate(&env);

        env.as_contract(&contract, || {
            crate::admin::set_admin(&env, admin.clone(), None).unwrap();
            add_guardian(&env, admin, guardian.clone()).unwrap();
            require_guardian_override(&env, &guardian, true, 0, 200, Vec::new(&env)).unwrap();
            assert_eq!(
                require_guardian_override(&env, &guardian, true, 0, 200, Vec::new(&env),),
                Err(PauseGuardianError::BadNonce)
            );
        });
    }

    #[test]
    fn guardian_override_rejects_expired_authorization() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 200);
        let contract = env.register_contract(None, PauseTestContract);
        let guardian = Address::generate(&env);
        env.as_contract(&contract, || {
            assert_eq!(
                require_guardian_override(&env, &guardian, true, 0, 199, Vec::new(&env),),
                Err(PauseGuardianError::Expired)
            );
        });
    }
}
