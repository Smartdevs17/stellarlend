import { describe, it, expect, beforeEach } from 'vitest';
import { RedemptionQueueService } from '../redemptionQueue';

describe('RedemptionQueueService', () => {
  let service: RedemptionQueueService;

  beforeEach(() => {
    service = new RedemptionQueueService();
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
    service.enqueue(user, asset, 5000n, -10); // already passed

    expect(service.canRedeem(user, asset)).toBe(true);
  });
});
