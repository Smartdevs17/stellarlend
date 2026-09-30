import { StablecoinReserveBalancer } from '../../src/automation/balancer';
import { PoolManager } from '../../src/pool/manager';
import { stablecoinConfig } from '../../src/config/automation';
import { MockPool } from '../mocks/pool';

describe('StablecoinReserveBalancer', () => {
  let poolManager: PoolManager;
  let balancer: StablecoinReserveBalancer;

  beforeEach(() => {
    poolManager = new PoolManager();
    balancer = new StablecoinReserveBalancer(poolManager, stablecoinConfig);
  });

  describe('checkAndRebalance', () => {
    it('should return no action when no deviations', async () => {
      const mockPools = [
        new MockPool('USDT_POOL_1', 1.005), // Within threshold
        new MockPool('USDC_POOL_2', 0.995)
      ];

      jest.spyOn(poolManager, 'getAllPools').mockResolvedValue(mockPools);
      const result = await balancer.checkAndRebalance();

      expect(result.executed).toBe(false);
      expect(result.poolsAdjusted).toEqual([]);
    });

    it('should execute rebalancing when deviations exist', async () => {
      const mockPools = [
        new MockPool('USDT_POOL_1', 1.05), // 5% over
        new MockPool('USDC_POOL_2', 0.95) // 5% under
      ];

      jest.spyOn(poolManager, 'getAllPools').mockResolvedValue(mockPools);
      jest.spyOn(poolManager, 'executeTransaction').mockResolvedValue({ success: true });

      const result = await balancer.checkAndRebalance();

      expect(result.executed).toBe(true);
      expect(result.poolsAdjusted.length).toBe(2);
    });

    it('should handle execution failure gracefully', async () => {
      const mockPools = [new MockPool('USDT_POOL_1', 1.05)];
      jest.spyOn(poolManager, 'getAllPools').mockResolvedValue(mockPools);
      jest.spyOn(poolManager, 'executeTransaction').mockRejectedValue(new Error('Test error'));

      const result = await balancer.checkAndRebalance();
      expect(result.executed).toBe(false);
      expect(result.error).toContain('Test error');
    });
  });

  describe('calculatePegDeviations', () => {
    it('should filter pools within threshold', () => {
      const pools = [
        new MockPool('POOL_1', 1.005), // Within 1%
        new MockPool('POOL_2', 1.02)  // Over threshold
      ];

      const deviations = balancer['calculatePegDeviations'](pools);
      expect(deviations.length).toBe(1);
      expect(deviations[0].poolId).toBe('POOL_2');
    });
  });
});