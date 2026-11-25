export interface QueueItem {
  userAddress: string;
  assetAddress: string;
  amount: bigint;
  unlockTimestamp: number;
}

export class RedemptionQueueService {
  private queue: Map<string, QueueItem> = new Map();

  public enqueue(userAddress: string, assetAddress: string, amount: bigint, cooldownSeconds: number): QueueItem {
    if (amount <= 0n) throw new Error('Amount must be positive');
    const key = `${userAddress}:${assetAddress}`;
    const item: QueueItem = {
      userAddress,
      assetAddress,
      amount,
      unlockTimestamp: Date.now() + cooldownSeconds * 1000,
    };
    this.queue.set(key, item);
    return item;
  }

  public canRedeem(userAddress: string, assetAddress: string): boolean {
    const key = `${userAddress}:${assetAddress}`;
    const item = this.queue.get(key);
    if (!item) return false;
    return Date.now() >= item.unlockTimestamp;
  }

  public getQueueItem(userAddress: string, assetAddress: string): QueueItem | undefined {
    return this.queue.get(`${userAddress}:${assetAddress}`);
  }
}
