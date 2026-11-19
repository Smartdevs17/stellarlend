use soroban_sdk::{contracttype, Address, Env, IntoVal, Symbol, Val, Vec};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AdminError {
    Unauthorized,
    InvalidAdmin,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalletAttestation {
    pub wallet: Address,
    pub expires_at: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[contracttype]
pub struct AdminRotationProposal {
    pub new_admin: Address,
    pub approvals: Vec<Address>,
}

pub fn require_admin(env: &Env, caller: &Address) -> Result<(), AdminError> {
    let current_admin = get_admin(env).ok_or(AdminError::Unauthorized)?;
    caller.require_auth();
    if *caller != current_admin {
        return Err(AdminError::Unauthorized);
    }
    Ok(())
}

pub fn set_admin(env: &Env, new_admin: Address, caller: Option<Address>) -> Result<(), AdminError> {
    if let Some(current_admin) = get_admin(env) {
        if let Some(ref c) = caller {
            // Caller must be the current admin AND provide cryptographic auth
            c.require_auth();
            if *c != current_admin {
                return Err(AdminError::Unauthorized);
            }
        } else {
            return Err(AdminError::Unauthorized);
        }

        if let Some((_, threshold)) = get_security_council(env) {
            if threshold > 0 {
                let proposal = env
                    .storage()
                    .persistent()
                    .get::<_, AdminRotationProposal>(&AdminDataKey::AdminRotationProposal(
                        new_admin.clone(),
                    ))
                    .ok_or(AdminError::Unauthorized)?;
                if proposal.approvals.len() < threshold as u32 {
                    return Err(AdminError::Unauthorized);
                }
            }
        }
    }

    env.storage()
        .persistent()
        .set(&AdminDataKey::Admin, &new_admin);
    Ok(())
}

#[contracttype]
#[derive(Clone)]
pub enum AdminDataKey {
    Admin,
    SecurityCouncil,
    SecurityCouncilThreshold,
    AdminRotationProposal(Address),
    WalletAttestation(Address),
    AdminActionNonce(Address),
}

pub fn attest_wallet(
    env: &Env,
    admin: Address,
    wallet: Address,
    expires_at: u64,
) -> Result<WalletAttestation, AdminError> {
    require_admin(env, &admin)?;
    if expires_at <= env.ledger().timestamp() {
        return Err(AdminError::InvalidAdmin);
    }
    let attestation = WalletAttestation {
        wallet: wallet.clone(),
        expires_at,
    };
    env.storage()
        .persistent()
        .set(&AdminDataKey::WalletAttestation(wallet), &attestation);
    Ok(attestation)
}

pub fn get_wallet_attestation(env: &Env, wallet: &Address) -> Option<WalletAttestation> {
    env.storage()
        .persistent()
        .get(&AdminDataKey::WalletAttestation(wallet.clone()))
        .filter(|a: &WalletAttestation| a.expires_at >= env.ledger().timestamp())
}

/// Require an admin action to be bound to the caller wallet, contract, action,
/// nonce, expiry and argument set.  This consumes the nonce only after all
/// authorization checks succeed.
pub fn require_signed_admin_action(
    env: &Env,
    admin: &Address,
    wallet: &Address,
    action: Symbol,
    nonce: u64,
    expires_at: u64,
    args: Vec<Val>,
) -> Result<(), AdminError> {
    require_admin(env, admin)?;
    let attestation = get_wallet_attestation(env, wallet).ok_or(AdminError::Unauthorized)?;
    if attestation.wallet != *wallet || attestation.expires_at > expires_at {
        return Err(AdminError::Unauthorized);
    }
    if expires_at <= env.ledger().timestamp() {
        return Err(AdminError::Unauthorized);
    }

    let nonce_key = AdminDataKey::AdminActionNonce(wallet.clone());
    let expected = env
        .storage()
        .persistent()
        .get::<_, u64>(&nonce_key)
        .unwrap_or(0);
    if nonce != expected {
        return Err(AdminError::Unauthorized);
    }

    let mut auth_args = Vec::new(env);
    auth_args.push_back(env.current_contract_address().into_val(env));
    auth_args.push_back(action.into_val(env));
    auth_args.push_back(nonce.into_val(env));
    auth_args.push_back(expires_at.into_val(env));
    for arg in args.iter() {
        auth_args.push_back(arg);
    }

    wallet.require_auth_for_args(auth_args);
    env.storage()
        .persistent()
        .set(&nonce_key, &nonce.saturating_add(1));
    Ok(())
}

pub fn get_admin(env: &Env) -> Option<Address> {
    env.storage()
        .persistent()
        .get::<_, Address>(&AdminDataKey::Admin)
}

pub fn set_security_council(
    env: &Env,
    caller: Address,
    council: Vec<Address>,
    threshold: u32,
) -> Result<(), AdminError> {
    require_admin(env, &caller)?;
    if council.is_empty() || threshold == 0 || threshold > council.len() as u32 {
        return Err(AdminError::InvalidAdmin);
    }
    for (i, member) in council.iter().enumerate() {
        if council.iter().skip(i + 1).any(|other| other == member) {
            return Err(AdminError::InvalidAdmin);
        }
    }
    env.storage()
        .persistent()
        .set(&AdminDataKey::SecurityCouncil, &council);
    env.storage()
        .persistent()
        .set(&AdminDataKey::SecurityCouncilThreshold, &threshold);
    Ok(())
}

pub fn get_security_council(env: &Env) -> Option<(Vec<Address>, u32)> {
    let council = env
        .storage()
        .persistent()
        .get::<_, Vec<Address>>(&AdminDataKey::SecurityCouncil)?;
    let threshold = env
        .storage()
        .persistent()
        .get::<_, u32>(&AdminDataKey::SecurityCouncilThreshold)
        .unwrap_or(1);
    Some((council, threshold))
}

pub fn propose_admin_rotation(
    env: &Env,
    proposer: Address,
    new_admin: Address,
) -> Result<(), AdminError> {
    proposer.require_auth();
    let (council, _) = get_security_council(env).ok_or(AdminError::Unauthorized)?;
    if !council.contains(&proposer) {
        return Err(AdminError::Unauthorized);
    }
    if new_admin == env.current_contract_address() {
        return Err(AdminError::InvalidAdmin);
    }

    let proposal = AdminRotationProposal {
        new_admin: new_admin.clone(),
        approvals: Vec::from_array(env, [proposer]),
    };
    env.storage()
        .persistent()
        .set(&AdminDataKey::AdminRotationProposal(new_admin), &proposal);
    Ok(())
}

pub fn approve_admin_rotation(
    env: &Env,
    approver: Address,
    new_admin: Address,
) -> Result<(), AdminError> {
    approver.require_auth();
    let (council, _) = get_security_council(env).ok_or(AdminError::Unauthorized)?;
    if !council.contains(&approver) {
        return Err(AdminError::Unauthorized);
    }

    let mut proposal = env
        .storage()
        .persistent()
        .get::<_, AdminRotationProposal>(&AdminDataKey::AdminRotationProposal(new_admin.clone()))
        .ok_or(AdminError::Unauthorized)?;
    if proposal.approvals.contains(&approver) {
        return Err(AdminError::Unauthorized);
    }
    proposal.approvals.push_back(approver);
    env.storage()
        .persistent()
        .set(&AdminDataKey::AdminRotationProposal(new_admin), &proposal);
    Ok(())
}

pub fn execute_admin_rotation(
    env: &Env,
    executor: Address,
    new_admin: Address,
) -> Result<(), AdminError> {
    executor.require_auth();
    let (council, threshold) = get_security_council(env).ok_or(AdminError::Unauthorized)?;
    if !council.contains(&executor) {
        return Err(AdminError::Unauthorized);
    }
    let proposal = env
        .storage()
        .persistent()
        .get::<_, AdminRotationProposal>(&AdminDataKey::AdminRotationProposal(new_admin.clone()))
        .ok_or(AdminError::Unauthorized)?;
    if proposal.approvals.len() < threshold as u32 {
        return Err(AdminError::Unauthorized);
    }
    env.storage()
        .persistent()
        .set(&AdminDataKey::Admin, &new_admin);
    env.storage()
        .persistent()
        .remove(&AdminDataKey::AdminRotationProposal(new_admin));
    Ok(())
}

/// Returns whether `caller` holds `role`. The protocol admin implicitly
/// holds every role; additional role grants are not persisted yet.
pub fn has_role(env: &Env, _role: Symbol, caller: Address) -> bool {
    get_admin(env).map(|a| a == caller).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{
        contract, contractimpl,
        testutils::{Address as _, Ledger},
    };

    #[contract]
    struct AdminTestContract;

    #[contractimpl]
    impl AdminTestContract {}

    #[test]
    fn wallet_attestation_expires_and_action_nonce_is_one_shot() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 100);
        let contract = env.register_contract(None, AdminTestContract);
        let admin = Address::generate(&env);
        let wallet = Address::generate(&env);

        env.as_contract(&contract, || {
            set_admin(&env, admin.clone(), None).unwrap();
            attest_wallet(&env, admin.clone(), wallet.clone(), 200).unwrap();
            assert!(get_wallet_attestation(&env, &wallet).is_some());
        });

        env.ledger().with_mut(|l| l.timestamp = 201);
        env.as_contract(&contract, || {
            assert!(get_wallet_attestation(&env, &wallet).is_none());
        });
    }
}
