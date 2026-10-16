import { Contract, Address, TransactionBuilder, scValToNative } from '@stellar/stellar-sdk';
import { Server as SorobanServer } from '@stellar/stellar-sdk/rpc';
import { config } from '../config';
import logger from '../utils/logger';

const RISK_SCORING_CONTRACT_ID = process.env.RISK_SCORING_CONTRACT_ID ?? '';

export interface RiskScoreData {
  pool: string;
  assetVolatilityScore: number;
  oracleDeviationScore: number;
  poolUtilizationScore: number;
  liquidationHistoryScore: number;
  overallScore: number;
  letterGrade: string;
  timestamp: number;
}

export interface RiskFactorBreakdown {
  assetVolatilityBps: number;
  oracleDeviationBps: number;
  poolUtilizationBps: number;
  liquidationHistoryBps: number;
}

export interface RiskWeights {
  assetVolatilityWeight: number;
  oracleDeviationWeight: number;
  poolUtilizationWeight: number;
  liquidationHistoryWeight: number;
}

export interface PoolRiskProfile {
  pool: string;
  currentScore: RiskScoreData;
  history: RiskScoreData[];
  trend: 'improving' | 'stable' | 'declining';
  factors: RiskFactorBreakdown;
}

export interface RiskAlert {
  id: string;
  pool: string;
  severity: 'low' | 'medium' | 'high' | 'critical';
  message: string;
  currentScore: number;
  threshold: number;
  letterGrade: string;
  timestamp: number;
  acknowledged: boolean;
}

export interface RiskAnalytics {
  totalPools: number;
  averageScore: number;
  distribution: Record<string, number>;
  alertsActive: number;
  poolsAtRisk: number;
  trendSummary: { improving: number; stable: number; declining: number };
}

const DEFAULT_THRESHOLDS = { critical: 550, high: 700, medium: 800 };

class RiskScoringService {
  private server: SorobanServer;
  private mockScores: Map<string, RiskScoreData> = new Map();
  private mockHistory: Map<string, RiskScoreData[]> = new Map();
  private alerts: RiskAlert[] = [];
  private alertCounter = 0;

  constructor() {
    this.server = new SorobanServer(config.stellar.sorobanRpcUrl);
    this.seedMockData();
  }

  private seedMockData(): void {
    const now = Math.floor(Date.now() / 1000);
    const pools = [
      { pool: 'pool-xlm-usdc', vol: 1800, oracle: 45, util: 7200, liq: 300 },
      { pool: 'pool-usdc-usdt', vol: 200, oracle: 10, util: 4500, liq: 50 },
      { pool: 'pool-eth-usdc', vol: 3500, oracle: 80, util: 8500, liq: 1200 },
      { pool: 'pool-xlm-usdt', vol: 1500, oracle: 35, util: 6800, liq: 200 },
    ];

    for (const p of pools) {
      const score = this.computeScore(p.pool, p.vol, p.oracle, p.util, p.liq, now);
      this.mockScores.set(p.pool, score);
      this.mockHistory.set(p.pool, [
        { ...score, timestamp: now - 86400 * 7, overallScore: Math.max(score.overallScore - 50, 0) },
        { ...score, timestamp: now - 86400 * 3, overallScore: Math.max(score.overallScore - 20, 0) },
        score,
      ]);
      this.evaluateAlerts(score);
    }
  }

  private evaluateAlerts(score: RiskScoreData): void {
    const thresholds = [
      { severity: 'critical' as const, threshold: DEFAULT_THRESHOLDS.critical },
      { severity: 'high' as const, threshold: DEFAULT_THRESHOLDS.high },
      { severity: 'medium' as const, threshold: DEFAULT_THRESHOLDS.medium },
    ];

    for (const { severity, threshold } of thresholds) {
      if (score.overallScore < threshold) {
        const existing = this.alerts.find(
          (a) => a.pool === score.pool && a.severity === severity && !a.acknowledged
        );
        if (!existing) {
          this.alerts.push({
            id: `alert-${++this.alertCounter}`,
            pool: score.pool,
            severity,
            message: `Pool ${score.pool} risk score ${score.overallScore} (${score.letterGrade}) below ${severity} threshold (${threshold})`,
            currentScore: score.overallScore,
            threshold,
            letterGrade: score.letterGrade,
            timestamp: score.timestamp,
            acknowledged: false,
          });
        }
      }
    }
  }

