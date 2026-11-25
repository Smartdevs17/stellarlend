import { describe, it, expect, beforeEach } from 'vitest';
import { CrossPoolMigrationService } from '../migration';

describe('CrossPoolMigrationService', () => {
  let service: CrossPoolMigrationService;

  beforeEach(() => {
    service = new CrossPoolMigrationService();
  });

  it('accurately calculates atomic swap out and protocol fees', () => {
    const result = service.simulateMigration({
      borrowerAddress: 'GABROWER123',
      sourcePoolId: 'POOL_A',
      destinationPoolId: 'POOL_B',
      collateralAsset: 'XLM',
      destinationAsset: 'USDC',
      amount: 1000000n,
      maxSlippageBps: 50,
    });

    expect(result.canMigrate).toBe(true);
    expect(result.protocolFee).toBe(1000n);
    expect(result.projectedAmountOut).toBe(999000n);
    expect(result.postMigrationHealthFactor).toBeGreaterThan(1.0);
  });

  it('rejects identical source and destination pools', () => {
    const result = service.simulateMigration({
      borrowerAddress: 'GABROWER123',
      sourcePoolId: 'POOL_A',
      destinationPoolId: 'POOL_A',
      collateralAsset: 'XLM',
      destinationAsset: 'XLM',
      amount: 1000000n,
      maxSlippageBps: 50,
    });

    expect(result.canMigrate).toBe(false);
    expect(result.reason).toContain('distinct');
  });
});
