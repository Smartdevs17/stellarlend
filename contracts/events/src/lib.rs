#![no_std]
//! Shared event envelope for pool, risk and liquidation modules (issue #1761).
//!
//! The module-specific fields live in [`EventPayload`], while identity and
//! observability fields live in [`EventContext`]. Indexers can therefore use a
//! single decoder and a single topic table for all three modules.

use core::cell::Cell;

/// Current envelope version. Bump only when a field is added or removed.
pub const SCHEMA_VERSION: u32 = 1;

/// Module that produced an event.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EventModule {
    Pool,
    Risk,
    Liquidation,
}

impl EventModule {
    /// Stable lower-case name used in metrics labels and log context.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pool => "pool",
            Self::Risk => "risk",
            Self::Liquidation => "liquidation",
        }
    }
}

/// Action being reported.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EventAction {
    Deposit,
    Withdraw,
    Borrow,
    Repay,
    Liquidate,
    RiskAlert,
    GracePeriodWarning,
    ConfigUpdated,
}

impl EventAction {
    /// Stable lower-case name used in metrics labels.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Deposit => "deposit",
            Self::Withdraw => "withdraw",
            Self::Borrow => "borrow",
            Self::Repay => "repay",
            Self::Liquidate => "liquidate",
            Self::RiskAlert => "risk_alert",
            Self::GracePeriodWarning => "grace_period_warning",
            Self::ConfigUpdated => "config_updated",
        }
    }
}

/// Severity shared by risk and liquidation alerts.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum EventSeverity {
    Info,
    Warning,
    Critical,
}

impl EventSeverity {
    /// Stable lower-case name used in metrics labels.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Critical => "critical",
        }
    }
}

/// Validation errors for a normalised event.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EventError {
    /// Envelope version is not [`SCHEMA_VERSION`].
    VersionMismatch,
    /// Actor or trace id was empty.
    MissingIdentity,
    /// Timestamp was zero.
    InvalidTimestamp,
    /// Amount or a payload field was negative.
    InvalidAmount,
    /// Health factor was outside `0..=1_000_000` bps.
    InvalidHealthFactor,
    /// Topic was not registered for the module/action pair.
    UnknownTopic,
    /// The sink could not publish the event.
    SinkFailure,
}

/// Identity and observability fields present on every event.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EventContext<'a> {
    /// Address (or account id) that triggered the event.
    pub actor: &'a str,
    /// Asset involved, when the action is asset-specific.
    pub asset: Option<&'a str>,
    /// Other party, e.g. the liquidator.
    pub counterparty: Option<&'a str>,
    /// Correlation id shared by all events from one transaction or scan.
    pub trace_id: &'a str,
    /// Unix timestamp in seconds.
    pub timestamp: u64,
}

/// Module-specific fields. Field names are identical across all modules.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EventPayload {
    /// Pool balance changes.
    Pool {
        balance_after: i128,
        debt_after: i128,
    },
    /// Risk threshold or utilisation readings.
    Risk {
        threshold_bps: i128,
        utilization_bps: i128,
    },
    /// Liquidation settlement amounts.
    Liquidation {
        debt_liquidated: i128,
        collateral_seized: i128,
        incentive_bps: i128,
    },
    /// Parameter or configuration changes.
    Config {
        previous_value: i128,
        new_value: i128,
    },
}

/// The one event shape consumed by contracts, indexers and SDK tools.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NormalizedEvent<'a> {
    pub version: u32,
    pub topic: &'static str,
    pub module: EventModule,
    pub action: EventAction,
    pub severity: EventSeverity,
    pub context: EventContext<'a>,
    pub amount: i128,
    pub health_factor_bps: Option<i128>,
    pub payload: EventPayload,
}

