import { describe, it, expect, beforeEach } from 'vitest';
import { RedemptionQueueService } from '../redemptionQueue';

describe('RedemptionQueueService', () => {
  let service: RedemptionQueueService;
  let now: number;

  beforeEach(() => {
    now = 1_700_000_000_000;
    service = new RedemptionQueueService({
      nowMilliseconds: () => now,
      nowSeconds: () => Math.floor(now / 1000),
      nowDate: () => new Date(now),
    });
  });

  it('enqueues and respects unlock duration', () => {
    const user = 'GUSER123';
    const asset = 'C_ILLIQUID_BOND';
    const item = service.enqueue(user, asset, 5000n, 100);

    expect(item.amount).toBe(5000n);
    expect(service.canRedeem(user, asset)).toBe(false);
  });

  it('allows redemption after cooldown elapsed', () => {
    const user = 'GUSER123';
    const asset = 'C_ILLIQUID_BOND';
    service.enqueue(user, asset, 5000n, 100);
    now += 100_000;

    expect(service.canRedeem(user, asset)).toBe(true);
  });
});
