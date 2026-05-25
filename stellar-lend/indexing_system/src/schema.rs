use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub const STANDARD_EVENT_SCHEMA_VERSION: u32 = 1;
pub const EVENT_SCHEMA_VERSION_FIELD: &str = "_schema_version";
pub const EVENT_TOPIC_FIELD: &str = "_event_topic";
pub const EVENT_SCHEMA_JSON: &str = include_str!("../../docs/event-schema.v1.json");

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EventSchemaDocument {
    pub schema_version: u32,
    pub topic_style: String,
    pub contracts: Vec<ContractEventSchema>,
    #[serde(default)]
    pub allow_overloaded_topics: Vec<OverloadedTopic>,
    #[serde(default)]
    pub legacy_manual_events: Vec<LegacyManualEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContractEventSchema {
    pub name: String,
    pub source_globs: Vec<String>,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OverloadedTopic {
    pub contract: String,
    pub topic: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LegacyManualEvent {
    pub contract: String,
    pub topic: String,
    pub format: String,
}

pub fn load_standard_event_schema() -> Result<EventSchemaDocument, serde_json::Error> {
    serde_json::from_str(EVENT_SCHEMA_JSON)
}

pub fn normalize_event_topic(event_name: &str) -> String {
    let mut out = String::new();
    let chars: Vec<char> = event_name.chars().collect();

    for (idx, ch) in chars.iter().copied().enumerate() {
        if ch == '-' || ch == ' ' {
            out.push('_');
            continue;
        }

        if ch.is_ascii_uppercase() {
            let prev = idx.checked_sub(1).and_then(|i| chars.get(i)).copied();
            let next = chars.get(idx + 1).copied();
            let starts_word = prev
                .map(|p| p.is_ascii_lowercase() || p.is_ascii_digit())
                .unwrap_or(false)
                || (prev.is_some() && next.map(|n| n.is_ascii_lowercase()).unwrap_or(false));
            if starts_word {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }

    out
}

pub fn annotate_event_payload(payload: &mut Map<String, Value>, event_name: &str) {
    payload.insert(
        EVENT_SCHEMA_VERSION_FIELD.to_string(),
        Value::Number(STANDARD_EVENT_SCHEMA_VERSION.into()),
    );
    payload.insert(
        EVENT_TOPIC_FIELD.to_string(),
        Value::String(normalize_event_topic(event_name)),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_embedded_schema_document() {
        let schema = load_standard_event_schema().expect("schema json must parse");
        assert_eq!(schema.schema_version, STANDARD_EVENT_SCHEMA_VERSION);
        assert!(schema.contracts.iter().any(|c| c.name == "hello-world"));
    }

    #[test]
    fn normalizes_pascal_case_event_names() {
        assert_eq!(normalize_event_topic("DepositEvent"), "deposit_event");
        assert_eq!(
            normalize_event_topic("AMMOperationEvent"),
            "amm_operation_event"
        );
        assert_eq!(normalize_event_topic("bridge-deposit"), "bridge_deposit");
    }
}
