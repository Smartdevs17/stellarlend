# StellarLend Oracle Service

Off-chain oracle integration service that fetches price data from multiple external sources and updates the smart contract on Soroban.

## Features

- **Multi-Source Price Fetching**: Aggregates prices from CoinGecko and Binance
- **Price Validation**: Validates prices for staleness, deviation, and bounds
- **Weighted Median**: Calculates weighted median from multiple sources for accuracy
- **Efficient Caching**: In-memory caching with configurable TTL to reduce API calls
- **Real-Time Price Feed**: Continuous price aggregation with anomaly detection, cross-feed correlation, and TWAP smoothing (see [docs/REALTIME_PRICE_FEED.md](../docs/REALTIME_PRICE_FEED.md))
- **Anomaly Detection**: Z-score, IQR, velocity, and adaptive thresholds to flag manipulated or erroneous prices
- **Health Monitoring**: Per-asset and system-wide health status with latency and failure tracking

## Prerequisites

- Node.js >= 18.0.0
- npm

## Installation

```bash
cd oracle
npm install
```

## Configuration

Copy the example environment file and configure:

```bash
cp .env.example .env
```

### Environment Variables

| Variable | Description | Required |
|----------|-------------|----------|
| `STELLAR_NETWORK` | Network: `testnet` or `mainnet` | Yes |
| `STELLAR_RPC_URL` | Soroban RPC endpoint (optional, uses network default) | No |
| `STELLAR_BASE_FEE` | Base fee for transactions (optional, uses network default) | No |
| `CONTRACT_ID` | StellarLend contract address | Yes |
| `ADMIN_SECRET_KEY` | Secret key for signing transactions | Yes |
| `COINGECKO_API_KEY` | CoinGecko Pro API key | No |
| `CACHE_TTL_SECONDS` | Cache TTL in seconds (default: 30) | No |
| `UPDATE_INTERVAL_MS` | Price update interval for push mode (default: 60000) | No |
| `ORACLE_MODE` | `push`, `pull`, or `both` (default: `push`) | No |
| `QUERY_PORT` | Port for the pull query API (default: 3002) | No |
| `MAX_PRICE_DEVIATION_PERCENT` | Max price deviation % (default: 10) | No |
| `LOG_LEVEL` | Logging: debug, info, warn, error | No |

### Network-Specific Defaults

The Oracle automatically configures appropriate defaults based on the `STELLAR_NETWORK` setting:

#### Testnet Defaults
- **RPC URL**: `https://soroban-testnet.stellar.org`
- **Base Fee**: `100,000` stroops

#### Mainnet Defaults
- **RPC URL**: `https://soroban.stellar.org`
- **Base Fee**: `200,000` stroops

#### Environment Variable Override
You can override network defaults by setting:
- `STELLAR_RPC_URL` - Custom RPC endpoint
- `STELLAR_BASE_FEE` - Custom base fee

Example configuration:
```bash
# Use mainnet with custom RPC
STELLAR_NETWORK=mainnet
STELLAR_RPC_URL=https://my-custom-rpc.example.com

# Use testnet with higher fee
STELLAR_NETWORK=testnet
STELLAR_BASE_FEE=150000
```

## Usage

### Development

```bash
npm run dev
```

### Production

```bash
npm run build
npm start
```

### Testing

```bash
npm test                 # Run all tests
npm run test:coverage    # With coverage report
npm run test:watch       # Watch mode
```

## Live Integration Test

To verify proper operation with real APIs (CoinGecko, Binance), run the live test script:

```bash
npx tsx tests/live-test.ts
```

This script will:
1. Initialize the CoinGecko and Binance providers.
2. Fetch live prices for XLM and BTC from each.
3. Aggregate the prices and display the result.

## Supported Assets

| Asset | CoinGecko | Binance |
|-------|-----------|---------|
| XLM   | Yes       | Yes     |
| USDC  | Yes       | Yes     |
| BTC   | Yes       | Yes     |
| ETH   | Yes       | Yes     |
| SOL   | Yes       | Yes     |

## Price Sources

### CoinGecko (Primary)
- Popular crypto price API
- Priority: 1, Weight: 60%

### Binance (Secondary)
- Public market data API
- Priority: 2, Weight: 40%

### Aggregation and outlier rejection

On-chain, when several sources quote the same asset, the price is the median of their quotes, not a single source's value. With an odd number of quotes the median is the middle one. With an even number it is the average of the two middle quotes, so a two- or four-source round is not biased toward its higher quote (`stellar-lend/contracts/hello-world/src/oracle.rs`).

Quotes that disagree with the round's median by more than the configured outlier band (`outlier_deviation_bps`, 10% by default) are dropped, and the median is taken again over the survivors. A single wildly wrong quote therefore cannot move the price. If fewer than `min_sources` quotes survive, no price is produced and the caller receives `NotEnoughSources`.

The TypeScript aggregator in `src/services/price-aggregator.ts` screens each round against its own median before validation, and logs every quote it rejects against consensus. Its default is a weighted median, so the even-count averaging above describes the on-chain rule.

## Programmatic Usage

