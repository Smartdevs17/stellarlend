import { insuranceService } from '../services/insurance/insurance.service';
import { budgetPlanner } from '../services/planner/budget-planner';
import { feeTierService } from '../services/fees/tier.service';
import { GasEstimatorService } from '../services/gas/estimator';
import { emergencyPauseService } from '../services/emergencyPause.service';
import { referralService } from '../services/referral.service';

describe('insurance marketplace', () => {
  it('purchases collateral-backed coverage and automatically approves a covered trigger', () => {
    const provider = insuranceService.onboardProvider({
      address: 'GPROVIDER',
      name: 'SafeCover',
      kycStatus: 'approved',
      collateral: 10_000,
    });
    const policy = insuranceService.createPolicy({
      providerId: provider.id,
      coverageAmount: 5_000,
      premiumBps: 100,
      durationDays: 30,
      terms: 'Oracle and contract risk',
      coveredTriggers: ['oracle_failure'],
      exclusions: ['user error'],
    });
    const coverage = insuranceService.purchase(policy.id, 'GLENDER', 'position-1');
    expect(coverage.premiumPaid).toBe(50);
    expect(
      insuranceService.submitClaim(coverage.id, 'oracle_failure', 'event:123', 5_000).status
    ).toBe('approved');
  });

  it('rejects insolvent coverage and allows denied claims to be disputed', () => {
    const provider = insuranceService.onboardProvider({
      address: 'GPROVIDER2',
      name: 'LimitedCover',
      kycStatus: 'approved',
      collateral: 1_000,
    });
    const policy = insuranceService.createPolicy({
      providerId: provider.id,
      coverageAmount: 1_000,
      premiumBps: 200,
      durationDays: 30,
      terms: 'Oracle risk',
      coveredTriggers: ['oracle_failure'],
      exclusions: ['contract compromise'],
    });
    const coverage = insuranceService.purchase(policy.id, 'GLENDER2', 'position-2');

    expect(() => insuranceService.purchase(policy.id, 'GLENDER3', 'position-3')).toThrow(
      'Insurer is insolvent'
    );
    const denied = insuranceService.submitClaim(
      coverage.id,
      'contract_compromise',
      'event:456',
      1_000
    );
    expect(denied.status).toBe('denied');
    expect(insuranceService.disputeClaim(denied.id).status).toBe('disputed');
  });
});

