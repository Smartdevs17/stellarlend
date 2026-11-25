import { describe, it, expect, beforeEach } from 'vitest';
import { InterestCheckpointService, CheckpointWindow } from '../interestCheckpoint';

describe('InterestCheckpointService', () => {
  let service: InterestCheckpointService;

  beforeEach(() => {
    service = new InterestCheckpointService();
  });

  it('accurately compounds index over block windows', () => {
    const window: CheckpointWindow = {
      poolId: 'POOL_XLM',
      startBlock: 100000n,
      endBlock: 101000n, // 1,000 blocks
      borrowRatePerBlock: 50n,
      priorBorrowIndex: 1000000000n,
    };

    const result = service.calculateWindowCheckpoint(window, 50000);
    expect(result.newBorrowIndex).toBeGreaterThan(window.priorBorrowIndex);
    expect(result.accruedInterestUsd).toBeGreaterThan(0);
  });

  it('throws on invalid block window', () => {
    const invalidWindow: CheckpointWindow = {
      poolId: 'POOL_XLM',
      startBlock: 100000n,
      endBlock: 100000n,
      borrowRatePerBlock: 50n,
      priorBorrowIndex: 1000000000n,
    };

    expect(() => service.calculateWindowCheckpoint(invalidWindow, 50000)).toThrow('Invalid block window');
  });
});
