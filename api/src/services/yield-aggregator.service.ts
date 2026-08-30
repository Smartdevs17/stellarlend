import logger from '../utils/logger';
import crypto from 'crypto';

export interface LendingPoolYield {
  poolId: string;
  poolName: string;
  asset: string;
  protocol: 'StellarLend Native' | 'Blend Protocol' | 'Soroswap AMM' | 'Aquarius LP' | 'UltraStellar';
  supplyApy: number;
  borrowApy: number;
  rewardApy: number;
  netApy: number;
  tvl: number;
  utilizationRate: number;
  riskScore: number; // 1 (safest) to 10 (highest risk)
  historicalAvg7d: number;
  historicalAvg30d: number;
  volatility30d: number;
  isPaused: boolean;
}

export interface RouteAllocation {
  poolId: string;
  poolName: string;
  protocol: string;
  allocatedAmount: number;
  allocationPercent: number;
  poolInitialApy: number;
  expectedMarginalApy: number;
  projectedAnnualEarnings: number;
}

export interface BestRateRouteResult {
  asset: string;
  depositAmount: number;
  strategy: 'highest_yield' | 'balanced_risk' | 'gas_optimized';
  blendedApy: number;
  projectedAnnualEarnings: number;
  allocations: RouteAllocation[];
  singlePoolBestApy: number;
  yieldAdvantagePercent: number;
  estimatedGasCostStroops: number;
  recommendation: string;
}

export interface YieldComparisonItem {
  poolId: string;
  poolName: string;
  protocol: string;
  asset: string;
  baseSupplyApy: number;
  rewardApy: number;
  netApy: number;
  historicalAvg7d: number;
  historicalAvg30d: number;
  tvl: number;
  utilizationRate: number;
  riskScore: number;
  riskAdjustedScore: number; // Net APY / riskScore
  maxCapacityWithoutSlippage: number;
}

export interface YieldAlert {
  id: string;
  userId: string;
  asset: string;
  targetApy: number;
  condition: 'above' | 'below' | 'opportunity_gain';
  active: boolean;
  createdAt: number;
  lastTriggered?: number;
}

export class YieldAggregatorService {
  private pools: Map<string, LendingPoolYield> = new Map();
  private alerts: Map<string, YieldAlert> = new Map();

  constructor() {
    this.seedDefaultPools();
  }

