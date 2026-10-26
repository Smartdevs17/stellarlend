use soroban_sdk::{contracttype, Address, Env, Symbol};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AdminError {
    Unauthorized,
    InvalidAdmin,
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
}

pub fn get_admin(env: &Env) -> Option<Address> {
    env.storage()
        .persistent()
        .get::<_, Address>(&AdminDataKey::Admin)
}

/// Returns whether `caller` holds `role`. The protocol admin implicitly
/// holds every role; additional role grants are not persisted yet.
pub fn has_role(env: &Env, _role: Symbol, caller: Address) -> bool {
    get_admin(env).map(|a| a == caller).unwrap_or(false)
}
