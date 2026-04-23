// ════════════════════════════════════════════════════════════════
// BATCH AGGREGATOR - Combines multiple transactions
// ════════════════════════════════════════════════════════════════

use soroban_sdk::{Env, Address, String, Vec, Val, Symbol};
use crate::{BatchOperation, BatchResult};

pub struct TransactionAggregator {
    /// Queue of pending transactions
    pub pending_transactions: Vec<BatchOperation>,
    
    /// Maximum batch size
    pub max_batch_size: u32,
    
    /// Auto-execute when batch is full?
    pub auto_execute: bool,
}

impl TransactionAggregator {
    /// Create new aggregator
    pub fn new(max_batch_size: u32, auto_execute: bool) -> Self {
        TransactionAggregator {
            pending_transactions: Vec::new(),
            max_batch_size,
            auto_execute,
        }
    }

    /// Add transaction to queue
    pub fn add_transaction(
        &mut self,
        function_name: String,
        params: Vec<Val>,
    ) -> bool {
        // Check if we're at capacity
        if self.pending_transactions.len() >= self.max_batch_size as usize {
            return false; // Queue full
        }

        let operation = BatchOperation {
            function_name,
            params,
            depends_on: None,
            required: true,
        };

        self.pending_transactions.push_back(operation);
        true
    }

    /// Get pending transaction count
    pub fn pending_count(&self) -> u32 {
        self.pending_transactions.len() as u32
    }

    /// Is batch ready to execute?
    pub fn is_batch_ready(&self) -> bool {
        self.pending_transactions.len() >= self.max_batch_size as usize
    }

    /// Get current batch
    pub fn get_pending_batch(&self) -> &Vec<BatchOperation> {
        &self.pending_transactions
    }

    /// Clear the batch
    pub fn clear_batch(&mut self) {
        self.pending_transactions = Vec::new();
    }

    /// Aggregate multiple operations with same function
    pub fn aggregate_similar_operations(
        operations: &Vec<BatchOperation>,
    ) -> Vec<BatchOperation> {
        let mut aggregated: Vec<BatchOperation> = Vec::new();

        for op in operations.iter() {
            // Try to find similar operation
            let mut found = false;

            for agg_op in aggregated.iter_mut() {
                if agg_op.function_name == op.function_name {
                    // Merge parameters
                    for param in op.params.iter() {
                        agg_op.params.push_back(param);
                    }
                    found = true;
                    break;
                }
            }

            if !found {
                aggregated.push_back(op.clone());
            }
        }

        aggregated
    }
}

/// Gas optimization strategies
pub enum GasOptimization {
    /// No optimization
    None,
    /// Combine similar operations
    Combine,
    /// Parallel execution where possible
    Parallel,
    /// Full optimization
    Full,
}

/// Optimize operations for gas efficiency
pub fn optimize_operations(
    env: &Env,
    operations: &Vec<BatchOperation>,
    strategy: GasOptimization,
) -> Vec<BatchOperation> {
    match strategy {
        GasOptimization::None => operations.clone(),
        
        GasOptimization::Combine => {
            TransactionAggregator::aggregate_similar_operations(operations)
        }
        
        GasOptimization::Parallel => {
            // Operations with no dependencies can run in parallel
            operations.clone() // Placeholder
        }
        
        GasOptimization::Full => {
            // Apply all optimizations
            let combined = TransactionAggregator::aggregate_similar_operations(operations);
            combined
        }
    }
}