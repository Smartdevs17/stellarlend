# Revenue Recognition Accounting

## Overview

The Revenue Recognition Accounting module implements subscription-based revenue recognition following accounting standards (ASC 606 / IFRS 15). It provides configurable rules for recognizing revenue over time, tracking deferred revenue, and handling contract modifications and cancellations.

## Features

- **Multiple Recognition Methods**
  - Straight-line recognition over subscription period
  - Usage-based recognition
  - Milestone-based recognition

- **Deferred Revenue Tracking**
  - Automatic calculation of deferred revenue
  - Real-time updates on revenue recognition
  - Merchant-level aggregation

- **Revenue Schedule Generation**
  - Automatic schedule creation based on recognition rules
  - Period-by-period breakdown
  - Progress tracking

- **Contract Modifications**
  - Handle mid-period contract changes
  - Automatic schedule regeneration
  - Proper accounting treatment

- **Cancellation Handling**
  - Pro-rata refund calculation
  - Deferred revenue adjustment
  - Cancellation tracking

- **Revenue Analytics**
  - Period-based reporting
  - Subscription metrics
  - Average subscription value

## Architecture

### Smart Contract Module

Located at: `stellar-lend/contracts/accounting/`

**Key Components:**
- `revenue.rs` - Core revenue recognition logic
- `types.rs` - Data structures and enums
- `storage.rs` - Storage key definitions
- `errors.rs` - Error types

### API Layer

Located at: `api/src/`

**Key Components:**
- `controllers/accounting.controller.ts` - HTTP request handlers
- `services/accounting.service.ts` - Business logic and contract interaction
- `routes/accounting.routes.ts` - API route definitions

### Frontend Layer

Located at: `app/`

**Key Components:**
- `stores/accountingStore.ts` - State management with Zustand
- `screens/RevenueReportScreen.tsx` - Revenue reporting UI

## Data Structures

### RevenueRecognitionRule

Defines how revenue should be recognized for a subscription.

```rust
pub struct RevenueRecognitionRule {
    pub subscription_id: u64,
    pub method: RecognitionMethod,
    pub recognition_period: u64,
    pub merchant_id: Address,
    pub total_amount: i128,
    pub start_time: u64,
    pub end_time: u64,
    pub created_at: u64,
}
```

### Recognition

Records a revenue recognition event.

```rust
pub struct Recognition {
    pub subscription_id: u64,
    pub merchant_id: Address,
    pub recognized_amount: i128,
    pub deferred_amount: i128,
    pub recognition_date: u64,
    pub period_start: u64,
    pub period_end: u64,
}
```

### RevenueSchedule

Complete schedule of revenue recognition for a subscription.

```rust
pub struct RevenueSchedule {
    pub subscription_id: u64,
    pub merchant_id: Address,
    pub total_amount: i128,
    pub total_recognized: i128,
    pub total_deferred: i128,
    pub entries: Vec<ScheduleEntry>,
    pub method: RecognitionMethod,
}
```

### SubscriptionState

Current state of a subscription.

```rust
pub struct SubscriptionState {
    pub subscription_id: u64,
    pub merchant_id: Address,
    pub total_amount: i128,
    pub recognized_amount: i128,
    pub deferred_amount: i128,
    pub start_time: u64,
    pub end_time: u64,
    pub is_active: bool,
    pub is_cancelled: bool,
    pub cancellation_time: Option<u64>,
    pub last_recognition_time: u64,
}
```

## Recognition Methods

### 1. Straight-Line Recognition

Revenue is recognized evenly over the subscription period.

**Formula:**
```
recognized_amount = (total_amount × elapsed_time) / total_period
```

**Use Case:** Standard subscription services with consistent value delivery.

**Example:**
- $12,000 annual subscription
- Recognized at $1,000 per month
- Deferred revenue decreases linearly

### 2. Usage-Based Recognition

Revenue is recognized based on actual usage metrics.

**Use Case:** Pay-as-you-go services, API usage, metered billing.

**Implementation:** Requires integration with usage tracking system.