```typescript
import { OracleService, loadConfig } from 'stellarlend-oracle';

const config = loadConfig();
const service = new OracleService(config);

// Start automatic updates
await service.start(['XLM', 'USDC', 'BTC']);

// Or fetch manually
const price = await service.fetchPrice('XLM');

// Stop service
service.stop();
```

### Real-Time Price Feed with Anomaly Detection

```typescript
import { createAggregator, createValidator, createPriceCache } from 'stellarlend-oracle';
import { createRealtimePriceFeed, FeedEventType } from './src/services/realtime-price-feed.js';

const aggregator = createAggregator(providers, validator, cache, { minSources: 1 });
const feed = createRealtimePriceFeed(aggregator, {
  assets: ['XLM', 'BTC', 'ETH'],
  pollIntervalMs: 5_000,
  enableAnomalyDetection: true,
  enableCorrelationAnalysis: true,
  enableTwapSmoothing: true,
});

feed.on(FeedEventType.ANOMALY_DETECTED, ({ asset, anomalies }) => {
  // Respond to anomalous prices before they reach the contract
});

await feed.start();
const enriched = await feed.getEnrichedPrice('BTC'); // includes healthScore, twap, anomalies
const health = feed.getSystemHealth();
await feed.stop();
```

## Project Structure

```
oracle/
├── src/
│   ├── index.ts              # Main entry point
│   ├── config.ts             # Configuration
│   ├── providers/            # Price providers
│   │   ├── coingecko.ts      # CoinGecko API
│   │   └── binance.ts        # Binance API
│   ├── services/             # Core services
│   │   ├── price-validator.ts
│   │   ├── price-aggregator.ts
│   │   ├── cache.ts
│   │   └── contract-updater.ts
│   ├── types/                # TypeScript types
│   └── utils/                # Utilities
├── tests/                    # Test suites
└── package.json
```

## Cheers!


## Oracle Modes and Query API

- **push** (default): periodically aggregates prices and submits them on-chain.
- **pull**: aggregates on demand and serves reads over HTTP without pushing.
- **both**: runs the push loop and the query API together.

Start in pull mode:

```bash
ORACLE_MODE=pull npm run dev
```

### Query endpoints (pull / both)

| Endpoint | Purpose |
|---|---|
| `GET /prices` | Latest aggregated price for every tracked asset (optional `?asset=XLM,USDC`) |
| `GET /prices/:asset` | Latest price for one asset |
| `GET /prices/:asset/history` | Historical prices with `from`/`to` (unix seconds), `limit` (max 100), and cursor pagination |
| `GET /openapi.json` | Generated OpenAPI 3.0 specification |
| `GET /health` | Liveness probe |

History responses include `pagination.nextCursor`; pass it back as `?cursor=` for
the next page. Results are cached for 5 seconds and invalidated when new price
entries arrive.

### OpenAPI

The spec is generated from `src/api/openapi.ts` (the same definitions the
server uses, so it cannot drift):

```bash
npm run openapi:generate   # writes oracle/openapi.json
npm run openapi:check      # CI: fails if the committed spec is stale
```

The test suite (`tests/openapi.test.ts`) asserts that the committed
`oracle/openapi.json` matches generated output, so `npm test` fails on drift.


## Oracle adapters and caching

`PriceProvider` in `src/providers/price-provider.ts` is the common service adapter
interface. CoinGecko and Binance implement it through `BasePriceProvider`;
`ProviderContainer` and `PriceAggregator` accept structural implementations as
well. A new integration supplies metadata, single/batch fetches and a health
probe, without depending on the aggregator's internals.

The service wraps each live provider in `CachedPriceProvider` using
`CACHE_TTL_SECONDS` and the configured source-staleness threshold. For custom
adapters:

```typescript
const cached = new CachedPriceProvider(provider, {
  ttlSeconds: 30,
  maxStalenessSeconds: 300,
  maxEntries: 1000,
});
```

- Cache keys are normalized asset names within one provider instance. Capacity
  is bounded with least-recently-used eviction.
- Quotes keep their original timestamp and source; cache hits never make an old
  quote appear fresh. Entries expire at the earlier of TTL or source-age limit.
  The TTL boundary is exclusive, and clock rollback invalidates a cached entry.
- Concurrent single-asset requests share one fetch. Batch requests retain the
  provider's batch endpoint and partial-success behavior, fetching only misses.
- Failed fetches and invalid or already-stale responses are not cached. A failed
  fetch remains visible to the aggregator's existing fallback/circuit breaker.
- Zero TTL disables retained quotes. Health checks always probe the live adapter.
- Structured debug logs identify adapter cache hits/misses by provider and asset;
  existing provider, circuit-breaker and service metrics remain available.

The on-chain module (`stellar-lend/contracts/hello-world/src/oracle.rs`) reuses
`stellarlend_oracle_client::PriceOracle` for every stored feed slot and
`PriceCache` for finalized-price storage. Source timestamps and the configured fallback source
identity are checked before a quote contributes to quorum. Stale feeds still
produce the existing incident records/events. Median filtering, TWAP and breaker
policy stay outside the adapters. The contract cache retains the existing
`CachedPrice` layout and inclusive TTL boundary; liquidation checks the breaker
before reading it, while spot/general reads resolve feeds again. No storage
migration or contract entry-point change is required.

