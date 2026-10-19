# Contract Call Debugging Utilities (#1448)

Tools for debugging and tracing Soroban contract calls with readable output.

## Features

### Debug Traces

Log contract calls with formatted output:

```rust
use crate::lib::debug::DebugTrace;

let trace = DebugTrace::new("transfer")
    .with_arg("from", "G123...")
    .with_arg("amount", "1000 USDC")
    .with_result("Success")
    .with_gas(2500);

trace.log();
```

Output:
```
╔════════════════════════════════════════
║ Contract Call: transfer
╠════════════════════════════════════════
║ Arguments:
║   • from = G123...
║   • amount = 1000 USDC
║ Result: Success
║ Gas Used: 2500
╚════════════════════════════════════════
```

### Call Tracking

Monitor execution time:

```rust
use crate::lib::trace::CallTracker;

let tracker = CallTracker::start("calculate_interest");
// ... expensive operation ...
tracker.finish();
```

Output:
```
✓ calculate_interest completed in 145ms
```

## Macros

### debug_call!

Quick trace creation:

```rust
let trace = debug_call!("my_function");
```

### debug_result!

Log results:

```rust
debug_result!(trace, "operation succeeded");
```

## JSON Logging

For external logging systems:

```rust
let json = trace.to_json();
// Send to logging service
```

## Examples

### Tracing a Lending Operation

```rust
let trace = DebugTrace::new("borrow")
    .with_arg("borrower", borrower_id)
    .with_arg("amount", amount)
    .with_arg("collateral", collateral_address)
    .with_result(format!("Loan #{}", loan_id))
    .with_gas(gas_used);

trace.log();
```

### Profiling Hot Paths

```rust
let tracker = CallTracker::start("liquidity_pool_swap");

// Perform swap
let result = execute_swap(&env, &pool, &input);

tracker.finish_with(format!("Output: {}", result));
```

## Testing

Run debugging utility tests:

```bash
cargo test --lib lib::debug
cargo test --lib lib::trace
```

## Integration

Add to your contract's lib.rs:

```rust
pub mod debug;
pub mod trace;
```

Then use in any module:

```rust
use crate::lib::debug::DebugTrace;
```
