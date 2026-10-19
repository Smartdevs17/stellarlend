// SPDX-License-Identifier: Apache-2.0
use soroban_sdk::{Env, symbol};
use crate::primitives::LoanPrimitive;

/// Trait for custom flash loan execution logic
pub trait FlashLoanExecutor {
    type Operation;
    
    /// Execute a single loan operation
    fn execute(&self, env: &Env, primitive: &LoanPrimitive<Self::Operation>) -> i128;
}

/// Default executor for simple flash loans
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DefaultExecutor;

impl FlashLoanExecutor for DefaultExecutor {
    type Operation = ();
    
    fn execute(&self, _env: &Env, _primitive: &LoanPrimitive<()>) -> i128 {
        // Default implementation returns 0 for simplicity
        // Custom logic would override this
        0
    }
}