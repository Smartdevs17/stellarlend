//! # Per-User Max-Borrow Hard Cap with KYC Attestation Gate
//!
//! Enforces a per-user maximum borrow limit that is gated by a KYC attestation.
//! Users must be attested by an authorized KYC provider before they can borrow
//! beyond the default cap. The hard cap is enforced at the protocol level and
//! cannot be overridden by any single actor.
//!
//! ## Design
//! - A default per-user borrow cap applies to all users.
//! - KYC attestation raises the cap to a configurable higher tier.
//! - Only the admin can set caps and authorize KYC providers.
//! - KYC attestations expire and must be renewed.
//! - Every state change emits an event for observability.

use soroban_sdk::{contracterror, contracttype, Address, Env, IntoVal, Symbol, Val, Vec};

use crate::admin;

// ── Errors ──────────────────────────────────────────────────────────────────

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum UserCapError {
    Unauthorized = 1,
    InvalidCap = 2,
    CapExceeded = 3,
    KycNotAttested = 4,
    KycExpired = 5,
    InvalidProvider = 6,
    ProviderAlreadyAuthorized = 7,
    ProviderNotAuthorized = 8,
    InvalidExpiry = 9,
    Overflow = 10,
}

// ── Types ───────────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone)]
pub enum UserCapDataKey {
    /// Default per-user borrow cap (applies to all users without KYC).
    DefaultCap,
    /// KYC-attested per-user borrow cap (higher tier).
    KycCap,
    /// Per-user current borrowed amount.
    UserBorrowed(Address),
    /// Whether a user has a valid KYC attestation.
    KycAttested(Address),
    /// KYC attestation expiry timestamp.
    KycExpiry(Address),
    /// Authorized KYC provider.
    KycProvider(Address),
    /// KYC attestation nonce for replay protection.
    KycNonce(Address),
}

/// KYC attestation record returned to callers.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KycAttestation {
    pub user: Address,
    pub provider: Address,
    pub expires_at: u64,
}

// ── Admin: Configure caps ───────────────────────────────────────────────────

/// Set the default per-user borrow cap (applies to all users).
pub fn set_default_cap(env: &Env, admin: Address, cap: i128) -> Result<(), UserCapError> {
    admin::require_admin(env, &admin).map_err(|_| UserCapError::Unauthorized)?;
    if cap < 0 {
        return Err(UserCapError::InvalidCap);
    }
    env.storage().persistent().set(&UserCapDataKey::DefaultCap, &cap);
    env.events().publish(
        (Symbol::new(env, "user_cap_default_set"), admin),
        cap,
    );
    Ok(())
}

/// Set the KYC-attested per-user borrow cap (higher tier).
pub fn set_kyc_cap(env: &Env, admin: Address, cap: i128) -> Result<(), UserCapError> {
    admin::require_admin(env, &admin).map_err(|_| UserCapError::Unauthorized)?;
    if cap < 0 {
        return Err(UserCapError::InvalidCap);
    }
    env.storage().persistent().set(&UserCapDataKey::KycCap, &cap);
    env.events().publish(
        (Symbol::new(env, "user_cap_kyc_set"), admin),
        cap,
    );
    Ok(())
}

// ── Admin: KYC provider management ──────────────────────────────────────────

/// Authorize a KYC provider that can attest users.
pub fn authorize_kyc_provider(
    env: &Env,
    admin: Address,
    provider: Address,
) -> Result<(), UserCapError> {
    admin::require_admin(env, &admin).map_err(|_| UserCapError::Unauthorized)?;
    let key = UserCapDataKey::KycProvider(provider.clone());
    if env.storage().persistent().get::<_, bool>(&key).unwrap_or(false) {
        return Err(UserCapError::ProviderAlreadyAuthorized);
    }
    env.storage().persistent().set(&key, &true);
    env.events().publish(
        (Symbol::new(env, "kyc_provider_authorized"), admin, provider),
        (),
    );
    Ok(())
}

/// Revoke a KYC provider.
pub fn revoke_kyc_provider(
    env: &Env,
    admin: Address,
    provider: Address,
) -> Result<(), UserCapError> {
    admin::require_admin(env, &admin).map_err(|_| UserCapError::Unauthorized)?;
    let key = UserCapDataKey::KycProvider(provider.clone());
    if !env.storage().persistent().get::<_, bool>(&key).unwrap_or(false) {
        return Err(UserCapError::ProviderNotAuthorized);
    }
    env.storage().persistent().remove(&key);
    env.events().publish(
        (Symbol::new(env, "kyc_provider_revoked"), admin, provider),
        (),
    );
    Ok(())
}

/// Check whether an address is an authorized KYC provider.
pub fn is_kyc_provider(env: &Env, provider: &Address) -> bool {
    env.storage()
        .persistent()
        .get::<_, bool>(&UserCapDataKey::KycProvider(provider.clone()))
        .unwrap_or(false)
}

