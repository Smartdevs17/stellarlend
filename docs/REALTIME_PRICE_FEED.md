# Real-Time Price Feed with Anomaly Detection

## Overview

The oracle service ships a production-ready **real-time price feed aggregation**
pipeline that continuously fetches, validates, aggregates, and enriches asset
prices while actively detecting anomalous or manipulated readings before they
are used to update the on-chain contract.

The pipeline orchestrates several subsystem services into a single, event-driven
service:

```
External APIs → PriceAggregator → [AnomalyDetector, FeedCorrelation, TWAPService, ManipulationDetector]
                                  → EnrichedPrice (aggregated + twap + anomaly metadata + health score)
                                  → typed events → ContractUpdater
```

## Architecture

### Components

| Component | Module | Responsibility |
|-----------|--------|----------------|
| `RealtimePriceFeed` | `oracle/src/services/realtime-price-feed.ts` | Top-level orchestrator: polling, concurrency, health, events, lifecycle |
| `AnomalyDetector` | `oracle/src/services/anomaly-detector.ts` | Statistical anomaly detection (Z-score, IQR, velocity, adaptive thresholds) |
| `FeedCorrelation` | `oracle/src/services/feed-correlation.ts` | Cross-feed correlation analysis and coordinated-move detection |
| `TWAPService` | `oracle/src/services/twap.service.ts` | Time-weighted average price smoothing with manipulation resistance |
| `ManipulationDetector` | `oracle/src/services/manipulation-detector.ts` | Source deviation and TWAP-vs-spot manipulation checks |
| `PriceHistoryService` | `oracle/src/services/price-history.ts` | Bounded historical price store backing anomaly/TWAP windows |
| `PriceAggregator` | `oracle/src/services/price-aggregator.ts` | Weighted-median aggregation across providers |

### Data Flow

