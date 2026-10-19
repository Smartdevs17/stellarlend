/**
 * Indexer Metrics Formatter — Issue #1423
 *
 * Formats indexer health and lag metrics as Prometheus text exposition format
 * (https://prometheus.io/docs/instrumenting/exposition_formats/) and structured JSON.
 */

import type { IndexerAlert } from './alerts';
import type { IndexerStatus } from './indexer';

export function formatIndexerMetricsAsPrometheus(
  status: IndexerStatus,
  alerts: IndexerAlert[] = []
): string {
  const lines: string[] = [];

  const addMetric = (
    name: string,
    help: string,
    type: 'gauge' | 'counter',
    value: number | null | undefined,
    labels: Record<string, string> = {}
  ) => {
    if (value === null || value === undefined || Number.isNaN(value)) {
      return;
    }
    lines.push(`# HELP ${name} ${help}`);
    lines.push(`# TYPE ${name} ${type}`);

    const labelEntries = Object.entries(labels);
    const labelStr =
      labelEntries.length > 0
        ? `{${labelEntries.map(([k, v]) => `${k}="${v}"`).join(',')}}`
        : '';
    lines.push(`${name}${labelStr} ${value}`);
  };

  // State gauges
  addMetric(
    'indexer_up',
    'Whether the indexer background polling worker is active (1=up, 0=down)',
    'gauge',
    status.running ? 1 : 0
  );

  addMetric(
    'indexer_syncing',
    'Whether the indexer is actively synchronizing with Soroban RPC (1=syncing, 0=idle)',
    'gauge',
    status.syncing ? 1 : 0
  );

  const healthScore =
    status.healthStatus === 'healthy' ? 2 : status.healthStatus === 'degraded' ? 1 : 0;
  addMetric(
    'indexer_health_status',
    'Overall indexer health status (2=healthy, 1=degraded, 0=unhealthy)',
    'gauge',
    healthScore
  );

  // Lag metrics
  addMetric(
    'indexer_lag_ledgers',
    'Number of ledgers the indexer is lagging behind network tip',
    'gauge',
    status.lagLedgers ?? 0
  );

  addMetric(
    'indexer_lag_seconds_estimate',
    'Estimated time lag behind network tip in seconds (~5s per ledger)',
    'gauge',
    status.lagSeconds ?? (status.lagLedgers !== null ? status.lagLedgers * 5 : 0)
  );

  addMetric(
    'indexer_latest_indexed_ledger',
    'Highest ledger number indexed into the store',
    'gauge',
    status.latestIndexedLedger
  );

  addMetric(
    'indexer_latest_network_ledger',
    'Highest known ledger number on the Stellar network',
    'gauge',
    status.latestNetworkLedger
  );

  // Volume & Ingestion counters
  addMetric(
    'indexer_events_total',
    'Total contract events ingested since startup',
    'counter',
    status.totalIngested
  );

  addMetric(
    'indexer_hot_events',
    'Current number of events stored in hot in-memory store',
    'gauge',
    status.hotEvents
  );

  // Sync performance & history
  addMetric(
    'indexer_sync_total',
    'Total sync cycles attempted by status',
    'counter',
    status.syncSuccessCount ?? 0,
    { status: 'success' }
  );

  addMetric(
    'indexer_sync_total',
    'Total sync cycles attempted by status',
    'counter',
    status.syncErrorCount ?? 0,
    { status: 'failure' }
  );

  if (status.lastSyncDurationMs !== null && status.lastSyncDurationMs !== undefined) {
    addMetric(
      'indexer_sync_duration_ms',
      'Duration of the last sync run in milliseconds',
      'gauge',
      status.lastSyncDurationMs
    );
  }

  if (status.lastSyncAt !== null) {
    addMetric(
      'indexer_last_sync_timestamp_seconds',
      'Unix timestamp in seconds when the last sync finished',
      'gauge',
      Math.floor(status.lastSyncAt / 1000)
    );

    const ageSeconds = Math.max(0, Math.floor((Date.now() - status.lastSyncAt) / 1000));
    addMetric(
      'indexer_last_sync_age_seconds',
      'Seconds elapsed since the last sync finished',
      'gauge',
      ageSeconds
    );
  }

  addMetric(
    'indexer_consecutive_errors',
    'Number of consecutive sync failures',
    'gauge',
    status.consecutiveErrors ?? 0
  );

  // Sinks
  for (const sink of status.sinks) {
    addMetric(
      'indexer_sink_failures_total',
      'Total event write failures per downstream sink',
      'counter',
      sink.failures,
      { sink: sink.name }
    );
  }

  // Latency percentiles
  if (status.queryLatency) {
    addMetric(
      'indexer_query_latency_ms',
      'Event store query latency in milliseconds',
      'gauge',
      status.queryLatency.p50Ms,
      { quantile: '0.50' }
    );
    addMetric(
      'indexer_query_latency_ms',
      'Event store query latency in milliseconds',
      'gauge',
      status.queryLatency.p95Ms,
      { quantile: '0.95' }
    );
    addMetric(
      'indexer_query_latency_ms',
      'Event store query latency in milliseconds',
      'gauge',
      status.queryLatency.maxMs,
      { quantile: 'max' }
    );
  }

  // Active alerts count by severity
  const criticalAlerts = alerts.filter((a) => a.severity === 'critical').length;
  const warningAlerts = alerts.filter((a) => a.severity === 'warning').length;
  const infoAlerts = alerts.filter((a) => a.severity === 'info').length;

  addMetric(
    'indexer_alerts_active',
    'Current number of active alerts by severity',
    'gauge',
    criticalAlerts,
    { severity: 'critical' }
  );
  addMetric(
    'indexer_alerts_active',
    'Current number of active alerts by severity',
    'gauge',
    warningAlerts,
    { severity: 'warning' }
  );
  addMetric(
    'indexer_alerts_active',
    'Current number of active alerts by severity',
    'gauge',
    infoAlerts,
    { severity: 'info' }
  );

  return lines.join('\n') + '\n';
}

export function formatIndexerMetricsAsJson(
  status: IndexerStatus,
  alerts: IndexerAlert[] = []
) {
  const now = Date.now();
  const lastSyncAgeSec =
    status.lastSyncAt !== null ? Math.max(0, Math.floor((now - status.lastSyncAt) / 1000)) : null;

  return {
    health: {
      status: status.healthStatus,
      running: status.running,
      syncing: status.syncing,
      lastSyncAgeSeconds: lastSyncAgeSec,
      consecutiveErrors: status.consecutiveErrors ?? 0,
      activeAlertsCount: alerts.length,
    },
    lag: {
      lagLedgers: status.lagLedgers,
      lagSeconds: status.lagSeconds,
      latestIndexedLedger: status.latestIndexedLedger,
      latestNetworkLedger: status.latestNetworkLedger,
    },
    sync: {
      lastSyncAt: status.lastSyncAt,
      lastSyncDurationMs: status.lastSyncDurationMs,
      totalSuccess: status.syncSuccessCount ?? 0,
      totalErrors: status.syncErrorCount ?? 0,
      lastError: status.lastError,
      lastErrorAt: status.lastErrorAt,
    },
    throughput: {
      totalIngested: status.totalIngested,
      hotEvents: status.hotEvents,
      archiveSegments: status.archive.segments,
      archivedEvents: status.archive.events,
    },
    latency: status.queryLatency,
    sinks: status.sinks,
    alerts: alerts,
    timestamp: new Date(now).toISOString(),
  };
}
