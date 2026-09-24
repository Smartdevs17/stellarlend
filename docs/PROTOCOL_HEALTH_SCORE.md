# Protocol Health Score

Reference for the composite protocol health score (issue #692).

Protocol health used to be assessed by reading several dashboards and forming a
judgement. That works until it matters — during an incident, when the question
"is the protocol healthy right now?" needs an answer in seconds and the person
answering may not be the person who built the dashboards. This score reduces
six independent risk signals to a single 0–100 number, keeps the components
visible so the number can be explained, and alerts when it crosses a threshold.

A single number is a summary, not a verdict. The components are always returned
alongside it precisely so that a low score can be attributed to a cause rather
than treated as an oracle in its own right.

## The score

`overallScore` is the weighted sum of six component scores, each also 0–100,
where 100 is healthy. Default weights:

| Component | Weight | Measures | Source |
| --- | ---: | --- | --- |
| `badDebt` | 0.25 | Bad debt as a fraction of total borrows | `stellar.service` |
| `liquidity` | 0.20 | Available liquidity vs. deposits, averaged per pool | `stellar.service` |
| `capitalEfficiency` | 0.15 | Utilization against the optimal band | `analytics.service` |
| `concentration` | 0.15 | Borrower/depositor concentration (HHI) | `concentrationMonitor.service` |
| `oracleHealth` | 0.15 | Price staleness, TWAP deviation, source diversity | `riskMonitoring.service` |
| `governanceHealth` | 0.10 | Staker participation and voting-power distribution | `staking.service` |

Weights reflect how quickly a component can become unrecoverable. Bad debt is
weighted highest because it is the one failure that cannot be undone by waiting;
governance health is weighted lowest because it degrades over weeks, not blocks.

Every component reuses an existing service rather than re-deriving its own view
of protocol state. That is deliberate: two subsystems computing "utilization"
slightly differently is exactly how a monitoring system starts disagreeing with
the thing it monitors.

### How each component scores

**`capitalEfficiency`** — 100 inside the 70–90% utilization band. Below it,
capital sits idle; above it, there is little buffer for withdrawals. The score
falls linearly with distance from the band, reaching 0 thirty utilization points
outside it (40% or 120%).

**`liquidity`** — the average available-liquidity ratio across pools, scaled so
that 30% available scores 100 and 0% available scores 0.

**`badDebt`** — bad debt over total borrows, scaled so 0% scores 100 and 1%
(100bps) scores 0. The 1% ceiling is the worst ratio treated as survivable.
Bad debt currently uses the same 0.1%-of-deposits synthetic convention as
`poolPerformance.service`, applied consistently rather than introducing a
second, differing assumption.

**`concentration`** — an inverse Herfindahl-Hirschman Index: HHI 0 (perfectly
dispersed) scores 100, HHI 10000 (a single participant) scores 0.

**`oracleHealth`** — 60% from per-asset freshness and TWAP deviation (staleness
reaching 300s or deviation reaching 200bps scores that asset 0), 40% from source
diversity against a target of three independent providers. The provider count is
static because the oracle service does not expose a live count today; it changes
rarely, and a stale constant is more honest than a fabricated query.

**`governanceHealth`** — the mean of staker participation (against a target of
50 distinct stakers) and voting-power distribution (inverse HHI). With no
stakers at all the component returns 50, not 0: absence of data is not evidence
of ill health, and scoring it as a failure would drag the composite down for a
protocol that simply has not launched governance yet.

## API

All routes are under `/api/health`.

| Method | Path | Purpose |
| --- | --- | --- |
| `GET` | `/score` | Full score: overall, all components, active weights, timestamp |
| `GET` | `/score/summary` | Overall score and a status label only |
| `GET` | `/score/history` | Recorded history points (`?limit=`) |
| `GET` | `/score/trend` | Direction, slope, volatility, per-component movement |
| `GET` | `/score/alerts` | Active threshold breaches (empty when healthy) |
| `GET` | `/score/weights` | Current component weights |
| `PUT` | `/score/weights` | Update weights (partial updates allowed) |
| `PUT` | `/score/alert-threshold` | Set the alert threshold (0–100) |

`GET /api/health/score` returns:

```json
{
  "overallScore": 82.4,
  "components": {
    "capitalEfficiency": 100,
    "liquidity": 78.2,
    "badDebt": 90.1,
    "concentration": 65.4,
    "oracleHealth": 88.0,
    "governanceHealth": 50
  },
  "weights": {
    "capitalEfficiency": 0.15,
    "liquidity": 0.2,
    "badDebt": 0.25,
    "concentration": 0.15,
    "oracleHealth": 0.15,
    "governanceHealth": 0.1
  },
  "timestamp": "2026-09-24T07:00:00.000Z"
}
```

Scores are cached for 300s. Updating weights invalidates the cache immediately,
so a weight change is reflected on the next request rather than up to five
minutes later.

### Updating weights

`PUT /api/health/score/weights` accepts a partial set and **normalizes the
result to sum to 1**:

```bash
curl -X PUT /api/health/score/weights \
  -H 'Content-Type: application/json' \
  -d '{"badDebt": 0.4, "oracleHealth": 0.25}'
```

Normalizing rather than rejecting a non-unit sum means callers can express
relative priorities without having to rebalance every other weight by hand. A
weight set summing to zero or less is rejected.

## Trending

`GET /api/health/score/trend` computes direction over the recorded history using
a least-squares slope, reported as `improving`, `stable`, or `declining`, with
the volatility (standard deviation) of the window and per-component movement.

The trend is computed from the same history points the score service records, so
it can never disagree with the scores it summarizes. History is capped at 90
points; a window with nothing in it returns an explicit empty trend rather than
a fabricated `stable`.

The slope matters more than the level. A score of 70 that is climbing and a
score of 70 that is falling call for different responses, and the overall number
alone cannot distinguish them.

## Alerting

`GET /api/health/score/alerts` returns an alert when the overall score is below
the configured threshold (default 60), and an empty array otherwise. The
threshold is set via `PUT /api/health/score/alert-threshold`.

The endpoint reports current state rather than emitting events, so a poller
cannot miss a breach that occurred between polls — it will still see the alert
as long as the condition holds.

## Related

- [`RISK_MONITORING_DASHBOARD.md`](./RISK_MONITORING_DASHBOARD.md) — the
  underlying risk signals.
- [`ORACLE_STRESS_TESTING.md`](./ORACLE_STRESS_TESTING.md) — how the oracle
  component's inputs are stress tested.
- [`POSITION_HEALTH_SIMULATION.md`](./POSITION_HEALTH_SIMULATION.md) —
  per-position health, distinct from this protocol-wide score.
