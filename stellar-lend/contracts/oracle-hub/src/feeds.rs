//! Feed slot bookkeeping shared by the read, health, and management paths.
//!
//! The hub stores one [`crate::types::PriceFeed`] per `(asset, slot)` pair. To
//! keep reads cheap — the price path is the hottest code in the contract — the
//! hub also keeps an ascending list of the slots that are actually registered
//! for an asset. Collection and health walks iterate that list instead of
//! probing every possible slot, so an asset with two feeds costs two storage
//! reads whether one or five slots exist.

use crate::storage::DataKey;
use crate::types::{FeedPriority, PriceFeed};
use soroban_sdk::{Bytes, Env, Vec};

/// Registered slots of an asset, ascending. Empty when nothing is registered.
pub fn feed_index(env: &Env, asset: &Bytes) -> Vec<u32> {
    env.storage()
        .instance()
        .get::<_, Vec<u32>>(&DataKey::FeedIndex(asset.clone()))
        .unwrap_or_else(|| Vec::new(env))
}

/// Add `slot` to the asset's index if it is not registered yet.
///
/// Returns `true` when the index changed.
pub fn register_slot(env: &Env, asset: &Bytes, slot: u32) -> bool {
    let mut index = feed_index(env, asset);
    let len = index.len();
    let mut i = 0;
    while i < len {
        if index.get(i).unwrap_or(u32::MAX) == slot {
            return false;
        }
        i += 1;
    }
    index.push_back(slot);
    env.storage()
        .instance()
        .set(&DataKey::FeedIndex(asset.clone()), &index);
    true
}

/// Whether the asset has a feed registered in `slot`.
pub fn is_registered(env: &Env, asset: &Bytes, slot: u32) -> bool {
    let index = feed_index(env, asset);
    let len = index.len();
    let mut i = 0;
    while i < len {
        if index.get(i).unwrap_or(u32::MAX) == slot {
            return true;
        }
        i += 1;
    }
    false
}

/// Configuration of one feed slot, if it exists.
pub fn feed(env: &Env, asset: &Bytes, priority: FeedPriority) -> Option<PriceFeed> {
    env.storage()
        .instance()
        .get::<_, PriceFeed>(&DataKey::Feed(asset.clone(), priority as u32))
}

/// Configuration of one feed slot by raw slot index.
pub fn feed_at(env: &Env, asset: &Bytes, slot: u32) -> Option<PriceFeed> {
    env.storage()
        .instance()
        .get::<_, PriceFeed>(&DataKey::Feed(asset.clone(), slot))
}

/// Every registered feed of an asset, in slot order.
pub fn feeds(env: &Env, asset: &Bytes) -> Vec<PriceFeed> {
    let index = feed_index(env, asset);
    let mut out = Vec::new(env);
    let len = index.len();
    let mut i = 0;
    while i < len {
        let slot = index.get(i).unwrap_or(u32::MAX);
        if let Some(feed) = feed_at(env, asset, slot) {
            out.push_back(feed);
        }
        i += 1;
    }
    out
}

/// Total number of slots across all assets.
pub fn feed_count(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get::<_, u32>(&DataKey::FeedCount)
        .unwrap_or(0)
}

/// Increment the global feed counter.
pub fn bump_feed_count(env: &Env) {
    let count = feed_count(env).saturating_add(1);
    env.storage().instance().set(&DataKey::FeedCount, &count);
}
