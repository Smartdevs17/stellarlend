import {
  aprToApy,
  apyToApr,
  aprToContinuousApy,
  apyToContinuousApr,
  ratePerLedgerToAnnual,
  computeHistoricalReturns,
  PoolSnapshot,
} from '../services/analytics/pool-performance';

describe('Pool Performance APY/APR Calculations & Historical Returns (Issue #735)', () => {
  test('aprToApy and apyToApr are inverses with daily compounding', () => {
    const originalApr = 0.08; // 8% APR
    const apy = aprToApy(originalApr, 365);
    expect(apy).toBeGreaterThan(originalApr); // Compounding increases effective yield

    const recoveredApr = apyToApr(apy, 365);
    expect(recoveredApr).toBeCloseTo(originalApr, 6);
  });

  test('continuous compounding conversion works accurately', () => {
    const apr = 0.10; // 10%
    const contApy = aprToContinuousApy(apr); // e^0.10 - 1 ≈ 0.10517
    expect(contApy).toBeCloseTo(0.10517, 4);

    const recoveredApr = apyToContinuousApr(contApy);
    expect(recoveredApr).toBeCloseTo(apr, 6);
  });

  test('ratePerLedgerToAnnual converts Stellar ledger rate to annual APR and APY', () => {
    // 1e-8 per ledger (approx 5 sec)
    const ratePerLedger = 0.00000001;
    const { apr, apy } = ratePerLedgerToAnnual(ratePerLedger, 6_307_200);
    expect(apr).toBeCloseTo(0.063072, 4);
    expect(apy).toBeGreaterThan(apr);
  });

  test('computeHistoricalReturns computes cumulative return, annualized return, volatility, and Sharpe ratio', () => {
    const snapshots: PoolSnapshot[] = [];
    const now = Date.now();
    for (let i = 0; i < 30; i++) {
      snapshots.push({
        poolAddress: 'pool_xlm_test',
        timestamp: new Date(now - (30 - i) * 86400000).toISOString(),
        tvl: 10_000_000,
        utilizationRate: 0.70,
        borrowApy: 0.08,
        supplyApy: 0.05 + (i % 5) * 0.002, // 5.0% to 5.8%
        badDebt: 0,
        totalDeposits: 10_000_000,
        totalBorrows: 7_000_000,
      });
    }

    const returns = computeHistoricalReturns('pool_xlm_test', snapshots);
    expect(returns.sampleCount).toBe(30);
    expect(returns.cumulativeReturn).toBeGreaterThan(0);
    expect(returns.annualizedReturn).toBeGreaterThan(0);
    expect(returns.volatility).toBeGreaterThanOrEqual(0);
    expect(returns.sharpeRatio).toBeDefined();
    expect(returns.maxDrawdown).toBeGreaterThanOrEqual(0);
  });
});
