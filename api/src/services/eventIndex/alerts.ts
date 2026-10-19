/**
 * Indexer Health Alerts — Issue #1423
 *
 * Evaluates indexer health and lag metrics against configured warning and
 * critical thresholds, tracking active alerts, history, and acknowledgements.
 */

export interface IndexerAlert {
  id: string;
  rule:
    | 'lag_warning'
    | 'lag_critical'
    | 'sync_stalled'
    | 'sync_error'
    | 'sink_failure'
    | 'indexer_stopped';
  severity: 'info' | 'warning' | 'critical';
  title: string;
  message: string;
  metric: string;
  currentValue: number | string | null;
  threshold: number | string | null;
  triggeredAt: number;
  acknowledged: boolean;
  acknowledgedAt?: number;
}

export interface AlertThresholds {
  /** Lag in ledgers that triggers a warning alert (default: 20 ledgers, ~100s) */
  lagWarningLedgers: number;
  /** Lag in ledgers that triggers a critical alert (default: 100 ledgers, ~500s) */
  lagCriticalLedgers: number;
  /** Seconds without a successful sync before warning (default: 60s) */
  stallWarningSeconds: number;
  /** Seconds without a successful sync before critical alert (default: 300s / 5m) */
  stallCriticalSeconds: number;
  /** Consecutive sync failures before warning alert (default: 3) */
  maxConsecutiveErrors: number;
}

export const DEFAULT_ALERT_THRESHOLDS: AlertThresholds = {
  lagWarningLedgers: 20,
  lagCriticalLedgers: 100,
  stallWarningSeconds: 60,
  stallCriticalSeconds: 300,
  maxConsecutiveErrors: 3,
};

export interface AlertEvaluationInput {
  running: boolean;
  lastSyncAt: number | null;
  lastError: string | null;
  lagLedgers: number | null;
  consecutiveErrors: number;
  sinks: Array<{ name: string; failures: number }>;
}

export class IndexerAlertManager {
  private thresholds: AlertThresholds;
  private activeAlerts: Map<string, IndexerAlert> = new Map();
  private alertHistory: IndexerAlert[] = [];
  private maxHistorySize = 100;

  constructor(thresholds?: Partial<AlertThresholds>) {
    this.thresholds = {
      ...DEFAULT_ALERT_THRESHOLDS,
      ...thresholds,
    };
  }

  getThresholds(): AlertThresholds {
    return { ...this.thresholds };
  }

  updateThresholds(newThresholds: Partial<AlertThresholds>): void {
    this.thresholds = {
      ...this.thresholds,
      ...newThresholds,
    };
  }