Run service regressions with `npm test` in `oracle/`. Contract adapter tests are
in `oracle::adapter_tests` and run with:

```bash
cd stellar-lend
cargo test -p hello-world --lib oracle::adapter_tests
```


## TWAP oracle fallback

`oracle/twap/` provides a time-weighted average price (TWAP) that acts as a
fallback when the primary spot feed is stale or errored. It is a pure,
dependency-free module (`src/twap.ts`) with a thin service wrapper
(`src/twap-service.ts`) that records observations and answers fallback reads.

### Recording observations

Every accepted spot quote is recorded with its source timestamp:

```typescript
import { createTwapOracle } from './src/twap.js';

const twap = createTwapOracle({
  windowSeconds: 300,        // averaging window
  maxObservationAgeSeconds: 600, // staleness guard for the newest observation
  maxDeviationBps: 500,      // 5% guard vs. the primary spot price
  minObservations: 2,        // refuse to answer below this
});

twap.record({ asset: 'XLM', price: 0.1234, timestamp: 1_700_000_000 });
```

- Observations are keyed by asset and kept in timestamp order. Out-of-order
  inserts are placed correctly; duplicates for the same timestamp replace the
  earlier value.
- Each asset retains at most `maxObservationsPerAsset` (default 1000) points;
  the oldest are evicted first.
- Non-finite, zero, or negative prices are rejected and never recorded.

### Computing the time-weighted average

The TWAP is the integral of price over time divided by the covered duration.
Each observation is treated as holding until the next one, so a price that
persists longer contributes proportionally more:

```
TWAP = Σ price_i * (t_{i+1} - t_i) / (t_last - t_first)
```

Only observations inside `[now - windowSeconds, now]` participate. The window
is clamped to the newest observation so a quiet feed does not dilute the
average with a long tail. With a single in-window observation the TWAP is that
price. The result includes `twap`, `observations`, `windowStart`, `windowEnd`,
and `coverageSeconds` so callers can reason about how much history backed it.

### When the fallback activates

`TwapService.getPrice(asset, primary)` takes the current primary spot quote and
returns a `TwapFallbackResult`:

| Primary state | Behaviour |
|---|---|
| Fresh and within deviation | Returns the primary price, `source: 'primary'` |
| Stale (older than `maxObservationAgeSeconds`) | Returns TWAP, `source: 'twap'`, `reason: 'stale_primary'` |
| Errored (fetch failed / no quote) | Returns TWAP, `source: 'twap'`, `reason: 'primary_error'` |
| Missing primary and no TWAP history | Returns `null` with `reason: 'no_data'` |

### Guards

- **Staleness**: a primary quote older than `maxObservationAgeSeconds` is not
  trusted; the TWAP is used instead. The TWAP itself must contain at least
  `minObservations` in-window points, otherwise the fallback declines.
- **Deviation**: when the TWAP differs from the last known primary price by more
  than `maxDeviationBps`, the fallback is rejected (`reason: 'deviation_exceeded'`)
  rather than serving a price that may be manipulated. The primary price is
  still returned if it is fresh.
- **Coverage**: if the in-window coverage is shorter than `minCoverageSeconds`
  (default 0, i.e. disabled), the fallback declines with `reason: 'insufficient_coverage'`.

### Configuration

| Variable | Default | Description |
|---|---|---|
| `TWAP_WINDOW_SECONDS` | `300` | Averaging window length |
| `TWAP_MAX_OBSERVATION_AGE_SECONDS` | `600` | Primary staleness threshold |
| `TWAP_MAX_DEVIATION_BPS` | `500` | Max TWAP-vs-primary deviation (basis points) |
| `TWAP_MIN_OBSERVATIONS` | `2` | Minimum in-window observations |
| `TWAP_MIN_COVERAGE_SECONDS` | `0` | Minimum covered duration (0 disables) |
| `TWAP_MAX_OBSERVATIONS_PER_ASSET` | `1000` | Retention cap per asset |

### Observability

On every fallback decision the service emits a structured log line and updates
counters:

- Log fields: `asset`, `source` (`primary`/`twap`), `reason`, `twap`,
  `primaryPrice`, `deviationBps`, `coverageSeconds`, `observations`.
- Metrics: `oracle_twap_fallback_total{asset,reason}` (counter),
  `oracle_twap_deviation_bps{asset}` (gauge), and
  `oracle_twap_coverage_seconds{asset}` (gauge). `reason` is one of
  `stale_primary`, `primary_error`, `deviation_exceeded`,
  `insufficient_coverage`, or `no_data`.

### Tests

```bash
cd oracle
npm test -- tests/twap.test.ts          # unit: recording, weighting, guards
npm test -- tests/twap-service.test.ts  # integration: fallback activation + metrics
```

The unit suite pins window clamping, out-of-order and duplicate observations,
retention eviction, and each guard. The integration suite drives the service
with a fake primary provider to cover the fresh, stale, errored, deviation, and
no-data paths, asserting the emitted log fields and metric counters.