  refreshScores(): void {
    const now = Math.floor(Date.now() / 1000);
    for (const [pool, score] of this.mockScores) {
      const jitter = Math.floor(Math.random() * 20) - 10;
      const updated = { ...score, overallScore: Math.max(0, Math.min(1000, score.overallScore + jitter)), timestamp: now };
      updated.letterGrade = this.scoreToGrade(updated.overallScore);
      this.mockScores.set(pool, updated);
      const history = this.mockHistory.get(pool) ?? [];
      history.push(updated);
      if (history.length > 10) history.shift();
      this.mockHistory.set(pool, history);
      this.evaluateAlerts(updated);
    }
  }

  private scoreToGrade(overallScore: number): string {
    if (overallScore >= 950) return 'A+';
    if (overallScore >= 900) return 'A';
    if (overallScore >= 850) return 'A-';
    if (overallScore >= 800) return 'B+';
    if (overallScore >= 750) return 'B';
    if (overallScore >= 700) return 'B-';
    if (overallScore >= 650) return 'C+';
    if (overallScore >= 600) return 'C';
    if (overallScore >= 550) return 'C-';
    return 'D';
  }

  private computeScore(
    pool: string,
    assetVolatilityBps: number,
    oracleDeviationBps: number,
    poolUtilizationBps: number,
    liquidationHistoryBps: number,
    timestamp: number,
  ): RiskScoreData {
    const volScore = assetVolatilityBps < 1000 ? 250 : assetVolatilityBps < 2500 ? 200 : assetVolatilityBps < 5000 ? 150 : 100;
    const oracleScore = oracleDeviationBps < 50 ? 250 : oracleDeviationBps < 100 ? 200 : oracleDeviationBps < 300 ? 150 : 100;
    const utilScore = poolUtilizationBps < 6000 ? 250 : poolUtilizationBps < 8000 ? 200 : poolUtilizationBps < 9500 ? 150 : 100;
    const liqScore = liquidationHistoryBps < 100 ? 250 : liquidationHistoryBps < 500 ? 200 : liquidationHistoryBps < 2000 ? 150 : 100;

    const overallScore = Math.min(
      Math.round(
        (volScore * 0.3 + oracleScore * 0.25 + utilScore * 0.25 + liqScore * 0.2)
      ),
      1000
    );

    let letterGrade: string;
    if (overallScore >= 950) letterGrade = 'A+';
    else if (overallScore >= 900) letterGrade = 'A';
    else if (overallScore >= 850) letterGrade = 'A-';
    else if (overallScore >= 800) letterGrade = 'B+';
    else if (overallScore >= 750) letterGrade = 'B';
    else if (overallScore >= 700) letterGrade = 'B-';
    else if (overallScore >= 650) letterGrade = 'C+';
    else if (overallScore >= 600) letterGrade = 'C';
    else if (overallScore >= 550) letterGrade = 'C-';
    else letterGrade = 'D';

    return {
      pool,
      assetVolatilityScore: volScore,
      oracleDeviationScore: oracleScore,
      poolUtilizationScore: utilScore,
      liquidationHistoryScore: liqScore,
      overallScore,
      letterGrade,
      timestamp,
    };
  }