  /**
   * Evaluate health metrics and return all currently active alerts.
   * Auto-resolves alerts whose trigger conditions no longer apply.
   */
  evaluate(input: AlertEvaluationInput): IndexerAlert[] {
    const now = Date.now();
    const evaluatedRules = new Set<string>();

    // 1. Indexer Stopped Check
    if (!input.running) {
      this.triggerOrRetain(
        'indexer_stopped',
        'warning',
        'Indexer Polling Inactive',
        'The indexer background polling worker is not running.',
        'running',
        'false',
        'true',
        now
      );
      evaluatedRules.add('indexer_stopped');
    }

    // 2. Ledger Lag Checks
    if (input.lagLedgers !== null) {
      if (input.lagLedgers >= this.thresholds.lagCriticalLedgers) {
        this.triggerOrRetain(
          'lag_critical',
          'critical',
          'Critical Indexer Lag',
          `Indexer is lagging ${input.lagLedgers} ledgers behind network tip (threshold: ${this.thresholds.lagCriticalLedgers}).`,
          'lagLedgers',
          input.lagLedgers,
          this.thresholds.lagCriticalLedgers,
          now
        );
        evaluatedRules.add('lag_critical');
        // Clear warning if upgraded to critical
        this.resolve('lag_warning');
      } else if (input.lagLedgers >= this.thresholds.lagWarningLedgers) {
        this.triggerOrRetain(
          'lag_warning',
          'warning',
          'Elevated Indexer Lag',
          `Indexer is lagging ${input.lagLedgers} ledgers behind network tip (threshold: ${this.thresholds.lagWarningLedgers}).`,
          'lagLedgers',
          input.lagLedgers,
          this.thresholds.lagWarningLedgers,
          now
        );
        evaluatedRules.add('lag_warning');
        this.resolve('lag_critical');
      } else {
        this.resolve('lag_warning');
        this.resolve('lag_critical');
      }
    }

    // 3. Sync Stall Checks
    if (input.lastSyncAt !== null) {
      const stallSeconds = Math.floor((now - input.lastSyncAt) / 1000);
      if (stallSeconds >= this.thresholds.stallCriticalSeconds) {
        this.triggerOrRetain(
          'sync_stalled',
          'critical',
          'Indexer Sync Stalled (Critical)',
          `No successful sync for ${stallSeconds}s (threshold: ${this.thresholds.stallCriticalSeconds}s).`,
          'stallSeconds',
          stallSeconds,
          this.thresholds.stallCriticalSeconds,
          now
        );
        evaluatedRules.add('sync_stalled');
      } else if (stallSeconds >= this.thresholds.stallWarningSeconds) {
        this.triggerOrRetain(
          'sync_stalled',
          'warning',
          'Indexer Sync Stalled (Warning)',
          `No successful sync for ${stallSeconds}s (threshold: ${this.thresholds.stallWarningSeconds}s).`,
          'stallSeconds',
          stallSeconds,
          this.thresholds.stallWarningSeconds,
          now
        );
        evaluatedRules.add('sync_stalled');
      } else {
        this.resolve('sync_stalled');
      }
    }

    // 4. Consecutive Sync Failures
    if (input.consecutiveErrors >= this.thresholds.maxConsecutiveErrors || input.lastError) {
      this.triggerOrRetain(
        'sync_error',
        input.consecutiveErrors >= this.thresholds.maxConsecutiveErrors ? 'critical' : 'warning',
        'Indexer Sync Error',
        input.lastError
          ? `Last sync failed: ${input.lastError} (${input.consecutiveErrors} consecutive error${input.consecutiveErrors > 1 ? 's' : ''})`
          : `Indexer failed ${input.consecutiveErrors} consecutive sync cycles.`,
        'consecutiveErrors',
        input.consecutiveErrors,
        this.thresholds.maxConsecutiveErrors,
        now
      );
      evaluatedRules.add('sync_error');
    } else {
      this.resolve('sync_error');
    }

    // 5. Downstream Sink Failures
    const failedSinks = input.sinks.filter((s) => s.failures > 0);
    if (failedSinks.length > 0) {
      const sinkDesc = failedSinks.map((s) => `${s.name} (${s.failures} failures)`).join(', ');
      this.triggerOrRetain(
        'sink_failure',
        'warning',
        'Downstream Sink Failures',
        `Events failed delivery to downstream sinks: ${sinkDesc}.`,
        'sinkFailures',
        failedSinks.reduce((acc, s) => acc + s.failures, 0),
        0,
        now
      );
      evaluatedRules.add('sink_failure');
    } else {
      this.resolve('sink_failure');
    }

    // Auto-resolve stopped alert if running now
    if (input.running) {
      this.resolve('indexer_stopped');
    }

    return this.getActiveAlerts();
  }

  private triggerOrRetain(
    rule: IndexerAlert['rule'],
    severity: IndexerAlert['severity'],
    title: string,
    message: string,
    metric: string,
    currentValue: number | string | null,
    threshold: number | string | null,
    now: number
  ): void {
    const existing = this.activeAlerts.get(rule);
    if (existing) {
      // Update values while preserving acknowledged state and trigger time
      existing.severity = severity;
      existing.message = message;
      existing.currentValue = currentValue;
      existing.threshold = threshold;
    } else {
      const alert: IndexerAlert = {
        id: `alert-${rule}-${now}`,
        rule,
        severity,
        title,
        message,
        metric,
        currentValue,
        threshold,
        triggeredAt: now,
        acknowledged: false,
      };
      this.activeAlerts.set(rule, alert);
      this.addToHistory(alert);
    }
  }

  private resolve(rule: string): void {
    this.activeAlerts.delete(rule);
  }

  private addToHistory(alert: IndexerAlert): void {
    this.alertHistory.unshift({ ...alert });
    if (this.alertHistory.length > this.maxHistorySize) {
      this.alertHistory.pop();
    }
  }

  getActiveAlerts(): IndexerAlert[] {
    return Array.from(this.activeAlerts.values()).sort((a, b) => {
      const rank = { critical: 3, warning: 2, info: 1 };
      return rank[b.severity] - rank[a.severity] || b.triggeredAt - a.triggeredAt;
    });
  }

  getAlertHistory(): IndexerAlert[] {
    return [...this.alertHistory];
  }

  acknowledgeAlert(id: string): boolean {
    for (const alert of this.activeAlerts.values()) {
      if (alert.id === id) {
        alert.acknowledged = true;
        alert.acknowledgedAt = Date.now();
        return true;
      }
    }
    return false;
  }

  clearAcknowledged(): number {
    let cleared = 0;
    for (const [key, alert] of this.activeAlerts.entries()) {
      if (alert.acknowledged) {
        this.activeAlerts.delete(key);
        cleared += 1;
      }
    }
    return cleared;
  }
}
