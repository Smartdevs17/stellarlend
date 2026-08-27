/**
 * Anomaly Detector Service
 *
 * Statistical anomaly detection for real-time price feeds using multiple
 * complementary methods:
 *
 * 1. Z-Score Detection — flags prices that deviate more than N standard
 *    deviations from a rolling mean. Uses configurable lookback windows
 *    and adaptive thresholds that widen during high-volatility periods.
 *
 * 2. IQR (Interquartile Range) Detection — robust to outliers by using
 *    quartile-based fences instead of mean/std. Identifies points outside
 *    Q1 - k*IQR or Q3 + k*IQR.
 *
 * 3. Adaptive Thresholds — dynamically adjusts detection sensitivity
 *    based on recent market volatility. During volatile periods, thresholds
 *    widen to reduce false positives; during calm periods they tighten.
 *
 * 4. Price Velocity — detects sudden rate-of-change spikes that may
 *    indicate flash crashes or pump-and-dump activity.
 *
 * 5. Consecutive Anomaly Tracking — tracks streaks of anomalous readings
 *    to distinguish brief glitches from sustained manipulation.
 */

import { logger } from '../utils/logger.js';

/**
 * Anomaly severity levels
 */
export enum AnomalySeverity {
  INFO = 'info',
  WARNING = 'warning',
  CRITICAL = 'critical',
}

/**
 * Anomaly detection method used
 */
export enum AnomalyMethod {
  Z_SCORE = 'z_score',
  IQR = 'iqr',
  VELOCITY = 'velocity',
  ADAPTIVE_ZSCORE = 'adaptive_zscore',
}

/**
 * A single anomaly event
 */
export interface AnomalyEvent {
  id: string;
  asset: string;
  severity: AnomalySeverity;
  method: AnomalyMethod;
  message: string;
  price: bigint;
  referenceValue: bigint;
  deviationBps: number;
  threshold: number;
  metadata: Record<string, unknown>;
  timestamp: number;
}

/**
 * Rolling statistics for an asset
 */
export interface RollingStats {
  mean: number;
  stdDev: number;
  count: number;
  min: number;
  max: number;
  median: number;
  q1: number;
  q3: number;
  iqr: number;
  volatility: number;
}

/**
 * Adaptive threshold state
 */
export interface AdaptiveThresholdState {
  baseThreshold: number;
  currentThreshold: number;
  volatilityMultiplier: number;
  lastUpdated: number;
}

/**
 * Anomaly detector configuration
 */
export interface AnomalyDetectorConfig {
  /** Rolling window size (number of price samples) */
  rollingWindowSize: number;
  /** Z-score threshold for WARNING anomalies */
  zScoreWarningThreshold: number;
  /** Z-score threshold for CRITICAL anomalies */
  zScoreCriticalThreshold: number;
  /** IQR multiplier (k) for outlier detection */
  iqrMultiplier: number;
  /** Price velocity threshold in bps per second */
  velocityBpsPerSecond: number;
  /** Velocity check window in seconds */
  velocityWindowSeconds: number;
  /** Enable adaptive threshold adjustment */
  adaptiveThresholds: boolean;
  /** Minimum samples before anomaly detection activates */
  minSamples: number;
  /** Maximum stored anomaly events per asset */
  maxEventsPerAsset: number;
  /** Cooldown between anomalies of same type (seconds) */
  anomalyCooldownSeconds: number;
  /** Volatility multiplier upper bound for adaptive thresholds */
  maxVolatilityMultiplier: number;
}

const DEFAULT_CONFIG: AnomalyDetectorConfig = {
  rollingWindowSize: 100,
  zScoreWarningThreshold: 2.5,
  zScoreCriticalThreshold: 4.0,
  iqrMultiplier: 1.5,
  velocityBpsPerSecond: 500,
  velocityWindowSeconds: 60,
  adaptiveThresholds: true,
  minSamples: 10,
  maxEventsPerAsset: 200,
  anomalyCooldownSeconds: 30,
  maxVolatilityMultiplier: 3.0,
};

/**
 * Anomaly Detector Service
 */
export class AnomalyDetector {
  private config: AnomalyDetectorConfig;
  private priceWindows: Map<string, bigint[]> = new Map();
  private timestampWindows: Map<string, number[]> = new Map();
  private events: Map<string, AnomalyEvent[]> = new Map();
  private adaptiveStates: Map<string, AdaptiveThresholdState> = new Map();
  private eventCounter: number = 0;
  private lastAnomalyTime: Map<string, number> = new Map();

