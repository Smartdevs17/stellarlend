//! Shared event schema constants for StellarLend contracts and indexers.
//!
//! Version 1 is intentionally additive: existing event topics and payloads remain stable while
//! new documentation, indexer metadata, and CI checks describe the canonical schema.

use soroban_sdk::contracttype;

pub const EVENT_SCHEMA_VERSION: u32 = 1;
pub const EVENT_SCHEMA_VERSION_FIELD: &str = "_schema_version";
pub const EVENT_TOPIC_FIELD: &str = "_event_topic";
pub const MAX_SOROBAN_EVENT_TOPICS: u32 = 4;

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum EventSchemaVersion {
    V1 = 1,
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum EventPayloadFormat {
    ContractEvent = 1,
    SingleValue = 2,
    LegacyTuple = 3,
}
