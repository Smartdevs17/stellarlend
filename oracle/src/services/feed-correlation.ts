/**
 * Feed Correlation Service
 *
 * Analyzes cross-feed price correlations to detect:
 *
 * 1. Coordinated Manipulation — when multiple unrelated assets move in
 *    lockstep, it may indicate a coordinated attack on the oracle.
 *
 * 2. Correlation Breakdown — when historically correlated assets (e.g.,
 *    BTC and ETH) suddenly diverge, it may indicate manipulation of one.
 *
 * 3. Cascading Effects — monitors how price changes in one asset propagate
 *    to related assets, detecting abnormal propagation patterns.
 *
 * Uses Pearson correlation coefficient on rolling windows of price returns
 * (percentage changes) rather than raw prices.
 */

import { logger } from '../utils/logger.js';

/**
 * Correlation event type
 */
export enum CorrelationEventType {
  COORDINATED_MOVE = 'coordinated_move',
  CORRELATION_BREAKDOWN = 'correlation_breakdown',
  ANOMALOUS_DIVERGENCE = 'anomalous_divergence',
}

/**
 * Correlation event severity
 */
export enum CorrelationSeverity {
  INFO = 'info',
  WARNING = 'warning',
  CRITICAL = 'critical',
}

/**
 * A correlation analysis event
 */
export interface CorrelationEvent {
  id: string;
  type: CorrelationEventType;
  severity: CorrelationSeverity;
  message: string;
  assets: string[];
  correlation: number;
  threshold: number;
  timestamp: number;
  metadata: Record<string, unknown>;
}

/**
 * Pairwise correlation result
 */
export interface CorrelationPair {
  asset1: string;
  asset2: string;
  correlation: number;
  sampleSize: number;
  pValue: number;
  isSignificant: boolean;
}

/**
 * Correlation matrix for all tracked assets
 */
export interface CorrelationMatrix {
  assets: string[];
  matrix: number[][];
  timestamp: number;
}

/**
 * Feed correlation configuration
 */
export interface FeedCorrelationConfig {
  /** Rolling window size for return calculations */
  rollingWindowSize: number;
  /** Minimum number of samples for correlation calculation */
  minSamples: number;
  /** Correlation threshold for coordinated move detection */
  coordinatedMoveThreshold: number;
  /** Correlation breakdown threshold (drop from historical) */
  correlationBreakdownThreshold: number;
  /** Minimum historical correlation for breakdown detection */
  minHistoricalCorrelation: number;
  /** Maximum events stored */
  maxEvents: number;
  /** Correlated asset groups (e.g., [[BTC, ETH], [USDC, USDT]]) */
  correlatedGroups: string[][];
  /** Cooldown between correlation events (seconds) */
  eventCooldownSeconds: number;
}

const DEFAULT_CONFIG: FeedCorrelationConfig = {
  rollingWindowSize: 50,
  minSamples: 15,
  coordinatedMoveThreshold: 0.85,
  correlationBreakdownThreshold: 0.4,
  minHistoricalCorrelation: 0.7,
  maxEvents: 200,
  correlatedGroups: [
    ['BTC', 'ETH'],
    ['USDC', 'USDT'],
  ],
  eventCooldownSeconds: 60,
};

/**
 * Feed Correlation Service
 */
export class FeedCorrelation {
  private config: FeedCorrelationConfig;
  private priceReturns: Map<string, number[]> = new Map();
  private events: CorrelationEvent[] = [];
  private eventCounter: number = 0;
  private lastEventTime: Map<string, number> = new Map();
  private previousCorrelations: Map<string, number> = new Map();

  constructor(config: Partial<FeedCorrelationConfig> = {}) {
    this.config = { ...DEFAULT_CONFIG, ...config };
    logger.info('Feed correlation initialized', {
      rollingWindowSize: this.config.rollingWindowSize,
      coordinatedMoveThreshold: this.config.coordinatedMoveThreshold,
      correlatedGroups: this.config.correlatedGroups.length,
    });
  }

  /**
   * Record a price observation and compute correlations.
   * Returns any correlation anomalies detected.
   */
  recordPrice(
    asset: string,
    price: bigint,
    previousPrice: bigint | null,
    timestamp: number
  ): CorrelationEvent[] {
    const upperAsset = asset.toUpperCase();
    const anomalies: CorrelationEvent[] = [];

    // Compute return if we have a previous price
    if (previousPrice !== null && previousPrice > 0n) {
      const priceNum = Number(price);
      const prevNum = Number(previousPrice);
      const returnPct = ((priceNum - prevNum) / prevNum) * 100;
      this.appendToReturns(upperAsset, returnPct);
    }

    // Check correlation with all correlated groups
    for (const group of this.config.correlatedGroups) {
      if (!group.includes(upperAsset)) continue;

      const groupAssets = group.filter((a) => a !== upperAsset);
      for (const otherAsset of groupAssets) {
        const anomaly = this.checkPairCorrelation(upperAsset, otherAsset, timestamp);
        if (anomaly) anomalies.push(anomaly);
      }
    }

    // Check for coordinated moves across all tracked assets
    const coordinatedAnomalies = this.checkCoordinatedMoves(timestamp);
    anomalies.push(...coordinatedAnomalies);

    // Store anomalies
    for (const event of anomalies) {
      this.storeEvent(event);
    }

    return anomalies;
  }

