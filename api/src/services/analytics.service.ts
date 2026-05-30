import {
  HistoricalRatePoint,
  PoolUtilizationPoint,
  RateComparison,
  ProtocolRevenuePoint,
  AnalyticsSummary,
  AnalyticsQuery,
  AnalyticsExportData,
} from '../types/analytics';
import { StellarService } from './stellar.service';
import { redisCacheService } from './redisCache.service';
import { config } from '../config';

const ANALYTICS_CACHE_TTL_S = 60;

function generateTimePoints(timeRange: string, count: number): number[] {
  const now = Date.now();
  const ranges: Record<string, number> = {
    '1d': 86400000,
    '7d': 604800000,
    '30d': 2592000000,
    '1y': 31536000000,
  };
  const range = ranges[timeRange] || 86400000;
  const interval = range / count;
  return Array.from({ length: count }, (_, i) => now - range + interval * i);
}

export async function getHistoricalRates(
  query: AnalyticsQuery
): Promise<HistoricalRatePoint[]> {
  const cacheKey = redisCacheService.buildKey(
    'protocol',
    `historical-rates:${query.timeRange}:${query.poolAddress || 'all'}`
  );

  const cached = await redisCacheService.get<HistoricalRatePoint[]>(cacheKey);
  if (cached) return cached;

  const stellarService = new StellarService();
  const timePoints = generateTimePoints(query.timeRange, 100);

  const rates: HistoricalRatePoint[] = await Promise.all(
    timePoints.map(async (timestamp) => {
      const rateData = await stellarService.getPoolRateAt(
        query.poolAddress || '',
        Math.floor(timestamp / 1000)
      );
      return {
        timestamp: new Date(timestamp).toISOString(),
        depositApy: rateData.depositApy,
        borrowApy: rateData.borrowApy,
        utilizationRate: rateData.utilizationRate,
        poolAddress: query.poolAddress,
      };
    })
  );

  await redisCacheService.set(cacheKey, rates, ANALYTICS_CACHE_TTL_S);
  return rates;
}

export async function getPoolUtilization(
  query: AnalyticsQuery
): Promise<PoolUtilizationPoint[]> {
  const cacheKey = redisCacheService.buildKey(
    'pool',
    `utilization:${query.timeRange}:${query.poolAddress || 'all'}`
  );

  const cached = await redisCacheService.get<PoolUtilizationPoint[]>(cacheKey);
  if (cached) return cached;

  const stellarService = new StellarService();
  const timePoints = generateTimePoints(query.timeRange, 100);

  const utilization: PoolUtilizationPoint[] = await Promise.all(
    timePoints.map(async (timestamp) => {
      const poolData = await stellarService.getPoolStateAt(
        query.poolAddress || '',
        Math.floor(timestamp / 1000)
      );
      return {
        timestamp: new Date(timestamp).toISOString(),
        utilizationRate: poolData.utilizationRate,
        totalDeposits: poolData.totalDeposits,
        totalBorrows: poolData.totalBorrows,
        poolAddress: query.poolAddress || 'all',
      };
    })
  );

  await redisCacheService.set(cacheKey, utilization, ANALYTICS_CACHE_TTL_S);
  return utilization;
}

export async function getRateComparison(): Promise<RateComparison[]> {
  const cacheKey = redisCacheService.buildKey('protocol', 'rate-comparison');
  const cached = await redisCacheService.get<RateComparison[]>(cacheKey);
  if (cached) return cached;

  const stellarService = new StellarService();
  const pools = await stellarService.getAllPools();

  const comparisons: RateComparison[] = pools.map((pool) => ({
    poolAddress: pool.address,
    poolName: pool.name,
    depositApy: pool.depositApy,
    borrowApy: pool.borrowApy,
    utilizationRate: pool.utilizationRate,
    tvl: pool.tvl,
  }));

  await redisCacheService.set(cacheKey, comparisons, ANALYTICS_CACHE_TTL_S);
  return comparisons;
}

