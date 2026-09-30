import { PoolManager } from '../pool/manager';
import { StablecoinConfig } from '../config/automation';
import { TransactionBuilder } from '../transaction/builder';
import { Logger } from '../utils/logger';

interface RebalanceResult {
  executed: boolean;
  poolsAdjusted: string[];
  totalValueTransferred: number;
  error?: string;
}

export class StablecoinReserveBalancer {
  private readonly logger: Logger;
  private readonly poolManager: PoolManager;
  private readonly config: StablecoinConfig;

  constructor(
    poolManager: PoolManager,
    config: StablecoinConfig,
    logger: Logger = new Logger('StablecoinBalancer')
  ) {
    this.poolManager = poolManager;
    this.config = config;
    this.logger = logger;
  }

  public async checkAndRebalance(): Promise<RebalanceResult> {
    const pools = await this.poolManager.getAllPools();
    const stablecoinPools = pools.filter(p => p.isStablecoin());
    const deviations = this.calculatePegDeviations(stablecoinPools);

    if (deviations.length === 0) {
      this.logger.info('No significant peg deviations detected');
      return { executed: false, poolsAdjusted: [], totalValueTransferred: 0 };
    }

    const arbitrageOps = this.calculateArbitrageOps(deviations);
    if (arbitrageOps.length === 0) {
      this.logger.warning('No viable arbitrage opportunities found');
      return { executed: false, poolsAdjusted: [], totalValueTransferred: 0 };
    }

    return this.executeRebalancing(arbitrageOps);
  }

  private calculatePegDeviations(pools: Pool[]): { poolId: string; deviation: number }[] {
    return pools
      .map(pool => {
        const currentPrice = pool.getCurrentPrice();
        const pegDeviation = Math.abs(currentPrice - this.config.targetPegPrice);
        return { poolId: pool.id, deviation: pegDeviation };
      })
      .filter(({ deviation }) => deviation > this.config.thresholdPercentage)
      .sort((a, b) => b.deviation - a.deviation);
  }

  private calculateArbitrageOps(deviations: { poolId: string; deviation: number }[]): {
    sourcePoolId: string;
    targetPoolId: string;
    amount: number;
    expectedValue: number;
  }[] {
    // Implementation uses existing pool math utilities
    // Returns sorted list of most profitable arbitrage opportunities
    return []; // Placeholder for actual implementation
  }

  private async executeRebalancing(ops: any[]): Promise<RebalanceResult> {
    const txBuilder = new TransactionBuilder();
    const adjustedPools: string[] = [];
    let totalTransferred = 0;

    for (const op of ops) {
      try {
        const tx = txBuilder.buildStablecoinTransfer(
          op.sourcePoolId,
          op.targetPoolId,
          op.amount
        );

        const result = await this.poolManager.executeTransaction(tx);
        if (result.success) {
          adjustedPools.push(op.sourcePoolId, op.targetPoolId);
          totalTransferred += op.expectedValue;
        }
      } catch (error) {
        this.logger.error(`Failed to execute transfer: ${error}`);
        return { executed: false, poolsAdjusted: adjustedPools, totalValueTransferred: totalTransferred, error: error.message };
      }
    }

    return { executed: true, poolsAdjusted: [...new Set(adjustedPools)], totalValueTransferred: totalTransferred };
  }
}