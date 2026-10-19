export interface TierRule {
  name: string;
  minDeposits: number;
  minBorrowVolume: number;
  minAccountDays: number;
  minLoyalDays: number;
  discountBps: number;
  loyaltyBonusBps: number;
}

export interface UserFeeMetrics {
  totalDeposits: number;
  borrowingVolume: number;
  accountAgeDays: number;
  daysSinceWithdrawal: number;
}

export interface FeeAnalytics {
  totalSavingsDistributed: number;
  totalTransactionsEvaluated: number;
  tierUserCounts: Record<string, number>;
  averageDiscountBps: number;
  tiersConfiguredCount: number;
  evaluatedAt: number;
}

export interface FeeTransparencyBreakdown {
  userAddress: string;
  operation: string;
  amount: number;
  baseFee: number;
  tierName: string;
  tierDiscountBps: number;
  loyaltyBonusBps: number;
  totalDiscountBps: number;
  totalDiscountAmount: number;
  netFee: number;
  effectiveDiscountRatePercent: number;
  tierComparisons: Array<{
    tierName: string;
    discountBps: number;
    estimatedFee: number;
    additionalSavings: number;
  }>;
  explanation: string;
}

const DEFAULT_TIERS: TierRule[] = [
  {
    name: 'Base',
    minDeposits: 0,
    minBorrowVolume: 0,
    minAccountDays: 0,
    minLoyalDays: 0,
    discountBps: 0,
    loyaltyBonusBps: 0,
  },
  {
    name: 'Bronze',
    minDeposits: 2_500,
    minBorrowVolume: 1_000,
    minAccountDays: 14,
    minLoyalDays: 7,
    discountBps: 500, // 5%
    loyaltyBonusBps: 50, // 0.5%
  },
  {
    name: 'Silver',
    minDeposits: 10_000,
    minBorrowVolume: 5_000,
    minAccountDays: 30,
    minLoyalDays: 14,
    discountBps: 1_000, // 10%
    loyaltyBonusBps: 100, // 1%
  },
  {
    name: 'Gold',
    minDeposits: 50_000,
    minBorrowVolume: 25_000,
    minAccountDays: 90,
    minLoyalDays: 30,
    discountBps: 2_500, // 25%
    loyaltyBonusBps: 250, // 2.5%
  },
  {
    name: 'Platinum',
    minDeposits: 250_000,
    minBorrowVolume: 100_000,
    minAccountDays: 180,
    minLoyalDays: 90,
    discountBps: 4_000, // 40%
    loyaltyBonusBps: 500, // 5%
  },
  {
    name: 'VIP Diamond',
    minDeposits: 1_000_000,
    minBorrowVolume: 500_000,
    minAccountDays: 365,
    minLoyalDays: 180,
    discountBps: 5_000, // 50%
    loyaltyBonusBps: 500, // 5%
  },
];

class FeeTierService {
  private tiers: TierRule[] = [...DEFAULT_TIERS];
  private savings = new Map<string, number>();
  private userTierMap = new Map<string, string>();
  private totalEvaluations = 0;
  private totalDiscountsBpsSum = 0;

  getTiers(): TierRule[] {
    return [...this.tiers];
  }

  configure(tiers: TierRule[]): TierRule[] {
    if (!tiers.length || tiers.some((tier) => tier.discountBps < 0 || tier.discountBps > 5_000)) {
      throw new Error('Discount must be between 0 and 5000 bps');
    }
    this.tiers = [...tiers].sort((a, b) => a.discountBps - b.discountBps);
    return this.tiers;
  }

  status(userAddress: string, metrics: UserFeeMetrics) {
    const eligible = this.tiers.filter(
      (tier) =>
        metrics.totalDeposits >= tier.minDeposits &&
        metrics.borrowingVolume >= tier.minBorrowVolume &&
        metrics.accountAgeDays >= tier.minAccountDays &&
        metrics.daysSinceWithdrawal >= tier.minLoyalDays
    );
    const current = eligible[eligible.length - 1] ?? this.tiers[0]!;
    const next = this.tiers.find((tier) => tier.discountBps > current.discountBps);
    const progress = (value: number, target: number) =>
      target === 0 ? 1 : Math.min(1, value / target);

    this.userTierMap.set(userAddress, current.name);

    return {
      userAddress,
      current,
      next,
      progress: next
        ? {
            deposits: progress(metrics.totalDeposits, next.minDeposits),
            borrowing: progress(metrics.borrowingVolume, next.minBorrowVolume),
            accountAge: progress(metrics.accountAgeDays, next.minAccountDays),
            loyalty: progress(metrics.daysSinceWithdrawal, next.minLoyalDays),
          }
        : null,
      totalSavings: this.savings.get(userAddress) ?? 0,
      evaluatedAt: Date.now(),
      effectiveAt: Date.now() + 7 * 86_400_000,
    };
  }

