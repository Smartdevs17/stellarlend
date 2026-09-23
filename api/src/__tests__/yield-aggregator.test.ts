import { yieldAggregatorService } from '../services/yield-aggregator.service';

describe('Yield Aggregator Service & Best-Rate Router (Issue #733)', () => {
  test('getAllPools returns candidate pools filtered by asset', () => {
    const usdcPools = yieldAggregatorService.getAllPools('USDC');
    expect(usdcPools.length).toBeGreaterThanOrEqual(3);
    for (const p of usdcPools) {
      expect(p.asset).toBe('USDC');
      expect(p.netApy).toBeGreaterThan(0);
    }
  });

  test('findBestRateRoute allocates 100% to top pool for small deposits', () => {
    const route = yieldAggregatorService.findBestRateRoute('USDC', 1_000, 'highest_yield');
    expect(route.asset).toBe('USDC');
    expect(route.depositAmount).toBe(1_000);
    expect(route.allocations.length).toBe(1);
    expect(route.allocations[0]!.allocationPercent).toBe(100);
    expect(route.blendedApy).toBeGreaterThan(0.08);
  });

  test('findBestRateRoute performs split routing for larger deposits to prevent dilution', () => {
    const route = yieldAggregatorService.findBestRateRoute('USDC', 500_000, 'highest_yield', 3);
    expect(route.allocations.length).toBeGreaterThan(1);
    const sumAlloc = route.allocations.reduce((s, a) => s + a.allocatedAmount, 0);
    expect(sumAlloc).toBeCloseTo(500_000, 0);
    expect(route.projectedAnnualEarnings).toBeGreaterThan(0);
  });

  test('findBestRateRoute adapts to balanced_risk strategy', () => {
    const highYieldRoute = yieldAggregatorService.findBestRateRoute('USDC', 10_000, 'highest_yield');
    const balancedRoute = yieldAggregatorService.findBestRateRoute('USDC', 10_000, 'balanced_risk');
    expect(balancedRoute).toBeDefined();
    expect(balancedRoute.strategy).toBe('balanced_risk');
  });

  test('comparePools produces side-by-side metric comparison', () => {
    const comparison = yieldAggregatorService.comparePools(undefined, 'XLM');
    expect(comparison.length).toBeGreaterThanOrEqual(2);
    for (const c of comparison) {
      expect(c.asset).toBe('XLM');
      expect(c.riskAdjustedScore).toBeGreaterThan(0);
      expect(c.maxCapacityWithoutSlippage).toBeGreaterThan(0);
    }
  });

  test('getYieldAnalytics provides historical data points and rate curve', () => {
    const pool = yieldAggregatorService.getAllPools()[0]!;
    const analytics = yieldAggregatorService.getYieldAnalytics(pool.poolId);
    expect(analytics.pool.poolId).toBe(pool.poolId);
    expect(analytics.history.length).toBe(30);
    expect(analytics.utilizationCurve.length).toBeGreaterThan(3);
    expect(analytics.summary.sharpeRatio).toBeDefined();
  });

  test('Yield alerts can be created, evaluated, and deleted', () => {
    const alert = yieldAggregatorService.createAlert('user_123', 'USDC', 0.05, 'above');
    expect(alert.id).toBeDefined();
    expect(alert.userId).toBe('user_123');

    const userAlerts = yieldAggregatorService.getUserAlerts('user_123');
    expect(userAlerts.length).toBe(1);

    const triggered = yieldAggregatorService.checkAlerts();
    expect(Array.isArray(triggered)).toBe(true);
    // USDC pools have >5% net APY, so alert should trigger
    const found = triggered.find((t) => t.alert.id === alert.id);
    expect(found).toBeDefined();

    const deleted = yieldAggregatorService.deleteAlert(alert.id);
    expect(deleted).toBe(true);
    expect(yieldAggregatorService.getUserAlerts('user_123').length).toBe(0);
  });
});