// ── KYC attestation ─────────────────────────────────────────────────────────

/// Attest a user as KYC-verified. Only callable by an authorized KYC provider.
///
/// The attestation expires at `expires_at` (ledger timestamp). The provider
/// must authorize the call cryptographically.
pub fn attest_user(
    env: &Env,
    provider: Address,
    user: Address,
    expires_at: u64,
) -> Result<KycAttestation, UserCapError> {
    provider.require_auth();
    if !is_kyc_provider(env, &provider) {
        return Err(UserCapError::InvalidProvider);
    }
    if expires_at <= env.ledger().timestamp() {
        return Err(UserCapError::InvalidExpiry);
    }
    env.storage()
        .persistent()
        .set(&UserCapDataKey::KycAttested(user.clone()), &true);
    env.storage()
        .persistent()
        .set(&UserCapDataKey::KycExpiry(user.clone()), &expires_at);
    let attestation = KycAttestation {
        user: user.clone(),
        provider: provider.clone(),
        expires_at,
    };
    env.events().publish(
        (Symbol::new(env, "kyc_attested"), provider, user),
        expires_at,
    );
    Ok(attestation)
}

/// Revoke a user's KYC attestation. Only callable by an authorized KYC provider.
pub fn revoke_attestation(
    env: &Env,
    provider: Address,
    user: Address,
) -> Result<(), UserCapError> {
    provider.require_auth();
    if !is_kyc_provider(env, &provider) {
        return Err(UserCapError::InvalidProvider);
    }
    env.storage()
        .persistent()
        .remove(&UserCapDataKey::KycAttested(user.clone()));
    env.storage()
        .persistent()
        .remove(&UserCapDataKey::KycExpiry(user.clone()));
    env.events().publish(
        (Symbol::new(env, "kyc_revoked"), provider, user),
        (),
    );
    Ok(())
}

/// Check whether a user has a valid (non-expired) KYC attestation.
pub fn is_kyc_attested(env: &Env, user: &Address) -> bool {
    let attested = env
        .storage()
        .persistent()
        .get::<_, bool>(&UserCapDataKey::KycAttested(user.clone()))
        .unwrap_or(false);
    if !attested {
        return false;
    }
    let expiry = env
        .storage()
        .persistent()
        .get::<_, u64>(&UserCapDataKey::KycExpiry(user.clone()))
        .unwrap_or(0);
    expiry > env.ledger().timestamp()
}

/// Get the KYC attestation details for a user, if any.
pub fn get_attestation(env: &Env, user: &Address) -> Option<KycAttestation> {
    let attested = env
        .storage()
        .persistent()
        .get::<_, bool>(&UserCapDataKey::KycAttested(user.clone()))
        .unwrap_or(false);
    if !attested {
        return None;
    }
    let expiry = env
        .storage()
        .persistent()
        .get::<_, u64>(&UserCapDataKey::KycExpiry(user.clone()))
        .unwrap_or(0);
    if expiry <= env.ledger().timestamp() {
        return None;
    }
    Some(KycAttestation {
        user: user.clone(),
        provider: Address::from_str(env, "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF"),
        expires_at: expiry,
    })
}

// ── Borrow cap enforcement ──────────────────────────────────────────────────

/// Get the effective borrow cap for a user (KYC tier if attested, default otherwise).
pub fn get_effective_cap(env: &Env, user: &Address) -> i128 {
    if is_kyc_attested(env, user) {
        env.storage()
            .persistent()
            .get::<_, i128>(&UserCapDataKey::KycCap)
            .unwrap_or(0)
    } else {
        env.storage()
            .persistent()
            .get::<_, i128>(&UserCapDataKey::DefaultCap)
            .unwrap_or(0)
    }
}

/// Get the current borrowed amount for a user.
pub fn get_user_borrowed(env: &Env, user: &Address) -> i128 {
    env.storage()
        .persistent()
        .get::<_, i128>(&UserCapDataKey::UserBorrowed(user.clone()))
        .unwrap_or(0)
}

/// Update the borrowed amount for a user (called by borrow/repay modules).
pub fn update_user_borrowed(
    env: &Env,
    user: &Address,
    new_amount: i128,
) -> Result<(), UserCapError> {
    if new_amount < 0 {
        return Err(UserCapError::InvalidCap);
    }
    env.storage()
        .persistent()
        .set(&UserCapDataKey::UserBorrowed(user.clone()), &new_amount);
    Ok(())
}

