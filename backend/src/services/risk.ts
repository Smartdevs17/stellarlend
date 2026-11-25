/**
 * Risk and volatility tier haircut service (issue #1742).
 *
 * Evaluates asset volatility metrics and provides calibrated haircut percentages
 * to safeguard pool borrowing ratios.
 */

export enum VolatilityBucket {
  LOW = 'LOW',
  MEDIUM = 'MEDIUM',
  HIGH = 'HIGH',
  EXTREME = 'EXTREME',
}

export const HAIRCUT_BPS: Record<VolatilityBucket, number> = {
  [VolatilityBucket.LOW]: 500,       // 5%
  [VolatilityBucket.MEDIUM]: 1500,   // 15%
  [VolatilityBucket.HIGH]: 3000,     // 30%
  [VolatilityBucket.EXTREME]: 5000,  // 50%
};

export const BPS_DIVISOR = 10000;

export interface AssetRiskProfile {
  assetSymbol: string;
  bucket: VolatilityBucket;
  customHaircutBps?: number;
}

export class RiskService {
  private profiles = new Map<string, AssetRiskProfile>();

  public registerAsset(profile: AssetRiskProfile): void {
    this.profiles.set(profile.assetSymbol.toUpperCase(), profile);
  }

  public getHaircutBps(assetSymbol: string): number {
    const profile = this.profiles.get(assetSymbol.toUpperCase());
    if (!profile) {
      // Default unrated assets to EXTREME haircut for safety
      return HAIRCUT_BPS[VolatilityBucket.EXTREME];
    }
    return profile.customHaircutBps ?? HAIRCUT_BPS[profile.bucket];
  }

  public applyHaircut(assetSymbol: string, rawNominalValue: bigint): bigint {
    const bps = BigInt(this.getHaircutBps(assetSymbol));
    const effectiveBps = BigInt(BPS_DIVISOR) - bps;
    return (rawNominalValue * effectiveBps) / BigInt(BPS_DIVISOR);
  }
}
