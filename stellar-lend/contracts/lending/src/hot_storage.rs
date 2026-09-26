//! # Packed Hot-Path Storage (issue #1043)
//!
//! Every `deposit` used to read three independent persistent entries
//! (`TotalAmount`, `CapAmount`, `MinAmount`) and write one of them back, and
//! every `withdraw` did the same read/write on `TotalAmount`. On Soroban each
//! persistent entry is a separate ledger key: it is charged its own read/write
//! fee, its own footprint slot and its own rent.
//!
//! This module packs the deposit hot-path globals into **one** ledger entry:
//!
//! | field   | meaning                                     | legacy key                    |
//! |---------|---------------------------------------------|-------------------------------|
//! | `total` | aggregate deposited collateral              | `DepositDataKey::TotalAmount` |
//! | `cap`   | protocol-wide deposit cap                   | `DepositDataKey::CapAmount`   |
//! | `min`   | minimum deposit (dust threshold)            | `DepositDataKey::MinAmount`   |
//!
//! A deposit now costs one read + one write of the packed slot (instead of three
//! reads + one write), and a withdraw costs one read + one write.
//!
//! ## Backward compatibility
//!
//! Contracts deployed before this change still hold the three legacy entries.
//! [`DepositHotSlot::load`] reads the packed slot first and, when it is absent,
//! reconstructs the state from the legacy keys. The first [`DepositHotSlot::commit`]
//! after such a fallback writes the packed slot and removes the legacy entries,
//! so migration happens transparently on the first hot-path write and never
//! costs an extra transaction.
//!
//! ## Lazy initialisation (issue #1046)
//!
//! A freshly initialised pool has **no** packed slot at all: reads fall back to
//! [`DepositHotState::DEFAULT`] without allocating storage, and the entry is
//! only created the first time a deposit, withdraw or settings update needs to
//! persist it.

use crate::deposit::DepositDataKey;
use soroban_sdk::{contracttype, Env};

/// Storage keys for packed hot-path state.
#[contracttype]
#[derive(Clone)]
pub enum HotStorageKey {
    /// Packed [`DepositHotState`].
    DepositState,
}

/// Deposit hot-path globals packed into a single ledger entry.
#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DepositHotState {
    /// Aggregate deposited collateral across all users.
    pub total: i128,
    /// Protocol-wide deposit cap (`i128::MAX` when unset).
    pub cap: i128,
    /// Minimum deposit amount; smaller deposits are rejected as dust.
    pub min: i128,
}

impl DepositHotState {
    /// Value a pool reads before anything has been persisted.
    pub const DEFAULT: DepositHotState = DepositHotState {
        total: 0,
        cap: i128::MAX,
        min: 0,
    };
}

/// Where a loaded [`DepositHotSlot`] came from; decides what `commit` must write.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotOrigin {
    /// Read from the packed entry.
    Packed,
    /// Reconstructed from pre-#1043 per-field entries (needs migration on commit).
    Legacy,
    /// Nothing stored yet — lazily defaulted.
    Default,
}

/// A loaded copy of the packed deposit state.
///
/// Callers mutate [`DepositHotSlot::state`] in memory and call
/// [`DepositHotSlot::commit`] exactly once, so a hot path touches the ledger
/// entry at most twice (one read, one write) no matter how many fields change.
#[derive(Clone, Copy, Debug)]
pub struct DepositHotSlot {
    pub state: DepositHotState,
    origin: SlotOrigin,
}

impl DepositHotSlot {
    /// Load the packed state, falling back to legacy keys and then defaults.
    pub fn load(env: &Env) -> Self {
        let storage = env.storage().persistent();
        if let Some(state) = storage.get::<_, DepositHotState>(&HotStorageKey::DepositState) {
            return DepositHotSlot {
                state,
                origin: SlotOrigin::Packed,
            };
        }

        let total: Option<i128> = storage.get(&DepositDataKey::TotalAmount);
        let cap: Option<i128> = storage.get(&DepositDataKey::CapAmount);
        let min: Option<i128> = storage.get(&DepositDataKey::MinAmount);

        if total.is_none() && cap.is_none() && min.is_none() {
            return DepositHotSlot {
                state: DepositHotState::DEFAULT,
                origin: SlotOrigin::Default,
            };
        }

        DepositHotSlot {
            state: DepositHotState {
                total: total.unwrap_or(DepositHotState::DEFAULT.total),
                cap: cap.unwrap_or(DepositHotState::DEFAULT.cap),
                min: min.unwrap_or(DepositHotState::DEFAULT.min),
            },
            origin: SlotOrigin::Legacy,
        }
    }

    /// Where this slot was loaded from.
    pub fn origin(&self) -> SlotOrigin {
        self.origin
    }

    /// Persist the packed state. If it was reconstructed from legacy entries,
    /// those entries are removed so the pool pays rent on a single key.
    pub fn commit(self, env: &Env) {
        let storage = env.storage().persistent();
        storage.set(&HotStorageKey::DepositState, &self.state);
        if self.origin == SlotOrigin::Legacy {
            storage.remove(&DepositDataKey::TotalAmount);
            storage.remove(&DepositDataKey::CapAmount);
            storage.remove(&DepositDataKey::MinAmount);
        }
    }
}

/// Read-only accessor for the packed deposit state (no writes, no allocation).
pub fn deposit_state(env: &Env) -> DepositHotState {
    DepositHotSlot::load(env).state
}

/// `true` once the packed deposit slot has been materialised in storage.
pub fn is_deposit_state_initialized(env: &Env) -> bool {
    env.storage()
        .persistent()
        .has(&HotStorageKey::DepositState)
}

/// Eagerly migrate legacy per-field entries into the packed slot.
///
/// Returns `true` if a migration was performed. Pools that already use the
/// packed slot (or have never stored anything) are left untouched.
pub fn migrate_deposit_state(env: &Env) -> bool {
    let slot = DepositHotSlot::load(env);
    if slot.origin() != SlotOrigin::Legacy {
        return false;
    }
    slot.commit(env);
    true
}