  apply(userAddress: string, baseFee: number, metrics: UserFeeMetrics, minimumFee: number = 0) {
    const status = this.status(userAddress, metrics);
    
    // Loyalty discount mechanism: add loyalty bonus bps if account has no recent withdrawals
    const loyaltyBonus = metrics.daysSinceWithdrawal >= status.current.minLoyalDays ? status.current.loyaltyBonusBps : 0;
    const totalDiscountBps = Math.min(5_000, status.current.discountBps + loyaltyBonus);

    const discounted = Math.max(minimumFee, baseFee * (1 - totalDiscountBps / 10_000));
    const saved = Math.max(0, baseFee - discounted);

    this.savings.set(userAddress, (this.savings.get(userAddress) ?? 0) + saved);
    this.totalEvaluations++;
    this.totalDiscountsBpsSum += totalDiscountBps;

    return {
      fee: Math.round(discounted * 100) / 100,
      saved: Math.round(saved * 100) / 100,
      discountBps: totalDiscountBps,
      tierDiscountBps: status.current.discountBps,
      loyaltyBonusBps: loyaltyBonus,
      tier: status.current.name,
    };
  }

  /**
   * Transparently compute and explain fee calculation with cross-tier comparison.
   */
  getTransparency(
    userAddress: string,
    operation: string,
    amount: number,
    baseFeePercent: number,
    metrics: UserFeeMetrics
  ): FeeTransparencyBreakdown {
    const status = this.status(userAddress, metrics);
    const baseFee = amount * (baseFeePercent / 100);
    const loyaltyBonus = metrics.daysSinceWithdrawal >= status.current.minLoyalDays ? status.current.loyaltyBonusBps : 0;
    const totalDiscountBps = Math.min(5_000, status.current.discountBps + loyaltyBonus);
    const netFee = Math.max(0, baseFee * (1 - totalDiscountBps / 10_000));
    const totalDiscountAmount = baseFee - netFee;

    const tierComparisons = this.tiers.map((t) => {
      const tBonus = metrics.daysSinceWithdrawal >= t.minLoyalDays ? t.loyaltyBonusBps : 0;
      const tTotalDiscountBps = Math.min(5_000, t.discountBps + tBonus);
      const estFee = baseFee * (1 - tTotalDiscountBps / 10_000);
      return {
        tierName: t.name,
        discountBps: tTotalDiscountBps,
        estimatedFee: Math.round(estFee * 100) / 100,
        additionalSavings: Math.max(0, Math.round((netFee - estFee) * 100) / 100),
      };
    });

    const explanation = `User qualifies for ${status.current.name} tier (${(status.current.discountBps / 100).toFixed(1)}% base discount) plus ${(loyaltyBonus / 100).toFixed(2)}% loyalty bonus for ${metrics.daysSinceWithdrawal} days since last withdrawal. Total fee reduced from $${baseFee.toFixed(2)} to $${netFee.toFixed(2)}.`;

    return {
      userAddress,
      operation,
      amount,
      baseFee: Math.round(baseFee * 100) / 100,
      tierName: status.current.name,
      tierDiscountBps: status.current.discountBps,
      loyaltyBonusBps: loyaltyBonus,
      totalDiscountBps,
      totalDiscountAmount: Math.round(totalDiscountAmount * 100) / 100,
      netFee: Math.round(netFee * 100) / 100,
      effectiveDiscountRatePercent: Math.round((totalDiscountBps / 100) * 100) / 100,
      tierComparisons,
      explanation,
    };
  }

  /**
   * Aggregate fee tier analytics across users and protocol operations.
   */
  getAnalytics(): FeeAnalytics {
    let totalSavingsDistributed = 0;
    for (const saved of this.savings.values()) {
      totalSavingsDistributed += saved;
    }

    const tierUserCounts: Record<string, number> = {};
    for (const t of this.tiers) {
      tierUserCounts[t.name] = 0;
    }

    for (const tierName of this.userTierMap.values()) {
      tierUserCounts[tierName] = (tierUserCounts[tierName] || 0) + 1;
    }

    const averageDiscountBps =
      this.totalEvaluations > 0 ? Math.round(this.totalDiscountsBpsSum / this.totalEvaluations) : 0;

    return {
      totalSavingsDistributed: Math.round(totalSavingsDistributed * 100) / 100,
      totalTransactionsEvaluated: this.totalEvaluations,
      tierUserCounts,
      averageDiscountBps,
      tiersConfiguredCount: this.tiers.length,
      evaluatedAt: Date.now(),
    };
  }
}

export const feeTierService = new FeeTierService();