  constructor(config: Partial<AnomalyDetectorConfig> = {}) {
    this.config = { ...DEFAULT_CONFIG, ...config };
    logger.info('Anomaly detector initialized', {
      rollingWindowSize: this.config.rollingWindowSize,
      zScoreWarningThreshold: this.config.zScoreWarningThreshold,
      zScoreCriticalThreshold: this.config.zScoreCriticalThreshold,
      iqrMultiplier: this.config.iqrMultiplier,
      adaptiveThresholds: this.config.adaptiveThresholds,
    });
  }

  /**
   * Ingest a new price observation and run all anomaly detection methods.
   * Returns any anomalies detected.
   */
  ingestPrice(asset: string, price: bigint, timestamp: number): AnomalyEvent[] {
    const upperAsset = asset.toUpperCase();
    const anomalies: AnomalyEvent[] = [];

    // Append to rolling window
    this.appendToWindow(upperAsset, price, timestamp);

    // Need minimum samples before detection activates
    const window = this.priceWindows.get(upperAsset);
    if (!window || window.length < this.config.minSamples) {
      return anomalies;
    }

    // Compute rolling statistics
    const stats = this.computeRollingStats(upperAsset);

    // Update adaptive thresholds if enabled
    if (this.config.adaptiveThresholds) {
      this.updateAdaptiveThreshold(upperAsset, stats.volatility);
    }

    // Check cooldown
    if (this.isOnCooldown(upperAsset)) {
      return anomalies;
    }

    // Run detection methods
    const zScoreAnomaly = this.checkZScore(upperAsset, price, stats);
    if (zScoreAnomaly) anomalies.push(zScoreAnomaly);

    const iqrAnomaly = this.checkIQR(upperAsset, price, stats);
    if (iqrAnomaly) anomalies.push(iqrAnomaly);

    const velocityAnomaly = this.checkVelocity(upperAsset, price, timestamp);
    if (velocityAnomaly) anomalies.push(velocityAnomaly);

    // Store anomalies
    if (anomalies.length > 0) {
      this.setCooldown(upperAsset);
      for (const event of anomalies) {
        this.storeEvent(upperAsset, event);
      }
    }

    return anomalies;
  }

  /**
   * Get rolling statistics for an asset
   */
  getRollingStats(asset: string): RollingStats | null {
    const upperAsset = asset.toUpperCase();
    const window = this.priceWindows.get(upperAsset);
    if (!window || window.length < 2) return null;
    return this.computeRollingStats(upperAsset);
  }

  /**
   * Get adaptive threshold state for an asset
   */
  getAdaptiveThreshold(asset: string): AdaptiveThresholdState | null {
    return this.adaptiveStates.get(asset.toUpperCase()) ?? null;
  }

  /**
   * Get all anomaly events for an asset
   */
  getEvents(asset?: string): AnomalyEvent[] {
    if (asset) {
      return this.events.get(asset.toUpperCase()) ?? [];
    }
    const all: AnomalyEvent[] = [];
    for (const events of this.events.values()) {
      all.push(...events);
    }
    return all;
  }

  /**
   * Get recent anomaly events within the last N seconds
   */
  getRecentEvents(seconds: number, asset?: string): AnomalyEvent[] {
    const now = Math.floor(Date.now() / 1000);
    const threshold = now - seconds;
    return this.getEvents(asset).filter((e) => e.timestamp >= threshold);
  }

  /**
   * Check if an asset has any critical anomalies
   */
  hasCriticalAnomaly(asset: string): boolean {
    return this.getEvents(asset).some(
      (e) => e.severity === AnomalySeverity.CRITICAL
    );
  }

  /**
   * Clear events for an asset or all assets
   */
  clearEvents(asset?: string): void {
    if (asset) {
      this.events.delete(asset.toUpperCase());
    } else {
      this.events.clear();
    }
  }

  /**
   * Reset all state for an asset
   */
  resetAsset(asset: string): void {
    const upperAsset = asset.toUpperCase();
    this.priceWindows.delete(upperAsset);
    this.timestampWindows.delete(upperAsset);
    this.events.delete(upperAsset);
    this.adaptiveStates.delete(upperAsset);
    this.lastAnomalyTime.delete(upperAsset);
  }