export async function getProtocolRevenue(
  query: AnalyticsQuery
): Promise<ProtocolRevenuePoint[]> {
  const cacheKey = redisCacheService.buildKey(
    'protocol',
    `revenue:${query.timeRange}`
  );

  const cached = await redisCacheService.get<ProtocolRevenuePoint[]>(cacheKey);
  if (cached) return cached;

  const stellarService = new StellarService();
  const timePoints = generateTimePoints(query.timeRange, 100);

  const revenue: ProtocolRevenuePoint[] = await Promise.all(
    timePoints.map(async (timestamp) => {
      const revData = await stellarService.getProtocolRevenueAt(
        Math.floor(timestamp / 1000)
      );
      return {
        timestamp: new Date(timestamp).toISOString(),
        cumulativeRevenue: revData.cumulativeRevenue,
        periodRevenue: revData.periodRevenue,
        revenueSource: 'interest',
      };
    })
  );

  await redisCacheService.set(cacheKey, revenue, ANALYTICS_CACHE_TTL_S);
  return revenue;
}

export async function getAnalyticsSummary(): Promise<AnalyticsSummary> {
  const cacheKey = redisCacheService.buildKey('protocol', 'analytics-summary');
  const cached = await redisCacheService.get<AnalyticsSummary>(cacheKey);
  if (cached) return cached;

  const stellarService = new StellarService();
  const [protocolStats, pools] = await Promise.all([
    stellarService.getProtocolStats(),
    stellarService.getAllPools(),
  ]);

  const avgDepositApy =
    pools.length > 0
      ? pools.reduce((sum, p) => sum + p.depositApy, 0) / pools.length
      : 0;
  const avgBorrowApy =
    pools.length > 0
      ? pools.reduce((sum, p) => sum + p.borrowApy, 0) / pools.length
      : 0;
  const avgUtilization =
    pools.length > 0
      ? pools.reduce((sum, p) => sum + p.utilizationRate, 0) / pools.length
      : 0;

  const summary: AnalyticsSummary = {
    totalPools: pools.length,
    averageDepositApy: avgDepositApy,
    averageBorrowApy: avgBorrowApy,
    averageUtilizationRate: avgUtilization,
    totalValueLocked: protocolStats.tvl,
    cumulativeRevenue: protocolStats.totalBorrows,
    activeUsers: protocolStats.numberOfUsers,
    snapshotTimestamp: new Date().toISOString(),
  };

  await redisCacheService.set(cacheKey, summary, ANALYTICS_CACHE_TTL_S);
  return summary;
}

export async function exportAnalytics(
  query: AnalyticsQuery,
  format: 'csv' | 'json'
): Promise<AnalyticsExportData | string> {
  const [historicalRates, poolUtilization, rateComparison, revenue, summary] =
    await Promise.all([
      getHistoricalRates(query),
      getPoolUtilization(query),
      getRateComparison(),
      getProtocolRevenue(query),
      getAnalyticsSummary(),
    ]);

  const data: AnalyticsExportData = {
    exportedAt: new Date().toISOString(),
    timeRange: query.timeRange,
    historicalRates,
    poolUtilization,
    rateComparison,
    revenue,
    summary,
  };

  if (format === 'csv') {
    return toAnalyticsCSV(data);
  }

  return data;
}

function toAnalyticsCSV(data: AnalyticsExportData): string {
  const header =
    'timestamp,depositApy,borrowApy,utilizationRate,poolAddress,cumulativeRevenue';
  const rows = data.historicalRates.map(
    (r) =>
      `${r.timestamp},${r.depositApy},${r.borrowApy},${r.utilizationRate},${r.poolAddress || ''},`
  );
  const revenueRows = data.revenue.map(
    (r) => `${r.timestamp},,,,,"${r.cumulativeRevenue}"`
  );
  return [header, ...rows, ...revenueRows].join('\n');
}
