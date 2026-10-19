//! Contract Call Debugging Utilities (#1448)
//!
//! Provides readable traces and debugging helpers for Soroban contract calls.

use soroban_sdk::{log, Address, Bytes, Env, Symbol, Val, Vec as SorobanVec};
use std::fmt;

/// Debug formatter for contract calls
#[derive(Clone)]
pub struct DebugTrace {
    function: String,
    args: Vec<String>,
    result: Option<String>,
    gas_used: Option<u64>,
    error: Option<String>,
}

impl DebugTrace {
    /// Create a new debug trace for a contract function call
    pub fn new(function: impl Into<String>) -> Self {
        Self {
            function: function.into(),
            args: Vec::new(),
            result: None,
            gas_used: None,
            error: None,
        }
    }

    /// Add an argument to the trace
    pub fn with_arg(mut self, name: impl Into<String>, value: impl fmt::Display) -> Self {
        self.args.push(format!("{} = {}", name.into(), value));
        self
    }

    /// Record the result of the call
    pub fn with_result(mut self, result: impl fmt::Display) -> Self {
        self.result = Some(result.to_string());
        self
    }

    /// Record gas usage
    pub fn with_gas(mut self, gas: u64) -> Self {
        self.gas_used = Some(gas);
        self
    }

    /// Record an error
    pub fn with_error(mut self, error: impl fmt::Display) -> Self {
        self.error = Some(error.to_string());
        self
    }

    /// Log the trace in a readable format
    pub fn log(&self) {
        println!("╔════════════════════════════════════════");
        println!("║ Contract Call: {}", self.function);
        println!("╠════════════════════════════════════════");

        if !self.args.is_empty() {
            println!("║ Arguments:");
            for arg in &self.args {
                println!("║   • {}", arg);
            }
        }

        if let Some(ref result) = self.result {
            println!("║ Result: {}", result);
        }

        if let Some(gas) = self.gas_used {
            println!("║ Gas Used: {}", gas);
        }

        if let Some(ref error) = self.error {
            println!("║ Error: {}", error);
        }

        println!("╚════════════════════════════════════════");
    }

    /// Format as JSON for logging
    pub fn to_json(&self) -> String {
        format!(
            r#"{{ "function": "{}", "args": [{}], "result": "{}", "gas": {} }}"#,
            self.function,
            self.args
                .iter()
                .map(|a| format!(r#""{}""#, a))
                .collect::<Vec<_>>()
                .join(", "),
            self.result.as_ref().unwrap_or(&"null".to_string()),
            self.gas_used.unwrap_or(0)
        )
    }
}

/// Macro for logging contract calls with readable format
#[macro_export]
macro_rules! debug_call {
    ($func:expr) => {{
        let trace = $crate::lib::debug::DebugTrace::new($func);
        trace.log();
        trace
    }};
}

/// Macro for logging contract call results
#[macro_export]
macro_rules! debug_result {
    ($trace:expr, $result:expr) => {{
        let trace = $trace.with_result($result);
        trace.log();
        trace
    }};
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_debug_trace_creation() {
        let trace = DebugTrace::new("test_function")
            .with_arg("param1", "value1")
            .with_arg("param2", "value2")
            .with_result("success");

        assert_eq!(trace.function, "test_function");
        assert_eq!(trace.args.len(), 2);
    }

    #[test]
    fn test_debug_trace_json() {
        let trace = DebugTrace::new("test")
            .with_arg("x", 42)
            .with_result("ok")
            .with_gas(12345);

        let json = trace.to_json();
        assert!(json.contains("\"function\": \"test\""));
        assert!(json.contains("\"gas\": 12345"));
    }
}
