//! Event helpers for cross-contract integration tests (Issue #688).
//!
//! Records and asserts contract events without requiring a live RPC.
//! Uses plain Rust types (not Soroban storage) so it works in any test env.

#![allow(unused)]

use soroban_sdk::{symbol_short, Env, Symbol};

/// A recorded event captured during a test.
#[derive(Clone, Debug)]
pub struct CapturedEvent {
    pub topic: String,
    pub label: String,
}

/// Ring buffer of captured events for assertion-friendly consumption.
#[derive(Default)]
pub struct EventRecorder {
    events: Vec<CapturedEvent>,
    capacity: usize,
}

impl EventRecorder {
    pub fn new(_env: &Env, capacity: usize) -> Self {
        Self {
            events: Vec::new(),
            capacity: capacity.max(1),
        }
    }

    /// Record an event (topic symbol + human-readable label).
    pub fn record(&mut self, _env: &Env, topic: Symbol, label: &str) {
        self.events.push(CapturedEvent {
            topic: topic.to_string(),
            label: label.to_string(),
        });
        // Ring-buffer trim: drop oldest when over capacity.
        while self.events.len() > self.capacity {
            self.events.remove(0);
        }
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    pub fn clear(&mut self) {
        self.events.clear();
    }

    /// Assert that at least one recorded event has the given topic symbol.
    pub fn assert_topic_seen(&self, topic: Symbol) {
        let expected = topic.to_string();
        let seen = self.events.iter().any(|e| e.topic == expected);
        assert!(seen, "expected event with topic {}", expected);
    }

    /// Assert total recorded event count.
    pub fn assert_count(&self, expected: usize) {
        assert_eq!(
            self.events.len(),
            expected,
            "expected {} events, got {}",
            expected,
            self.events.len()
        );
    }

    /// All recorded events (for custom assertions).
    pub fn events(&self) -> &[CapturedEvent] {
        &self.events
    }
}

/// Common topic symbols used across StellarLend contracts.
pub fn topic_deposit() -> Symbol {
    symbol_short!("deposit")
}

pub fn topic_borrow() -> Symbol {
    symbol_short!("borrow")
}

pub fn topic_repay() -> Symbol {
    symbol_short!("repay")
}

pub fn topic_withdraw() -> Symbol {
    symbol_short!("withdraw")
}

pub fn topic_liquidate() -> Symbol {
    symbol_short!("liquidate")
}

pub fn topic_transfer() -> Symbol {
    symbol_short!("transfer")
}
