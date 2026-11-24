/**
 * Issue #1758 — grace-period notifications before health-factor liquidation.
 *
 * Dependency-free on purpose: the API service and the worker process can both
 * host it, and all side effects (delivery, logging, metrics, clock) are
 * injected. Thresholds and cooldowns follow the risk model used by the pool:
 * health factor 1.0 is liquidation, so warnings are sent at 1.5 / 1.2 / 1.0.
 */

export type HealthStage = 'safe' | 'warning' | 'grace' | 'critical';
export type AlertSeverity = 'info' | 'warning' | 'critical';
export type NotificationChannel = 'push' | 'email' | 'webhook';

/** Minimal position view required by the alert service. */
export interface PositionSnapshot {
  address: string;
  asset: string;
  healthFactor: number;
  collateralValue: number;
  debtValue: number;
  liquidationPrice?: number;
  lastUpdated?: number;
}

export interface GracePeriodThresholds {
  /** First warning, health factor 1.5 by default. */
  warningHf: number;
  /** Grace period starts, health factor 1.2 by default. */
  graceHf: number;
  /** Liquidation boundary, health factor 1.0 by default. */
  liquidationHf: number;
}

export const DEFAULT_GRACE_PERIOD_THRESHOLDS: Readonly<GracePeriodThresholds> = Object.freeze({
  warningHf: 1.5,
  graceHf: 1.2,
  liquidationHf: 1.0,
});

/** Structured log hook; compatible with the API's Winston logger. */
export interface Logger {
  info(message: string, context?: Readonly<Record<string, unknown>>): void;
  warn(message: string, context?: Readonly<Record<string, unknown>>): void;
  error(message: string, context?: Readonly<Record<string, unknown>>): void;
}

/** Counter/gauge hook; compatible with a Prometheus client. */
export interface Metrics {
  increment(name: string, labels?: Readonly<Record<string, string>>): void;
  observe(name: string, value: number, labels?: Readonly<Record<string, string>>): void;
}

/** Delivery hook; implementations may fan out to push/email/webhook. */
export interface Notifier {
  send(alert: GracePeriodAlert): Promise<DeliveryResult>;
}

export interface DeliveryResult {
  delivered: boolean;
  channel?: NotificationChannel;
  error?: string;
}

export interface GracePeriodAlert {
  id: string;
  type: 'grace_period_warning';
  stage: Exclude<HealthStage, 'safe'>;
  severity: AlertSeverity;
  address: string;
  asset: string;
  title: string;
  message: string;
  healthFactor: number;
  thresholds: GracePeriodThresholds;
  createdAt: number;
  metadata: Record<string, string | number | boolean>;
}

export interface GracePeriodConfig {
  thresholds?: Partial<GracePeriodThresholds>;
  /** Minimum delay between repeat alerts at the same stage (default 15m). */
  cooldownMs?: number;
  /** Maximum alerts per position while the stage does not escalate. */
  maxAlertsPerPosition?: number;
  now?: () => number;
  logger?: Logger;
  metrics?: Metrics;
}

export interface GraceAlertState {
  stage: Exclude<HealthStage, 'safe'>;
  stageRank: number;
  lastNotifiedAt: number;
  count: number;
}

export type DecisionReason =
  | 'safe'
  | 'alert_sent'
  | 'cooldown'
  | 'max_reached'
  | 'delivery_failed';

export interface GraceDecision {
  address: string;
  asset: string;
  stage: HealthStage;
  reason: DecisionReason;
  delivered: boolean;
  alert?: GracePeriodAlert;
}

export interface BatchDecision {
  decisions: GraceDecision[];
  alertsSent: number;
  delivered: number;
  failed: number;
}

const STAGE_RANK: Readonly<Record<HealthStage, number>> = Object.freeze({
  safe: 0,
  warning: 1,
  grace: 2,
  critical: 3,
});

export const noOpLogger: Logger = {
  info: () => undefined,
  warn: () => undefined,
  error: () => undefined,
};

export const noOpMetrics: Metrics = {
  increment: () => undefined,
  observe: () => undefined,
};

