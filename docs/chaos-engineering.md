# Chaos Engineering Experiments

Reference: **Issue #689** — Implement chaos engineering test suite for network failures.

This document is the experiment registry and runbook for StellarLend chaos
testing. Each experiment states a hypothesis, the failure mode injected, the
steady-state (acceptance) criteria, and how to run it.

## How to Run

```bash
# Full cycle: inject → steady state → recover → report
bash scripts/fuzz/run_chaos.sh run

# Individual suites
bash scripts/fuzz/run_chaos.sh inject partition   # network partition simulation
bash scripts/fuzz/run_chaos.sh inject rpc         # RPC outage suite
bash scripts/fuzz/run_chaos.sh recover            # recovery procedure verification
bash scripts/fuzz/run_chaos.sh report             # rebuild markdown report

# Directly via npm
cd tests/chaos && npm ci && npm test
npm run test:partition   # partition suite only
npm run test:rpc         # RPC suite only
npm run test:report      # JSON report → chaos-report.json
```

Reports are written to `tests/chaos/chaos-report.{json,md}` and uploaded as CI
artifacts by `.github/workflows/chaos-testing.yml`.

## Experiment Registry

| ID | Experiment | Failure injected | Steady-state hypothesis | Halt criteria |
|----|------------|------------------|-------------------------|---------------|
| CH-01 | API ⇄ RPC partition | Hard split between API and RPC; submit/simulate endpoints unreachable | Health reads keep serving; submission fails cleanly with partition errors; no split-brain writes | Overlapping partitions allowed; inconsistent state after heal |
| CH-02 | Oracle ⇄ Contract partition | Price feed severed; contract cannot observe oracle updates | Contract rejects stale/missing feeds (staleness guard); accepts fresh prices after heal | Stale price accepted while feed is partitioned |
| CH-03 | Partial endpoint partition | Subset of endpoints (`submit-transaction`) isolated | Non-partitioned endpoints continue to serve | Non-isolated endpoints fail |
| CH-04 | RPC timeout / outage | RPC endpoints fail for a duration-bounded window | Retries with exponential backoff recover; API serves cached data; metrics track failover attempts | Retry exhaustion without recovery; unhandled errors on reads |
| CH-05 | Slow RPC / latency | Elevated response latency | Backoff absorbs latency; recovery within timeout budget | Timeouts cascade to user-facing 5xx |
| CH-06 | Oracle feed disruption | Oracle data becomes stale mid-flight | Fallback/circuit breaker engages; contracts reject stale data | Stale oracle data accepted |
| CH-07 | Graceful degradation (E2E) | Provider + contract updater failures | Health status reports `degraded`; reads continue; writes return 503 | Reads fail during outage; degraded status never reported |
| CH-08 | Recovery procedures | All of the above, followed by heal | `dataConsistency` holds; `successfulRecoveries` increments per failure type; partition lifecycle timestamps recorded | `dataConsistency === false` after heal |

## Suites

| Suite | Location | Covers |
|-------|----------|--------|
| Network partition simulation | `tests/chaos/network-partition.test.ts` | CH-01 … CH-03, CH-08 (partition lifecycle) |
| Network failures / RPC outages | `tests/chaos/network-failures.test.ts` | CH-04 … CH-06, CH-08 |
| Graceful degradation + recovery E2E | `tests/e2e/chaos-engineering.e2e.test.ts` | CH-07, CH-08 |
| Contract resilience | `stellar-lend/contracts/hello-world/src/tests/network_failure_resilience_test.rs` | Circuit breaker, rate limiter congestion, monitor degradation under failure |

## Contract-Side Resilience Under Test

Simulated network failures must pair with on-chain defenses:

- **Circuit breaker** (`hello-world/src/circuit_breaker.rs`) — tiered pause when failure thresholds trip
- **Rate limiter congestion control** (`hello-world/src/rate_limiter.rs`) — `report_congression` / adaptive scaling when ledgers are slow
- **Health monitor** (`hello-world/src/monitor.rs`) — `HealthStatus::{Up, Degraded, Down}`
- **Oracle staleness / circuit breaker** (`hello-world/src/oracle.rs`)

## Reporting

`scripts/fuzz/chaos_report.sh` converts Jest JSON results into a markdown
report containing:

- Overall pass/fail status and per-test results
- Failure details (when any)
- The experiment registry table above

CI uploads `chaos-report.md` and `chaos-report.json` for every run.

## Halt Criteria (runbook)

Abort a live chaos experiment and trigger recovery if:

1. Data inconsistency is detected after heal (`dataConsistency === false`)
2. Reads fail while a read-path partition should be isolating writes only
3. Stale oracle prices are accepted by the contract path
4. Recovery does not complete within the failure duration budget

Recovery steps are documented in [recovery.md](recovery.md).

## CI Integration

`.github/workflows/chaos-testing.yml` runs:

| Job | Trigger | Purpose |
|-----|---------|---------|
| `chaos-unit` | push/PR (chaos paths) + nightly | Partition + RPC suites, markdown report artifact |
| `chaos-e2e` | push/PR (chaos paths) + nightly | Graceful degradation E2E |
| `chaos-recovery` | after `chaos-unit` | `run_chaos.sh recover` automation |
| `chaos-gate` | always | Aggregate gate — all chaos jobs must pass |
