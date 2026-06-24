import { StellarService } from './stellar.service';

export interface PoolHealthMetrics {
  poolId: string;
  utilizationRate: number;
  totalSupplied: string;
  totalBorrowed: string;
  availableLiquidity: string;
  averageLtv: number;
  concentrationRisk: number;
}

export interface LiquidationRiskEntry {
  user: string;
  poolId: string;
  healthFactor: number;
  collateralValue: string;
  debtValue: string;
  liquidationThreshold: number;
  riskLevel: 'Safe' | 'Warning' | 'Danger' | 'Critical';
}

export interface OracleHealthStatus {
  asset: string;
  lastUpdateTimestamp: number;
  price: string;
  stalenessSeconds: number;
  deviationFromTwap: number;
  isHealthy: boolean;
}

export interface ProtocolSafetyScore {
  overallScore: number;
  liquidityScore: number;
  solvencyScore: number;
  oracleHealthScore: number;
  concentrationScore: number;
  timestamp: number;
}

export interface AlertConfig {
  healthFactorThreshold: number;
  utilizationThreshold: number;
  concentrationThreshold: number;
  oracleStalenessThreshold: number;
}

export interface RiskAlert {
  id: string;
  severity: 'low' | 'medium' | 'high' | 'critical';
  type: string;
  message: string;
  timestamp: number;
  acknowledged: boolean;
}

class RiskMonitoringService {
  private stellarService: StellarService;
  private alertConfig: AlertConfig = {
    healthFactorThreshold: 12000,
    utilizationThreshold: 9000,
    concentrationThreshold: 3000,
    oracleStalenessThreshold: 300,
  };

  constructor() {
    this.stellarService = new StellarService();
  }

  async getPoolHealthMetrics(poolId: string): Promise<PoolHealthMetrics> {
    const totalSupplied = '1000000';
    const totalBorrowed = '750000';
    
    const utilizationRate = (parseInt(totalBorrowed) / parseInt(totalSupplied)) * 10000;
    const availableLiquidity = (parseInt(totalSupplied) - parseInt(totalBorrowed)).toString();

    return {
      poolId,
      utilizationRate,
      totalSupplied,
      totalBorrowed,
      availableLiquidity,
      averageLtv: 6500,
      concentrationRisk: 2500,
    };
  }

  async getLiquidationRiskHeatmap(poolId?: string): Promise<Map<string, LiquidationRiskEntry[]>> {
    const heatmap = new Map<string, LiquidationRiskEntry[]>();
    
    const mockEntry: LiquidationRiskEntry = {
      user: 'GABC...',
      poolId: poolId || 'pool-1',
      healthFactor: 11000,
      collateralValue: '100000',
      debtValue: '80000',
      liquidationThreshold: 8000,
      riskLevel: 'Danger',
    };

    heatmap.set(poolId || 'pool-1', [mockEntry]);
    return heatmap;
  }

  async getOracleHealthStatus(): Promise<OracleHealthStatus[]> {
    return [
      {
        asset: 'XLM',
        lastUpdateTimestamp: Date.now() - 120000,
        price: '0.12',
        stalenessSeconds: 120,
        deviationFromTwap: 150,
        isHealthy: true,
      },
      {
        asset: 'USDC',
        lastUpdateTimestamp: Date.now() - 60000,
        price: '1.00',
        stalenessSeconds: 60,
        deviationFromTwap: 10,
        isHealthy: true,
      },
    ];
  }

  async getProtocolSafetyScore(): Promise<ProtocolSafetyScore> {
    const liquidityScore = 8500;
    const solvencyScore = 9000;
    const oracleHealthScore = 8800;
    const concentrationScore = 7500;

    const overallScore = Math.floor(
      (liquidityScore * 0.25 +
        solvencyScore * 0.35 +
        oracleHealthScore * 0.2 +
        concentrationScore * 0.2)
    );

    return {
      overallScore,
      liquidityScore,
      solvencyScore,
      oracleHealthScore,
      concentrationScore,
      timestamp: Date.now(),
    };
  }

  async getMetricTrends(metric: string, period: string): Promise<any[]> {
    const trends = [];
    const now = Date.now();
    const periodMs = period === '24h' ? 24 * 60 * 60 * 1000 : 7 * 24 * 60 * 60 * 1000;
    
    for (let i = 0; i < 10; i++) {
      trends.push({
        timestamp: now - (periodMs * i / 10),
        value: 7000 + Math.random() * 2000,
        metric,
      });
    }

    return trends.reverse();
  }

  async getActiveAlerts(severity?: string, limit: number = 50): Promise<RiskAlert[]> {
    const alerts: RiskAlert[] = [
      {
        id: '1',
        severity: 'high',
        type: 'liquidation_risk',
        message: 'User GABC... health factor below threshold',
        timestamp: Date.now() - 300000,
        acknowledged: false,
      },
      {
        id: '2',
        severity: 'medium',
        type: 'high_utilization',
        message: 'Pool utilization above 90%',
        timestamp: Date.now() - 600000,
        acknowledged: false,
      },
    ];

    let filtered = alerts;
    if (severity) {
      filtered = alerts.filter(a => a.severity === severity);
    }

    return filtered.slice(0, limit);
  }

  async updateAlertConfiguration(config: Partial<AlertConfig>): Promise<void> {
    this.alertConfig = { ...this.alertConfig, ...config };
  }

  async getUserRiskProfile(address: string): Promise<any> {
    return {
      address,
      healthFactor: 13500,
      totalCollateralValue: '150000',
      totalDebtValue: '90000',
      ltv: 6000,
      liquidationPrice: '0.08',
      riskLevel: 'Warning',
      positions: [
        {
          poolId: 'pool-1',
          collateral: '100000',
          debt: '60000',
          healthFactor: 14000,
        },
      ],
    };
  }

  private checkForAlerts(): void {
  }
}

export const riskMonitoringService = new RiskMonitoringService();
