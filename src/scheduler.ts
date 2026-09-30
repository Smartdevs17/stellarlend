import { StablecoinReserveBalancer } from './automation/balancer';
import { PoolManager } from './pool/manager';
import { stablecoinConfig } from './config/automation';
import { Logger } from './utils/logger';

class AutomationScheduler {
  private balancer: StablecoinReserveBalancer;
  private logger: Logger;

  constructor() {
    const poolManager = new PoolManager();
    this.balancer = new StablecoinReserveBalancer(poolManager, stablecoinConfig);
    this.logger = new Logger('AutomationScheduler');
  }

  public scheduleStablecoinRebalancing() {
    setInterval(async () => {
      try {
        const result = await this.balancer.checkAndRebalance();
        if (result.executed) {
          this.logger.info(`Rebalancing executed. Adjusted ${result.poolsAdjusted.length} pools. Total: $${result.totalValueTransferred.toLocaleString()}`);
        }
      } catch (error) {
        this.logger.error(`Rebalancing failed: ${error}`);
      }
    }, stablecoinConfig.checkIntervalMinutes * 60 * 1000);
  }
}

export { AutomationScheduler };
