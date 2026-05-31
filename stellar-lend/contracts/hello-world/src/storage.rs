use soroban_sdk::{Env, IntoVal, TryFromVal, Val};

#[soroban_sdk::contracttype]
pub struct SnapshotValue {
    pub value: Val,
    pub timestamp: u64,
}

pub fn get_snapshot<K, T>(e: &Env, key: &K, force_direct: bool) -> Option<T>
where
    K: IntoVal<Env, Val> + TryFromVal<Env, Val>,
    T: IntoVal<Env, Val> + TryFromVal<Env, Val>,
{
    if force_direct {
        return e.storage().persistent().get::<K, SnapshotValue>(key).map(|s| s.value.into_val(e));
    }

    None
}
