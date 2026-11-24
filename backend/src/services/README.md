# Grace-period alerts (issue #1758)

`gracePeriodAlerts.ts` turns position health-factor readings into a small,
deduplicated set of notifications sent before liquidation.

## Stages

| Health factor | Stage | Action |
| --- | --- | --- |
| `>= 1.5` | `safe` | clear previous alert state |
| `1.2 .. 1.5` | `warning` | notify once, then 15-minute cooldown |
| `1.0 .. 1.2` | `grace` | escalate immediately (bypasses cooldown) |
| `< 1.0` | `critical` | escalate immediately, critical severity |

Thresholds, cooldown, maximum alerts per position, clock, logger, metrics, and
delivery are all injectable. Escalation bypasses the cooldown; repeated alerts
at the same stage stop after `maxAlertsPerPosition` (3 by default). A failed
delivery does not update state, so the next scan retries it.

## Integration

```ts
const alerts = new GracePeriodAlertService(notifier, {
  logger,
  metrics,
  thresholds: { warningHf: 1.5, graceHf: 1.2, liquidationHf: 1.0 },
});
const decision = await alerts.process(position);
```

`workers/src/jobs/gracePeriodScan.ts` is the recommended scheduler. Every
alert carries the address, asset, health factor, required price drop, and a
timestamps; every decision increments a counter and an HF gauge.

## Validation

```bash
node --test backend/src/services/__tests__/gracePeriodAlerts.test.mjs
```
