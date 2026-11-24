# Grace-period scan job (issue #1758)

`gracePeriodScan.ts` is the worker-side scheduler for the grace-period alert
service. It has no runtime imports, so it can run in the worker process, in the
API process, or in tests.

```ts
const scheduler = startGracePeriodScan(positionSource, alertService, {
  intervalMs: 60_000,
  logger,
  metrics,
});
// scheduler.stop() on shutdown
```

Concurrent ticks share one in-flight scan. Invalid positions are skipped and
counted; a failing position or an unavailable source never aborts the rest of
the batch. The report exposes `scanned`, `eligible`, `skipped`, `alertsSent`,
`delivered`, `failed`, and per-position errors.

## Validation

```bash
node --test workers/src/jobs/__tests__/gracePeriodScan.test.mjs
```