  private seedDefaultPools() {
    const defaultPools: LendingPoolYield[] = [
      {
        poolId: 'xlm-stellarlend-main',
        poolName: 'StellarLend Core XLM',
        asset: 'XLM',
        protocol: 'StellarLend Native',
        supplyApy: 0.052,
        borrowApy: 0.078,
        rewardApy: 0.015,
        netApy: 0.067,
        tvl: 12_500_000,
        utilizationRate: 0.68,
        riskScore: 2,
        historicalAvg7d: 0.065,
        historicalAvg30d: 0.063,
        volatility30d: 0.004,
        isPaused: false,
      },
      {
        poolId: 'xlm-blend-isolated',
        poolName: 'Blend XLM Prime Pool',
        asset: 'XLM',
        protocol: 'Blend Protocol',
        supplyApy: 0.048,
        borrowApy: 0.072,
        rewardApy: 0.008,
        netApy: 0.056,
        tvl: 8_200_000,
        utilizationRate: 0.62,
        riskScore: 3,
        historicalAvg7d: 0.055,
        historicalAvg30d: 0.054,
        volatility30d: 0.006,
        isPaused: false,
      },
      {
        poolId: 'xlm-aquarius-lp',
        poolName: 'Aquarius XLM/USDC Staking',
        asset: 'XLM',
        protocol: 'Aquarius LP',
        supplyApy: 0.061,
        borrowApy: 0.095,
        rewardApy: 0.022,
        netApy: 0.083,
        tvl: 4_500_000,
        utilizationRate: 0.74,
        riskScore: 5,
        historicalAvg7d: 0.081,
        historicalAvg30d: 0.079,
        volatility30d: 0.012,
        isPaused: false,
      },
      {
        poolId: 'usdc-stellarlend-main',
        poolName: 'StellarLend Core USDC',
        asset: 'USDC',
        protocol: 'StellarLend Native',
        supplyApy: 0.085,
        borrowApy: 0.115,
        rewardApy: 0.020,
        netApy: 0.105,
        tvl: 25_000_000,
        utilizationRate: 0.82,
        riskScore: 2,
        historicalAvg7d: 0.102,
        historicalAvg30d: 0.098,
        volatility30d: 0.005,
        isPaused: false,
      },
      {
        poolId: 'usdc-blend-pool',
        poolName: 'Blend Yield USDC',
        asset: 'USDC',
        protocol: 'Blend Protocol',
        supplyApy: 0.079,
        borrowApy: 0.108,
        rewardApy: 0.014,
        netApy: 0.093,
        tvl: 18_000_000,
        utilizationRate: 0.76,
        riskScore: 3,
        historicalAvg7d: 0.092,
        historicalAvg30d: 0.091,
        volatility30d: 0.007,
        isPaused: false,
      },
      {
        poolId: 'usdc-soroswap-liquidity',
        poolName: 'Soroswap USDC Farm',
        asset: 'USDC',
        protocol: 'Soroswap AMM',
        supplyApy: 0.092,
        borrowApy: 0.130,
        rewardApy: 0.028,
        netApy: 0.120,
        tvl: 6_000_000,
        utilizationRate: 0.86,
        riskScore: 4,
        historicalAvg7d: 0.118,
        historicalAvg30d: 0.112,
        volatility30d: 0.015,
        isPaused: false,
      },
      {
        poolId: 'eurc-stellarlend-main',
        poolName: 'StellarLend Core EURC',
        asset: 'EURC',
        protocol: 'StellarLend Native',
        supplyApy: 0.042,
        borrowApy: 0.065,
        rewardApy: 0.012,
        netApy: 0.054,
        tvl: 7_500_000,
        utilizationRate: 0.58,
        riskScore: 2,
        historicalAvg7d: 0.053,
        historicalAvg30d: 0.051,
        volatility30d: 0.003,
        isPaused: false,
      },
      {
        poolId: 'btc-stellarlend-main',
        poolName: 'StellarLend Core WBTC',
        asset: 'BTC',
        protocol: 'StellarLend Native',
        supplyApy: 0.028,
        borrowApy: 0.045,
        rewardApy: 0.009,
        netApy: 0.037,
        tvl: 15_000_000,
        utilizationRate: 0.52,
        riskScore: 3,
        historicalAvg7d: 0.036,
        historicalAvg30d: 0.035,
        volatility30d: 0.004,
        isPaused: false,
      },
    ];

    for (const pool of defaultPools) {
      this.pools.set(pool.poolId, pool);
    }
  }

  /**
   * Get all lending pools across protocols with live APYs.
   */
  getAllPools(asset?: string): LendingPoolYield[] {
    const list = Array.from(this.pools.values()).filter((p) => !p.isPaused);
    if (asset) {
      return list.filter((p) => p.asset.toUpperCase() === asset.toUpperCase());
    }
    return list;
  }

