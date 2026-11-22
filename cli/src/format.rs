use crate::error::CliError;
use serde::Serialize;

/// Parse human-readable Stellar decimal amounts (7 fractional digits max)
/// or special sentinel values like "max" into stroops (i128).
///
/// 1 XLM = 10_000_000 stroops (7 decimal places).
pub fn parse_amount(s: &str) -> Result<i128, CliError> {
    let trimmed = s.trim();
    if trimmed.eq_ignore_ascii_case("max") {
        return Ok(i128::MAX);
    }

    let parts: Vec<&str> = trimmed.split('.').collect();
    match parts.len() {
        1 => {
            let whole: i128 = parts[0]
                .parse()
                .map_err(|_| CliError::Validation(format!("Invalid integer amount: '{s}'")))?;
            whole
                .checked_mul(10_000_000)
                .ok_or_else(|| CliError::Validation(format!("Amount overflow: '{s}'")))
        }
        2 => {
            let whole: i128 = if parts[0].is_empty() {
                0
            } else {
                parts[0]
                    .parse()
                    .map_err(|_| CliError::Validation(format!("Invalid whole amount: '{s}'")))?
            };

            let frac_str = parts[1];
            if frac_str.len() > 7 {
                return Err(CliError::Validation(format!(
                    "Too many decimal places in '{s}' (maximum 7 allowed for Stellar/Soroban)"
                )));
            }

            // Pad fraction to 7 digits
            let mut padded = frac_str.to_string();
            while padded.len() < 7 {
                padded.push('0');
            }
            let frac: i128 = padded
                .parse()
                .map_err(|_| CliError::Validation(format!("Invalid fractional amount: '{s}'")))?;

            let whole_stroops = whole
                .checked_mul(10_000_000)
                .ok_or_else(|| CliError::Validation(format!("Amount overflow: '{s}'")))?;

            if whole >= 0 {
                whole_stroops
                    .checked_add(frac)
                    .ok_or_else(|| CliError::Validation(format!("Amount overflow: '{s}'")))
            } else {
                whole_stroops
                    .checked_sub(frac)
                    .ok_or_else(|| CliError::Validation(format!("Amount overflow: '{s}'")))
            }
        }
        _ => Err(CliError::Validation(format!(
            "Invalid decimal format: '{s}'"
        ))),
    }
}

/// Formats stroops (i128) into standard human-readable Stellar amount string.
pub fn format_amount(stroops: i128) -> String {
    if stroops == i128::MAX {
        return "MAX".to_string();
    }

    let is_neg = stroops < 0;
    let abs_val = stroops.abs();
    let whole = abs_val / 10_000_000;
    let frac = abs_val % 10_000_000;

    let sign = if is_neg { "-" } else { "" };
    format!("{}{}.{:07}", sign, whole, frac)
}

/// Formats basis points (bps) into percentage string (e.g. 500 bps -> 5.00%).
pub fn format_percent(bps: u32) -> String {
    let whole = bps / 100;
    let frac = bps % 100;
    format!("{}.{:02}%", whole, frac)
}

/// Formats health factor with warning indicators.
pub fn format_health_factor(hf: f64) -> String {
    if hf >= 1.5 {
        format!("{:.4} (Safe)", hf)
    } else if hf >= 1.0 {
        format!("{:.4} (Warning - At Risk)", hf)
    } else {
        format!("{:.4} (Liquidatable!)", hf)
    }
}

/// Prints a serializable result either as pretty JSON or human-readable format.
pub fn print_result<T: Serialize + std::fmt::Display>(val: &T, as_json: bool) {
    if as_json {
        match serde_json::to_string_pretty(val) {
            Ok(json) => println!("{json}"),
            Err(e) => eprintln!("Failed to format JSON: {e}"),
        }
    } else {
        println!("{val}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_amount() {
        assert_eq!(parse_amount("10").unwrap(), 100_000_000);
        assert_eq!(parse_amount("10.5").unwrap(), 105_000_000);
        assert_eq!(parse_amount("0.0000001").unwrap(), 1);
        assert_eq!(parse_amount("max").unwrap(), i128::MAX);
        assert_eq!(parse_amount("MAX").unwrap(), i128::MAX);
        assert!(parse_amount("10.12345678").is_err());
        assert!(parse_amount("abc").is_err());
    }

    #[test]
    fn test_format_amount() {
        assert_eq!(format_amount(100_000_000), "10.0000000");
        assert_eq!(format_amount(105_000_000), "10.5000000");
        assert_eq!(format_amount(1), "0.0000001");
        assert_eq!(format_amount(i128::MAX), "MAX");
    }

    #[test]
    fn test_format_percent() {
        assert_eq!(format_percent(500), "5.00%");
        assert_eq!(format_percent(1234), "12.34%");
        assert_eq!(format_percent(5), "0.05%");
    }
}
