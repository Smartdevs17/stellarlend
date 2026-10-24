# Disaster recovery runbook

This runbook covers the off-chain API, Oracle, PostgreSQL data, Redis cache, and
the operator access needed to recover them. Soroban ledger state is authoritative
for on-chain positions; do not treat a contract redeployment as a restoration of
existing contract state.

## Recovery objectives

| Service/data | Target RTO | Target RPO | Recovery source |
| --- | ---: | ---: | --- |
| API and Oracle | 4 hours | 1 hour | Reviewed release artifact, configuration and secrets |
| PostgreSQL audit/subscription data | 4 hours | 1 hour | Encrypted off-site backup, with a successful restore test |
| Redis cache | 4 hours | 0 | Rebuild from PostgreSQL and the Stellar ledger |
| Soroban contract state | Network-dependent | 0 after ledger finality | Stellar ledger; verify contract IDs and ledger state |

These are service objectives, not claims about current hosting. The RPO for
PostgreSQL is only achievable when encrypted backups complete at least hourly
and are copied to a separate failure domain. Before production sign-off, record
the actual provider, backup cadence, retention, measured restore time, and owner
in the incident record. If the provider cannot meet an objective, escalate and
revise the objective with the service owner rather than silently accepting drift.

## Roles and prerequisites

- Incident lead declares the incident, records timestamps, assigns a recovery
  operator, and coordinates user communications.
- Recovery operator uses least-privilege, audited access to the hosting control
  plane, database backups, DNS/load balancer, and secret store.
- Contract administrator confirms deployed contract IDs and ledger status.
- Keep encrypted backups and signing credentials in separate failure domains.
  Never put database dumps, deployer keys, or production secrets in this repo or
  GitHub Actions artifacts.

## Recovery procedure

1. **Declare and contain.** Record detection time and affected regions/services.
   Stop unsafe writes or deployments; do not pause repay or other user safety
   operations without the incident lead's approval.
2. **Select the recovery point.** Identify the latest verified PostgreSQL
   backup at or before the incident. Check its checksum, age, encryption, and
   restore-test status. Reject corrupt or unverified backups.
3. **Restore into an isolated target.** Provision the replacement service and
   database in the selected recovery region. Restore into a new database first;
   do not overwrite the only surviving copy. Reapply schema migrations from the
   exact reviewed release.
4. **Restore configuration.** Retrieve secrets from the approved secret store,
   rotate credentials that may have been exposed, and use the matching Stellar
   network/RPC configuration. Do not copy secrets through shell history or
   workflow logs.
5. **Validate before routing traffic.** Confirm database migrations and
   representative audit/subscription rows, API health, Oracle freshness, cache
   rebuild, and read-only contract queries. Compare contract IDs and on-chain
   state with the deployment manifest and a recent finalized ledger.
6. **Cut over.** Route traffic to the recovered region using the configured
   global load balancer or DNS failover. Keep the prior environment isolated
   until consistency and security checks pass.
7. **Close out.** Record detection, restore, and service-return timestamps;
   calculate measured RTO/RPO; record data loss, actions, and follow-ups.

## Restore rehearsal and sign-off

Run a restore rehearsal at least quarterly and after a database/provider
migration:

1. Restore a recent encrypted backup into an isolated non-production database.
2. Run the same schema migrations and release build intended for production.
3. Run API tests and verify representative restored data and a ledger-backed
   read-only request. Confirm secrets and public production routes were not used.
4. Measure restore time and backup age; compare them with the targets above.
5. Attach the sanitized command log and measurements to the operations review.
   Do not mark the RTO/RPO acceptance criteria complete until an operator has
   recorded a successful rehearsal in the actual hosting environment.

The repository's local Docker Compose file is for development, not a production
backup or recovery system. Configure provider-managed encrypted backups and
off-site retention before relying on this runbook.

## Rollback versus recovery

Use [the deployment guide](../DEPLOYMENT.md#rollback-procedures) for code
rollback. A code rollback does not restore database contents or revert
transactions already finalized on Stellar. For contract incidents, preserve
ledger evidence and follow the contract-specific upgrade/rollback runbook; do
not redeploy a contract as a substitute for recovering its state.