  /**
   * Best-rate routing algorithm:
   * Finds the optimal single or multi-pool allocation for a given deposit amount.
   * Models marginal rate degradation when adding supply to finite pools.
   */
  findBestRateRoute(
    asset: string,
    depositAmount: number,
    strategy: 'highest_yield' | 'balanced_risk' | 'gas_optimized' = 'highest_yield',
    maxSplits: number = 3
  ): BestRateRouteResult {
    const candidatePools = this.getAllPools(asset);
    if (candidatePools.length === 0) {
      throw new Error(`No active yield pools found for asset ${asset}`);
    }

    if (depositAmount <= 0) {
      throw new Error('Deposit amount must be greater than zero');
    }

    // Score candidate pools according to strategy
    const scoredPools = candidatePools.map((p) => {
      let score = p.netApy;
      if (strategy === 'balanced_risk') {
        // Penalize higher risk scores
        score = p.netApy / (1 + p.riskScore * 0.1);
      } else if (strategy === 'gas_optimized') {
        // Slight bias towards primary protocol to minimize multi-contract calls
        if (p.protocol === 'StellarLend Native') score += 0.005;
      }
      return { pool: p, score };
    });

    scoredPools.sort((a, b) => b.score - a.score);

    // If gas optimized or small deposit (< 1% of top pool TVL), 100% route to best pool
    const topPool = scoredPools[0]!.pool;
    const isSmallDeposit = depositAmount < topPool.tvl * 0.01;

    let allocations: RouteAllocation[] = [];

    if (strategy === 'gas_optimized' || isSmallDeposit || scoredPools.length === 1 || maxSplits === 1) {
      // Single pool allocation
      allocations = [
        {
          poolId: topPool.poolId,
          poolName: topPool.poolName,
          protocol: topPool.protocol,
          allocatedAmount: depositAmount,
          allocationPercent: 100,
          poolInitialApy: topPool.netApy,
          expectedMarginalApy: topPool.netApy * (1 - Math.min(0.05, depositAmount / (topPool.tvl * 2))),
          projectedAnnualEarnings: depositAmount * topPool.netApy,
        },
      ];
    } else {
      // Split routing optimization: distribute capital across top candidates to prevent APY dilution
      const poolsToUse = scoredPools.slice(0, Math.min(maxSplits, scoredPools.length));
      
      // Compute allocation weights based on TVL capacity and yield score
      const totalCapacityWeight = poolsToUse.reduce(
        (sum, item) => sum + item.score * Math.sqrt(item.pool.tvl),
        0
      );

      allocations = poolsToUse.map((item) => {
        const weight = (item.score * Math.sqrt(item.pool.tvl)) / totalCapacityWeight;
        const allocatedAmount = Math.round(depositAmount * weight * 100) / 100;
        const allocationPercent = Math.round(weight * 1000) / 10;
        
        // Slippage / dilution adjustment
        const dilutionFactor = 1 - Math.min(0.04, allocatedAmount / (item.pool.tvl * 2));
        const expectedMarginalApy = item.pool.netApy * dilutionFactor;

        return {
          poolId: item.pool.poolId,
          poolName: item.pool.poolName,
          protocol: item.pool.protocol,
          allocatedAmount,
          allocationPercent,
          poolInitialApy: item.pool.netApy,
          expectedMarginalApy: Math.round(expectedMarginalApy * 10000) / 10000,
          projectedAnnualEarnings: Math.round(allocatedAmount * expectedMarginalApy * 100) / 100,
        };
      });

      // Ensure total allocated matches depositAmount exactly
      const allocatedSum = allocations.reduce((s, a) => s + a.allocatedAmount, 0);
      const diff = depositAmount - allocatedSum;
      if (diff !== 0 && allocations.length > 0) {
        allocations[0]!.allocatedAmount += diff;
      }
    }

    const blendedApy =
      allocations.reduce((sum, a) => sum + a.expectedMarginalApy * (a.allocatedAmount / depositAmount), 0);
    const projectedAnnualEarnings = allocations.reduce((sum, a) => sum + a.projectedAnnualEarnings, 0);
    const singlePoolBestApy = topPool.netApy;
    const yieldAdvantagePercent =
      Math.round(((blendedApy - singlePoolBestApy) / singlePoolBestApy) * 10000) / 100;

    let recommendation = `Routed ${depositAmount.toLocaleString()} ${asset} across ${allocations.length} pool(s) for an effective APY of ${(blendedApy * 100).toFixed(2)}%.`;
    if (allocations.length > 1) {
      recommendation += ` Split routing avoids pool dilution and balances risk across ${allocations.map((a) => a.poolName).join(', ')}.`;
    }

    return {
      asset,
      depositAmount,
      strategy,
      blendedApy: Math.round(blendedApy * 10000) / 10000,
      projectedAnnualEarnings: Math.round(projectedAnnualEarnings * 100) / 100,
      allocations,
      singlePoolBestApy,
      yieldAdvantagePercent,
      estimatedGasCostStroops: allocations.length * 150_000,
      recommendation,
    };
  }

  /**
   * Compare multiple pools side-by-side.
   */
  comparePools(poolIds?: string[], asset?: string): YieldComparisonItem[] {
    let poolList = Array.from(this.pools.values());
    if (poolIds && poolIds.length > 0) {
      poolList = poolList.filter((p) => poolIds.includes(p.poolId));
    } else if (asset) {
      poolList = poolList.filter((p) => p.asset.toUpperCase() === asset.toUpperCase());
    }

    return poolList.map((p) => ({
      poolId: p.poolId,
      poolName: p.poolName,
      protocol: p.protocol,
      asset: p.asset,
      baseSupplyApy: p.supplyApy,
      rewardApy: p.rewardApy,
      netApy: p.netApy,
      historicalAvg7d: p.historicalAvg7d,
      historicalAvg30d: p.historicalAvg30d,
      tvl: p.tvl,
      utilizationRate: p.utilizationRate,
      riskScore: p.riskScore,
      riskAdjustedScore: Math.round((p.netApy / p.riskScore) * 10000) / 10000,
      maxCapacityWithoutSlippage: Math.round(p.tvl * 0.05), // 5% of TVL before slippage
    }));
  }

