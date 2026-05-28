export interface HistoricalRatePoint {
  timestamp: string;
  depositApy: number;
  borrowApy: number;
  utilizationRate: number;
  poolAddress?: string;
}

export interface PoolUtilizationPoint {
  timestamp: string;
  utilizationRate: number;
  totalDeposits: string;
  totalBorrows: string;
  poolAddress: string;
}

export interface RateComparison {
  poolAddress: string;
  poolName?: string;
  depositApy: number;
  borrowApy: number;
  utilizationRate: number;
  tvl: string;
}

export interface ProtocolRevenuePoint {
  timestamp: string;
  cumulativeRevenue: string;
  periodRevenue: string;
  revenueSource: 'interest' | 'fees' | 'liquidation';
}

export interface AnalyticsSummary {
  totalPools: number;
  averageDepositApy: number;
  averageBorrowApy: number;
  averageUtilizationRate: number;
  totalValueLocked: string;
  cumulativeRevenue: string;
  activeUsers: number;
  snapshotTimestamp: string;
}

export interface AnalyticsQuery {
  timeRange: '1d' | '7d' | '30d' | '1y';
  poolAddress?: string;
  limit?: number;
  cursor?: string;
}

export interface AnalyticsExportData {
  exportedAt: string;
  timeRange: string;
  historicalRates: HistoricalRatePoint[];
  poolUtilization: PoolUtilizationPoint[];
  rateComparison: RateComparison[];
  revenue: ProtocolRevenuePoint[];
  summary: AnalyticsSummary;
}

// WebSocket analytics message types
export interface WsAnalyticsMessage {
  type: 'analytics_update';
  channel: 'apy' | 'utilization' | 'revenue';
  data: HistoricalRatePoint | PoolUtilizationPoint | ProtocolRevenuePoint;
  timestamp: number;
}
