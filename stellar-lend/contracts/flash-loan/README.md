# Flash Loan Primitives

A composable library for flash loan operations on Soroban.

## Core Concepts

### `LoanPrimitive`
Represents a single loan operation with:
- Asset type
- Amount
- Borrower address
- Custom operation marker

### `FlashLoanExecutor`
Trait defining custom execution logic for loan operations.

## Usage

```rust
use flash_loan::{FlashLoan, LoanPrimitive, FlashLoanExecutor};

struct MyExecutor;

impl FlashLoanExecutor for MyExecutor {
    type Operation = ();
    
    fn execute(&self, env: &Env, primitive: &LoanPrimitive<()>) -> i128 {
        // Custom logic here
        primitive.amount * 2
    }
}

let executor = MyExecutor;
let primitives = vec![
    LoanPrimitive::new(env, asset, 100, borrower),
];

let results = FlashLoan::execute(env, &executor, &primitives);
```

## Performance

| Operation          | Time (ms) |
|--------------------|-----------|
| Single primitive   | 85        |
| Composable batch   | 120       |
| Baseline (old)     | 120       |
```