describe('budget planner (#737)', () => {
  it('projects returns and exports sequential actions', () => {
    const plan = budgetPlanner.build({
      capital: 10_000,
      horizonDays: 365,
      rebalanceThresholdBps: 100,
      maxRiskExposureBps: { A: 10_000 },
      pools: [
        { poolId: 'pool-a', weightBps: 5_000, apyBps: 500, riskGrade: 'A', capacity: 10_000 },
        { poolId: 'pool-b', weightBps: 5_000, apyBps: 300, riskGrade: 'A', capacity: 10_000 },
      ],
    });
    expect(plan.totalProjectedReturn).toBe(400);
    expect(plan.steps).toHaveLength(2);
    expect(plan.rebalance).toHaveLength(1);
    expect(plan.riskAssessment).toBeDefined();
    expect(plan.riskAssessment?.portfolioRiskScore).toBeGreaterThanOrEqual(0);
  });

  it('caps allocations at pool capacity and enforces risk budgets', () => {
    const capped = budgetPlanner.build({
      capital: 10_000,
      horizonDays: 365,
      rebalanceThresholdBps: 100,
      maxRiskExposureBps: { A: 10_000 },
      pools: [
        {
          poolId: 'pool-capped',
          weightBps: 10_000,
          apyBps: 500,
          riskGrade: 'A',
          capacity: 4_000,
        },
      ],
    });
    expect(capped.allocations[0]?.amount).toBe(4_000);

    expect(() =>
      budgetPlanner.build({
        capital: 10_000,
        horizonDays: 365,
        rebalanceThresholdBps: 100,
        maxRiskExposureBps: { C: 2_500 },
        pools: [
          {
            poolId: 'pool-risky',
            weightBps: 10_000,
            apyBps: 900,
            riskGrade: 'C',
            capacity: 10_000,
          },
        ],
      })
    ).toThrow('Risk budget exceeded for grade C');
  });

  it('calculates compound yield projections across horizons and scenarios', () => {
    const projections = budgetPlanner.calculateYieldProjections({
      capital: 10_000,
      horizonDays: 365,
      compoundingFrequency: 'monthly',
      pools: [
        { poolId: 'pool-usdc', weightBps: 10_000, apyBps: 1200, riskGrade: 'A', capacity: 50_000 },
      ],
    });

    expect(projections.horizons).toHaveLength(6);
    expect(projections.scenarios).toHaveLength(3);
    // Compounded monthly return > simple return
    const oneYearHorizon = projections.horizons.find((h) => h.horizonDays === 365);
    expect(oneYearHorizon?.compoundedReturn).toBeGreaterThan(oneYearHorizon?.simpleReturn ?? 0);
  });

  it('optimizes budget allocations using balanced Sharpe-style weighting', () => {
    const optimized = budgetPlanner.optimize({
      capital: 10_000,
      strategy: 'balanced',
      pools: [
        { poolId: 'pool-safe', weightBps: 5_000, apyBps: 600, riskGrade: 'AAA', capacity: 100_000 },
        { poolId: 'pool-risky', weightBps: 5_000, apyBps: 1200, riskGrade: 'B', capacity: 100_000 },
      ],
    });

    expect(optimized.optimizedAllocations).toHaveLength(2);
    expect(optimized.projectedReturn).toBeGreaterThan(0);
    expect(optimized.recommendations.length).toBeGreaterThan(0);
  });

  it('creates tracked plans, records actuals, and monitors alerts', () => {
    const plan = budgetPlanner.createTrackedPlan({
      userAddress: 'GLENDER_TEST',
      name: 'Test Plan',
      capital: 10_000,
      horizonDays: 365,
      pools: [
        { poolId: 'pool-1', weightBps: 10_000, apyBps: 500, riskGrade: 'A', capacity: 10_000 },
      ],
    });

    expect(plan.id).toBeDefined();
    expect(plan.projectedReturn).toBe(500);

    const updated = budgetPlanner.recordActualPerformance(plan.id, 550);
    expect(updated.actualReturn).toBe(550);
    expect(updated.status).toBe('on_track');

    // Configure alert
    const alert = budgetPlanner.configureAlert('GLENDER_TEST', {
      type: 'yield_drop',
      poolId: 'pool-1',
      threshold: 600,
      enabled: true,
    });
    expect(alert.id).toBeDefined();

    const triggered = budgetPlanner.evaluateAlerts('GLENDER_TEST', [
      { poolId: 'pool-1', currentApyBps: 450 },
    ]);
    expect(triggered).toHaveLength(1);
    expect(triggered[0].type).toBe('yield_drop');
  });
});

describe('fee tiers', () => {
  it('caps discounts and enforces the minimum fee', () => {
    const result = feeTierService.apply(
      'GUSER',
      100,
      {
        totalDeposits: 300_000,
        borrowingVolume: 150_000,
        accountAgeDays: 365,
        daysSinceWithdrawal: 120,
      },
      60
    );
    expect(result.tier).toBe('Platinum');
    expect(result.fee).toBe(60);
    expect(result.saved).toBe(40);
  });

  it('rejects governance tiers above the fifty-percent discount cap', () => {
    expect(() =>
      feeTierService.configure([
        {
          name: 'Invalid',
          minDeposits: 0,
          minBorrowVolume: 0,
          minAccountDays: 0,
          minLoyalDays: 0,
          discountBps: 5_001,
          loyaltyBonusBps: 0,
        },
      ])
    ).toThrow('Discount must be between 0 and 5000 bps');
  });
});