/** Ensure thresholds are strictly ordered and positive. */
export function validateThresholds(
  thresholds: GracePeriodThresholds,
): GracePeriodThresholds {
  const { warningHf, graceHf, liquidationHf } = thresholds;
  if (
    !Number.isFinite(warningHf) ||
    !Number.isFinite(graceHf) ||
    !Number.isFinite(liquidationHf) ||
    liquidationHf <= 0 ||
    graceHf <= liquidationHf ||
    warningHf <= graceHf
  ) {
    throw new RangeError('thresholds must satisfy warningHf > graceHf > liquidationHf > 0');
  }
  return { warningHf, graceHf, liquidationHf };
}

/** Classify a position into the grace-period funnel. */
export function classifyHealth(
  position: Pick<PositionSnapshot, 'healthFactor'>,
  thresholds: GracePeriodThresholds = DEFAULT_GRACE_PERIOD_THRESHOLDS,
): HealthStage {
  validateThresholds(thresholds);
  const hf = position.healthFactor;
  if (!Number.isFinite(hf) || hf >= thresholds.warningHf) {
    return 'safe';
  }
  if (hf >= thresholds.graceHf) {
    return 'warning';
  }
  if (hf >= thresholds.liquidationHf) {
    return 'grace';
  }
  return 'critical';
}

function severityFor(stage: Exclude<HealthStage, 'safe'>): AlertSeverity {
  return stage === 'critical' ? 'critical' : 'warning';
}

function titleFor(stage: Exclude<HealthStage, 'safe'>): string {
  switch (stage) {
    case 'warning':
      return 'Health factor is approaching the warning threshold';
    case 'grace':
      return 'Grace period: add collateral or repay debt';
    case 'critical':
      return 'Liquidation imminent';
  }
}

/** Build a deterministic, testable alert payload. */
export function buildGracePeriodAlert(
  position: PositionSnapshot,
  stage: Exclude<HealthStage, 'safe'>,
  thresholds: GracePeriodThresholds,
  now: () => number = Date.now,
): GracePeriodAlert {
  const createdAt = now();
  const dropToLiquidationBps =
    position.healthFactor > 0
      ? Math.max(
          0,
          Math.round(
            (1 - thresholds.liquidationHf / position.healthFactor) * 10_000,
          ),
        )
      : 10_000;

  return {
    id: `${position.address}:${position.asset}:${stage}:${createdAt}`,
    type: 'grace_period_warning',
    stage,
    severity: severityFor(stage),
    address: position.address,
    asset: position.asset,
    title: titleFor(stage),
    message:
      stage === 'critical'
        ? `Health factor ${position.healthFactor.toFixed(3)} is below liquidation.`
        : `Health factor ${position.healthFactor.toFixed(3)}; add collateral or repay debt before liquidation.`,
    healthFactor: position.healthFactor,
    thresholds: { ...thresholds },
    createdAt,
    metadata: {
      collateralValue: position.collateralValue,
      debtValue: position.debtValue,
      liquidationPrice: position.liquidationPrice ?? 0,
      requiredPriceDropBps: dropToLiquidationBps,
      stageRank: STAGE_RANK[stage],
    },
  };
}

/**
 * Deduplicating alert service.
 *
 * Escalation bypasses the cooldown; repeats at the same stage are suppressed
 * until `cooldownMs` elapses, and a position never receives more than
 * `maxAlertsPerPosition` alerts at one stage. Returning to `safe` clears the
 * state so a new risk episode starts fresh.
 */
export class GracePeriodAlertService {
  private readonly notifier: Notifier;
  private readonly thresholds: GracePeriodThresholds;
  private readonly cooldownMs: number;
  private readonly maxAlertsPerPosition: number;
  private readonly now: () => number;
  private readonly logger: Logger;
  private readonly metrics: Metrics;
  private readonly states = new Map<string, GraceAlertState>();

  constructor(notifier: Notifier, config: GracePeriodConfig = {}) {
    this.notifier = notifier;
    this.thresholds = validateThresholds({
      ...DEFAULT_GRACE_PERIOD_THRESHOLDS,
      ...config.thresholds,
    });
    this.cooldownMs = config.cooldownMs ?? 15 * 60 * 1000;
    this.maxAlertsPerPosition = config.maxAlertsPerPosition ?? 3;
    this.now = config.now ?? Date.now;
    this.logger = config.logger ?? noOpLogger;
    this.metrics = config.metrics ?? noOpMetrics;

    if (this.cooldownMs < 0 || this.maxAlertsPerPosition < 1) {
      throw new RangeError('cooldownMs must be >= 0 and maxAlertsPerPosition >= 1');
    }
  }