  /**
   * Compute Pearson correlation between two assets
   */
  computeCorrelation(asset1: string, asset2: string): CorrelationPair | null {
    const returns1 = this.priceReturns.get(asset1.toUpperCase());
    const returns2 = this.priceReturns.get(asset2.toUpperCase());

    if (!returns1 || !returns2) return null;

    // Align to common length (most recent samples)
    const len = Math.min(returns1.length, returns2.length, this.config.rollingWindowSize);
    if (len < this.config.minSamples) return null;

    const r1 = returns1.slice(-len);
    const r2 = returns2.slice(-len);

    const correlation = this.pearsonCorrelation(r1, r2);
    const pValue = this.computePValue(correlation, len);

    return {
      asset1: asset1.toUpperCase(),
      asset2: asset2.toUpperCase(),
      correlation,
      sampleSize: len,
      pValue,
      isSignificant: pValue < 0.05,
    };
  }

  /**
   * Get the full correlation matrix for all tracked assets
   */
  getCorrelationMatrix(): CorrelationMatrix {
    const assets = Array.from(this.priceReturns.keys()).sort();
    const n = assets.length;
    const matrix: number[][] = Array.from({ length: n }, () => Array(n).fill(0));

    for (let i = 0; i < n; i++) {
      matrix[i]![i] = 1; // Self-correlation
      for (let j = i + 1; j < n; j++) {
        const pair = this.computeCorrelation(assets[i]!, assets[j]!);
        const corr = pair?.correlation ?? 0;
        matrix[i]![j] = corr;
        matrix[j]![i] = corr;
      }
    }

    return { assets, matrix, timestamp: Math.floor(Date.now() / 1000) };
  }

  /**
   * Get returns history for an asset
   */
  getReturns(asset: string): number[] {
    return this.priceReturns.get(asset.toUpperCase()) ?? [];
  }

  /**
   * Get correlation events
   */
  getEvents(asset?: string): CorrelationEvent[] {
    if (asset) {
      const upper = asset.toUpperCase();
      return this.events.filter((e) => e.assets.includes(upper));
    }
    return [...this.events];
  }

  /**
   * Get recent correlation events
   */
  getRecentEvents(seconds: number, asset?: string): CorrelationEvent[] {
    const now = Math.floor(Date.now() / 1000);
    const threshold = now - seconds;
    return this.getEvents(asset).filter((e) => e.timestamp >= threshold);
  }

  /**
   * Check if two assets are expected to be correlated
   */
  areExpectedCorrelates(asset1: string, asset2: string): boolean {
    const a1 = asset1.toUpperCase();
    const a2 = asset2.toUpperCase();
    return this.config.correlatedGroups.some(
      (group) => group.includes(a1) && group.includes(a2)
    );
  }

  /**
   * Clear returns for an asset or all assets
   */
  clearReturns(asset?: string): void {
    if (asset) {
      this.priceReturns.delete(asset.toUpperCase());
    } else {
      this.priceReturns.clear();
    }
  }

  /**
   * Get configuration
   */
  getConfig(): FeedCorrelationConfig {
    return { ...this.config };
  }

  /**
   * Update configuration at runtime
   */
  updateConfig(config: Partial<FeedCorrelationConfig>): void {
    this.config = { ...this.config, ...config };
    logger.info('Feed correlation config updated', config);
  }

  /**
   * Get statistics
   */
  getStats(): {
    trackedAssets: number;
    totalEvents: number;
    correlatedGroups: number;
  } {
    return {
      trackedAssets: this.priceReturns.size,
      totalEvents: this.events.length,
      correlatedGroups: this.config.correlatedGroups.length,
    };
  }

  // ── Private: Return Management ──────────────────────────────────────

  private appendToReturns(asset: string, returnPct: number): void {
    let returns = this.priceReturns.get(asset);
    if (!returns) {
      returns = [];
      this.priceReturns.set(asset, returns);
    }

    returns.push(returnPct);

    while (returns.length > this.config.rollingWindowSize) {
      returns.shift();
    }
  }

  // ── Private: Correlation Checking ───────────────────────────────────

