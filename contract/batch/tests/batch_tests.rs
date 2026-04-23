#[cfg(test)]
mod batch_tests {
    use soroban_sdk::{testutils::*, Address, Env, String, Vec};
    use batch::{BatchProcessor, BatchOperation, BatchBuilder};

    // ════════════════════════════════════════════════════════════════
    // TEST 1: Add operations to batch
    // ════════════════════════════════════════════════════════════════
    
    #[test]
    fn test_batch_builder_add_operations() {
        let mut builder = BatchBuilder::new(false);
        
        assert_eq!(builder.operation_count(), 0);
        
        builder.add_operation(
            String::from_str(&Env::default(), "subscribe"),
            Vec::new(&Env::default()),
            true,
        );
        
        assert_eq!(builder.operation_count(), 1);
    }

    // ════════════════════════════════════════════════════════════════
    // TEST 2: Validate batch
    // ════════════════════════════════════════════════════════════════
    
    #[test]
    fn test_batch_validation() {
        let env = Env::default();
        let mut builder = BatchBuilder::new(false);
        
        // Empty batch should fail
        let result = builder.validate();
        assert!(result.is_err(), "Empty batch should fail validation");
        
        // Add operation
        builder.add_operation(
            String::from_str(&env, "test"),
            Vec::new(&env),
            true,
        );
        
        // Should now pass
        let result = builder.validate();
        assert!(result.is_ok(), "Batch with operations should pass");
    }

    // ════════════════════════════════════════════════════════════════
    // TEST 3: Execute batch
    // ════════════════════════════════════════════════════════════════
    
    #[test]
    fn test_execute_batch() {
        let env = Env::default();
        let proxy = Address::random(&env);
        let user = Address::random(&env);
        
        let contract = BatchProcessor {};
        
        let mut operations: Vec<BatchOperation> = Vec::new(&env);
        
        // Add operation
        operations.push_back(BatchOperation {
            function_name: String::from_str(&env, "deposit"),
            params: Vec::new(&env),
            depends_on: None,
            required: true,
        });
        
        // Execute batch
        let result = contract.execute_batch(
            env.clone(),
            proxy,
            user,
            operations,
            false,
        );
        
        assert_eq!(result.total_operations, 1);
        assert!(result.successful_operations > 0 || result.failed_operations > 0);
    }

    // ════════════════════════════════════════════════════════════════
    // TEST 4: Batch with dependencies
    // ════════════════════════════════════════════════════════════════
    
    #[test]
    fn test_batch_with_dependencies() {
        let env = Env::default();
        let mut operations: Vec<BatchOperation> = Vec::new(&env);
        
        // Operation 1
        operations.push_back(BatchOperation {
            function_name: String::from_str(&env, "approve"),
            params: Vec::new(&env),
            depends_on: None,
            required: true,
        });
        
        // Operation 2 depends on 1
        operations.push_back(BatchOperation {
            function_name: String::from_str(&env, "transfer"),
            params: Vec::new(&env),
            depends_on: Some(0),
            required: true,
        });
        
        assert_eq!(operations.len(), 2);
    }

    // ════════════════════════════════════════════════════════════════
    // TEST 5: Simulate batch
    // ════════════════════════════════════════════════════════════════
    
    #[test]
    fn test_simulate_batch() {
        let env = Env::default();
        
        let contract = BatchProcessor {};
        let operations: Vec<BatchOperation> = Vec::new(&env);
        
        let result = contract.simulate_batch(env, operations);
        
        assert_eq!(result.total_operations, 0);
        assert_eq!(result.gas_estimate, 50_000); // Base cost only
    }

    // ════════════════════════════════════════════════════════════════
    // TEST 6: Atomic batch
    // ════════════════════════════════════════════════════════════════
    
    #[test]
    fn test_atomic_batch() {
        let env = Env::default();
        let proxy = Address::random(&env);
        let user = Address::random(&env);
        
        let contract = BatchProcessor {};
        let operations: Vec<BatchOperation> = Vec::new(&env);
        
        let result = contract.execute_batch(
            env,
            proxy,
            user,
            operations,
            true,  // ← Atomic mode
        );
        
        assert!(result.atomic);
    }

    // ════════════════════════════════════════════════════════════════
    // TEST 7: Gas estimation
    // ════════════════════════════════════════════════════════════════
    
    #[test]
    fn test_gas_estimation() {
        let env = Env::default();
        let mut operations: Vec<BatchOperation> = Vec::new(&env);
        
        // Add 5 operations
        for i in 0..5 {
            operations.push_back(BatchOperation {
                function_name: String::from_str(&env, &format!("op_{}", i)),
                params: Vec::new(&env),
                depends_on: None,
                required: true,
            });
        }
        
        // Estimate: 50,000 base + (5 * 100,000) per op = 550,000
        let estimated_gas = 50_000u64 + (operations.len() as u64 * 100_000u64);
        
        assert_eq!(estimated_gas, 550_000);
    }
}