# StellarLend REST API

REST API for StellarLend core lending operations (deposit, borrow, repay, withdraw) with Stellar Horizon and Soroban RPC integration.

## Features

- Unsigned transaction preparation for deposit, borrow, repay, withdraw operations
- Signed transaction submission endpoint
- Request validation and error handling
- Transaction submission and monitoring
- Rate limiting and security middleware
- 95%+ test coverage

## Quick Start

```bash
cd api
npm install
cp .env.example .env
# Edit .env with your configuration
npm run dev
```

## Configuration

Required environment variables in `.env`:

```env
PORT=3000
STELLAR_NETWORK=testnet
HORIZON_URL=https://horizon-testnet.stellar.org
SOROBAN_RPC_URL=https://soroban-testnet.stellar.org
CONTRACT_ID=<your_deployed_contract_id>
JWT_SECRET=<your_secret_key>
```

### Security Configuration

```env
# Request body size limit (prevents DoS attacks)
# Examples: 100kb, 1mb, 1gb. Defaults to 100kb if not set.
# Returns 413 Payload Too Large when exceeded.
BODY_SIZE_LIMIT=100kb

# Rate limiting (already set by default, configurable)
RATE_LIMIT_WINDOW_MS=900000
RATE_LIMIT_MAX_REQUESTS=100
```

## API Endpoints

### Health Check
`GET /api/health` - Check service status

### Prepare Transaction
`GET /api/lending/prepare/:operation`
```json
{
  "userAddress": "G...",
  "amount": "10000000"
}
```

Response:
```json
{
  "unsignedXdr": "AAAA...",
  "operation": "deposit",
  "expiresAt": "2026-03-28T12:34:56.000Z"
}
```

### Submit Signed Transaction
`POST /api/lending/submit`
```json
{
  "signedXdr": "AAAA..."
}
```

Response:
```json
{
  "success": true,
  "transactionHash": "abc123...",
  "status": "success",
  "ledger": 12345
}
```

### Simulate Transaction
`POST /api/lending/simulate`
```json
{
  "transactionXdr": "AAAA..."
}
```
Or an operation specification (`operation`, `userAddress`, `amount`, optional `assetAddress`). Returns the Soroban simulation outcome: `status` (`success`, `restore_required`, `error`), `minResourceFee`, `resources`, decoded `result` with required `auth` entries, diagnostic `events`, `stateChanges` and any `restorePreamble`. See [docs/api/transaction-simulation.md](../docs/api/transaction-simulation.md).

### Estimate Transaction Fees
`POST /api/gas/estimate-transaction`
```json
{
  "transactionXdr": "AAAA...",
  "feeMarginPercent": 10
}
```

Response:
```json
{
  "success": true,
  "fees": {
    "inclusionFee": "100",
    "resourceFee": "40100",
    "totalFee": "40200",
    "feeMarginPercent": 10,
    "recommendedFee": "44220"
  },
  "resources": { "cpuInstructions": "250000", "readBytes": "1024", "writeBytes": "256" }
}
```
Sign with `recommendedFee` as the transaction fee. A failed simulation returns `422`.

### Pool Utilization Time Series (Issue #1650)

Pre-aggregated hourly utilization data served from the `pool_utilization_timeseries` materialized view.  Replaces the previous full-table scan on `pool_snapshots` that was the measured tail-latency source.

#### Endpoints

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/pool-performance/utilization-timeseries/:poolAddress` | Hourly utilization buckets for a single pool |
| `GET` | `/api/pool-performance/utilization-timeseries` | Hourly utilization buckets for all pools |
| `POST` | `/api/pool-performance/utilization-timeseries/refresh` | Trigger a manual view refresh (admin) |

#### Query parameters

| Parameter | Values | Default | Description |
|-----------|--------|---------|-------------|
| `period` | `7d`, `30d`, `90d`, `1y` | `30d` | Time window |

#### Single-pool response

```json
{
  "success": true,
  "data": {
    "poolAddress": "GABCDEF...",
    "period": "30d",
    "servedFromView": true,
    "buckets": [
      {
        "bucket":            "2026-11-19T10:00:00.000Z",
        "poolAddress":       "GABCDEF...",
        "avgUtilization":    0.65,
        "minUtilization":    0.60,
        "maxUtilization":    0.70,
        "medianUtilization": 0.65,
        "stddevUtilization": 0.03,
        "avgTvl":            2000000,
        "avgTotalBorrows":   1300000,
        "avgTotalDeposits":  2000000,
        "avgBorrowApy":      0.08,
        "avgSupplyApy":      0.055,
        "sampleCount":       3
      }
    ]
  }
}
```

#### Database migration

The view is created by:

```
api/src/db/migrations/20261119000000_pool_utilization_timeseries_mv.sql
```

The migration also installs a throttled `AFTER INSERT` trigger on `pool_snapshots` that calls `REFRESH MATERIALIZED VIEW CONCURRENTLY pool_utilization_timeseries` at most once every 55 minutes, matching the hourly snapshot cadence of the cron job (`api/src/jobs/poolSnapshot.job.ts`).

To apply the migration manually:

```bash
psql "$DATABASE_URL" -f api/src/db/migrations/20261119000000_pool_utilization_timeseries_mv.sql
```

To force a refresh outside the automatic cadence:

```sql
SELECT refresh_pool_utilization_timeseries();
-- or via the API:
-- POST /api/pool-performance/utilization-timeseries/refresh
```

Refresh history is tracked in `pool_mv_refresh_log`.

### Cache and Prefetch
`GET /api/cache/stats` reports read cache hit rates per kind, store counters and the prefetch scheduler state. `POST /api/cache/invalidate` (operator) drops cached read data. Read-heavy GET endpoints answer with an `X-Cache` header; send `Cache-Control: no-cache` to bypass. See [docs/api/read-cache-and-prefetch.md](../docs/api/read-cache-and-prefetch.md).

### Paginated List Endpoints
All list endpoints use cursor-based pagination (Horizon style) and return the same structure:
```json
{
  "data": [ ... ],
  "pagination": {
    "cursor": "nextCursorValue or null",
    "hasMore": true|false,
    "limit": 10
  }
}
```

Query parameters:
- `limit` (optional; default from `PAGINATION_DEFAULT_LIMIT`, max `PAGINATION_MAX_LIMIT`)
- `cursor` (optional; page cursor)

Example endpoint:
`GET /api/lending/transactions/{userAddress}?limit=10&cursor=12345`

For a given filter and sort order, event-index pages use keyset cursors over
`(ledger, event id)` rather than offsets. The returned cursor is exclusive: the
next page continues strictly after its last event, including when several
events share a ledger or new events arrive between requests. Keep the filters
and sort order unchanged while following a cursor.
All amounts in stroops (1 XLM = 10,000,000 stroops)

Clients must sign the returned XDR locally. The API does not accept Stellar secret keys.

## Testing

```bash
npm test              # Run all tests
npm test -- --coverage  # With coverage report
```

Test coverage: 95%+ (branches, functions, lines, statements)

## Production Build

```bash
npm run build
npm start
```

## Project Structure

```
api/src/
├── __tests__/      # Test files
├── config/         # Configuration
├── controllers/    # Request handlers
├── middleware/     # Validation, auth, errors
├── routes/         # API routes
├── services/       # Stellar integration
├── types/          # TypeScript types
└── utils/          # Logger, errors
```

## License

MIT