  private checkPairCorrelation(
    asset1: string,
    asset2: string,
    timestamp: number
  ): CorrelationEvent | null {
    const pair = this.computeCorrelation(asset1, asset2);
    if (!pair || !pair.isSignificant) return null;

    const pairKey = [asset1, asset2].sort().join(':');

    // Check for coordinated move
    if (pair.correlation > this.config.coordinatedMoveThreshold) {
      // Check if this is a NEW high correlation (not just maintaining existing)
      const prevCorr = this.previousCorrelations.get(pairKey) ?? 0;
      this.previousCorrelations.set(pairKey, pair.correlation);

      // Only alert if correlation increased significantly
      if (prevCorr < this.config.coordinatedMoveThreshold) {
        return this.createEvent(
          CorrelationEventType.COORDINATED_MOVE,
          CorrelationSeverity.WARNING,
          `${asset1} and ${asset2} show high correlation (${pair.correlation.toFixed(3)})`,
          [asset1, asset2],
          pair.correlation,
          this.config.coordinatedMoveThreshold,
          timestamp,
          { sampleSize: pair.sampleSize, pValue: pair.pValue, prevCorrelation: prevCorr }
        );
      }
    }

    // Check for correlation breakdown
    const prevCorr = this.previousCorrelations.get(pairKey);
    this.previousCorrelations.set(pairKey, pair.correlation);

    if (
      prevCorr !== undefined &&
      prevCorr > this.config.minHistoricalCorrelation &&
      pair.correlation < prevCorr - this.config.correlationBreakdownThreshold
    ) {
      const severity =
        pair.correlation < 0
          ? CorrelationSeverity.CRITICAL
          : CorrelationSeverity.WARNING;

      return this.createEvent(
        CorrelationEventType.CORRELATION_BREAKDOWN,
        severity,
        `Correlation between ${asset1} and ${asset2} dropped from ${prevCorr.toFixed(3)} to ${pair.correlation.toFixed(3)}`,
        [asset1, asset2],
        pair.correlation,
        prevCorr,
        timestamp,
        { sampleSize: pair.sampleSize, pValue: pair.pValue, previousCorrelation: prevCorr }
      );
    }

    return null;
  }

  private checkCoordinatedMoves(timestamp: number): CorrelationEvent[] {
    const anomalies: CorrelationEvent[] = [];
    const assets = Array.from(this.priceReturns.keys());

    if (assets.length < 3) return anomalies;

    // Check if many assets are moving in the same direction simultaneously
    const recentReturns: Array<{ asset: string; return: number }> = [];
    for (const asset of assets) {
      const returns = this.priceReturns.get(asset);
      if (returns && returns.length > 0) {
        recentReturns.push({
          asset,
          return: returns[returns.length - 1]!,
        });
      }
    }

    if (recentReturns.length < 3) return anomalies;

    // Count assets moving significantly in same direction
    const upMoves = recentReturns.filter((r) => r.return > 1); // > 1%
    const downMoves = recentReturns.filter((r) => r.return < -1); // < -1%

    const significantMoves =
      upMoves.length > downMoves.length ? upMoves : downMoves;
    const direction = upMoves.length > downMoves.length ? 'up' : 'down';
    const fraction = significantMoves.length / recentReturns.length;

    // If > 70% of assets moved > 1% in same direction, flag it
    if (fraction > 0.7 && significantMoves.length >= 3) {
      const assetsInvolved = significantMoves.map((r) => r.asset);
      const key = `coordinated_${direction}_${assetsInvolved.sort().join(':')}`;

      if (!this.isOnCooldown(key, timestamp)) {
        this.setCooldown(key, timestamp);

        anomalies.push(
          this.createEvent(
            CorrelationEventType.COORDINATED_MOVE,
            CorrelationSeverity.CRITICAL,
            `${significantMoves.length}/${recentReturns.length} assets moved ${direction} >1% simultaneously`,
            assetsInvolved,
            fraction,
            0.7,
            timestamp,
            { direction, fraction, assets: assetsInvolved }
          )
        );
      }
    }

    return anomalies;
  }

  // ── Private: Statistical Methods ────────────────────────────────────

  private pearsonCorrelation(x: number[], y: number[]): number {
    const n = x.length;
    if (n < 2) return 0;

    const meanX = x.reduce((s, v) => s + v, 0) / n;
    const meanY = y.reduce((s, v) => s + v, 0) / n;

    let numerator = 0;
    let denomX = 0;
    let denomY = 0;

    for (let i = 0; i < n; i++) {
      const dx = x[i]! - meanX;
      const dy = y[i]! - meanY;
      numerator += dx * dy;
      denomX += dx * dx;
      denomY += dy * dy;
    }

    const denominator = Math.sqrt(denomX * denomY);
    if (denominator === 0) return 0;

    return numerator / denominator;
  }

