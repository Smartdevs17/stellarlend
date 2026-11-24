# Stablecoin peg monitor

The peg monitor checks watched stablecoins against their $1 peg on a schedule and triggers an automated rebalance when one breaks its threshold (issue #1534). The schedule is declared in `bots/rebalance/rebalance.schedule.json`, and the logic lives in `api/src/jobs/stablecoinPeg.job.ts`.

## Schedule entry

```json
{
  "name": "stablecoin-peg",
  "enabled": true,
  "cron": "*/5 * * * *",
  "timezone": "UTC",
  "deviationThresholdPercent": 1,
  "slippageTolerancePercent": 0.3,
  "dryRun": false,
  "targetQueue": "rebalance-stablecoin-peg",
  "assets": ["USDC", "USDT"]
}
```

- `assets` is required for this job. An empty or missing list fails schedule validation.
- For this job, `deviationThresholdPercent` is the depeg threshold in percent away from $1. The default of `1` means a 100 bps move.
- Set `dryRun: true` to simulate and log without executing.

## States

Deviation is measured as `|price − 1| / 1` in basis points.

| State | Condition (default threshold 1%) | Action |
| --- | --- | --- |
| `ok` | Deviation below 50 bps | None |
| `warning` | 50 bps up to 100 bps | Warning logged |
| `depeg` | 100 bps or more | Rebalance, subject to cooldown and `dryRun` |
| `unknown` | Price missing, not finite, or not positive | Warning logged; never triggers a rebalance |

The warning level is always half the depeg threshold.

## Rebalance behaviour

1. Read the latest price of each watched asset from the treasury rebalancer's price store.
2. Classify each asset. If none is `depeg`, stop here.
3. Simulate the rebalance with the entry's slippage tolerance. If the simulation has no actions, stop.
4. If `dryRun` is set, stop and report `executed: false`.
5. If the last automated rebalance for this job was less than **15 minutes** ago, stop and report `executed: false`. This cooldown stops a price oscillating around the threshold from causing repeated execution.
6. Otherwise execute the rebalance and record the time.

## Where it runs

- The API's declarative job (`api/src/jobs/rebalance.job.ts`) dispatches the `stablecoin-peg` entry to the peg monitor. It never runs it through the treasury drift logic.
- Local and dev use the API's single in-process interval, which is hourly. That is the cadence the local runner gets, not the five-minute cron.
- Production attaches each entry to a BullMQ worker on its `targetQueue`, which runs it on its own cron.

## Limitations

- Prices come from the treasury rebalancer's price store. The monitor does not yet read a live oracle feed, so keep that store current before relying on the monitor in production.
- The bot package owns the declarative schedule. The peg logic itself lives in the API, because the API cannot import from `bots/` under its `rootDir`.