  getThresholds(): Readonly<GracePeriodThresholds> {
    return { ...this.thresholds };
  }

  private key(position: Pick<PositionSnapshot, 'address' | 'asset'>): string {
    return `${position.address}:${position.asset}`;
  }

  getState(address: string, asset: string): Readonly<GraceAlertState> | undefined {
    return this.states.get(`${address}:${asset}`);
  }

  reset(address?: string, asset?: string): void {
    if (address === undefined) {
      this.states.clear();
      return;
    }
    if (asset === undefined) {
      for (const key of this.states.keys()) {
        if (key.startsWith(`${address}:`)) {
          this.states.delete(key);
        }
      }
      return;
    }
    this.states.delete(`${address}:${asset}`);
  }

  async process(position: PositionSnapshot): Promise<GraceDecision> {
    const stage = classifyHealth(position, this.thresholds);
    const labels = { address: position.address, asset: position.asset, stage };
    const key = this.key(position);

    if (stage === 'safe') {
      this.states.delete(key);
      this.metrics.increment('grace_period.position_safe', labels);
      return {
        address: position.address,
        asset: position.asset,
        stage,
        reason: 'safe',
        delivered: false,
      };
    }

    const rank = STAGE_RANK[stage];
    const state = this.states.get(key);
    if (state && state.stageRank >= rank && state.count >= this.maxAlertsPerPosition) {
      this.metrics.increment('grace_period.suppressed', labels);
      return {
        address: position.address,
        asset: position.asset,
        stage,
        reason: 'max_reached',
        delivered: false,
      };
    }
    if (
      state &&
      state.stageRank >= rank &&
      this.now() - state.lastNotifiedAt < this.cooldownMs
    ) {
      this.metrics.increment('grace_period.suppressed', labels);
      return {
        address: position.address,
        asset: position.asset,
        stage,
        reason: 'cooldown',
        delivered: false,
      };
    }

    const alert = buildGracePeriodAlert(position, stage, this.thresholds, this.now);
    try {
      const result = await this.notifier.send(alert);
      if (!result.delivered) {
        this.logger.warn('Grace-period alert delivery failed', {
          error: result.error ?? 'unknown error',
          address: position.address,
          asset: position.asset,
          stage,
        });
        this.metrics.increment('grace_period.delivery_failed', labels);
        return {
          address: position.address,
          asset: position.asset,
          stage,
          reason: 'delivery_failed',
          delivered: false,
          alert,
        };
      }

      const sameStage = state !== undefined && state.stageRank === rank;
      this.states.set(key, {
        stage,
        stageRank: rank,
        lastNotifiedAt: alert.createdAt,
        count: sameStage && state !== undefined ? state.count + 1 : 1,
      });
      this.logger.info('Grace-period alert delivered', {
        address: position.address,
        asset: position.asset,
        stage,
        alertId: alert.id,
        channel: result.channel ?? 'default',
      });
      this.metrics.increment('grace_period.alert_sent', labels);
      this.metrics.observe('grace_period.health_factor', position.healthFactor, labels);
      return {
        address: position.address,
        asset: position.asset,
        stage,
        reason: 'alert_sent',
        delivered: true,
        alert,
      };
    } catch (error) {
      this.logger.error('Grace-period alert threw during delivery', {
        error: error instanceof Error ? error.message : String(error),
        address: position.address,
        asset: position.asset,
        stage,
      });
      this.metrics.increment('grace_period.delivery_failed', labels);
      return {
        address: position.address,
        asset: position.asset,
        stage,
        reason: 'delivery_failed',
        delivered: false,
        alert,
      };
    }
  }

  async processBatch(positions: readonly PositionSnapshot[]): Promise<BatchDecision> {
    const decisions: GraceDecision[] = [];
    let alertsSent = 0;
    let delivered = 0;
    let failed = 0;
    for (const position of positions) {
      const decision = await this.process(position);
      decisions.push(decision);
      if (decision.reason === 'alert_sent') {
        alertsSent += 1;
        if (decision.delivered) {
          delivered += 1;
        }
      } else if (decision.reason === 'delivery_failed') {
        failed += 1;
      }
    }
    return { decisions, alertsSent, delivered, failed };
  }
}