  private computePValue(correlation: number, sampleSize: number): number {
    // Two-tailed t-test for significance of correlation
    if (sampleSize <= 2) return 1;

    const t =
      (correlation * Math.sqrt(sampleSize - 2)) /
      Math.sqrt(1 - correlation * correlation);

    // Approximate p-value using t-distribution (simplified)
    const df = sampleSize - 2;
    const absT = Math.abs(t);

    // Rough approximation for p-value
    // For df > 2, approximate using normal distribution for large samples
    if (df > 30) {
      // Use normal approximation
      const z = absT;
      return 2 * (1 - this.normalCDF(z));
    }

    // For small df, use a rough approximation
    const x = df / (df + t * t);
    return this.incompleteBeta(df / 2, 0.5, x);
  }

  private normalCDF(x: number): number {
    // Approximation of the standard normal CDF
    const a1 = 0.254829592;
    const a2 = -0.284496736;
    const a3 = 1.421413741;
    const a4 = -1.453152027;
    const a5 = 1.061405429;
    const p = 0.3275911;

    const sign = x >= 0 ? 1 : -1;
    const absX = Math.abs(x) / Math.sqrt(2);

    const t = 1 / (1 + p * absX);
    const y = 1 - ((((a5 * t + a4) * t + a3) * t + a2) * t + a1) * t * Math.exp(-absX * absX);

    return 0.5 * (1 + sign * y);
  }

  private incompleteBeta(a: number, b: number, x: number): number {
    // Simple approximation using continued fraction for incomplete beta
    if (x <= 0) return 0;
    if (x >= 1) return 1;

    // Use simple numerical integration (trapezoidal) for small a, b
    const steps = 100;
    const dx = x / steps;
    let sum = 0;

    for (let i = 0; i <= steps; i++) {
      const t = i * dx;
      const weight = i === 0 || i === steps ? 0.5 : 1;
      const val =
        Math.pow(t, a - 1) * Math.pow(1 - t, b - 1);
      sum += weight * val * dx;
    }

    const beta = this.betaFunction(a, b);
    return sum / beta;
  }

  private betaFunction(a: number, b: number): number {
    return (
      (this.gammaFunction(a) * this.gammaFunction(b)) /
      this.gammaFunction(a + b)
    );
  }

  private gammaFunction(z: number): number {
    // Stirling approximation for gamma function
    if (z < 0.5) {
      return Math.PI / (Math.sin(Math.PI * z) * this.gammaFunction(1 - z));
    }

    z -= 1;
    const g = 7;
    const c = [
      0.99999999999980993,
      676.5203681218851,
      -1259.1392167224028,
      771.32342877765313,
      -176.61502916214059,
      12.507343278686905,
      -0.13857109526572012,
      9.9843695780195716e-6,
      1.5056327351493116e-7,
    ];

    let x = c[0]!;
    for (let i = 1; i < g + 2; i++) {
      x += c[i]! / (z + i);
    }

    const t = z + g + 0.5;
    return Math.sqrt(2 * Math.PI) * Math.pow(t, z + 0.5) * Math.exp(-t) * x;
  }

  // ── Private: Cooldown ──────────────────────────────────────────────

  private isOnCooldown(key: string, timestamp: number): boolean {
    const lastTime = this.lastEventTime.get(key);
    if (lastTime === undefined) return false;
    return timestamp - lastTime < this.config.eventCooldownSeconds;
  }

  private setCooldown(key: string, timestamp: number): void {
    this.lastEventTime.set(key, timestamp);
  }

  // ── Private: Event Management ───────────────────────────────────────

  private createEvent(
    type: CorrelationEventType,
    severity: CorrelationSeverity,
    message: string,
    assets: string[],
    correlation: number,
    threshold: number,
    timestamp: number,
    metadata: Record<string, unknown>
  ): CorrelationEvent {
    this.eventCounter++;
    return {
      id: `corr_${this.eventCounter}_${timestamp}`,
      type,
      severity,
      message,
      assets: assets.map((a) => a.toUpperCase()),
      correlation,
      threshold,
      timestamp,
      metadata,
    };
  }

  private storeEvent(event: CorrelationEvent): void {
    this.events.push(event);

    while (this.events.length > this.config.maxEvents) {
      this.events.shift();
    }

    logger.warn(
      `[CORRELATION] ${event.severity.toUpperCase()} ${event.type}: ${event.message}`,
      { assets: event.assets, correlation: event.correlation }
    );
  }
}

/**
 * Create a feed correlation instance
 */
export function createFeedCorrelation(
  config?: Partial<FeedCorrelationConfig>
): FeedCorrelation {
  return new FeedCorrelation(config);
}
