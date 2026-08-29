import { feeTierService } from '../services/fees/tier.service';

describe('Fee Tier System & Loyalty Discounts (Issue #736)', () => {
  test('getTiers returns configured default tiers in increasing order', () => {
    const tiers = feeTierService.getTiers();
    expect(tiers.length).toBeGreaterThanOrEqual(4);
    for (let i = 1; i < tiers.length; i++) {
      expect(tiers[i]!.discountBps).toBeGreaterThanOrEqual(tiers[i - 1]!.discountBps);
    }
  });

  test('status identifies tier based on user deposits, volume, age, and loyalty', () => {
    const status = feeTierService.status('wallet_gold', {
      totalDeposits: 60_000,
      borrowingVolume: 30_000,
      accountAgeDays: 100,
      daysSinceWithdrawal: 40,
    });
    expect(status.current.name).toBe('Gold');
    expect(status.current.discountBps).toBe(2500); // 25%
    expect(status.next).toBeDefined();
    expect(status.progress).toBeDefined();
  });

  test('apply computes discounted fee and incorporates loyalty bonus', () => {
    const result = feeTierService.apply(
      'wallet_silver_loyal',
      100, // $100 base fee
      {
        totalDeposits: 12_000,
        borrowingVolume: 6_000,
        accountAgeDays: 35,
        daysSinceWithdrawal: 20, // >= 14 loyal days => qualifies for loyalty bonus
      },
      0
    );

    // Silver discount = 1000 bps (10%), loyalty bonus = 100 bps (1%) => total 11%
    expect(result.discountBps).toBe(1100);
    expect(result.fee).toBe(89);
    expect(result.saved).toBe(11);
  });

  test('getTransparency provides itemized fee calculation and cross-tier comparisons', () => {
    const breakdown = feeTierService.getTransparency(
      'wallet_user',
      'borrow',
      10_000,
      0.5, // 0.5% base fee => $50
      {
        totalDeposits: 15_000,
        borrowingVolume: 8_000,
        accountAgeDays: 45,
        daysSinceWithdrawal: 25,
      }
    );

    expect(breakdown.baseFee).toBe(50);
    expect(breakdown.netFee).toBeLessThan(50);
    expect(breakdown.tierComparisons.length).toBeGreaterThanOrEqual(4);
    expect(breakdown.explanation).toContain('Silver');
  });

  test('getAnalytics tracks protocol-wide savings and tier distributions', () => {
    const analytics = feeTierService.getAnalytics();
    expect(analytics.totalSavingsDistributed).toBeGreaterThan(0);
    expect(analytics.tiersConfiguredCount).toBeGreaterThanOrEqual(4);
    expect(analytics.tierUserCounts).toBeDefined();
  });
});
