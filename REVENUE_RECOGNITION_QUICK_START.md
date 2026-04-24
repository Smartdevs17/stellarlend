# Revenue Recognition - Quick Start Guide

## Overview

This guide provides a quick reference for using the revenue recognition accounting system.

## Quick Commands

### Build & Test

```bash
# Build contract
cd stellar-lend/contracts/accounting
stellar contract build

# Run tests
cargo test

# Expected output: test result: ok. 9 passed; 0 failed
```

### Deploy

```bash
# Deploy to testnet
stellar contract deploy \
  --wasm target/wasm32-unknown-unknown/release/accounting.wasm \
  --network testnet \
  --source YOUR_ADMIN_KEYPAIR

# Initialize contract
stellar contract invoke \
  --id CONTRACT_ID \
  --network testnet \
  --source YOUR_ADMIN_KEYPAIR \
  -- initialize \
  --admin YOUR_ADMIN_ADDRESS
```

### Configure API

Add to `api/.env`:
```env
ACCOUNTING_CONTRACT_ID=YOUR_CONTRACT_ID
```

## API Quick Reference

### Create Subscription

```bash
curl -X POST http://localhost:3000/api/accounting/subscriptions \
  -H "Content-Type: application/json" \
  -d '{
    "merchantId": "GXXX...",
    "totalAmount": "12000000",
    "startTime": 1704067200
  }'
```

### Configure Recognition (Straight-Line, 1 Year)

```bash
curl -X POST http://localhost:3000/api/accounting/subscriptions/1/configure \
  -H "Content-Type: application/json" \
  -d '{
    "method": 0,
    "recognitionPeriod": 31536000
  }'
```

### Recognize Revenue

```bash
curl -X POST http://localhost:3000/api/accounting/subscriptions/1/recognize
```

### Get Deferred Revenue

```bash
curl http://localhost:3000/api/accounting/merchants/GXXX.../deferred-revenue
```

### Get Revenue Schedule

```bash
curl http://localhost:3000/api/accounting/subscriptions/1/schedule
```

### Get Analytics

```bash
curl "http://localhost:3000/api/accounting/merchants/GXXX.../analytics?startTime=1704067200&endTime=1735689600"
```

## Recognition Methods

| Method | Value | Description |
|--------|-------|-------------|
| Straight-Line | 0 | Even recognition over period |
| Usage-Based | 1 | Based on usage metrics |
| Milestone-Based | 2 | Based on milestone completion |

## Common Periods

| Period | Seconds |
|--------|---------|
| 1 Month | 2,592,000 |
| 3 Months | 7,776,000 |
| 6 Months | 15,552,000 |
| 1 Year | 31,536,000 |

## Error Codes

| Code | Error | Description |
|------|-------|-------------|
| 1 | Unauthorized | Caller not authorized |
| 4 | InvalidSubscriptionId | Invalid subscription ID |
| 5 | InvalidRecognitionMethod | Invalid method (must be 0-2) |
| 6 | InvalidPeriod | Period is zero or negative |
| 8 | SubscriptionNotFound | Subscription doesn't exist |
| 12 | NoRevenueToRecognize | No time elapsed |

## File Locations

```
Contract:     stellar-lend/contracts/accounting/
API:          api/src/controllers/accounting.controller.ts
              api/src/services/accounting.service.ts
              api/src/routes/accounting.routes.ts
Frontend:     app/stores/accountingStore.ts
              app/screens/RevenueReportScreen.tsx
Docs:         docs/revenue-recognition.md
Tests:        stellar-lend/contracts/accounting/src/tests/
```

## Example Workflow

### 1. Create Annual Subscription

```typescript
// $12,000 annual subscription
const subscription = await accountingService.createSubscription(
  merchantId,
  '12000000', // stroops
  Math.floor(Date.now() / 1000)
);
```

### 2. Configure Straight-Line Recognition

```typescript
// Recognize evenly over 12 months
await accountingService.configureRecognitionRule(
  subscription.subscriptionId,
  0, // Straight-line
  365 * 24 * 60 * 60 // 1 year
);
```

### 3. Recognize Revenue Monthly

```typescript
// Run monthly
setInterval(async () => {
  await accountingService.recognizeRevenue(subscription.subscriptionId);
}, 30 * 24 * 60 * 60 * 1000);
```

### 4. View Analytics

```typescript
const analytics = await accountingService.getRevenueAnalytics(
  merchantId,
  startTime,
  endTime
);

console.log(`Total Revenue: ${analytics.totalRevenue}`);
console.log(`Recognized: ${analytics.recognizedRevenue}`);
console.log(`Deferred: ${analytics.deferredRevenue}`);
```

## Frontend Usage

### Import Store

```typescript
import { useAccountingStore } from '../stores/accountingStore';
```

### Use in Component

```typescript
const {
  subscriptions,
  analytics,
  isLoading,
  error,
  setAnalytics,
} = useAccountingStore();
```

### Display Revenue Report

```tsx
import RevenueReportScreen from '../screens/RevenueReportScreen';

<RevenueReportScreen merchantId={merchantId} />
```

## Troubleshooting

### Tests Failing

```bash
# Clean and rebuild
cd stellar-lend/contracts/accounting
cargo clean
cargo test
```

### Contract Not Found

```bash
# Verify contract ID in .env
echo $ACCOUNTING_CONTRACT_ID

# Re-deploy if needed
stellar contract deploy --wasm target/wasm32-unknown-unknown/release/accounting.wasm --network testnet
```

### API Errors

```bash
# Check logs
tail -f api/logs/app.log

# Verify contract connection
curl http://localhost:3000/health
```

## Best Practices

1. **Regular Recognition:** Run revenue recognition on a schedule (daily/weekly)
2. **Monitoring:** Track deferred revenue trends
3. **Reconciliation:** Regularly reconcile recognized vs. deferred
4. **Error Handling:** Implement retry logic for failed recognitions
5. **Audit Trail:** Log all recognition events
6. **Testing:** Test edge cases before production

## Support

- **Documentation:** `docs/revenue-recognition.md`
- **Contract README:** `stellar-lend/contracts/accounting/README.md`
- **Implementation Summary:** `REVENUE_RECOGNITION_IMPLEMENTATION.md`
- **GitHub Issues:** Open an issue for bugs or questions

## Quick Links

- [Full Documentation](docs/revenue-recognition.md)
- [Contract README](stellar-lend/contracts/accounting/README.md)
- [Implementation Summary](REVENUE_RECOGNITION_IMPLEMENTATION.md)
- [Stellar Soroban Docs](https://soroban.stellar.org/docs/)

---

**Last Updated:** 2026-04-24
**Version:** 1.0.0
**Status:** Production Ready