  async getPoolRiskScore(pool: string): Promise<RiskScoreData | null> {
    if (!RISK_SCORING_CONTRACT_ID) {
      return this.mockScores.get(pool) ?? null;
    }
    try {
      const contract = new Contract(RISK_SCORING_CONTRACT_ID);
      const account = await this.server.getAccount(config.stellar.readOnlySimulationAccount);
      const tx = new TransactionBuilder(account, {
        fee: '100',
        networkPassphrase: config.stellar.networkPassphrase,
      })
        .addOperation(contract.call('get_pool_risk_score', new Address(pool).toScVal()))
        .setTimeout(30)
        .build();

      const sim = await this.server.simulateTransaction(tx);
      if (!('result' in sim) || !sim.result) return this.mockScores.get(pool) ?? null;

      const raw = scValToNative(sim.result.retval) as Record<string, unknown>;
      return {
        pool,
        assetVolatilityScore: Number((raw as any)['asset_volatility_score'] ?? 0),
        oracleDeviationScore: Number((raw as any)['oracle_deviation_score'] ?? 0),
        poolUtilizationScore: Number((raw as any)['pool_utilization_score'] ?? 0),
        liquidationHistoryScore: Number((raw as any)['liquidation_history_score'] ?? 0),
        overallScore: Number((raw as any)['overall_score'] ?? 0),
        letterGrade: String((raw as any)['letter_grade'] ?? 'N/A'),
        timestamp: Number((raw as any)['timestamp'] ?? 0),
      };
    } catch (err) {
      logger.warn('Risk score fetch failed', { pool, err: String(err) });
      return this.mockScores.get(pool) ?? null;
    }
  }

  async getAllPoolScores(): Promise<RiskScoreData[]> {
    const scores: RiskScoreData[] = [];
    for (const [pool] of this.mockScores) {
      const score = await this.getPoolRiskScore(pool);
      if (score) scores.push(score);
    }
    return scores;
  }

  async getPoolRiskProfile(pool: string): Promise<PoolRiskProfile | null> {
    const currentScore = await this.getPoolRiskScore(pool);
    if (!currentScore) return null;

    const history = this.mockHistory.get(pool) ?? [currentScore];
    const trend = history.length >= 2
      ? history[history.length - 1].overallScore > history[0].overallScore
        ? 'improving'
        : history[history.length - 1].overallScore < history[0].overallScore
        ? 'declining'
        : 'stable'
      : 'stable';

    return {
      pool,
      currentScore,
      history,
      trend,
      factors: {
        assetVolatilityBps: currentScore.assetVolatilityScore,
        oracleDeviationBps: currentScore.oracleDeviationScore,
        poolUtilizationBps: currentScore.poolUtilizationScore,
        liquidationHistoryBps: currentScore.liquidationHistoryScore,
      },
    };
  }

  async getScoreDistribution(): Promise<Record<string, number>> {
    const scores = await this.getAllPoolScores();
    const distribution: Record<string, number> = {};
    for (const s of scores) {
      distribution[s.letterGrade] = (distribution[s.letterGrade] || 0) + 1;
    }
    return distribution;
  }

  async getDefaultWeights(): Promise<RiskWeights> {
    return {
      assetVolatilityWeight: 3000,
      oracleDeviationWeight: 2500,
      poolUtilizationWeight: 2500,
      liquidationHistoryWeight: 2000,
    };
  }

  async getRiskAlerts(severity?: string): Promise<RiskAlert[]> {
    this.refreshScores();
    const active = this.alerts.filter((a) => !a.acknowledged);
    return severity ? active.filter((a) => a.severity === severity) : active;
  }

  acknowledgeAlert(alertId: string): RiskAlert | null {
    const alert = this.alerts.find((a) => a.id === alertId);
    if (!alert) return null;
    alert.acknowledged = true;
    return alert;
  }

  async getAnalytics(): Promise<RiskAnalytics> {
    const scores = await this.getAllPoolScores();
    const distribution = await this.getScoreDistribution();
    const alerts = await this.getRiskAlerts();

    const averageScore = scores.length > 0
      ? Math.round(scores.reduce((sum, s) => sum + s.overallScore, 0) / scores.length)
      : 0;

    const trendSummary = { improving: 0, stable: 0, declining: 0 };
    for (const [pool] of this.mockScores) {
      const profile = await this.getPoolRiskProfile(pool);
      if (profile) trendSummary[profile.trend]++;
    }

    return {
      totalPools: scores.length,
      averageScore,
      distribution,
      alertsActive: alerts.length,
      poolsAtRisk: scores.filter((s) => s.overallScore < DEFAULT_THRESHOLDS.high).length,
      trendSummary,
    };
  }
}

export const riskScoringService = new RiskScoringService();