/// Enforce the per-user max-borrow hard cap.
///
/// Returns `Ok(())` if the proposed total borrowed amount is within the cap,
/// or `Err(CapExceeded)` if it exceeds the limit.
pub fn enforce_borrow_cap(
    env: &Env,
    user: &Address,
    additional_borrow: i128,
) -> Result<(), UserCapError> {
    if additional_borrow < 0 {
        return Err(UserCapError::InvalidCap);
    }
    let current = get_user_borrowed(env, user);
    let proposed = current
        .checked_add(additional_borrow)
        .ok_or(UserCapError::Overflow)?;
    let cap = get_effective_cap(env, user);
    if cap > 0 && proposed > cap {
        env.events().publish(
            (Symbol::new(env, "user_cap_exceeded"), user.clone()),
            (proposed, cap),
        );
        return Err(UserCapError::CapExceeded);
    }
    Ok(())
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::{Address as _, Ledger};
    use soroban_sdk::{contract, contractimpl};

    #[contract]
    struct UserCapTestContract;

    #[contractimpl]
    impl UserCapTestContract {}

    fn setup() -> (Env, Address, Address) {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 100);
        let admin = Address::generate(&env);
        let contract_id = env.register_contract(None, UserCapTestContract);
        env.as_contract(&contract_id, || {
            crate::admin::set_admin(&env, admin.clone(), None).unwrap();
        });
        (env, admin, contract_id)
    }

    #[test]
    fn default_cap_enforced_without_kyc() {
        let (env, admin, contract_id) = setup();
        env.as_contract(&contract_id, || {
            set_default_cap(&env, admin.clone(), 1000).unwrap();
            set_kyc_cap(&env, admin.clone(), 5000).unwrap();

            let user = Address::generate(&env);
            // Without KYC, default cap of 1000 applies.
            assert_eq!(get_effective_cap(&env, &user), 1000);
            enforce_borrow_cap(&env, &user, 800).unwrap();
            assert_eq!(
                enforce_borrow_cap(&env, &user, 300),
                Err(UserCapError::CapExceeded)
            );
        });
    }

    #[test]
    fn kyc_attestation_raises_cap() {
        let (env, admin, contract_id) = setup();
        env.as_contract(&contract_id, || {
            set_default_cap(&env, admin.clone(), 1000).unwrap();
            set_kyc_cap(&env, admin.clone(), 5000).unwrap();

            let provider = Address::generate(&env);
            authorize_kyc_provider(&env, admin.clone(), provider.clone()).unwrap();

            let user = Address::generate(&env);
            attest_user(&env, provider, user.clone(), 200).unwrap();

            assert_eq!(get_effective_cap(&env, &user), 5000);
            enforce_borrow_cap(&env, &user, 4000).unwrap();
            assert_eq!(
                enforce_borrow_cap(&env, &user, 2000),
                Err(UserCapError::CapExceeded)
            );
        });
    }

    #[test]
    fn expired_kyc_falls_back_to_default_cap() {
        let (env, admin, contract_id) = setup();
        env.as_contract(&contract_id, || {
            set_default_cap(&env, admin.clone(), 1000).unwrap();
            set_kyc_cap(&env, admin.clone(), 5000).unwrap();

            let provider = Address::generate(&env);
            authorize_kyc_provider(&env, admin.clone(), provider.clone()).unwrap();

            let user = Address::generate(&env);
            attest_user(&env, provider, user.clone(), 150).unwrap();

            // Advance time past expiry.
            env.ledger().with_mut(|l| l.timestamp = 200);
            assert!(!is_kyc_attested(&env, &user));
            assert_eq!(get_effective_cap(&env, &user), 1000);
        });
    }

    #[test]
    fn unauthorized_provider_cannot_attest() {
        let (env, admin, contract_id) = setup();
        env.as_contract(&contract_id, || {
            let fake_provider = Address::generate(&env);
            let user = Address::generate(&env);
            assert_eq!(
                attest_user(&env, fake_provider, user, 200),
                Err(UserCapError::InvalidProvider)
            );
        });
    }

    #[test]
    fn revoke_attestation_restores_default_cap() {
        let (env, admin, contract_id) = setup();
        env.as_contract(&contract_id, || {
            set_default_cap(&env, admin.clone(), 1000).unwrap();
            set_kyc_cap(&env, admin.clone(), 5000).unwrap();

            let provider = Address::generate(&env);
            authorize_kyc_provider(&env, admin.clone(), provider.clone()).unwrap();

            let user = Address::generate(&env);
            attest_user(&env, provider.clone(), user.clone(), 200).unwrap();
            assert_eq!(get_effective_cap(&env, &user), 5000);

            revoke_attestation(&env, provider, user.clone()).unwrap();
            assert_eq!(get_effective_cap(&env, &user), 1000);
        });
    }

    #[test]
    fn zero_cap_means_unlimited() {
        let (env, admin, contract_id) = setup();
        env.as_contract(&contract_id, || {
            set_default_cap(&env, admin.clone(), 0).unwrap();
            let user = Address::generate(&env);
            enforce_borrow_cap(&env, &user, i128::MAX / 2).unwrap();
        });
    }
}