### 3. Milestone-Based Recognition

Revenue is recognized when specific milestones are achieved.

**Use Case:** Project-based services, deliverable-based contracts.

**Implementation:** Requires milestone completion tracking.

## API Endpoints

### Create Subscription

```http
POST /api/accounting/subscriptions
Content-Type: application/json

{
  "merchantId": "GXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX",
  "totalAmount": "12000000",
  "startTime": 1704067200
}
```

**Response:**
```json
{
  "success": true,
  "data": {
    "subscriptionId": 1
  }
}
```

### Configure Recognition Rule

```http
POST /api/accounting/subscriptions/:subscriptionId/configure
Content-Type: application/json

{
  "method": 0,
  "recognitionPeriod": 31536000
}
```

**Parameters:**
- `method`: 0 = Straight-Line, 1 = Usage-Based, 2 = Milestone-Based
- `recognitionPeriod`: Duration in seconds

### Recognize Revenue

```http
POST /api/accounting/subscriptions/:subscriptionId/recognize
```

**Response:**
```json
{
  "success": true,
  "data": {
    "subscriptionId": 1,
    "merchantId": "GXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX",
    "recognizedAmount": "1000000",
    "deferredAmount": "11000000",
    "recognitionDate": 1704153600,
    "periodStart": 1704067200,
    "periodEnd": 1704153600
  }
}
```

### Get Deferred Revenue

```http
GET /api/accounting/merchants/:merchantId/deferred-revenue
```

**Response:**
```json
{
  "success": true,
  "data": {
    "deferredRevenue": "24000000"
  }
}
```

### Get Revenue Schedule

```http
GET /api/accounting/subscriptions/:subscriptionId/schedule
```

**Response:**
```json
{
  "success": true,
  "data": {
    "subscriptionId": 1,
    "merchantId": "GXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX",
    "totalAmount": "12000000",
    "totalRecognized": "3000000",
    "totalDeferred": "9000000",
    "entries": [
      {
        "periodStart": 1704067200,
        "periodEnd": 1706745600,
        "scheduledAmount": "1000000",
        "recognizedAmount": "1000000",
        "isRecognized": true
      }
    ],
    "method": 0
  }
}
```

### Get Revenue Analytics

```http
GET /api/accounting/merchants/:merchantId/analytics?startTime=1704067200&endTime=1735689600
```

**Response:**
```json
{
  "success": true,
  "data": {
    "merchantId": "GXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX",
    "periodStart": 1704067200,
    "periodEnd": 1735689600,
    "totalRevenue": "36000000",
    "recognizedRevenue": "12000000",
    "deferredRevenue": "24000000",
    "subscriptionCount": 3,
    "averageSubscriptionValue": "12000000"
  }
}
```

### Modify Contract

```http
PUT /api/accounting/subscriptions/:subscriptionId/modify
Content-Type: application/json

{
  "newAmount": "15000000"
}
```

### Cancel Subscription

```http
POST /api/accounting/subscriptions/:subscriptionId/cancel
```

**Response:**
```json
{
  "success": true,
  "data": {
    "refundAmount": "9000000"
  },
  "message": "Subscription cancelled successfully"
}
```

## Edge Cases

### Contract Modifications

When a contract is modified mid-period:

1. Calculate recognized revenue up to modification date
2. Adjust deferred revenue for the difference
3. Regenerate revenue schedule with new amount
4. Continue recognition from modification date

**Example:**
- Original: $12,000/year
- After 3 months: Upgrade to $18,000/year
- Recognized: $3,000 (3 months)
- New deferred: $15,000 (remaining 9 months at new rate)

### Mid-Period Cancellations

When a subscription is cancelled mid-period:

1. Calculate pro-rata refund for unused period
2. Adjust deferred revenue
3. Mark subscription as cancelled
4. Stop future recognition

**Formula:**
```
refund_amount = total_amount × (remaining_period / total_period)
```

**Example:**
- $12,000 annual subscription
- Cancelled after 3 months
- Recognized: $3,000
- Refund: $9,000 (9 months remaining)

