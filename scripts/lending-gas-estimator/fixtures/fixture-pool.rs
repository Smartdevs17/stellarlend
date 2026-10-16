// Fixture for the storage indexer tests (#1011).
//
// Deliberately exercises every pattern the indexer has to survive:
//   - direct `env.storage().<tier>().<op>(..)` calls and aliased `storage` handles
//   - a turbofish between the method name and its argument list
//   - a packed `#[contracttype]` write reached through `&self.field`
//   - a legacy fallback read followed by deletes on commit
//   - a loop whose body writes storage
//   - an RAII `Drop` guard that writes instance storage on scope exit
//   - cross-contract calls via `invoke_contract` and a token client
//   - unreachable-looking code (strings and comments that mention `storage`)

use soroban_sdk::{contracttype, token, Address, Bytes, Env, IntoVal, Symbol, Vec};

/// Storage keys for the fixture pool.
#[contracttype]
#[derive(Clone)]
pub enum PoolDataKey {
    Admin,
    Total,
    Cap,
    UserPosition(Address),
}

/// Packed hot-path state.
#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PackedState {
    pub total: i128,
    pub cap: i128,
}

/// Packed borrow limits.
#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    pub debt_ceiling: i128,
    pub min_borrow: i128,
}

/// Errors raised by the fixture pool.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum PoolError {
    Unauthorized = 1,
    Paused = 2,
}

/// A loaded copy of the packed state.
pub struct PackedSlot {
    pub state: PackedState,
    legacy: bool,
}

impl PackedSlot {
    /// One read on the warm path, four on the legacy path.
    pub fn load(env: &Env) -> Self {
        let storage = env.storage().persistent();
        if let Some(state) = storage.get::<_, PackedState>(&PoolDataKey::Total) {
            return PackedSlot { state, legacy: false };
        }
        let total: Option<i128> = storage.get(&PoolDataKey::Total);
        let cap: Option<i128> = storage.get(&PoolDataKey::Cap);
        PackedSlot {
            state: PackedState {
                total: total.unwrap_or(0),
                cap: cap.unwrap_or(PackedState { total: 0, cap: 0 }.cap),
            },
            legacy: true,
        }
    }

    /// One write, plus three deletes when the slot was rebuilt from legacy keys.
    pub fn commit(self, env: &Env) {
        let storage = env.storage().persistent();
        storage.set(&PoolDataKey::Total, &self.state);
        if self.legacy {
            storage.remove(&PoolDataKey::Total);
            storage.remove(&PoolDataKey::Cap);
        }
    }
}

/// RAII reentrancy guard writing instance storage.
pub struct FlashGuard {
    env: Env,
    guard_key: PoolDataKey,
    active_key: PoolDataKey,
}

impl FlashGuard {
    fn new(env: &Env) -> Self {
        let guard_key = PoolDataKey::Admin;
        let active_key = PoolDataKey::Cap;
        if env.storage().instance().get(&guard_key).unwrap_or(false) {
            panic!("reentrancy");
        }
        env.storage().instance().set(&guard_key, &true);
        env.storage().instance().set(&active_key, &true);
        FlashGuard { env: env.clone(), guard_key, active_key }
    }
}

impl Drop for FlashGuard {
    fn drop(&mut self) {
        self.env.storage().instance().set(&self.guard_key, &false);
        self.env.storage().instance().set(&self.active_key, &false);
    }
}

pub fn is_paused(env: &Env) -> bool {
    if env.storage().persistent().has(&PoolDataKey::Admin) {
        return true;
    }
    env.storage().persistent().get(&PoolDataKey::Cap).unwrap_or(false)
}

pub fn read_limits(env: &Env) -> Limits {
    let storage = env.storage().persistent();
    if storage.has(&PoolDataKey::Total) {
        storage.get(&PoolDataKey::Cap)
    } else {
        Limits { debt_ceiling: 1, min_borrow: 1 }
    }
}

/// Writes one entry per batch element and re-checks the pause flag.
pub fn batch_apply(env: &Env, assets: Vec<Address>, amount: i128) -> i128 {
    if is_paused(env) {
        return 0;
    }
    let mut total = 0i128;
    for asset in assets.iter() {
        env.storage()
            .persistent()
            .set(&PoolDataKey::UserPosition(asset.clone()), &amount);
        total = total + amount;
    }
    env.storage().persistent().set(&PoolDataKey::Total, &total);
    total
}

/// Reads a key twice through the same variable handle.
pub fn guarded_read(env: &Env) -> Option<i128> {
    let key = PoolDataKey::Cap;
    if !env.storage().persistent().has(&key) {
        return None;
    }
    env.storage().persistent().get(&key)
}

pub fn flash(env: &Env, receiver: Address, amount: i128) -> Result<(), PoolError> {
    let _guard = FlashGuard::new(env);
    let client = token::Client::new(env, &PoolDataKey::Admin);
    client.transfer(&receiver, &amount);
    env.invoke_contract::<i128>(&receiver, &Symbol::new(env, "settle"), amount.into_val(env));
    Ok(())
}

/// Applies one deposit: the packed slot is read, the position is written, the
/// slot is committed.
pub fn apply_deposit(env: &Env, user: Address, amount: i128) -> Result<i128, PoolError> {
    let mut slot = PackedSlot::load(env);
    slot.state.total = slot.state.total + amount;
    env.storage()
        .persistent()
        .set(&PoolDataKey::UserPosition(user.clone()), &amount);
    slot.commit(env);
    Ok(slot.state.total)
}

pub fn pause_probe(env: &Env) -> String {
    // Not storage: "env.storage().persistent().set(" inside a string literal.
    let note = "env.storage().persistent().set(&PoolDataKey::Total, &1)";
    // Not storage: commented out env.storage().persistent().get(&PoolDataKey::Cap)
    let _unused: Option<Bytes> = None;
    if amount_is_zero(0) {
        return note;
    }
    note
}

fn amount_is_zero(amount: i128) -> bool {
    amount == 0
}
