import {
  IndexerAlertManager,
  DEFAULT_ALERT_THRESHOLDS,
  formatIndexerMetricsAsPrometheus,
  formatIndexerMetricsAsJson,
  type IndexerStatus,
} from '../services/eventIndex';

describe('Indexer Health & Lag Monitoring (Issue #1423)', () => {
  describe('IndexerAlertManager', () => {
    let alertManager: IndexerAlertManager;

    beforeEach(() => {
      alertManager = new IndexerAlertManager({
        lagWarningLedgers: 20,
        lagCriticalLedgers: 100,
        stallWarningSeconds: 60,
        stallCriticalSeconds: 300,
        maxConsecutiveErrors: 3,
      });
    });

    it('returns empty alerts when indexer is completely healthy', () => {
      const alerts = alertManager.evaluate({
        running: true,
        lastSyncAt: Date.now() - 5000, // 5s ago
        lastError: null,
        lagLedgers: 2,
        consecutiveErrors: 0,
        sinks: [{ name: 'elasticsearch', failures: 0 }],
      });

      expect(alerts).toHaveLength(0);
      expect(alertManager.getActiveAlerts()).toHaveLength(0);
    });

    it('triggers lag_warning when lag is between warning and critical thresholds', () => {
      const alerts = alertManager.evaluate({
        running: true,
        lastSyncAt: Date.now() - 5000,
        lastError: null,
        lagLedgers: 35,
        consecutiveErrors: 0,
        sinks: [],
      });

      expect(alerts).toHaveLength(1);
      expect(alerts[0]).toMatchObject({
        rule: 'lag_warning',
        severity: 'warning',
        metric: 'lagLedgers',
        currentValue: 35,
        threshold: 20,
      });
    });

    it('triggers lag_critical when lag exceeds critical threshold and resolves warning', () => {
      alertManager.evaluate({
        running: true,
        lastSyncAt: Date.now() - 5000,
        lastError: null,
        lagLedgers: 35,
        consecutiveErrors: 0,
        sinks: [],
      });

      const alerts = alertManager.evaluate({
        running: true,
        lastSyncAt: Date.now() - 5000,
        lastError: null,
        lagLedgers: 120,
        consecutiveErrors: 0,
        sinks: [],
      });

      expect(alerts).toHaveLength(1);
      expect(alerts[0]).toMatchObject({
        rule: 'lag_critical',
        severity: 'critical',
        currentValue: 120,
        threshold: 100,
      });
    });

    it('auto-resolves lag alerts when lag catches back up to tip', () => {
      alertManager.evaluate({
        running: true,
        lastSyncAt: Date.now() - 5000,
        lastError: null,
        lagLedgers: 50,
        consecutiveErrors: 0,
        sinks: [],
      });
      expect(alertManager.getActiveAlerts()).toHaveLength(1);

      const resolved = alertManager.evaluate({
        running: true,
        lastSyncAt: Date.now() - 5000,
        lastError: null,
        lagLedgers: 5,
        consecutiveErrors: 0,
        sinks: [],
      });
      expect(resolved).toHaveLength(0);
      expect(alertManager.getActiveAlerts()).toHaveLength(0);
    });

    it('triggers sync_stalled when last sync time exceeds stall threshold', () => {
      const alerts = alertManager.evaluate({
        running: true,
        lastSyncAt: Date.now() - 120_000, // 2 minutes ago
        lastError: null,
        lagLedgers: 0,
        consecutiveErrors: 0,
        sinks: [],
      });

      expect(alerts).toHaveLength(1);
      expect(alerts[0].rule).toBe('sync_stalled');
      expect(alerts[0].severity).toBe('warning');
    });

    it('triggers critical sync_error when consecutive errors exceed threshold', () => {
      const alerts = alertManager.evaluate({
        running: true,
        lastSyncAt: Date.now() - 5000,
        lastError: 'Soroban RPC 503 Service Unavailable',
        lagLedgers: 10,
        consecutiveErrors: 4,
        sinks: [],
      });

      expect(alerts).toHaveLength(1);
      expect(alerts[0]).toMatchObject({
        rule: 'sync_error',
        severity: 'critical',
      });
      expect(alerts[0].message).toContain('Soroban RPC 503');
    });

    it('triggers sink_failure when downstream sinks report failures', () => {
      const alerts = alertManager.evaluate({
        running: true,
        lastSyncAt: Date.now() - 5000,
        lastError: null,
        lagLedgers: 0,
        consecutiveErrors: 0,
        sinks: [{ name: 'elasticsearch', failures: 3 }],
      });

      expect(alerts).toHaveLength(1);
      expect(alerts[0]).toMatchObject({
        rule: 'sink_failure',
        severity: 'warning',
      });
      expect(alerts[0].message).toContain('elasticsearch (3 failures)');
    });

    it('allows acknowledging alerts and clearing acknowledged ones', () => {
      const alerts = alertManager.evaluate({
        running: true,
        lastSyncAt: Date.now() - 5000,
        lastError: null,
        lagLedgers: 40,
        consecutiveErrors: 0,
        sinks: [],
      });

      const alertId = alerts[0].id;
      expect(alerts[0].acknowledged).toBe(false);

      const ackResult = alertManager.acknowledgeAlert(alertId);
      expect(ackResult).toBe(true);

      const active = alertManager.getActiveAlerts();
      expect(active[0].acknowledged).toBe(true);
      expect(typeof active[0].acknowledgedAt).toBe('number');

      const cleared = alertManager.clearAcknowledged();
      expect(cleared).toBe(1);
      expect(alertManager.getActiveAlerts()).toHaveLength(0);
      expect(alertManager.getAlertHistory()).toHaveLength(1);
    });
  });

  describe('formatIndexerMetricsAsPrometheus', () => {
    const mockStatus: IndexerStatus = {
      running: true,
      syncing: false,
      healthStatus: 'healthy',
      schemaVersion: 1,
      cursor: 'cursor-123',
      lastSyncAt: 1735689600000,
      lastSyncDurationMs: 45,
      lastError: null,
      lastErrorAt: null,
      totalIngested: 1500,
      hotEvents: 300,
      hotRetentionMs: 2592000000,
      latestIndexedLedger: 5000,
      latestNetworkLedger: 5002,
      lagLedgers: 2,
      lagSeconds: 10,
      syncSuccessCount: 20,
      syncErrorCount: 1,
      consecutiveErrors: 0,
      archive: {
        segments: 2,
        events: 1200,
        oldestLedger: 1000,
        newestLedger: 4700,
      },
      sinks: [{ name: 'elasticsearch', failures: 0 }],
      queryLatency: {
        samples: 50,
        p50Ms: 0.8,
        p95Ms: 2.1,
        maxMs: 5.4,
      },
      syncHistory: [],
      alerts: [],
    };

    it('produces valid Prometheus text exposition with proper headers and metrics', () => {
      const text = formatIndexerMetricsAsPrometheus(mockStatus, []);

      expect(text).toContain('# HELP indexer_up Whether the indexer');
      expect(text).toContain('# TYPE indexer_up gauge');
      expect(text).toContain('indexer_up 1');

      expect(text).toContain('# HELP indexer_lag_ledgers');
      expect(text).toContain('indexer_lag_ledgers 2');

      expect(text).toContain('indexer_lag_seconds_estimate 10');
      expect(text).toContain('indexer_latest_indexed_ledger 5000');
      expect(text).toContain('indexer_latest_network_ledger 5002');
      expect(text).toContain('indexer_events_total 1500');
      expect(text).toContain('indexer_hot_events 300');
      expect(text).toContain('indexer_sync_duration_ms 45');
      expect(text).toContain('indexer_sync_total{status="success"} 20');
      expect(text).toContain('indexer_sync_total{status="failure"} 1');
      expect(text).toContain('indexer_sink_failures_total{sink="elasticsearch"} 0');
      expect(text).toContain('indexer_query_latency_ms{quantile="0.95"} 2.1');
    });
  });

  describe('formatIndexerMetricsAsJson', () => {
    it('produces structured health and lag diagnostics', () => {
      const mockStatus: IndexerStatus = {
        running: true,
        syncing: false,
        healthStatus: 'healthy',
        schemaVersion: 1,
        cursor: null,
        lastSyncAt: Date.now() - 3000,
        lastSyncDurationMs: 25,
        lastError: null,
        lastErrorAt: null,
        totalIngested: 100,
        hotEvents: 100,
        hotRetentionMs: 2592000000,
        latestIndexedLedger: 1000,
        latestNetworkLedger: 1005,
        lagLedgers: 5,
        lagSeconds: 25,
        syncSuccessCount: 10,
        syncErrorCount: 0,
        consecutiveErrors: 0,
        archive: { segments: 0, events: 0, oldestLedger: null, newestLedger: null },
        sinks: [],
        queryLatency: { samples: 10, p50Ms: 1, p95Ms: 2, maxMs: 3 },
        syncHistory: [],
        alerts: [],
      };

      const json = formatIndexerMetricsAsJson(mockStatus, []);
      expect(json.health.status).toBe('healthy');
      expect(json.lag.lagLedgers).toBe(5);
      expect(json.lag.lagSeconds).toBe(25);
      expect(json.sync.lastSyncDurationMs).toBe(25);
      expect(json.throughput.totalIngested).toBe(100);
    });
  });
});
