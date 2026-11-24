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
    TargetNotAuthorized = 6,
    RoleAlreadyAssigned = 7,
    RoleNotAssigned = 8,
}

#[contracttype]
#[derive(Clone)]
pub enum PauseDataKey {
    Guardian(Address),
    OverrideNonce(Address),
    /// Role-scoped target for a guardian (e.g., "borrow", "withdraw", "liquidate").
    GuardianTarget(Address, Symbol),
    /// List of targets a guardian is scoped to.
    GuardianTargets(Address),
}

/// Assign a role-scoped target to a guardian. The guardian can only pause
/// operations matching this target.
pub fn assign_guardian_role(
    env: &Env,
    admin: Address,
    guardian: Address,
    target: Symbol,
) -> Result<(), PauseGuardianError> {
    crate::admin::require_admin(env, &admin).map_err(|_| PauseGuardianError::Unauthorized)?;
    if !is_guardian(env, &guardian) {
        return Err(PauseGuardianError::GuardianNotFound);
    }
    let key = PauseDataKey::GuardianTarget(guardian.clone(), target.clone());
    if env.storage().persistent().get::<_, bool>(&key).unwrap_or(false) {
        return Err(PauseGuardianError::RoleAlreadyAssigned);
    }
    env.storage().persistent().set(&key, &true);

    let mut targets: Vec<Symbol> = env
        .storage()
        .persistent()
        .get(&PauseDataKey::GuardianTargets(guardian.clone()))
        .unwrap_or(Vec::new(env));
    targets.push_back(target.clone());
    env.storage()
        .persistent()
        .set(&PauseDataKey::GuardianTargets(guardian.clone()), &targets);

    env.events().publish(
        (Symbol::new(env, "guardian_role_assigned"), admin, guardian, target),
        (),
    );
    Ok(())
}

/// Revoke a role-scoped target from a guardian.
pub fn revoke_guardian_role(
    env: &Env,
    admin: Address,
    guardian: Address,
    target: Symbol,
) -> Result<(), PauseGuardianError> {
    crate::admin::require_admin(env, &admin).map_err(|_| PauseGuardianError::Unauthorized)?;
    let key = PauseDataKey::GuardianTarget(guardian.clone(), target.clone());
    if !env.storage().persistent().get::<_, bool>(&key).unwrap_or(false) {
        return Err(PauseGuardianError::RoleNotAssigned);
    }
    env.storage().persistent().remove(&key);

    let targets: Vec<Symbol> = env
        .storage()
        .persistent()
        .get(&PauseDataKey::GuardianTargets(guardian.clone()))
        .unwrap_or(Vec::new(env));
    let mut updated = Vec::new(env);
    for t in targets.iter() {
        if t != target {
            updated.push_back(t);
        }
    }
    env.storage()
        .persistent()
        .set(&PauseDataKey::GuardianTargets(guardian.clone()), &updated);

    env.events().publish(
        (Symbol::new(env, "guardian_role_revoked"), admin, guardian, target),
        (),
    );
    Ok(())
}

/// Get all targets a guardian is scoped to.
pub fn get_guardian_targets(env: &Env, guardian: &Address) -> Vec<Symbol> {
    env.storage()
        .persistent()
        .get(&PauseDataKey::GuardianTargets(guardian.clone()))
        .unwrap_or(Vec::new(env))
}

/// Check whether a guardian is scoped to a specific target.
pub fn is_guardian_for_target(env: &Env, guardian: &Address, target: &Symbol) -> bool {
    env.storage()
        .persistent()
        .get::<_, bool>(&PauseDataKey::GuardianTarget(guardian.clone(), target.clone()))
        .unwrap_or(false)
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

/// Require a guardian override scoped to a specific target.
///
/// The guardian must be assigned to the target role. If the guardian has no
/// role assignments at all, they are treated as a global guardian (backward
/// compatible). If they have roles but not this one, the override is rejected.
pub fn require_guardian_override_for_target(
    env: &Env,
    guardian: &Address,
    target: &Symbol,
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

    let targets = get_guardian_targets(env, guardian);
    // If guardian has no role assignments, treat as global guardian.
    if targets.len() > 0 && !is_guardian_for_target(env, guardian, target) {
        return Err(PauseGuardianError::TargetNotAuthorized);
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
    auth_args.push_back(target.clone().into_val(env));
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

    #[test]
    fn role_scoped_guardian_can_only_pause_assigned_target() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 100);
        let contract = env.register_contract(None, PauseTestContract);
        let admin = Address::generate(&env);
        let guardian = Address::generate(&env);

        env.as_contract(&contract, || {
            crate::admin::set_admin(&env, admin.clone(), None).unwrap();
            add_guardian(&env, admin.clone(), guardian.clone()).unwrap();

            let borrow_target = Symbol::new(&env, "borrow");
            let withdraw_target = Symbol::new(&env, "withdraw");
            assign_guardian_role(&env, admin.clone(), guardian.clone(), borrow_target.clone()).unwrap();

            // Guardian can pause borrow.
            require_guardian_override_for_target(
                &env, &guardian, &borrow_target, true, 0, 200, Vec::new(&env),
            ).unwrap();

            // Guardian cannot pause withdraw (not assigned).
            assert_eq!(
                require_guardian_override_for_target(
                    &env, &guardian, &withdraw_target, true, 1, 200, Vec::new(&env),
                ),
                Err(PauseGuardianError::TargetNotAuthorized)
            );
        });
    }

    #[test]
    fn global_guardian_without_roles_can_pause_anything() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 100);
        let contract = env.register_contract(None, PauseTestContract);
        let admin = Address::generate(&env);
        let guardian = Address::generate(&env);

        env.as_contract(&contract, || {
            crate::admin::set_admin(&env, admin.clone(), None).unwrap();
            add_guardian(&env, admin, guardian.clone()).unwrap();

            // No roles assigned → global guardian.
            let target = Symbol::new(&env, "borrow");
            require_guardian_override_for_target(
                &env, &guardian, &target, true, 0, 200, Vec::new(&env),
            ).unwrap();
        });
    }

    #[test]
    fn revoke_role_restores_global_access() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 100);
        let contract = env.register_contract(None, PauseTestContract);
        let admin = Address::generate(&env);
        let guardian = Address::generate(&env);

        env.as_contract(&contract, || {
            crate::admin::set_admin(&env, admin.clone(), None).unwrap();
            add_guardian(&env, admin.clone(), guardian.clone()).unwrap();

            let target = Symbol::new(&env, "borrow");
            assign_guardian_role(&env, admin.clone(), guardian.clone(), target.clone()).unwrap();
            assert_eq!(get_guardian_targets(&env, &guardian).len(), 1);

            revoke_guardian_role(&env, admin, guardian.clone(), target).unwrap();
            assert_eq!(get_guardian_targets(&env, &guardian).len(), 0);
        });
    }

    #[test]
    fn duplicate_role_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 100);
        let contract = env.register_contract(None, PauseTestContract);
        let admin = Address::generate(&env);
        let guardian = Address::generate(&env);

        env.as_contract(&contract, || {
            crate::admin::set_admin(&env, admin.clone(), None).unwrap();
            add_guardian(&env, admin.clone(), guardian.clone()).unwrap();

            let target = Symbol::new(&env, "borrow");
            assign_guardian_role(&env, admin.clone(), guardian.clone(), target.clone()).unwrap();
            assert_eq!(
                assign_guardian_role(&env, admin, guardian, target),
                Err(PauseGuardianError::RoleAlreadyAssigned)
            );
        });
    }
}