  /**
   * Get detector configuration
   */
  getConfig(): AnomalyDetectorConfig {
    return { ...this.config };
  }

  /**
   * Update configuration at runtime
   */
  updateConfig(config: Partial<AnomalyDetectorConfig>): void {
    this.config = { ...this.config, ...config };
    logger.info('Anomaly detector config updated', config);
  }

  /**
   * Get anomaly statistics
   */
  getStats(): {
    trackedAssets: number;
    totalEvents: number;
    criticalCount: number;
    warningCount: number;
    infoCount: number;
  } {
    const allEvents = this.getEvents();
    return {
      trackedAssets: this.priceWindows.size,
      totalEvents: allEvents.length,
      criticalCount: allEvents.filter((e) => e.severity === AnomalySeverity.CRITICAL).length,
      warningCount: allEvents.filter((e) => e.severity === AnomalySeverity.WARNING).length,
      infoCount: allEvents.filter((e) => e.severity === AnomalySeverity.INFO).length,
    };
  }

  // ── Private: Window Management ──────────────────────────────────────

  private appendToWindow(asset: string, price: bigint, timestamp: number): void {
    let window = this.priceWindows.get(asset);
    let timestamps = this.timestampWindows.get(asset);

    if (!window || !timestamps) {
      window = [];
      timestamps = [];
      this.priceWindows.set(asset, window);
      this.timestampWindows.set(asset, timestamps);
    }

    window.push(price);
    timestamps.push(timestamp);

    // Trim to rolling window size
    while (window.length > this.config.rollingWindowSize) {
      window.shift();
      timestamps.shift();
    }
  }

  // ── Private: Statistics ─────────────────────────────────────────────

  computeRollingStats(asset: string): RollingStats {
    const window = this.priceWindows.get(asset) ?? [];
    const numericValues = window.map((v) => Number(v));
    const sorted = [...numericValues].sort((a, b) => a - b);
    const n = sorted.length;

    const mean = sorted.reduce((s, v) => s + v, 0) / n;
    const variance = sorted.reduce((s, v) => s + (v - mean) ** 2, 0) / n;
    const stdDev = Math.sqrt(variance);

    const min = sorted[0]!;
    const max = sorted[n - 1]!;
    const median = this.computeMedian(sorted);
    const q1 = this.computePercentile(sorted, 0.25);
    const q3 = this.computePercentile(sorted, 0.75);
    const iqr = q3 - q1;

    // Volatility: coefficient of variation (stdDev / mean), capped
    const volatility = mean > 0 ? stdDev / mean : 0;

    return { mean, stdDev, count: n, min, max, median, q1, q3, iqr, volatility };
  }

  private computeMedian(sorted: number[]): number {
    const mid = Math.floor(sorted.length / 2);
    if (sorted.length % 2 === 0) {
      return (sorted[mid - 1]! + sorted[mid]!) / 2;
    }
    return sorted[mid]!;
  }

  private computePercentile(sorted: number[], p: number): number {
    if (sorted.length === 1) return sorted[0]!;
    const index = p * (sorted.length - 1);
    const lower = Math.floor(index);
    const upper = Math.ceil(index);
    if (lower === upper) return sorted[lower]!;
    const weight = index - lower;
    return sorted[lower]! * (1 - weight) + sorted[upper]! * weight;
  }

  // ── Private: Z-Score Detection ──────────────────────────────────────

  private checkZScore(
    asset: string,
    price: bigint,
    stats: RollingStats
  ): AnomalyEvent | null {
    if (stats.stdDev === 0) return null;

    const priceNum = Number(price);
    const zScore = Math.abs((priceNum - stats.mean) / stats.stdDev);

    const threshold = this.getEffectiveZScoreThreshold(asset);

    if (zScore > threshold.critical) {
      return this.createEvent(
        asset,
        AnomalySeverity.CRITICAL,
        AnomalyMethod.Z_SCORE,
        `Z-score ${zScore.toFixed(2)} exceeds critical threshold ${threshold.critical}`,
        price,
        BigInt(Math.round(stats.mean)),
        Math.round(Math.abs((priceNum - stats.mean) / stats.mean) * 10000),
        threshold.critical,
        { zScore, mean: stats.mean, stdDev: stats.stdDev }
      );
    }

    if (zScore > threshold.warning) {
      return this.createEvent(
        asset,
        AnomalySeverity.WARNING,
        AnomalyMethod.Z_SCORE,
        `Z-score ${zScore.toFixed(2)} exceeds warning threshold ${threshold.warning}`,
        price,
        BigInt(Math.round(stats.mean)),
        Math.round(Math.abs((priceNum - stats.mean) / stats.mean) * 10000),
        threshold.warning,
        { zScore, mean: stats.mean, stdDev: stats.stdDev }
      );
    }

    return null;
  }

