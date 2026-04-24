# Accounting Contract - Revenue Recognition

## Overview

The Accounting Contract implements subscription revenue recognition accounting for the StellarLend protocol. It provides configurable revenue recognition rules, deferred revenue tracking, and comprehensive analytics.

## Features

- **Revenue Recognition Methods**
  - Straight-line recognition
  - Usage-based recognition
  - Milestone-based recognition

- **Deferred Revenue Management**
  - Automatic tracking
  - Real-time updates
  - Merchant aggregation

- **Revenue Schedules**
  - Automatic generation
  - Period-by-period breakdown
  - Progress tracking

- **Contract Lifecycle**
  - Modifications
  - Cancellations
  - Pro-rata calculations

- **Analytics**
  - Period-based reporting
  - Subscription metrics
  - Revenue forecasting

## Contract Entrypoints

### Initialization

```rust
pub fn initialize(env: Env, admin: Address) -> Result<(), AccountingError>
```

Initialize the contract with an admin address.

### Revenue Recognition

```rust
pub fn recognize_revenue(
    env: Env,
    caller: Address,
    subscription_id: u64,
) -> Result<Recognition, AccountingError>
```

Recognize revenue for a subscription based on configured rules.

### Configuration

```rust
pub fn configure_recognition_rule(
    env: Env,
    caller: Address,
    subscription_id: u64,
    method: u32,
    recognition_period: u64,
) -> Result<(), AccountingError>
```

Configure how revenue should be recognized for a subscription.

**Parameters:**
- `method`: 0 = Straight-Line, 1 = Usage-Based, 2 = Milestone-Based
- `recognition_period`: Duration in seconds

### Query Functions

```rust
pub fn get_deferred_revenue(
    env: Env,
    merchant_id: Address,
) -> Result<i128, AccountingError>
```

Get total deferred revenue for a merchant.

```rust
pub fn get_revenue_schedule(
    env: Env,
    subscription_id: u64,
) -> Result<RevenueSchedule, AccountingError>
```

Get the complete revenue schedule for a subscription.

```rust
pub fn get_revenue_analytics(
    env: Env,
    merchant_id: Address,
    start_time: u64,
    end_time: u64,
) -> Result<RevenueAnalytics, AccountingError>
```

Get revenue analytics for a merchant within a time period.

### Contract Management

```rust
pub fn handle_contract_modification(
    env: Env,
    caller: Address,
    subscription_id: u64,
    new_amount: i128,
) -> Result<(), AccountingError>
```

Handle mid-period contract modifications.

```rust
pub fn handle_cancellation(
    env: Env,
    caller: Address,
    subscription_id: u64,
) -> Result<i128, AccountingError>
```

Handle subscription cancellation and calculate refund.

## Building

```bash
# Build the contract
stellar contract build

# Or use make
make build
```

## Testing

```bash
# Run all tests
cargo test

# Run with output
cargo test -- --nocapture

# Run specific test
cargo test test_straight_line_revenue_recognition

# Or use make
make test
```

## Deployment

### Testnet

```bash
# Build
stellar contract build

# Deploy
stellar contract deploy \
  --wasm target/wasm32-unknown-unknown/release/accounting.wasm \
  --network testnet \
  --source <admin-keypair>

# Initialize
stellar contract invoke \
  --id <contract-id> \
  --network testnet \
  --source <admin-keypair> \
  -- initialize \
  --admin <admin-address>
```

### Mainnet

```bash
# Build and optimize
stellar contract build
stellar contract optimize \
  --wasm target/wasm32-unknown-unknown/release/accounting.wasm

# Deploy
stellar contract deploy \
  --wasm target/wasm32-unknown-unknown/release/accounting-optimized.wasm \
  --network mainnet \
  --source <admin-keypair>
```

## Usage Example

```rust
// Create subscription (helper function)
let subscription_id = create_subscription(
    &env,
    merchant_id.clone(),
    12_000_000, // 12M stroops
    start_time,
)?;

// Configure straight-line recognition over 1 year
configure_recognition_rule(
    &env,
    merchant_id.clone(),
    subscription_id,
    0, // Straight-line
    365 * 24 * 60 * 60, // 1 year
)?;

// Recognize revenue (call monthly)
let recognition = recognize_revenue(
    &env,
    merchant_id.clone(),
    subscription_id,
)?;

// Check deferred revenue
let deferred = get_deferred_revenue(&env, merchant_id)?;
```

## Error Handling

The contract uses a comprehensive error enum:

```rust
pub enum AccountingError {
    Unauthorized = 1,
    AlreadyInitialized = 2,
    NotInitialized = 3,
    InvalidSubscriptionId = 4,
    InvalidRecognitionMethod = 5,
    InvalidPeriod = 6,
    InvalidAmount = 7,
    SubscriptionNotFound = 8,
    SubscriptionAlreadyExists = 9,
    SubscriptionNotActive = 10,
    SubscriptionAlreadyCancelled = 11,
    NoRevenueToRecognize = 12,
    RecognitionPeriodNotElapsed = 13,
    InvalidMerchant = 14,
    ArithmeticOverflow = 15,
    ArithmeticUnderflow = 16,
    DivisionByZero = 17,
}
```

## Storage Keys

The contract uses the following storage keys:

- `Admin` - Contract admin address
- `Initialized` - Initialization flag
- `RecognitionRule(u64)` - Recognition rule for subscription
- `SubscriptionState(u64)` - Subscription state
- `DeferredRevenue(Address)` - Merchant deferred revenue
- `RecognizedRevenue(Address)` - Merchant recognized revenue
- `RevenueSchedule(u64)` - Revenue schedule
- `MerchantSubscriptions(Address)` - List of merchant subscriptions
- `NextSubscriptionId` - Counter for subscription IDs

## Security Considerations

1. **Authorization**: All write operations require caller authentication
2. **Merchant Validation**: Only subscription owner can modify/recognize
3. **Admin Controls**: Admin can initialize but not modify subscriptions
4. **Arithmetic Safety**: All calculations use checked arithmetic
5. **State Consistency**: Atomic updates prevent inconsistent states

## Performance

- **Gas Efficiency**: Optimized for minimal storage operations
- **Batch Operations**: Support for bulk recognition (future)
- **Caching**: Efficient storage key design

## Compliance

Implements revenue recognition following:
- ASC 606 (US GAAP)
- IFRS 15 (International)

## Contributing

See [CONTRIBUTING.md](../../../../CONTRIBUTING.md) for guidelines.

## License

MIT License - see [LICENSE](../../../../LICENSE) for details.
