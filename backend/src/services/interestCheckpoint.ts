export interface CheckpointWindow {
  poolId: string;
  startBlock: bigint;
  endBlock: bigint;
  borrowRatePerBlock: bigint;
  priorBorrowIndex: bigint;
}

export interface CheckpointResult {
  poolId: string;
  endBlock: bigint;
  newBorrowIndex: bigint;
  accruedInterestUsd: number;
}

export class InterestCheckpointService {
  public calculateWindowCheckpoint(window: CheckpointWindow, totalBorrowsUsd: number): CheckpointResult {
    if (window.endBlock <= window.startBlock) {
      throw new Error('Invalid block window: endBlock must exceed startBlock');
    }

    const deltaBlocks = window.endBlock - window.startBlock;
    const factorScaling = 1000000000n;
    const interestFactor = (deltaBlocks * window.borrowRatePerBlock) / factorScaling;
    const newBorrowIndex = window.priorBorrowIndex + ((window.priorBorrowIndex * interestFactor) / factorScaling);

    const accruedInterestUsd = totalBorrowsUsd * (Number(interestFactor) / Number(factorScaling));

    return {
      poolId: window.poolId,
      endBlock: window.endBlock,
      newBorrowIndex,
      accruedInterestUsd,
    };
  }
}
