import { systemClock, type Clock } from '../lib/clock';

export interface QueueItem {
  userAddress: string;
  assetAddress: string;
  amount: bigint;
  unlockTimestamp: number;
}

export class RedemptionQueueService {
  private queue: Map<string, QueueItem> = new Map();

  constructor(private readonly clock: Clock = systemClock) {}

  public enqueue(userAddress: string, assetAddress: string, amount: bigint, cooldownSeconds: number): QueueItem {
    if (amount <= 0n) throw new Error('Amount must be positive');
    const key = `${userAddress}:${assetAddress}`;
    const item: QueueItem = {
      userAddress,
      assetAddress,
      amount,
      unlockTimestamp: this.clock.nowMilliseconds() + cooldownSeconds * 1000,
    };
    this.queue.set(key, item);
    return item;
  }

  public canRedeem(userAddress: string, assetAddress: string): boolean {
    const key = `${userAddress}:${assetAddress}`;
    const item = this.queue.get(key);
    if (!item) return false;
    return this.clock.nowMilliseconds() >= item.unlockTimestamp;
  }

  public getQueueItem(userAddress: string, assetAddress: string): QueueItem | undefined {
    return this.queue.get(`${userAddress}:${assetAddress}`);
  }
}
