# Indexer Health & Lag Monitoring Guide

Issue reference: **#1423 — Add indexer health monitoring**

This guide documents the health monitoring, lag tracking, metrics exposition, alerting rules, and operator dashboard for the StellarLend Soroban contract event indexer.

---

## 1. Overview & Architecture

The StellarLend Event Indexer continuously synchronizes smart contract events from the Stellar network ledger. Previously, indexer operations were opaque: operators could not easily determine if the indexer was falling behind, stalled due to RPC rate limiting, failing to deliver to downstream sinks, or completely halted.

Issue #1423 introduces full observability across three primary pillars:
1. **Metrics**: Real-time measurement of ledger lag, sync execution duration, ingested events, consecutive errors, query latency, and sink health, exported in standard Prometheus format and structured JSON.
2. **Alerts**: Real-time evaluation against configurable operational thresholds (lag warnings, critical lag, stalls, consecutive errors, sink failures), with Prometheus alert rules and API-based alert lifecycle management (including operator acknowledgements).
3. **Dashboard**: An interactive frontend monitoring interface ([`IndexerHealthDashboard.tsx`](file:///frontend/src/components/IndexerHealthDashboard.tsx)) featuring live KPI cards, SVG lag timeline visualizations, component diagnostics, and alert management, integrated into [`EventAnalyticsDashboard.tsx`](file:///frontend/src/components/EventAnalyticsDashboard.tsx).

```
 +------------------------+
 | Stellar RPC / Network  |
 +-----------+------------+
             |
             v
 +--------------------------------------------------------------------------+
 |                      StellarLend Event Indexer                          |
 |                                                                          |
 |  [ Ingestion Poller ] ---> [ Event Memory / Archive ]                   |
 |           |                               |                              |
 |           v                               v                              |
 |  [ Sync Telemetry ]               [ Downstream Sinks ]                   |
 |   - latestIndexedLedger            - Analytics DB                        |
 |   - latestNetworkLedger            - WebSocket Push                      |
 |   - syncDurationMs                 - Webhook Dispatcher                  |
 |   - consecutiveErrors                                                    |
 +-----------+--------------------------------------------------------------+
             |
             +----------------------------+
             |                            |
             v                            v
 +------------------------+   +------------------------+
 |  IndexerAlertManager   |   | Metrics Formatter      |
 |  - Lag thresholds      |   | - Prometheus text      |
 |  - Stall detection     |   | - Structured JSON      |
 |  - Acknowledgements    |   |                        |
 +-----------+------------+   +-----------+------------+
             |                            |
             +-------------+--------------+
                           |
                           v
        +------------------------------------+
        |      HTTP API & Health Probes      |
        |  - GET  /api/events/indexer/health | (200 / 503 probe)
        |  - GET  /api/events/indexer/metrics| (Prometheus / JSON)
        |  - GET  /api/events/indexer/alerts |
        |  - POST /api/events/indexer/alerts/:id/ack
        |  - GET  /api/metrics/indexer       |
        +------------------+-----------------+
                           |
              +------------+------------+
              |                         |
              v                         v
+---------------------------+ +----------------------------+
| Prometheus & Alertmanager | | Indexer Health Dashboard   |
| (monitoring/alerts/*.yml) | | (React + Tailwind + SVG)   |
+---------------------------+ +----------------------------+
```

---

## 2. Metrics Exposition

### Endpoints

| Method | Endpoint | Accept Header | Description | Auth |
| :--- | :--- | :--- | :--- | :--- |
| `GET` | `/api/events/indexer/metrics` | `text/plain` (default) | Standard Prometheus exposition format | Public / Scraper |
| `GET` | `/api/events/indexer/metrics` | `application/json` | Structured JSON metrics object | Public / Scraper |
| `GET` | `/api/metrics/indexer` | Any | Unified metrics controller route | Public / Scraper |
| `GET` | `/api/events/indexer/health` | `application/json` | Health status probe (200 OK / 503 Service Unavailable) | Public / K8s Probe |

### Prometheus Metric Definitions

| Metric Name | Type | Description |
| :--- | :--- | :--- |
| `indexer_up` | Gauge | `1` if the indexer polling loop is running, `0` otherwise |
| `indexer_syncing` | Gauge | `1` if an ingest cycle is actively in-flight, `0` otherwise |
| `indexer_health_status` | Gauge | `0` = healthy, `1` = degraded (warning alerts), `2` = unhealthy (critical alerts) |
| `indexer_lag_ledgers` | Gauge | Number of ledgers between the network head and the latest indexed ledger |
| `indexer_lag_seconds_estimate` | Gauge | Estimated time lag in seconds (based on 5.0 seconds per Stellar ledger) |
| `indexer_latest_indexed_ledger` | Gauge | Sequence number of the most recently indexed ledger |
| `indexer_latest_network_ledger` | Gauge | Sequence number of the highest known network ledger |
| `indexer_events_total` | Counter | Total number of contract events ingested since startup |
| `indexer_hot_events` | Gauge | Current number of unarchived events held in hot memory |
| `indexer_sync_total` | Counter | Cumulative number of sync cycles by `status` label (`status="success"` or `status="error"`) |
| `indexer_sync_duration_ms` | Gauge | Execution time of the most recent sync cycle in milliseconds |
| `indexer_last_sync_age_seconds` | Gauge | Seconds elapsed since the last successful sync cycle |
| `indexer_consecutive_errors` | Gauge | Current count of consecutive failed sync cycles |
| `indexer_sink_failures_total` | Counter | Downstream event sink delivery failures partitioned by `sink` name |
| `indexer_query_latency_ms` | Gauge | In-memory search query latency quantiles partitioned by `quantile` (`0.50`, `0.95`, `max`) |
| `indexer_alerts_active` | Gauge | Count of active alerts partitioned by `severity` (`info`, `warning`, `critical`) |

### Example Prometheus Scrape Output

```
# HELP indexer_up Whether the contract event indexer is running
# TYPE indexer_up gauge
indexer_up 1

# HELP indexer_syncing Whether a sync cycle is currently active
# TYPE indexer_syncing gauge
indexer_syncing 0

# HELP indexer_health_status Current health state (0=healthy, 1=degraded, 2=unhealthy)
# TYPE indexer_health_status gauge
indexer_health_status 0

# HELP indexer_lag_ledgers Difference between network head ledger and latest indexed ledger
# TYPE indexer_lag_ledgers gauge
indexer_lag_ledgers 3

# HELP indexer_lag_seconds_estimate Estimated lag in seconds based on 5.0s/ledger
# TYPE indexer_lag_seconds_estimate gauge
indexer_lag_seconds_estimate 15.00

# HELP indexer_latest_indexed_ledger Sequence number of the latest indexed ledger
# TYPE indexer_latest_indexed_ledger gauge
indexer_latest_indexed_ledger 500000

# HELP indexer_latest_network_ledger Sequence number of the latest known network ledger
# TYPE indexer_latest_network_ledger gauge
indexer_latest_network_ledger 500003

# HELP indexer_events_total Total number of events ingested
# TYPE indexer_events_total counter
indexer_events_total 12450

# HELP indexer_sync_total Total number of sync cycles completed
# TYPE indexer_sync_total counter
indexer_sync_total{status="success"} 420
indexer_sync_total{status="error"} 0

# HELP indexer_sync_duration_ms Duration of the last sync run in milliseconds
# TYPE indexer_sync_duration_ms gauge
indexer_sync_duration_ms 42.15

# HELP indexer_alerts_active Current active alerts by severity
# TYPE indexer_alerts_active gauge
indexer_alerts_active{severity="critical"} 0
indexer_alerts_active{severity="warning"} 0
indexer_alerts_active{severity="info"} 0
```

---

## 3. Alerting Rules & Thresholds

The indexer evaluates rules on every sync cycle and status request.

### Built-in Thresholds

```typescript
export interface AlertThresholds {
  lagWarningLedgers: 20;     // Lag >= 20 ledgers (~100 seconds)
  lagCriticalLedgers: 100;   // Lag >= 100 ledgers (~500 seconds)
  stallWarningSeconds: 60;   // No successful sync in >= 60 seconds while running
  stallCriticalSeconds: 300; // No successful sync in >= 300 seconds (5 min) while running
  maxConsecutiveErrors: 3;   // >= 3 failed syncs in a row triggers warning/critical
}
```

### Alert Rules Catalog

| Rule ID | Severity | Condition | Description |
| :--- | :--- | :--- | :--- |
| `lag_critical` | `critical` | `lagLedgers >= 100` | Critical indexer lag: indexer is more than 100 ledgers (~500s) behind network head. |
| `lag_warning` | `warning` | `lagLedgers >= 20` | Elevated indexer lag: indexer is 20–99 ledgers behind network head. |
| `sync_stalled` | `critical` / `warning` | `lastSyncAge >= 300s` (`critical`) or `>= 60s` (`warning`) | Sync stalled: running indexer has not completed a sync cycle within threshold. |
| `sync_error` | `critical` / `warning` | `consecutiveErrors >= 3` | Consecutive sync failures: upstream RPC errors, deserialization, or network timeout. |
| `sink_failure` | `warning` | `failures > 0` | Downstream event sink delivery failures detected. |
| `indexer_stopped` | `warning` | `running == false` | Indexer polling loop is currently stopped. |

### Prometheus Alertmanager Rules

Pre-configured Prometheus alert rules are defined in [`monitoring/alerts/indexer.rules.yml`](file:///monitoring/alerts/indexer.rules.yml):

- `IndexerCriticalLag`: Triggers if `indexer_lag_ledgers > 100` for 2 minutes.
- `IndexerHighLag`: Triggers if `indexer_lag_ledgers > 20` for 5 minutes.
- `IndexerSyncStalled`: Triggers if `indexer_up == 1` and `indexer_last_sync_age_seconds > 180` for 2 minutes.
- `IndexerProcessDown`: Triggers if `indexer_up == 0` for 1 minute.
- `IndexerConsecutiveErrors`: Triggers if `indexer_consecutive_errors >= 3` for 1 minute.
- `IndexerSinkFailure`: Triggers if `increase(indexer_sink_failures_total[5m]) > 5`.

---

## 4. Alert Lifecycle & Management API

Alerts are automatically updated:
- **Auto-resolution**: When the underlying metric recovers (e.g. lag drops below threshold or sync resumes), the alert automatically clears from the active list and moves to alert history.
- **Operator Acknowledgement**: Operators can acknowledge active alerts via the API or dashboard to silence repeated notifications while remediation is in progress.
- **Operator Clear**: Operators with the `operator` role can clear acknowledged alerts.

### Alert Management Endpoints

#### 1. List Alerts
```http
GET /api/events/indexer/alerts
```
Response:
```json
{
  "active": [
    {
      "id": "lag_warning:lag_ledgers",
      "rule": "lag_warning",
      "severity": "warning",
      "title": "Elevated indexer lag",
      "message": "Indexer is 28 ledgers behind network head (~140s)",
      "metric": "lag_ledgers",
      "currentValue": 28,
      "threshold": 20,
      "triggeredAt": 1792395900000,
      "acknowledged": false
    }
  ],
  "history": [],
  "thresholds": {
    "lagWarningLedgers": 20,
    "lagCriticalLedgers": 100,
    "stallWarningSeconds": 60,
    "stallCriticalSeconds": 300,
    "maxConsecutiveErrors": 3
  }
}
```

#### 2. Acknowledge Alert
```http
POST /api/events/indexer/alerts/:id/ack
```
Response:
```json
{
  "message": "Alert acknowledged",
  "alert": {
    "id": "lag_warning:lag_ledgers",
    "acknowledged": true,
    "acknowledgedAt": 1792395950000
  }
}
```

#### 3. Clear Acknowledged Alerts (Operator role required)
```http
POST /api/events/indexer/alerts/clear
Authorization: Bearer <jwt-token-with-operator-role>
```
Response:
```json
{
  "message": "Cleared 1 acknowledged alerts",
  "clearedCount": 1
}
```

---

## 5. Kubernetes & Health Check Integration

The `/api/events/indexer/health` endpoint serves as an automated health probe:
- **HTTP 200 OK**: Returned when `healthStatus` is `healthy` or `degraded` (warning alerts present).
- **HTTP 503 Service Unavailable**: Returned when `healthStatus` is `unhealthy` (critical alerts present, such as critical lag >= 100 ledgers or stall > 300s).

Kubernetes probe configuration example:
```yaml
livenessProbe:
  httpGet:
    path: /api/events/indexer/health
    port: 3000
  initialDelaySeconds: 30
  periodSeconds: 15
  timeoutSeconds: 5
  failureThreshold: 3

readinessProbe:
  httpGet:
    path: /api/events/indexer/health
    port: 3000
  initialDelaySeconds: 10
  periodSeconds: 10
  timeoutSeconds: 3
  failureThreshold: 2
```

---

## 6. Frontend Dashboard

The frontend monitoring interface is provided by [`IndexerHealthDashboard.tsx`](file:///frontend/src/components/IndexerHealthDashboard.tsx) and embedded in [`EventAnalyticsDashboard.tsx`](file:///frontend/src/components/EventAnalyticsDashboard.tsx).

### Features
1. **Status Header & Health Badge**:
   - Live visual indicator (`HEALTHY` in emerald, `DEGRADED` in amber, `CRITICAL` in rose).
   - Sync activity pulse (`Syncing...` animation).
   - Configurable auto-refresh interval selector (`5s`, `10s`, `30s`, `Off`) and manual refresh trigger.
   - Manual sync trigger button for operators.
2. **Key Metric Cards (KPIs)**:
   - **Indexer Lag**: Displays lag in ledgers and estimated seconds (`~X.Xs`), color-coded by threshold.
   - **Ledger Head Comparison**: Compares latest indexed ledger sequence number against latest network ledger.
   - **Sync Duration & Age**: Shows duration of last sync cycle in milliseconds, success rate, and time since last sync.
   - **Ingested Events**: Shows total lifetime events ingested and active hot memory events.
3. **Active Alerts Panel**:
   - Lists active alerts with severity badges and human-readable explanations.
   - Provides an inline "Acknowledge" button with instant UI updates.
4. **Historical Lag & Sync Timeline**:
   - Interactive SVG timeline chart showing rolling sync latency, ledger lag, and success/error status over the last 50 sync cycles.
   - Hover tooltips detailing ledger numbers, sync duration, and errors for any given cycle.
5. **Component & Sink Diagnostics**:
   - System component status grid (Polling loop, Ingestion engine, Memory store, Cold archive, Schema validator).
   - Downstream event sinks status table with delivery failure counters.
6. **Navigation & Drilldown**:
   - Tab navigation in `EventAnalyticsDashboard` allowing instant toggling between "Event Analytics" and "Indexer Health & Lag".
   - Direct drilldown link on the "Indexer lag" tile in `EventAnalyticsDashboard`.
