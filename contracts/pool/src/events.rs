//! Normalised event builders for pool, risk, and liquidation modules (#1761).
//!
//! Every builder returns the shared envelope from `contracts/events`. Callers
//! must supply a trace id so a deposit → risk alert → liquidation sequence can
//! be correlated by indexers and dashboards.

use stellarlend_events::{
    emit, new_event, EventAction, EventContext, EventError, EventModule, EventPayload,
    EventSeverity, EventSink, EventTelemetry, NormalizedEvent,
};

/// Build the shared context for an event.
pub fn context<'a>(
    actor: &'a str,
    asset: Option<&'a str>,
    counterparty: Option<&'a str>,
    trace_id: &'a str,
    timestamp: u64,
) -> EventContext<'a> {
    EventContext {
        actor,
        asset,
        counterparty,
        trace_id,
        timestamp,
    }
}

/// Deposit event: `amount` added and the resulting balance/debt.
pub fn deposit<'a>(
    context: EventContext<'a>,
    amount: i128,
    balance_after: i128,
    debt_after: i128,
) -> NormalizedEvent<'a> {
    new_event(
        EventModule::Pool,
        EventAction::Deposit,
        EventSeverity::Info,
        context,
        amount,
        None,
        EventPayload::Pool {
            balance_after,
            debt_after,
        },
    )
}

/// Withdrawal event: `amount` removed from the position.
pub fn withdraw<'a>(
    context: EventContext<'a>,
    amount: i128,
    balance_after: i128,
    debt_after: i128,
) -> NormalizedEvent<'a> {
    new_event(
        EventModule::Pool,
        EventAction::Withdraw,
        EventSeverity::Info,
        context,
        amount,
        None,
        EventPayload::Pool {
            balance_after,
            debt_after,
        },
    )
}

/// Borrow event: `amount` of debt added.
pub fn borrow<'a>(
    context: EventContext<'a>,
    amount: i128,
    balance_after: i128,
    debt_after: i128,
) -> NormalizedEvent<'a> {
    new_event(
        EventModule::Pool,
        EventAction::Borrow,
        EventSeverity::Info,
        context,
        amount,
        None,
        EventPayload::Pool {
            balance_after,
            debt_after,
        },
    )
}

/// Repayment event: `amount` of debt repaid.
pub fn repay<'a>(
    context: EventContext<'a>,
    amount: i128,
    balance_after: i128,
    debt_after: i128,
) -> NormalizedEvent<'a> {
    new_event(
        EventModule::Pool,
        EventAction::Repay,
        EventSeverity::Info,
        context,
        amount,
        None,
        EventPayload::Pool {
            balance_after,
            debt_after,
        },
    )
}

/// Risk alert emitted when utilisation or health crosses a threshold.
pub fn risk_alert<'a>(
    context: EventContext<'a>,
    health_factor_bps: i128,
    threshold_bps: i128,
    utilization_bps: i128,
    severity: EventSeverity,
) -> NormalizedEvent<'a> {
    new_event(
        EventModule::Risk,
        EventAction::RiskAlert,
        severity,
        context,
        threshold_bps,
        Some(health_factor_bps),
        EventPayload::Risk {
            threshold_bps,
            utilization_bps,
        },
    )
}

/// Grace-period warning emitted before a health factor reaches liquidation.
pub fn grace_period_warning<'a>(
    context: EventContext<'a>,
    health_factor_bps: i128,
    threshold_bps: i128,
    utilization_bps: i128,
    severity: EventSeverity,
) -> NormalizedEvent<'a> {
    new_event(
        EventModule::Risk,
        EventAction::GracePeriodWarning,
        severity,
        context,
        threshold_bps,
        Some(health_factor_bps),
        EventPayload::Risk {
            threshold_bps,
            utilization_bps,
        },
    )
}

/// Liquidation settlement event. `amount` is the debt liquidated.
pub fn liquidation<'a>(
    context: EventContext<'a>,
    debt_liquidated: i128,
    collateral_seized: i128,
    incentive_bps: i128,
    health_factor_bps: i128,
) -> NormalizedEvent<'a> {
    new_event(
        EventModule::Liquidation,
        EventAction::Liquidate,
        EventSeverity::Critical,
        context,
        debt_liquidated,
        Some(health_factor_bps),
        EventPayload::Liquidation {
            debt_liquidated,
            collateral_seized,
            incentive_bps,
        },
    )
}

/// Configuration change event, using the same envelope as risk events.
pub fn config_updated<'a>(
    context: EventContext<'a>,
    previous_value: i128,
    new_value: i128,
) -> NormalizedEvent<'a> {
    new_event(
        EventModule::Pool,
        EventAction::ConfigUpdated,
        EventSeverity::Info,
        context,
        new_value,
        None,
        EventPayload::Config {
            previous_value,
            new_value,
        },
    )
}

/// Validate, publish and record the outcome through the shared helpers.
pub fn emit_with<S: EventSink, T: EventTelemetry>(
    sink: &S,
    telemetry: &T,
    event: &NormalizedEvent<'_>,
) -> Result<(), EventError> {
    emit(sink, telemetry, event)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> EventContext<'static> {
        context(
            "borrower-1",
            Some("XLM"),
            Some("liquidator-1"),
            "tx-42",
            1_700_000_000,
        )
    }

    #[test]
    fn pool_actions_share_the_envelope() {
        let event = deposit(ctx(), 1_000, 1_000, 0);
        assert_eq!(event.topic, "lend_deposit");
        assert_eq!(event.version, stellarlend_events::SCHEMA_VERSION);
        assert_eq!(event.context.trace_id, "tx-42");
        assert_eq!(
            event.payload,
            EventPayload::Pool {
                balance_after: 1_000,
                debt_after: 0,
            }
        );
        assert_eq!(stellarlend_events::validate_event(&event), Ok(()));
    }

    #[test]
    fn risk_and_liquidation_use_the_same_fields() {
        let warning = grace_period_warning(ctx(), 12_000, 12_500, 5_000, EventSeverity::Warning);
        assert_eq!(warning.topic, "risk_grace_warn");
        assert_eq!(warning.health_factor_bps, Some(12_000));

        let liquidation_event = liquidation(ctx(), 1_000, 1_100, 500, 9_800);
        assert_eq!(liquidation_event.topic, "liq_execute");
        assert_eq!(liquidation_event.severity, EventSeverity::Critical);
        assert_eq!(
            liquidation_event.payload,
            EventPayload::Liquidation {
                debt_liquidated: 1_000,
                collateral_seized: 1_100,
                incentive_bps: 500,
            }
        );
    }

    #[test]
    fn builders_can_produce_invalid_events_for_validation_tests() {
        let mut event = repay(ctx(), -1, 0, 0);
        assert_eq!(
            stellarlend_events::validate_event(&event),
            Err(EventError::InvalidAmount)
        );
        event.amount = 1;
        assert_eq!(stellarlend_events::validate_event(&event), Ok(()));
    }
}