describe('emergency withdrawal mechanism (#740)', () => {
  it('executes emergency withdrawal with 80% reduced fee savings', () => {
    const execution = emergencyPauseService.executeEmergencyWithdrawal({
      userAddress: 'GEMERGENCY_USER',
      assetAddress: 'USDC',
      amount: 10_000,
    });

    // Standard fee: 50 bps = 50 units. Emergency fee: 10 bps = 10 units.
    expect(execution.feeAmount).toBe(10);
    expect(execution.netAmount).toBe(9_990);
    expect(execution.feeSavings).toBe(40); // 80% saved!
    expect(execution.status).toBe('confirmed');
  });

  it('enforces emergency withdrawal per-transaction limits', () => {
    emergencyPauseService.updateLimits({ maxPerTransaction: 100_000 });

    expect(() =>
      emergencyPauseService.executeEmergencyWithdrawal({
        userAddress: 'GWHALE',
        assetAddress: 'USDC',
        amount: 200_000,
      })
    ).toThrow('exceeds max emergency withdrawal limit');
  });

  it('generates emergency analytics and reporting', () => {
    const analytics = emergencyPauseService.getEmergencyAnalytics();
    expect(analytics.totalEmergencyWithdrawn).toBeGreaterThanOrEqual(10_000);
    expect(analytics.totalFeeSavingsDelivered).toBeGreaterThan(0);

    const report = emergencyPauseService.generateEmergencyReport();
    expect(report.reportId).toBeDefined();
    expect(report.analytics).toBeDefined();
    expect(report.limits).toBeDefined();
  });
});

describe('referral tracking and affiliate rewards (#739)', () => {
  it('generates unique codes, tracks multi-tier referrals, and calculates tiers', () => {
    const referrerCode = referralService.generateCode('GAFFILIATE_1');
    expect(referrerCode).toBeDefined();
    expect(referrerCode.length).toBe(8);

    const reg = referralService.register('GREFEREE_1', referrerCode);
    expect(reg.referrer).toBe('GAFFILIATE_1');

    // Accrue fee
    referralService.accrueFee('GREFEREE_1', 1_000); // 1000 protocol fee
    const stats = referralService.getStats('GAFFILIATE_1');
    expect(stats?.totalReferrals).toBe(1);
    expect(stats?.totalEarned).toBe(100); // 10% of 1000
    expect(stats?.claimable).toBe(100);
  });

  it('provides a referral leaderboard ranking affiliates', () => {
    const leaderboard = referralService.getLeaderboard(10, 'totalEarned');
    expect(leaderboard.length).toBeGreaterThan(0);
    expect(leaderboard[0].rank).toBe(1);
    expect(leaderboard[0].maskedAddress).toBeDefined();
  });

  it('allows program configuration updates and batch reward distributions', () => {
    const initialConfig = referralService.getConfig();
    expect(initialConfig.l1FeeSharePct).toBe(10);

    const updatedConfig = referralService.updateConfig({ l1FeeSharePct: 12 });
    expect(updatedConfig.l1FeeSharePct).toBe(12);

    const analytics = referralService.getGlobalAnalytics();
    expect(analytics.totalAffiliates).toBeGreaterThan(0);

    const dist = referralService.distributeRewards();
    expect(dist.distributedCount).toBeGreaterThanOrEqual(0);
  });
});

describe('gas estimator and optimization (#738)', () => {
  it('compares every supported operation from cheapest to most expensive', async () => {
    const estimator = new GasEstimatorService();
    const costs = {
      deposit: 10,
      withdraw: 20,
      borrow: 30,
      repay: 40,
      liquidation: 60,
      flash_loan: 50,
      emergency_withdraw: 25,
    } as const;

    jest.spyOn(estimator, 'getHistoricalData').mockImplementation(async (operation, period) => ({
      operation,
      averageCost: String(costs[operation]),
      minCost: String(costs[operation]),
      maxCost: String(costs[operation]),
      stdDeviation: '0',
      sampleCount: 1,
      period,
    }));

    const comparison = await estimator.compareOperations();

    expect(comparison.operations).toHaveLength(7);
    expect(comparison.operations.map(({ operation }) => operation)).toEqual([
      'deposit',
      'withdraw',
      'emergency_withdraw',
      'borrow',
      'repay',
      'flash_loan',
      'liquidation',
    ]);
  });

  it('generates gas analytics report with peak and off-peak recommendations', async () => {
    const estimator = new GasEstimatorService();
    const report = await estimator.getGasAnalytics('7d');
    expect(report.period).toBe('7d');
    expect(report.operationsRanked.length).toBeGreaterThan(0);
    expect(report.peakHours.length).toBeGreaterThan(0);
    expect(report.offPeakHours.length).toBeGreaterThan(0);
  });
});