/// Stable, indexer-friendly topic for a module/action pair.
pub fn topic_for(module: EventModule, action: EventAction) -> &'static str {
    match (module, action) {
        (EventModule::Pool, EventAction::Deposit) => "lend_deposit",
        (EventModule::Pool, EventAction::Withdraw) => "lend_withdraw",
        (EventModule::Pool, EventAction::Borrow) => "lend_borrow",
        (EventModule::Pool, EventAction::Repay) => "lend_repay",
        (EventModule::Liquidation, EventAction::Liquidate) => "liq_execute",
        (EventModule::Risk, EventAction::RiskAlert) => "risk_alert",
        (EventModule::Risk, EventAction::GracePeriodWarning) => "risk_grace_warn",
        (EventModule::Risk, EventAction::ConfigUpdated)
        | (EventModule::Pool, EventAction::ConfigUpdated)
        | (EventModule::Liquidation, EventAction::ConfigUpdated) => "cfg_updated",
        (EventModule::Pool, EventAction::RiskAlert)
        | (EventModule::Pool, EventAction::GracePeriodWarning)
        | (EventModule::Pool, EventAction::Liquidate)
        | (EventModule::Liquidation, EventAction::Deposit)
        | (EventModule::Liquidation, EventAction::Withdraw)
        | (EventModule::Liquidation, EventAction::Borrow)
        | (EventModule::Liquidation, EventAction::Repay)
        | (EventModule::Liquidation, EventAction::RiskAlert)
        | (EventModule::Liquidation, EventAction::GracePeriodWarning)
        | (EventModule::Risk, EventAction::Deposit)
        | (EventModule::Risk, EventAction::Withdraw)
        | (EventModule::Risk, EventAction::Borrow)
        | (EventModule::Risk, EventAction::Repay)
        | (EventModule::Risk, EventAction::Liquidate) => "unmapped_event",
    }
}

/// Build a normalised event. Callers should use the typed builders in the
/// module crates rather than constructing this directly.
#[allow(clippy::too_many_arguments)]
pub fn new_event<'a>(
    module: EventModule,
    action: EventAction,
    severity: EventSeverity,
    context: EventContext<'a>,
    amount: i128,
    health_factor_bps: Option<i128>,
    payload: EventPayload,
) -> NormalizedEvent<'a> {
    NormalizedEvent {
        version: SCHEMA_VERSION,
        topic: topic_for(module, action),
        module,
        action,
        severity,
        context,
        amount,
        health_factor_bps,
        payload,
    }
}

/// Validate every field that an indexer relies on.
pub fn validate_event(event: &NormalizedEvent<'_>) -> Result<(), EventError> {
    if event.version != SCHEMA_VERSION {
        return Err(EventError::VersionMismatch);
    }
    if event.topic == "unmapped_event" {
        return Err(EventError::UnknownTopic);
    }
    if event.context.actor.is_empty() || event.context.trace_id.is_empty() {
        return Err(EventError::MissingIdentity);
    }
    if event.context.timestamp == 0 {
        return Err(EventError::InvalidTimestamp);
    }
    if event.amount < 0 {
        return Err(EventError::InvalidAmount);
    }
    if let Some(hf) = event.health_factor_bps {
        if hf < 0 || hf > 1_000_000 {
            return Err(EventError::InvalidHealthFactor);
        }
    }
    match event.payload {
        EventPayload::Pool {
            balance_after,
            debt_after,
        } => {
            if balance_after < 0 || debt_after < 0 {
                return Err(EventError::InvalidAmount);
            }
        }
        EventPayload::Risk {
            threshold_bps,
            utilization_bps,
        } => {
            if threshold_bps < 0 || utilization_bps < 0 || utilization_bps > 10_000 {
                return Err(EventError::InvalidAmount);
            }
        }
        EventPayload::Liquidation {
            debt_liquidated,
            collateral_seized,
            incentive_bps,
        } => {
            if debt_liquidated < 0 || collateral_seized < 0 || incentive_bps < 0 {
                return Err(EventError::InvalidAmount);
            }
        }
        EventPayload::Config {
            previous_value,
            new_value,
        } => {
            if previous_value < 0 || new_value < 0 {
                return Err(EventError::InvalidAmount);
            }
        }
    }
    Ok(())
}

/// Contract-side publication hook. The host environment implements this.
pub trait EventSink {
    /// Publish a validated event to the event stream.
    fn publish(&self, event: &NormalizedEvent<'_>) -> Result<(), EventError>;
}

/// Observability hook for accepted and rejected events.
pub trait EventTelemetry {
    /// Record the outcome of a publish attempt.
    fn record(&self, event: &NormalizedEvent<'_>, accepted: bool);
}

/// Counters kept in `core::cell` so no allocator or host dependency is needed.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct EventMetrics {
    published: u64,
    rejected: u64,
    pool_events: u64,
    risk_events: u64,
    liquidation_events: u64,
}

impl EventMetrics {
    /// Number of successfully published events.
    pub fn published(self) -> u64 {
        self.published
    }

    /// Number of validation or sink failures.
    pub fn rejected(self) -> u64 {
        self.rejected
    }

    /// Successfully published pool events.
    pub fn pool_events(self) -> u64 {
        self.pool_events
    }