### Zero-Amount Periods

If no time has elapsed since last recognition:
- Return `NoRevenueToRecognize` error
- Do not update state
- Maintain consistency

### Full Period Recognition

When current time exceeds subscription end time:
- Recognize all remaining deferred revenue
- Set deferred revenue to 0
- Mark subscription as complete

## Testing

### Unit Tests

Located at: `contracts/accounting/src/tests/revenue_test.rs`

**Test Coverage:**
- Initialization
- Subscription creation
- Recognition rule configuration
- Straight-line recognition
- Contract modifications
- Cancellations
- Analytics
- Error cases

**Run Tests:**
```bash
cd stellar-lend/contracts/accounting
cargo test
```

### Integration Tests

Test the full flow from API to contract:

```bash
cd api
npm test -- accounting
```

## Deployment

### 1. Build Contract

```bash
cd stellar-lend/contracts/accounting
stellar contract build
```

### 2. Deploy to Testnet

```bash
stellar contract deploy \
  --wasm target/wasm32-unknown-unknown/release/accounting.wasm \
  --network testnet \
  --source <admin-keypair>
```

### 3. Initialize Contract

```bash
stellar contract invoke \
  --id <contract-id> \
  --network testnet \
  --source <admin-keypair> \
  -- initialize \
  --admin <admin-address>
```

### 4. Configure API

Add to `api/.env`:
```
ACCOUNTING_CONTRACT_ID=<contract-id>
```

### 5. Deploy API

```bash
cd api
npm run build
npm start
```

## Usage Examples

### Example 1: Annual Subscription

```typescript
// Create subscription
const subscription = await accountingService.createSubscription(
  merchantId,
  '12000000', // $12,000 in stroops
  Math.floor(Date.now() / 1000)
);

// Configure straight-line recognition over 12 months
await accountingService.configureRecognitionRule(
  subscription.subscriptionId,
  0, // Straight-line
  365 * 24 * 60 * 60 // 1 year
);

// Recognize revenue monthly
setInterval(async () => {
  await accountingService.recognizeRevenue(subscription.subscriptionId);
}, 30 * 24 * 60 * 60 * 1000);
```

### Example 2: Contract Upgrade

```typescript
// Original subscription
const subscriptionId = 1;

// After 3 months, upgrade
await accountingService.handleContractModification(
  subscriptionId,
  '18000000' // Upgrade to $18,000
);

// Schedule is automatically regenerated
const schedule = await accountingService.getRevenueSchedule(subscriptionId);
```

### Example 3: Early Cancellation

```typescript
// Cancel after 3 months
const refund = await accountingService.handleCancellation(subscriptionId);

console.log(`Refund amount: ${refund}`);
// Refund amount: 9000000 (9 months remaining)
```

## Best Practices

1. **Regular Recognition**: Run revenue recognition on a schedule (daily/weekly)
2. **Audit Trail**: Log all recognition events for compliance
3. **Reconciliation**: Regularly reconcile recognized vs. deferred revenue
4. **Error Handling**: Implement retry logic for failed recognitions
5. **Monitoring**: Set up alerts for unusual patterns
6. **Testing**: Test edge cases thoroughly before production
7. **Documentation**: Keep detailed records of recognition policies

## Compliance

This implementation follows:
- **ASC 606** (US GAAP): Revenue from Contracts with Customers
- **IFRS 15** (International): Revenue from Contracts with Customers

Key principles:
1. Identify the contract
2. Identify performance obligations
3. Determine transaction price
4. Allocate price to obligations
5. Recognize revenue when obligations are satisfied

## Future Enhancements

- [ ] Multi-currency support
- [ ] Tax calculation integration
- [ ] Automated reconciliation reports
- [ ] Advanced analytics dashboard
- [ ] Webhook notifications for recognition events
- [ ] Bulk operations for multiple subscriptions
- [ ] Revenue forecasting
- [ ] Integration with accounting systems (QuickBooks, Xero)

## Support

For issues or questions:
- Open an issue on GitHub
- Check the documentation
- Contact the development team