  /**
   * Get comprehensive yield analytics and historical curve for a specific pool.
   */
  getYieldAnalytics(poolId: string) {
    const pool = this.pools.get(poolId);
    if (!pool) {
      throw new Error(`Pool not found: ${poolId}`);
    }

    // Historical daily points for 30 days
    const now = Date.now();
    const dayMs = 24 * 60 * 60 * 1000;
    const history = [];
    for (let i = 29; i >= 0; i--) {
      const timestamp = new Date(now - i * dayMs).toISOString().split('T')[0];
      const variance = (Math.sin(i) * pool.volatility30d);
      const supplyApy = Math.max(0.01, pool.supplyApy + variance);
      history.push({
        date: timestamp,
        supplyApy: Math.round(supplyApy * 10000) / 10000,
        netApy: Math.round((supplyApy + pool.rewardApy) * 10000) / 10000,
        utilization: Math.round((pool.utilizationRate + variance * 2) * 100) / 100,
      });
    }

    // Rate curve simulation across utilization levels
    const utilizationCurve = [0.1, 0.25, 0.5, 0.7, 0.8, 0.9, 0.95].map((u) => {
      const base = 0.02;
      const kink = 0.8;
      const slope1 = 0.06;
      const slope2 = 0.35;
      const rate = u <= kink ? base + u * slope1 : base + kink * slope1 + (u - kink) * slope2;
      return {
        utilizationPercent: u * 100,
        borrowRate: Math.round(rate * 10000) / 100,
        supplyRate: Math.round(rate * u * 0.9 * 10000) / 100, // 90% goes to suppliers
      };
    });

    return {
      pool,
      history,
      utilizationCurve,
      summary: {
        currentNetApy: pool.netApy,
        historicalAvg7d: pool.historicalAvg7d,
        historicalAvg30d: pool.historicalAvg30d,
        volatility: pool.volatility30d,
        sharpeRatio: Math.round((pool.netApy / (pool.volatility30d || 0.001)) * 100) / 100,
      },
    };
  }

  /**
   * Create an alert subscription for yield thresholds or re-routing opportunities.
   */
  createAlert(userId: string, asset: string, targetApy: number, condition: 'above' | 'below' | 'opportunity_gain'): YieldAlert {
    const id = crypto.randomBytes(6).toString('hex');
    const alert: YieldAlert = {
      id,
      userId,
      asset: asset.toUpperCase(),
      targetApy,
      condition,
      active: true,
      createdAt: Date.now(),
    };
    this.alerts.set(id, alert);
    logger.info(`Yield alert created: ${id} for user ${userId} on ${asset}`);
    return alert;
  }

  /**
   * Get active alerts for a user.
   */
  getUserAlerts(userId: string): YieldAlert[] {
    return Array.from(this.alerts.values()).filter((a) => a.userId === userId && a.active);
  }

  /**
   * Delete an alert.
   */
  deleteAlert(alertId: string): boolean {
    return this.alerts.delete(alertId);
  }

  /**
   * Evaluate all active alerts against current pool yields.
   */
  checkAlerts() {
    const triggered: Array<{ alert: YieldAlert; currentPoolYield: LendingPoolYield; message: string }> = [];

    for (const alert of this.alerts.values()) {
      if (!alert.active) continue;
      const pools = this.getAllPools(alert.asset);

      for (const p of pools) {
        if (alert.condition === 'above' && p.netApy >= alert.targetApy) {
          triggered.push({
            alert,
            currentPoolYield: p,
            message: `${p.poolName} net APY is now ${(p.netApy * 100).toFixed(2)}%, exceeding your target of ${(alert.targetApy * 100).toFixed(2)}%!`,
          });
          alert.lastTriggered = Date.now();
        } else if (alert.condition === 'below' && p.netApy <= alert.targetApy) {
          triggered.push({
            alert,
            currentPoolYield: p,
            message: `${p.poolName} net APY dropped to ${(p.netApy * 100).toFixed(2)}%, below your threshold of ${(alert.targetApy * 100).toFixed(2)}%!`,
          });
          alert.lastTriggered = Date.now();
        }
      }
    }

    return triggered;
  }
}

export const yieldAggregatorService = new YieldAggregatorService();
