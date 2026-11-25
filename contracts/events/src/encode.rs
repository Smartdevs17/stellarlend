//! Compact, schema-aware encoding for normalized contract events.
//!
//! The wire format omits field names and the derivable topic, uses stable
//! numeric tags for enums, and varint-encodes integers. See the crate README
//! for the version 1 field order.

use crate::{
    validate_event, EventAction, EventError, EventModule, EventPayload, EventSeverity,
    NormalizedEvent, SCHEMA_VERSION,
};

/// Errors raised while encoding a normalized event.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EncodeError {
    /// The event failed the normal envelope validation.
    InvalidEvent(EventError),
    /// The caller-provided output buffer could not hold the complete event.
    BufferTooSmall,
}

struct Writer<'a> {
    output: &'a mut [u8],
    position: usize,
}

impl Writer<'_> {
    fn write_byte(&mut self, value: u8) -> Result<(), EncodeError> {
        let slot = self
            .output
            .get_mut(self.position)
            .ok_or(EncodeError::BufferTooSmall)?;
        *slot = value;
        self.position += 1;
        Ok(())
    }

    fn write_unsigned(&mut self, mut value: u128) -> Result<(), EncodeError> {
        while value >= 0x80 {
            self.write_byte((value as u8 & 0x7f) | 0x80)?;
            value >>= 7;
        }
        self.write_byte(value as u8)
    }

    fn write_signed(&mut self, value: i128) -> Result<(), EncodeError> {
        let zigzag = ((value as u128) << 1) ^ ((value >> 127) as u128);
        self.write_unsigned(zigzag)
    }

    fn write_string(&mut self, value: &str) -> Result<(), EncodeError> {
        self.write_unsigned(value.len() as u128)?;
        for byte in value.bytes() {
            self.write_byte(byte)?;
        }
        Ok(())
    }

    fn write_optional_string(&mut self, value: Option<&str>) -> Result<(), EncodeError> {
        match value {
            Some(value) => {
                self.write_byte(1)?;
                self.write_string(value)
            }
            None => self.write_byte(0),
        }
    }
}

fn module_tag(module: EventModule) -> u8 {
    match module {
        EventModule::Pool => 0,
        EventModule::Risk => 1,
        EventModule::Liquidation => 2,
    }
}

fn action_tag(action: EventAction) -> u8 {
    match action {
        EventAction::Deposit => 0,
        EventAction::Withdraw => 1,
        EventAction::Borrow => 2,
        EventAction::Repay => 3,
        EventAction::Liquidate => 4,
        EventAction::RiskAlert => 5,
        EventAction::GracePeriodWarning => 6,
        EventAction::ConfigUpdated => 7,
    }
}

fn severity_tag(severity: EventSeverity) -> u8 {
    match severity {
        EventSeverity::Info => 0,
        EventSeverity::Warning => 1,
        EventSeverity::Critical => 2,
    }
}

/// Encode a validated event into the caller-provided buffer.
///
/// The returned length selects the encoded prefix of `output`. The buffer is
/// never allocated or retained by this crate.
pub fn encode_event(
    event: &NormalizedEvent<'_>,
    output: &mut [u8],
) -> Result<usize, EncodeError> {
    validate_event(event).map_err(EncodeError::InvalidEvent)?;

    let mut writer = Writer {
        output,
        position: 0,
    };

    writer.write_byte(SCHEMA_VERSION as u8)?;
    writer.write_byte(module_tag(event.module))?;
    writer.write_byte(action_tag(event.action))?;
    writer.write_byte(severity_tag(event.severity))?;

    writer.write_string(event.context.actor)?;
    writer.write_optional_string(event.context.asset)?;
    writer.write_optional_string(event.context.counterparty)?;
    writer.write_string(event.context.trace_id)?;
    writer.write_unsigned(event.context.timestamp as u128)?;
    writer.write_signed(event.amount)?;
    match event.health_factor_bps {
        Some(value) => {
            writer.write_byte(1)?;
            writer.write_signed(value)?;
        }
        None => writer.write_byte(0)?,
    }

    match event.payload {
        EventPayload::Pool {
            balance_after,
            debt_after,
        } => {
            writer.write_byte(0)?;
            writer.write_signed(balance_after)?;
            writer.write_signed(debt_after)?;
        }
        EventPayload::Risk {
            threshold_bps,
            utilization_bps,
        } => {
            writer.write_byte(1)?;
            writer.write_signed(threshold_bps)?;
            writer.write_signed(utilization_bps)?;
        }
        EventPayload::Liquidation {
            debt_liquidated,
            collateral_seized,
            incentive_bps,
        } => {
            writer.write_byte(2)?;
            writer.write_signed(debt_liquidated)?;
            writer.write_signed(collateral_seized)?;
            writer.write_signed(incentive_bps)?;
        }
        EventPayload::Config {
            previous_value,
            new_value,
        } => {
            writer.write_byte(3)?;
            writer.write_signed(previous_value)?;
            writer.write_signed(new_value)?;
        }
    }

    Ok(writer.position)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{new_event, EventContext};

    fn sample() -> NormalizedEvent<'static> {
        new_event(
            EventModule::Pool,
            EventAction::Deposit,
            EventSeverity::Info,
            EventContext {
                actor: "a",
                asset: None,
                counterparty: None,
                trace_id: "t",
                timestamp: 1,
            },
            1,
            None,
            EventPayload::Pool {
                balance_after: 1,
                debt_after: 0,
            },
        )
    }

    #[test]
    fn encodes_v1_using_compact_stable_tags_and_varints() {
        let mut output = [0; 32];
        let len = encode_event(&sample(), &mut output).unwrap();
        assert_eq!(
            &output[..len],
            &[1, 0, 0, 0, 1, b'a', 0, 0, 1, b't', 1, 2, 0, 0, 2, 0]
        );
    }

    #[test]
    fn reports_small_buffers_and_invalid_events() {
        assert_eq!(
            encode_event(&sample(), &mut [0; 2]),
            Err(EncodeError::BufferTooSmall)
        );

        let mut invalid = sample();
        invalid.context.timestamp = 0;
        assert_eq!(
            encode_event(&invalid, &mut [0; 32]),
            Err(EncodeError::InvalidEvent(EventError::InvalidTimestamp))
        );
    }
}
