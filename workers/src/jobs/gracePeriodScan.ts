/**
 * Issue #1758 — scheduled grace-period scanner.
 *
 * The job owns no notification logic: it reads positions from an injected
 * source and delegates decisions to `GracePeriodAlertService` (or any service
 * with the same shape). This keeps the worker runnable in tests and in the
 * API process without new runtime dependencies.
 */

import type {
  GraceDecision,
  Logger,
  Metrics,
  PositionSnapshot,
} from '../../backend/src/services/gracePeriodAlerts';

export interface GracePeriodServiceLike {
  process(position: PositionSnapshot): Promise<GraceDecision>;
}

/** Source of open, non-liquidated positions. */
export interface PositionSource {
  listOpenPositions(): Promise<readonly PositionSnapshot[]>;
}

export interface ScanOptions {
  logger?: Logger;
  metrics?: Metrics;
  now?: () => number;
}

export interface ScanError {
  address: string;
  asset: string;
  message: string;
}

export interface ScanReport {
  scanned: number;
  eligible: number;
  skipped: number;
  alertsSent: number;
  delivered: number;
  failed: number;
  errors: ScanError[];
  startedAt: number;
  finishedAt: number;
}

export interface SchedulerOptions extends ScanOptions {
  /** Default 60s. Timers are unref'd so they never hold a process open. */
  intervalMs?: number;
  onReport?: (report: ScanReport) => void;
}

export interface GracePeriodScheduler {
  runOnce(): Promise<ScanReport>;
  stop(): void;
}

const noOpLogger: Logger = {
  info: () => undefined,
  warn: () => undefined,
  error: () => undefined,
};

const noOpMetrics: Metrics = {
  increment: () => undefined,
  observe: () => undefined,
};

/** Reject malformed snapshots before they reach the risk service. */
export function isValidPosition(position: PositionSnapshot): boolean {
  return (
    typeof position.address === 'string' &&
    position.address.length > 0 &&
    typeof position.asset === 'string' &&
    position.asset.length > 0 &&
    Number.isFinite(position.healthFactor) &&
    position.healthFactor >= 0 &&
    Number.isFinite(position.collateralValue) &&
    position.collateralValue >= 0 &&
    Number.isFinite(position.debtValue) &&
    position.debtValue >= 0
  );
}

/** Run one scan. Per-position errors never abort the rest of the batch. */
export async function runGracePeriodScan(
  source: PositionSource,
  service: GracePeriodServiceLike,
  options: ScanOptions = {},
): Promise<ScanReport> {
  const logger = options.logger ?? noOpLogger;
  const metrics = options.metrics ?? noOpMetrics;
  const now = options.now ?? Date.now;
  const startedAt = now();
  const report: ScanReport = {
    scanned: 0,
    eligible: 0,
    skipped: 0,
    alertsSent: 0,
    delivered: 0,
    failed: 0,
    errors: [],
    startedAt,
    finishedAt: startedAt,
  };

  let positions: readonly PositionSnapshot[];
  try {
    positions = await source.listOpenPositions();
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    logger.error('Grace-period scan could not load positions', { error: message });
    metrics.increment('grace_period.scan.source_error');
    report.finishedAt = now();
    metrics.observe('grace_period.scan.duration_ms', report.finishedAt - startedAt);
    return report;
  }

  report.scanned = positions.length;
  for (const position of positions) {
    if (!isValidPosition(position)) {
      report.skipped += 1;
      logger.warn('Skipping invalid position snapshot', {
        address: position?.address ?? 'unknown',
        asset: position?.asset ?? 'unknown',
      });
      continue;
    }
    report.eligible += 1;
    try {
      const decision = await service.process(position);
      if (decision.reason === 'alert_sent') {
        report.alertsSent += 1;
        if (decision.delivered) {
          report.delivered += 1;
        }
      } else if (decision.reason === 'delivery_failed') {
        report.failed += 1;
      }
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      report.failed += 1;
      report.errors.push({
        address: position.address,
        asset: position.asset,
        message,
      });
      logger.error('Grace-period decision failed', {
        address: position.address,
        asset: position.asset,
        error: message,
      });
    }
  }

  report.finishedAt = now();
  metrics.increment('grace_period.scan.completed', {
    sent: String(report.alertsSent),
    failed: String(report.failed),
  });
  metrics.observe('grace_period.scan.duration_ms', report.finishedAt - startedAt);
  logger.info('Grace-period scan complete', {
    scanned: report.scanned,
    eligible: report.eligible,
    skipped: report.skipped,
    alertsSent: report.alertsSent,
    failed: report.failed,
  });
  return report;
}

/** Start an idempotent, self-unref'ing scan loop. */
export function startGracePeriodScan(
  source: PositionSource,
  service: GracePeriodServiceLike,
  options: SchedulerOptions = {},
): GracePeriodScheduler {
  const intervalMs = options.intervalMs ?? 60_000;
  if (intervalMs < 1_000) {
    throw new RangeError('intervalMs must be at least 1000');
  }

  let inFlight: Promise<ScanReport> | null = null;
  const runOnce = (): Promise<ScanReport> => {
    if (inFlight !== null) {
      return inFlight;
    }
    const pending = runGracePeriodScan(source, service, options)
      .then((report) => {
        options.onReport?.(report);
        return report;
      })
      .finally(() => {
        inFlight = null;
      });
    inFlight = pending;
    return pending;
  };

  void runOnce();
  const timer = setInterval(() => void runOnce(), intervalMs);
  const maybeTimer = timer as { unref?: () => void };
  if (typeof maybeTimer.unref === 'function') {
    maybeTimer.unref();
  }

  return {
    runOnce,
    stop: () => clearInterval(timer),
  };
}
