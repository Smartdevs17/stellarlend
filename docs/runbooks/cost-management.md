# Cost dashboard and budget alerts

The cost reporter produces a monthly dashboard from provider exports normalized
to a small JSON schema. This keeps the checks provider-neutral; the service
owner must configure an exporter for the selected hosting, database, RPC, and
Stellar-related billing sources.

## Configure budgets and exports

Copy [budget.example.json](../../ops/costs/budget.example.json) and
[snapshot.example.json](../../ops/costs/snapshot.example.json) to protected
operational storage. The examples are illustrative only and are not real
budgets or measured spend.

The budget contains a currency, warning percentage, monthly total, and monthly
budget per category. The snapshot must contain the same currency, a `YYYY-MM`
month, an ISO timestamp, and one or more actuals per category:

```json
{
  "month": "2026-10",
  "currency": "USD",
  "updatedAt": "2026-10-18T00:00:00.000Z",
  "actuals": [
    { "category": "api", "provider": "normalized provider export", "amount": 120.50 }
  ]
}
```

Every snapshot category must exist in the budget. Convert provider exports to
the configured currency before reporting; do not mix currencies or omit an
unavailable data source. Include a zero-amount entry when a configured category
has no spend so a missing export cannot look like zero. Store actual cost snapshots and credentials only in
access-controlled operational storage. Do not commit real billing data to a
public repository.

## Generate dashboard and alerts

```bash
node scripts/ops/cost-report.mjs \
  --budget /secure/path/budget.json \
  --snapshot /secure/path/monthly-snapshot.json
```

The report lists actuals, monthly budgets, and budget utilization per category
and overall. It emits a warning at the configured threshold and exits with
failure when any category or the total reaches its budget. On a protected,
private GitHub repository, run the `Platform operations` workflow with
`cost-report` and approved snapshot paths to publish the dashboard to the
workflow summary. The example workflow defaults are demo data, not production
monitoring.

## Review cadence and response

- Finance/service owner reviews the dashboard monthly and after a major
  deployment; record the reviewer, date, variances, and approved budget changes.
- Configure the provider exporter at least daily. Investigate missing or stale
  data rather than treating it as zero spend.
- At the warning threshold, identify the cost driver, forecast month-end spend,
  and notify the service owner. At or above 100%, follow the team's approved
  budget escalation and reduce or approve spend before increasing limits.
- Retain the normalized source totals and review record in restricted storage
  for the organization's required retention period.

Validate the reporter with:

```bash
node --test scripts/ops/cost-report.test.mjs
node scripts/ops/cost-report.mjs \
  --budget ops/costs/budget.example.json \
  --snapshot ops/costs/snapshot.example.json
```
