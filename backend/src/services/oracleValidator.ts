export interface OraclePriceRecord {
  assetSymbol: string;
  priceUsd: number;
  publishedAt: number;
}

export class OracleStalenessValidator {
  private defaultMaxStalenessMs: number;

  constructor(maxStalenessSeconds: number = 300) {
    this.defaultMaxStalenessMs = maxStalenessSeconds * 1000;
  }

  public validatePriceRecord(record: OraclePriceRecord, customStalenessMs?: number): number {
    if (record.priceUsd <= 0) {
      throw new Error(`Invalid non-positive oracle price for ${record.assetSymbol}: ${record.priceUsd}`);
    }

    const now = Date.now();
    if (record.publishedAt > now + 30000) {
      throw new Error(`Oracle price timestamp is in the future for ${record.assetSymbol}`);
    }

    const allowedAge = customStalenessMs || this.defaultMaxStalenessMs;
    const age = now - record.publishedAt;

    if (age > allowedAge) {
      throw new Error(`Oracle price for ${record.assetSymbol} is stale (age: ${Math.round(age / 1000)}s, max: ${allowedAge / 1000}s)`);
    }

    return record.priceUsd;
  }
}