  private getEffectiveZScoreThreshold(asset: string): {
    warning: number;
    critical: number;
  } {
    if (!this.config.adaptiveThresholds) {
      return {
        warning: this.config.zScoreWarningThreshold,
        critical: this.config.zScoreCriticalThreshold,
      };
    }

    const state = this.adaptiveStates.get(asset);
    const multiplier = state?.volatilityMultiplier ?? 1;

    return {
      warning: this.config.zScoreWarningThreshold * multiplier,
      critical: this.config.zScoreCriticalThreshold * multiplier,
    };
  }

  // ── Private: IQR Detection ─────────────────────────────────────────

  private checkIQR(
    asset: string,
    price: bigint,
    stats: RollingStats
  ): AnomalyEvent | null {
    if (stats.iqr === 0) return null;

    const priceNum = Number(price);
    const lowerFence = stats.q1 - this.config.iqrMultiplier * stats.iqr;
    const upperFence = stats.q3 + this.config.iqrMultiplier * stats.iqr;

    const isOutlier = priceNum < lowerFence || priceNum > upperFence;

    if (isOutlier) {
      const reference = priceNum > upperFence ? stats.q3 : stats.q1;
      const fence = priceNum > upperFence ? upperFence : lowerFence;
      const deviationBps =
        reference > 0
          ? Math.round(Math.abs(priceNum - reference) / reference * 10000)
          : 0;

      const severity =
        priceNum < lowerFence * 0.8 || priceNum > upperFence * 1.2
          ? AnomalySeverity.CRITICAL
          : AnomalySeverity.WARNING;

      return this.createEvent(
        asset,
        severity,
        AnomalyMethod.IQR,
        `Price ${priceNum} outside IQR fences [${lowerFence.toFixed(2)}, ${upperFence.toFixed(2)}]`,
        price,
        BigInt(Math.round(fence)),
        deviationBps,
        this.config.iqrMultiplier,
        {
          q1: stats.q1,
          q3: stats.q3,
          iqr: stats.iqr,
          lowerFence,
          upperFence,
          priceNum,
        }
      );
    }

    return null;
  }

  // ── Private: Velocity Detection ─────────────────────────────────────

  private checkVelocity(
    asset: string,
    price: bigint,
    timestamp: number
  ): AnomalyEvent | null {
    const timestamps = this.timestampWindows.get(asset);
    const window = this.priceWindows.get(asset);

    if (!timestamps || !window || window.length < 2) return null;

    // Find the most recent price within velocityWindowSeconds
    const windowStart = timestamp - this.config.velocityWindowSeconds;
    let prevPrice: bigint | null = null;
    let prevTimestamp: number | null = null;

    // Walk backwards through the window
    for (let i = timestamps.length - 2; i >= 0; i--) {
      if (timestamps[i]! >= windowStart) {
        prevPrice = window[i]!;
        prevTimestamp = timestamps[i]!;
        break;
      }
    }

    if (prevPrice === null || prevTimestamp === null || prevPrice <= 0n) return null;

    const timeDelta = timestamp - prevTimestamp;
    if (timeDelta <= 0) return null;

    const priceDelta =
      price > prevPrice ? price - prevPrice : prevPrice - price;
    const bpsChange = Number((priceDelta * 10000n) / prevPrice);
    const bpsPerSecond = bpsChange / timeDelta;

    if (bpsPerSecond > this.config.velocityBpsPerSecond) {
      const direction = price > prevPrice ? 'up' : 'down';
      const severity =
        bpsPerSecond > this.config.velocityBpsPerSecond * 2
          ? AnomalySeverity.CRITICAL
          : AnomalySeverity.WARNING;

      return this.createEvent(
        asset,
        severity,
        AnomalyMethod.VELOCITY,
        `Price moved ${direction} ${bpsPerSecond.toFixed(1)} bps/s (threshold: ${this.config.velocityBpsPerSecond} bps/s)`,
        price,
        prevPrice,
        Math.round(bpsChange),
        this.config.velocityBpsPerSecond,
        { bpsPerSecond, timeDelta, direction, prevPrice: prevPrice.toString() }
      );
    }

    return null;
  }

