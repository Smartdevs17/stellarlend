# Recovery Procedures

Reference: **Issue #689** — recovery procedure automation for network-failure
chaos experiments (see [chaos-engineering.md](chaos-engineering.md)).

This runbook describes how the protocol and its clients recover from injected
network failures, and how those procedures are automated in CI.

## Automated Recovery Cycle

```bash
bash scripts/fuzz/run_chaos.sh recover
# or the full cycle:
bash scripts/fuzz/run_chaos.sh run
```

The orchestrator:

1. **Injects** each failure mode (partition, RPC outage, oracle disruption)
2. **Verifies steady state** — reads keep serving, writes fail cleanly, stale
   data is rejected
3. **Heals** the failure and runs recovery assertions
4. **Reports** results to `tests/chaos/chaos-report.md`

## Procedure by Failure Mode

### API ⇄ RPC partition

1. Detection: `submit-transaction` / `simulate-transaction` return partition errors
2. Mitigation: API marks itself degraded; reads served from cache/fallback
3. Heal: restore connectivity; verify `canCall('api', 'rpc')` is true
4. Verify: post-heal submissions succeed; partition recorded with heal timestamp

### Oracle ⇄ Contract partition

1. Detection: price propagation fails (`pushPrice` returns false); feed ages past
   the staleness threshold
2. Mitigation: contract-side staleness guard rejects old feeds; circuit breaker
   may engage
3. Heal: restore feed; push fresh prices
4. Verify: fresh prices accepted; no stale acceptance occurred during the window

### RPC outage / timeouts

1. Detection: endpoint errors with duration-bounded failure window
2. Mitigation: exponential backoff retries (`submitTransactionWithRetry`);
   API continues serving cached data
3. Heal: failure window elapses or `stopFailure()` called
4. Verify: `successfulRecoveries` increments; `dataConsistency` stays true

### Oracle feed disruption

1. Detection: cached feeds cleared / marked stale
2. Mitigation: fallback data or circuit breaker; contract rejects stale data
   (`validateContractsRejectStaleData`)
3. Heal: feed restored
4. Verify: fallback engaged during outage; fresh data accepted after

## Graceful Degradation (API)

During any write-path outage the API must:

- Report health status `degraded` (not `down`) while reads work
- Return **503** for writes and **200** for cached reads
- Recover to `healthy` and accept writes once the outage clears

Covered by `tests/e2e/chaos-engineering.e2e.test.ts`.

## Contract-Level Recovery

| Mechanism | Entrypoints | Purpose |
|-----------|-------------|---------|
| Circuit breaker | activate/deactivate, tiered auto-triggers | Pause liquidations/borrows when thresholds trip |
| Rate limiter congestion | `report_congestion`, adaptive factor scaling | Absorb slow-ledger periods without hard failures |
| Health monitor | `monitor_report_health` → `Up`/`Degraded`/`Down` | Surface degradation to indexers |
| Social recovery | `start_recovery` / `approve_recovery` / `execute_recovery` | Admin key recovery via guardians |
| Deployment rollback | `scripts/rollback.sh` | Revert a bad deployment |

## Verification Checklist

- [ ] All failure modes heal without manual state repair
- [ ] `dataConsistency === true` after every recovery
- [ ] No stale oracle price was accepted during any disruption
- [ ] Degraded status was reported (not a hard outage) while reads worked
- [ ] Chaos report generated and archived

## CI

Recovery automation runs in `.github/workflows/chaos-testing.yml`
(`chaos-recovery` job → `scripts/fuzz/run_chaos.sh recover`) and uploads the
markdown report as a build artifact.