1. `RealtimePriceFeed.start()` runs an initial `runPriceCycle()` and then polls on a configurable interval.
2. Each asset is processed concurrently (bounded by `maxConcurrency`).
3. `PriceAggregator.getPrice(asset)` returns an `AggregatedPrice`.
4. The price is recorded into history, then passed through anomaly detection, correlation analysis, TWAP, and manipulation checks.
5. A computed **health score** (0-100) and an `EnrichedPrice` are produced.
6. Typed events are emitted for downstream consumers (see [Events](#events)).

## Anomaly Detection

`AnomalyDetector` combines several complementary statistical methods. It only
activates after `minSamples` observations exist for an asset, and enforces an
`anomalyCooldownSeconds` cooldown to avoid alert storms.

### Methods

| Method | Detection signal | Severity |
|--------|------------------|----------|
| **Z-Score** | Price deviates more than `zScoreWarningThreshold` / `zScoreCriticalThreshold` std-dev from the rolling mean | `warning` / `critical` |
| **IQR** | Price falls outside `Q1 - k*IQR` / `Q3 + k*IQR` fences | `warning` / `critical` |
| **Velocity** | Price changes faster than `velocityBpsPerSecond` within `velocityWindowSeconds` | `warning` / `critical` (2x threshold → critical) |
| **Adaptive thresholds** | Thresholds widen during high volatility and tighten in calm markets to reduce false positives | — |

### Rolling Statistics

`getRollingStats(asset)` reports `mean`, `stdDev`, `min`, `max`, `median`, `q1`,
`q3`, `iqr`, `volatility` over a rolling, bounded window.

## Feed Correlation

`FeedCorrelation` tracks Pearson correlation of price *returns* (percentage
changes) across assets.

| Event type | Meaning |
|-----------|---------|
| `COORDINATED_MOVE` | Unexpectedly high correlation or >70% of assets moving >1% in the same direction |
| `CORRELATION_BREAKDOWN` | Historically correlated pair (e.g., BTC/ETH) suddenly diverges |
| `ANOMALOUS_DIVERGENCE` | Related assets behave inconsistently |

Configured correlated groups, e.g. `[['BTC','ETH'], ['USDC','USDT']]`, are
checked for coordinated moves and breakdowns.

## Configuration

`createRealtimePriceFeed(aggregator, config)` accepts a partial
`RealtimeFeedConfig`:

```typescript
interface RealtimeFeedConfig {
  assets: string[];                    // default: ['XLM','USDC','BTC','ETH']
  pollIntervalMs: number;              // default: 10_000
  maxPriceAgeSeconds: number;          // default: 120
  enableAnomalyDetection: boolean;     // default: true
  enableCorrelationAnalysis: boolean;  // default: true
  enableTwapSmoothing: boolean;        // default: true
  maxConcurrency: number;              // default: 5
  anomalyWindowSize: number;           // default: 100
  updateTimeoutMs: number;             // default: 30_000
}
```

`AnomalyDetector` and `FeedCorrelation` accept their own partial config objects
(see their module documentation for every tunable).

## Events

`RealtimePriceFeed` extends `EventEmitter` and emits typed events:

| Event | Payload |
|-------|---------|
| `PRICE_UPDATE` | `EnrichedPrice` |
| `ANOMALY_DETECTED` | `{ asset, anomalies }` |
| `CORRELATION_ALERT` | `CorrelationEvent` |
| `FEED_HEALTH_CHANGE` | `{ status, assets? }` |
| `AGGREGATION_COMPLETE` | `{ asset, price, twapPrice, sources, healthScore }` |
| `FEED_ERROR` | `{ asset, error }` |

```typescript
feed.on(FeedEventType.ANOMALY_DETECTED, ({ asset, anomalies }) => {
  for (const anomaly of anomalies) {
    console.log(`[${anomaly.severity}] ${asset}: ${anomaly.message}`);
  }
});
```

## Health Monitoring

- `getHealthStatuses()` — per-asset `FeedHealthStatus` including staleness, failure count, and average latency.
- `getSystemHealth()` — aggregate health, uptime, cycle count, anomaly + correlation stats.

## Programmatic Usage

```typescript
import { createAggregator, createValidator, createPriceCache } from 'stellarlend-oracle';
import { createRealtimePriceFeed } from './src/services/realtime-price-feed.js';

const aggregator = createAggregator(providers, validator, cache, { minSources: 1 });

const feed = createRealtimePriceFeed(aggregator, {
  assets: ['XLM', 'BTC', 'ETH'],
  pollIntervalMs: 5_000,
  enableAnomalyDetection: true,
  enableCorrelationAnalysis: true,
  enableTwapSmoothing: true,
});

await feed.start();                    // begin polling
const enriched = await feed.getEnrichedPrice('BTC');
console.log(enriched.healthScore, enriched.twapPrice);

const health = feed.getSystemHealth(); // operational overview
await feed.stop();                     // stop polling
```

## Testing

The feature ships with unit tests, an integration test, and performance
benchmarks in `oracle/tests/`:

| Test file | Coverage |
|-----------|----------|
| `anomaly-detector.test.ts` | Z-score, IQR, velocity, adaptive thresholds, events, stats, config (~98% stmts) |
| `feed-correlation.test.ts` | Pearson correlation, matrices, coordinated moves, breakdown, events (~88% stmts) |
| `realtime-price-feed.test.ts` | Pipeline, events, health, lifecycle, subsystem access (~94% stmts) |
| `realtime-pipeline.integration.test.ts` | Full multi-provider pipeline, anomaly propagation, degradation, concurrency |
| `performance-benchmark.test.ts` | Throughput, latency, and memory bounds under sustained load |

```bash
cd oracle
npm test                                      # run all tests
npx vitest --run --coverage                   # coverage report
```

## Performance Benchmarks

Sustained-load benchmarks (run under `performance-benchmark.test.ts`):

| Scenario | Budget |
|----------|--------|
| 1000 anomaly ingests | < 500ms |
| 50 assets × 100 price points | < 1s |
| 500 correlated price records | < 1s |
| 50 full price cycles (5 assets) | < 5s |
| single-asset processing latency | < 100ms avg |
| 200-cycle memory usage | bounded (no unbounded growth) |

## See Also

- [Oracle Configuration Guide](ORACLE_CONFIGURATION_GUIDE.md)
- [Oracle Service README](../oracle/README.md)
