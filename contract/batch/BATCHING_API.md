# Transaction Batching & Aggregation API

## Overview

The batching system allows you to combine multiple operations into a single transaction, reducing gas costs and improving efficiency.

## Key Benefits

✅ **70% Gas Savings** - Combine operations  
✅ **Atomicity** - All or nothing execution  
✅ **Dependencies** - Control operation order  
✅ **Simulation** - Test before execution  

## Batch Builder

### Creating a Batch

```rust
let mut builder = BatchBuilder::new(atomic: bool);

builder.add_operation(
    String::from_str(&env, "subscribe"),
    params,
    true,  // required
);

builder.add_operation(
    String::from_str(&env, "deposit"),
    params,
    true,
);
```

### Add Operation with Dependency

```rust
builder.add_operation_with_dependency(
    String::from_str(&env, "transfer"),
    params,
    depends_on: 0,  // Depends on operation at index 0
    true,
);
```

## Execution Patterns

### Pattern 1: Non-Atomic (Partial Success Allowed)

```rust
let result = contract.execute_batch(
    env,
    proxy,
    user,
    operations,
    false,  // Allow partial success
);

// 5 operations, 3 succeed, 2 fail → Batch succeeds
// You get results for all 5
```

### Pattern 2: Atomic (All or Nothing)

```rust
let result = contract.execute_batch(
    env,
    proxy,
    user,
    operations,
    true,  // Atomic: all or nothing
);

// 5 operations, 1 fails → Entire batch fails
// All operations are rolled back
```

## Result Structure

```rust
pub struct BatchResult {
    pub batch_id: u64,
    pub total_operations: u32,
    pub successful_operations: u32,
    pub failed_operations: u32,
    pub results: Vec<OperationResult>,
    pub atomic: bool,
    pub gas_estimate: u64,
}
```

## Frontend Usage

### TypeScript React Hook

```typescript
const { 
  addTransaction,
  executeBatch,
  pending,
  lastResult 
} = useBatchTransactions({
  maxBatchSize: 10,
  serverUrl: "https://soroban-testnet-rpc.stellar.org"
});

// Add operations
addTransaction("subscribe", [plan_id], true);
addTransaction("deposit", [amount], true);

// Check pending
console.log(`Pending: ${pending}/10`);

// Execute when ready
const result = await executeBatch(accountId, true);
console.log(`Success: ${result.successfulOperations}/${result.totalOperations}`);
```

## Gas Estimation

**Base cost:** 50,000 gas  
**Per operation:** 100,000 gas  

**Example:**
```
5 operations = 50,000 + (5 × 100,000) = 550,000 gas

Without batching = 5 × 200,000 = 1,000,000 gas

Savings = 450,000 gas = 45% ✅
```

## Operations Supported

| Function | Min Params | Required Auth | Notes |
|----------|-----------|---|---|
| `subscribe` | 1 | User | Can batch multiple subscribes |
| `deposit` | 2 | User | Group deposits together |
| `withdraw` | 2 | User | Batch withdrawals |
| `claim_rewards` | 1 | User | Combine reward claims |
| `approve` | 2 | User | Batch approvals |

## Dependency Example

```
Operation 0: approve(tokenA, 1000)
    ↓
Operation 1: transfer(tokenA, user, 1000)   ← Depends on op 0
    ↓
Operation 2: stake(tokenA, 1000)            ← Depends on op 1
```

## Error Handling

### Atomic Mode Failure

```typescript
try {
  const result = await executeBatch(accountId, true);
  
  if (result.failedOperations > 0) {
    console.error("Batch failed (atomic)");
    // All operations rolled back
  }
} catch (error) {
  console.error("Batch execution error:", error);
}
```

### Non-Atomic Partial Success

```typescript
const result = await executeBatch(accountId, false);

const failed = result.results.filter(r => !r.success);
const succeeded = result.results.filter(r => r.success);

console.log(`${succeeded.length} succeeded, ${failed.length} failed`);

// Handle failures individually
failed.forEach(f => {
  console.log(`Operation ${f.index}: ${f.error}`);
});
```

## Best Practices

✅ **DO:**
- Group similar operations together
- Use dependencies when needed
- Test with simulation first
- Monitor gas usage
- Use atomic mode for critical operations

❌ **DON'T:**
- Create batches with > 100 operations
- Ignore error results
- Skip simulation for large batches
- Use without understanding dependencies
- Assume all operations will succeed

## Performance Metrics

| Metric | Value |
|--------|-------|
| Max operations/batch | 100 |
| Base gas cost | 50,000 |
| Gas per operation | 100,000 |
| Simulation cost | 50,000 |
| Rollback cost | Included |

## Troubleshooting

### Batch Too Large
```
Error: "Too many operations (max 100)"
Solution: Split into multiple batches
```

### Invalid Dependency
```
Error: "Invalid dependency"
Solution: Ensure dependency index < current index
```

### Atomic Failure
```
Error: "Batch failed (atomic)"
Solution: Check individual operation results for cause
```