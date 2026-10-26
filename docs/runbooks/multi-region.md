# Multi-region deployment and failover

This runbook defines a provider-neutral active/passive deployment for the API
and Oracle. Deploy identical reviewed releases in two independent regions and
put them behind a provider-managed global load balancer or DNS failover service.
The provider-specific routing policy, region names, health-check path, DNS TTL,
and data-replication mechanism must be recorded by the service owner before
production cutover; no hosting provider or region is assumed by this repository.

## Topology and prerequisites

- Primary and secondary instances run the same immutable release and database
  schema. Both have independent health endpoints and use the correct Stellar
  network.
- Configure the global router to send traffic to the primary and remove an
  endpoint when its health check fails; use a documented recovery threshold
  and cooldown to avoid flapping.
- Keep the secondary region warm and deploy it from the same release artifact.
- PostgreSQL must use a supported cross-region replication or restore strategy.
  Define its consistency and failover procedure before enabling writes. Redis
  is disposable cache state and must not be the source of truth.
- Keep secrets in the provider's secret manager and make them available to both
  regions without copying them into images or repository files.

## Probe both regions

The repository provides a bounded, read-only health probe:

```bash
node scripts/ops/failover-check.mjs \
  --primary https://<primary-host>/api/health \
  --secondary https://<secondary-host>/api/health
```

With both endpoints healthy, the check reports the primary as selected. If the
primary is unhealthy but the secondary is healthy, it reports the secondary as
a failover candidate and succeeds; an unhealthy secondary fails the check. It
does not mutate DNS or load-balancer configuration. A successful probe is not
proof of traffic failover until the provider router is configured and its
cutover has been exercised.

Run a controlled failover test in staging:

1. Confirm both regions serve the same build and both health endpoints pass.
2. Record the current route and database replication lag.
3. Make the primary health check fail using the hosting provider's staging
   controls; do not stop production instances to test.
4. Run the probe above and confirm the secondary is selected. Confirm the
   provider router removes the primary and sends synthetic read traffic to the
   secondary within the agreed recovery objective.
5. Verify writes are safe for the configured data topology, restore the primary,
   confirm replication is caught up, and fail back under observation.
6. Record cutover/failback times, lost or duplicated requests, replication lag,
   and follow-up actions. Repeat quarterly and after routing changes.

CI can test the probe's selection behavior with mocked endpoints:

```bash
node --test scripts/ops/failover-check.test.mjs
```

The mock test does not replace the staging routing exercise. Do not claim
multi-region readiness until the actual provider-managed router and data
replication have passed the controlled drill.