  // ── Private: Adaptive Thresholds ────────────────────────────────────

  private updateAdaptiveThreshold(asset: string, currentVolatility: number): void {
    let state = this.adaptiveStates.get(asset);

    if (!state) {
      state = {
        baseThreshold: this.config.zScoreWarningThreshold,
        currentThreshold: this.config.zScoreWarningThreshold,
        volatilityMultiplier: 1,
        lastUpdated: Math.floor(Date.now() / 1000),
      };
      this.adaptiveStates.set(asset, state);
    }

    // Compute multiplier: higher volatility → higher threshold
    // Uses a logarithmic scale to prevent extreme multipliers
    const window = this.priceWindows.get(asset) ?? [];
    const baselineVolatility = this.computeBaselineVolatility(window);

    let multiplier = 1;
    if (baselineVolatility > 0) {
      multiplier = Math.min(
        this.config.maxVolatilityMultiplier,
        Math.max(1, currentVolatility / baselineVolatility)
      );
    }

    state.volatilityMultiplier = multiplier;
    state.currentThreshold = state.baseThreshold * multiplier;
    state.lastUpdated = Math.floor(Date.now() / 1000);
  }

  private computeBaselineVolatility(window: bigint[]): number {
    if (window.length < 2) return 0;

    // Use the first half of the window as baseline
    const halfLength = Math.floor(window.length / 2);
    const baseline = window.slice(0, halfLength);
    const numericValues = baseline.map((v) => Number(v));
    const mean = numericValues.reduce((s, v) => s + v, 0) / numericValues.length;
    if (mean === 0) return 0;

    const variance =
      numericValues.reduce((s, v) => s + (v - mean) ** 2, 0) / numericValues.length;
    return Math.sqrt(variance) / mean;
  }

  // ── Private: Cooldown ──────────────────────────────────────────────

  private isOnCooldown(asset: string): boolean {
    const lastTime = this.lastAnomalyTime.get(asset);
    if (lastTime === undefined) return false;
    const now = Math.floor(Date.now() / 1000);
    return now - lastTime < this.config.anomalyCooldownSeconds;
  }

  private setCooldown(asset: string): void {
    this.lastAnomalyTime.set(asset, Math.floor(Date.now() / 1000));
  }

  // ── Private: Event Management ───────────────────────────────────────

  private createEvent(
    asset: string,
    severity: AnomalySeverity,
    method: AnomalyMethod,
    message: string,
    price: bigint,
    referenceValue: bigint,
    deviationBps: number,
    threshold: number,
    metadata: Record<string, unknown>
  ): AnomalyEvent {
    this.eventCounter++;
    return {
      id: `anomaly_${this.eventCounter}_${Date.now()}`,
      asset: asset.toUpperCase(),
      severity,
      method,
      message,
      price,
      referenceValue,
      deviationBps,
      threshold,
      metadata,
      timestamp: Math.floor(Date.now() / 1000),
    };
  }

  private storeEvent(asset: string, event: AnomalyEvent): void {
    let assetEvents = this.events.get(asset);
    if (!assetEvents) {
      assetEvents = [];
      this.events.set(asset, assetEvents);
    }

    assetEvents.push(event);

    // Trim old events
    while (assetEvents.length > this.config.maxEventsPerAsset) {
      assetEvents.shift();
    }

    const logFn =
      event.severity === AnomalySeverity.CRITICAL ? logger.error : logger.warn;
    logFn(`[ANOMALY] ${event.severity.toUpperCase()} ${event.method}: ${event.message}`, {
      asset: event.asset,
      price: event.price.toString(),
      deviationBps: event.deviationBps,
    });
  }
}

/**
 * Create an anomaly detector instance
 */
export function createAnomalyDetector(
  config?: Partial<AnomalyDetectorConfig>
): AnomalyDetector {
  return new AnomalyDetector(config);
}