    /// Successfully published risk events.
    pub fn risk_events(self) -> u64 {
        self.risk_events
    }

    /// Successfully published liquidation events.
    pub fn liquidation_events(self) -> u64 {
        self.liquidation_events
    }

    fn record(&mut self, module: EventModule, accepted: bool) {
        if !accepted {
            self.rejected += 1;
            return;
        }
        self.published += 1;
        match module {
            EventModule::Pool => self.pool_events += 1,
            EventModule::Risk => self.risk_events += 1,
            EventModule::Liquidation => self.liquidation_events += 1,
        }
    }
}

/// In-process telemetry implementation; contracts may wrap this with their
/// own metric exporter.
#[derive(Default)]
pub struct InMemoryTelemetry {
    metrics: Cell<EventMetrics>,
}

impl InMemoryTelemetry {
    /// Snapshot of all counters.
    pub fn snapshot(&self) -> EventMetrics {
        self.metrics.get()
    }
}

impl EventTelemetry for InMemoryTelemetry {
    fn record(&self, event: &NormalizedEvent<'_>, accepted: bool) {
        // `Cell::get` copies the struct, so update the copy and write it back.
        let mut updated = self.metrics.get();
        updated.record(event.module, accepted);
        self.metrics.set(updated);
    }
}

/// Validate, publish, and record the outcome in one call.
pub fn emit<S: EventSink, T: EventTelemetry>(
    sink: &S,
    telemetry: &T,
    event: &NormalizedEvent<'_>,
) -> Result<(), EventError> {
    if let Err(error) = validate_event(event) {
        telemetry.record(event, false);
        return Err(error);
    }
    let result = sink.publish(event);
    telemetry.record(event, result.is_ok());
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    struct RecordingSink {
        published: Cell<u32>,
    }

    impl EventSink for RecordingSink {
        fn publish(&self, _event: &NormalizedEvent<'_>) -> Result<(), EventError> {
            self.published.set(self.published.get() + 1);
            Ok(())
        }
    }

    fn sample(module: EventModule, action: EventAction) -> NormalizedEvent<'static> {
        let payload = match module {
            EventModule::Pool => EventPayload::Pool {
                balance_after: 100,
                debt_after: 0,
            },
            EventModule::Risk => EventPayload::Risk {
                threshold_bps: 8_000,
                utilization_bps: 5_000,
            },
            EventModule::Liquidation => EventPayload::Liquidation {
                debt_liquidated: 100,
                collateral_seized: 120,
                incentive_bps: 500,
            },
        };
        new_event(
            module,
            action,
            EventSeverity::Info,
            EventContext {
                actor: "user-1",
                asset: Some("XLM"),
                counterparty: None,
                trace_id: "tx-1",
                timestamp: 1_700_000_000,
            },
            100,
            Some(12_000),
            payload,
        )
    }

    #[test]
    fn topics_are_stable_per_module() {
        assert_eq!(topic_for(EventModule::Pool, EventAction::Deposit), "lend_deposit");
        assert_eq!(topic_for(EventModule::Risk, EventAction::RiskAlert), "risk_alert");
        assert_eq!(topic_for(EventModule::Liquidation, EventAction::Liquidate), "liq_execute");
    }

    #[test]
    fn a_valid_event_is_published_and_counted() {
        let sink = RecordingSink {
            published: Cell::new(0),
        };
        let telemetry = InMemoryTelemetry::default();
        let event = sample(EventModule::Pool, EventAction::Deposit);

        assert_eq!(emit(&sink, &telemetry, &event), Ok(()));
        assert_eq!(sink.published.get(), 1);
        assert_eq!(telemetry.snapshot().published(), 1);
        assert_eq!(telemetry.snapshot().pool_events(), 1);
    }

    #[test]
    fn invalid_events_are_rejected_before_publication() {
        let sink = RecordingSink {
            published: Cell::new(0),
        };
        let telemetry = InMemoryTelemetry::default();
        let mut event = sample(EventModule::Risk, EventAction::GracePeriodWarning);
        event.context.timestamp = 0;

        assert_eq!(emit(&sink, &telemetry, &event), Err(EventError::InvalidTimestamp));
        assert_eq!(sink.published.get(), 0);
        assert_eq!(telemetry.snapshot().rejected(), 1);
    }

    #[test]
    fn unmapped_pairs_are_rejected() {
        let event = sample(EventModule::Liquidation, EventAction::Deposit);
        assert_eq!(validate_event(&event), Err(EventError::UnknownTopic));
    }
}
