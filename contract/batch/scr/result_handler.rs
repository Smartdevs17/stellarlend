// ════════════════════════════════════════════════════════════════
// RESULT HANDLER - Process batch results
// ════════════════════════════════════════════════════════════════

use soroban_sdk::{Env, String, Vec, Symbol};
use crate::{BatchResult, OperationResult};

pub struct ResultHandler;

impl ResultHandler {
    /// Get successful operations
    pub fn get_successful_operations(
        batch_result: &BatchResult,
    ) -> Vec<OperationResult> {
        let mut successful: Vec<OperationResult> = Vec::new();

        for result in batch_result.results.iter() {
            if result.success {
                successful.push_back(result.clone());
            }
        }

        successful
    }

    /// Get failed operations
    pub fn get_failed_operations(
        batch_result: &BatchResult,
    ) -> Vec<OperationResult> {
        let mut failed: Vec<OperationResult> = Vec::new();

        for result in batch_result.results.iter() {
            if !result.success {
                failed.push_back(result.clone());
            }
        }

        failed
    }

    /// Check if batch was fully successful
    pub fn is_fully_successful(batch_result: &BatchResult) -> bool {
        batch_result.failed_operations == 0
    }

    /// Get success rate as percentage
    pub fn get_success_rate(batch_result: &BatchResult) -> u32 {
        if batch_result.total_operations == 0 {
            return 0;
        }

        ((batch_result.successful_operations as u64 * 100)
            / batch_result.total_operations as u64) as u32
    }

    /// Log batch results
    pub fn log_results(env: &Env, batch_result: &BatchResult) {
        env.events().publish(
            Symbol::new(env, "batch_summary"),
            (
                batch_result.batch_id,
                batch_result.total_operations,
                batch_result.successful_operations,
                batch_result.failed_operations,
                batch_result.gas_estimate,
            ),
        );
    }

    /// Get result details as string
    pub fn get_result_summary(
        env: &Env,
        batch_result: &BatchResult,
    ) -> String {
        let success_rate = Self::get_success_rate(batch_result);
        
        format!(
            "Batch {}: {}/{} ops successful ({}%), Gas: {}",
            batch_result.batch_id,
            batch_result.successful_operations,
            batch_result.total_operations,
            success_rate,
            batch_result.gas_estimate,
        )
    }

    /// Rollback batch if atomic execution failed
    pub fn handle_atomic_failure(
        env: &Env,
        batch_result: &BatchResult,
    ) -> bool {
        if !batch_result.atomic {
            return false;
        }

        if batch_result.failed_operations > 0 {
            // Rollback all operations
            env.events().publish(
                Symbol::new(env, "batch_rolled_back"),
                batch_result.batch_id,
            );
            return true;
        }

        false
    }
}