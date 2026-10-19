# TWAP Oracle Security Measures

## Overview

This document describes the security measures implemented for lending pool price manipulation protection using TWAP (Time-Weighted Average Price) oracle.

## Security Architecture

### 1. Multi-Source Price Aggregation

**Implementation**: `oracle/src/services/price-aggregator.ts`

- Aggregates prices from multiple independent sources
- Uses weighted median calculation for robust price determination
- Configurable minimum source requirements
- Automatic fallback to available sources when some fail

### 2. TWAP (Time-Weighted Average Price)

**Implementation**: `stellar-lend/contracts/hello-world/src/oracle.rs`

- Computes average price over configurable time window (default: 30 minutes)
- Uses stored price observations for historical analysis
- Provides manipulation-resistant pricing for liquidations
- Falls back to spot price when insufficient history exists

### 3. Circuit Breaker System

**Implementation**: `stellar-lend/contracts/hello-world/src/oracle.rs`

- Per-asset circuit breakers that halt pricing during extreme deviations
- Configurable deviation thresholds (default: 25%)
- Automatic cooldown period (default: 10 minutes)
- Gradual stabilization requirement before re-enabling

### 4. Source Deviation Monitoring

**Implementation**: `oracle/src/services/manipulation-detector.ts`

- Continuous monitoring of individual source deviations from median
- Alert thresholds (2% deviation) and pause thresholds (10% deviation)
- Consecutive deviation tracking to identify persistent manipulation
- Automatic flagging of suspicious sources

### 5. Volatility Spike Detection

**Implementation**: `oracle/src/services/manipulation-detector.ts`

- Short-window volatility monitoring (10-minute windows)
- Detection of rapid price movements (>20% in 10 minutes)
- Automatic circuit breaker activation for extreme volatility

### 6. Price Staleness Protection

**Implementation**: `stellar-lend/contracts/hello-world/src/oracle.rs`

- Configurable staleness thresholds (default: 1 hour)
- Automatic rejection of stale price feeds
- Incident reporting for stale data detection

### 7. Cache with TTL

**Implementation**: `stellar-lend/contracts/hello-world/src/oracle.rs`

- Price caching for gas efficiency
- Configurable TTL (default: 5 minutes)
- Automatic cache invalidation for fresh data

## Configuration Parameters

### Oracle Configuration (`OracleConfig`)

```rust
pub struct OracleConfig {
    pub max_deviation_bps: i128,           // 500 (5%)
    pub max_staleness_seconds: u64,        // 3600 (1 hour)
    pub cache_ttl_seconds: u64,            // 300 (5 minutes)
    pub min_price: i128,                   // 1
    pub max_price: i128,                   // i128::MAX
    pub twap_window_seconds: u64,          // 1800 (30 minutes)
    pub max_observations: u32,             // 64
    pub min_sources: u32,                  // 1
    pub outlier_deviation_bps: i128,       // 1000 (10%)
    pub breaker_deviation_bps: i128,       // 2500 (25%)
    pub breaker_cooldown_seconds: u64,     // 600 (10 minutes)
}
```

### Manipulation Detector Configuration

```typescript
interface ManipulationDetectorConfig {
    sourceAlertBps: number;           // 200 (2%)
    sourcePauseBps: number;           // 1000 (10%)
    twapSpotAlertBps: number;         // 500 (5%)
    twapSpotPauseBps: number;         // 2500 (25%)
    volatilityBps: number;            // 2000 (20%)
    volatilityWindowSeconds: number;  // 600 (10 minutes)
    minSourcesForSafety: number;      // 2
    maxAlerts: number;                // 100
}
```

## Attack Scenarios & Defenses

### 1. Flash Loan Price Manipulation

**Attack**: Attacker uses flash loans to manipulate spot price momentarily

**Defense**:
- TWAP smoothing over 30-minute window
- Circuit breaker triggers on >25% deviation from last safe price
- Liquidation pricing uses TWAP instead of spot price

### 2. Oracle Feed Manipulation

**Attack**: Compromised oracle feed provides false prices

**Defense**:
- Multi-source aggregation with outlier filtering
- Source deviation monitoring with automatic alerts
- Minimum source requirements for safe pricing
- Automatic fallback to median when sources deviate

### 3. Stale Price Exploitation

**Attack**: Attacker exploits outdated price feeds

**Defense**:
- Configurable staleness thresholds
- Automatic rejection of stale prices
- Incident reporting for stale data detection
- Circuit breaker activation for persistent staleness

### 4. Volatility Manipulation

**Attack**: Attacker creates artificial volatility to trigger liquidations

**Defense**:
- Short-window volatility monitoring
- Automatic circuit breaker for rapid price movements
- Gradual stabilization requirement before re-enabling

## Testing Coverage

### Unit Tests

- `oracle/tests/manipulation-detector.test.ts` (319 lines)
- `stellar-lend/contracts/hello-world/src/tests/oracle_test.rs` (913 lines)
- `stellar-lend/contracts/hello-world/src/tests/oracle_circuit_breaker_test.rs`
- `stellar-lend/contracts/hello-world/src/tests/oracle_staleness_fallback_test.rs`
- `stellar-lend/contracts/hello-world/src/tests/oracle_configuration_test.rs`

### Integration Tests

- `oracle/tests/oracle-integration.test.ts`
- `tests/e2e/scenarios/oracle-integration.e2e.test.ts`
- `tests/stress/oracle-contract/oracle-stress.test.ts`

### Test Coverage

- Price feed updates with validation
- Price staleness checks
- Price deviation validation
- Price caching with TTL
- Fallback oracle support
- Circuit breaker activation and recovery
- TWAP calculation accuracy
- Source deviation monitoring
- Volatility spike detection

## Security Best Practices

### 1. Configuration Management

- Use conservative defaults for production
- Regularly review and adjust thresholds based on market conditions
- Implement admin controls for parameter changes

### 2. Monitoring & Alerting

- Monitor circuit breaker activations
- Track source deviation patterns
- Alert on repeated staleness incidents
- Review TWAP vs spot price deviations

### 3. Incident Response

- Document all oracle incidents
- Maintain incident timeline for analysis
- Implement gradual recovery procedures
- Regular security audits

### 4. Testing

- Comprehensive unit tests for all security features
- Integration tests for end-to-end scenarios
- Stress tests for high-load conditions
- Regular regression testing

## Performance Considerations

### Gas Optimization

- Price caching reduces storage reads
- Efficient TWAP calculation with bounded history
- Configurable observation limits (default: 64)
- Circuit breaker state management optimization

### Latency

- Cache TTL balances freshness vs performance
- Failover mode for reduced latency when primary source healthy
- Priority-based provider selection

## Conclusion

The TWAP oracle security implementation provides comprehensive protection against price manipulation attacks through multiple defense layers:

1. **Multi-source aggregation** prevents single-point-of-failure
2. **TWAP smoothing** resists flash loan attacks
3. **Circuit breakers** halt operations during extreme conditions
4. **Source monitoring** detects compromised feeds early
5. **Volatility protection** prevents artificial price swings

These measures work together to ensure secure and reliable pricing for the lending protocol.