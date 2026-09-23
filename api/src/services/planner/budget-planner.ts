export type CompoundingFrequency = 'simple' | 'daily' | 'monthly' | 'annually';

export type OptimizationStrategy = 'max_yield' | 'min_risk' | 'balanced';

export interface PoolAllocation {
  poolId: string;
  weightBps: number;
  apyBps: number;
  riskGrade: string;
  capacity: number;
  actualReturn?: number;
}

export interface BudgetPlanInput {
  capital: number;
  horizonDays: number;
  goalAmount?: number;
  rebalanceThresholdBps: number;
  maxRiskExposureBps: Record<string, number>;
  pools: PoolAllocation[];
  compoundingFrequency?: CompoundingFrequency;
}

export interface ScenarioProjection {
  scenario: 'conservative' | 'base' | 'optimistic';
  projectedReturn: number;
  projectedBalance: number;
  effectiveApyBps: number;
}

export interface HorizonProjection {
  horizonDays: number;
  label: string;
  simpleReturn: number;
  compoundedReturn: number;
  projectedBalance: number;
}

export interface YieldProjectionResult {
  capital: number;
  horizonDays: number;
  compoundingFrequency: CompoundingFrequency;
  baseProjectedReturn: number;
  baseProjectedBalance: number;
  effectiveAnnualApyBps: number;
  netReturnAfterFees: number;
  estimatedFees: number;
  horizons: HorizonProjection[];
  scenarios: ScenarioProjection[];
}

export interface RiskAssessmentResult {
  portfolioRiskScore: number; // 0 (safest) - 100 (riskiest)
  safetyRating: 'Low' | 'Moderate' | 'Elevated' | 'High';
  diversificationScore: number; // 0 - 100
  hhi: number; // 0 - 10,000
  riskGradeBreakdown: Record<string, { amount: number; percentage: number }>;
  maxDrawdownEstimate: number; // estimated loss in severe stress
  warnings: string[];
}

export interface BudgetOptimizationInput {
  capital: number;
  horizonDays?: number;
  strategy: OptimizationStrategy;
  targetYieldBps?: number;
  maxRiskExposureBps?: Record<string, number>;
  pools: PoolAllocation[];
}

export interface BudgetOptimizationResult {
  strategy: OptimizationStrategy;
  capital: number;
  optimizedAllocations: (PoolAllocation & { amount: number; projectedReturn: number })[];
  projectedReturn: number;
  projectedApyBps: number;
  riskScore: number;
  yieldImprovementBps: number;
  recommendations: string[];
}

export interface BudgetAlertConfig {
  id: string;
  userAddress: string;
  type: 'yield_drop' | 'risk_breach' | 'variance_drift' | 'rebalance_needed';
  poolId?: string;
  threshold: number; // e.g. apy drop in bps, or variance %
  enabled: boolean;
  createdAt: number;
}

export interface BudgetAlertTriggered {
  alertId: string;
  type: BudgetAlertConfig['type'];
  poolId?: string;
  message: string;
  severity: 'info' | 'warning' | 'critical';
  currentValue: number;
  thresholdValue: number;
  timestamp: number;
}

export interface TrackedBudgetPlan {
  id: string;
  userAddress: string;
  name: string;
  capital: number;
  horizonDays: number;
  goalAmount?: number;
  allocations: PoolAllocation[];
  projectedReturn: number;
  actualReturn: number;
  createdAt: number;
  updatedAt: number;
  status: 'on_track' | 'underperforming' | 'outperforming';
  varianceBps: number;
}

const RISK_GRADE_SCORES: Record<string, number> = {
  AAA: 5,
  AA: 12,
  A: 25,
  BBB: 40,
  BB: 55,
  B: 70,
  C: 85,
  D: 95,
};

// In-memory persistence for budget tracking and alerts
const trackedPlans = new Map<string, TrackedBudgetPlan>();
const budgetAlerts = new Map<string, BudgetAlertConfig[]>(); // userAddress -> alerts

export const budgetPlanner = {
  /**
   * Core budget plan builder (compatible with existing interface and tests)
   */
  build(input: BudgetPlanInput) {
    if (input.capital <= 0 || input.horizonDays <= 0) {
      throw new Error('Capital and horizon must be positive');
    }
    const weight = input.pools.reduce((sum, pool) => sum + pool.weightBps, 0);
    if (weight !== 10_000) {
      throw new Error('Pool weights must total 10000 bps');
    }

    const riskTotals: Record<string, number> = {};
    const compounding = input.compoundingFrequency ?? 'simple';

    const allocations = input.pools.map((pool) => {
      const requested = (input.capital * pool.weightBps) / 10_000;
      const amount = Math.min(requested, pool.capacity);
      riskTotals[pool.riskGrade] = (riskTotals[pool.riskGrade] ?? 0) + amount;

      const rate = pool.apyBps / 10_000;
      const t = input.horizonDays / 365;
      let projectedReturn = 0;

      if (compounding === 'daily') {
        projectedReturn = amount * (Math.pow(1 + rate / 365, 365 * t) - 1);
      } else if (compounding === 'monthly') {
        projectedReturn = amount * (Math.pow(1 + rate / 12, 12 * t) - 1);
      } else if (compounding === 'annually') {
        projectedReturn = amount * (Math.pow(1 + rate, t) - 1);
      } else {
        projectedReturn = amount * rate * t;
      }

      return {
        ...pool,
        amount,
        projectedReturn,
        variance: pool.actualReturn == null ? null : pool.actualReturn - projectedReturn,
      };
    });

    for (const [grade, amount] of Object.entries(riskTotals)) {
      const exposureBps = (amount / input.capital) * 10_000;
      if (exposureBps > (input.maxRiskExposureBps[grade] ?? 10_000)) {
        throw new Error(`Risk budget exceeded for grade ${grade}`);
      }
    }

    const highest = [...allocations].sort((a, b) => b.apyBps - a.apyBps)[0];
    const rebalance = allocations
      .filter((pool) => highest && highest.apyBps - pool.apyBps > input.rebalanceThresholdBps)
      .map((pool) => ({
        from: pool.poolId,
        to: highest!.poolId,
        reason: `APY spread ${highest!.apyBps - pool.apyBps} bps`,
      }));

    const totalProjectedReturn = allocations.reduce((sum, pool) => sum + pool.projectedReturn, 0);
    const projectedBalance = input.capital + totalProjectedReturn;
    const goalOnTrack =
      input.goalAmount == null || projectedBalance >= input.goalAmount;

    // Integrated risk assessment
    const riskAssessment = this.assessRisk(input.pools, input.capital);

    return {
      allocations,
      totalProjectedReturn,
      projectedBalance,
      goalOnTrack,
      rebalance,
      riskAssessment,
      steps: allocations
        .filter((p) => p.amount > 0)
        .map((p, index) => ({
          order: index + 1,
          action: 'deposit',
          poolId: p.poolId,
          amount: p.amount,
        })),
    };
  },

  /**
   * Yield projection calculator with compounding and multi-horizon scenarios
   */
  calculateYieldProjections(params: {
    capital: number;
    horizonDays: number;
    compoundingFrequency?: CompoundingFrequency;
    pools: PoolAllocation[];
    protocolFeeBps?: number;
  }): YieldProjectionResult {
    const { capital, horizonDays, compoundingFrequency = 'monthly', pools, protocolFeeBps = 15 } = params;
    if (capital <= 0 || horizonDays <= 0) {
      throw new Error('Capital and horizon must be positive');
    }

    const totalWeight = pools.reduce((acc, p) => acc + p.weightBps, 0);
    const effectiveApyBps = totalWeight > 0
      ? pools.reduce((acc, p) => acc + (p.apyBps * p.weightBps), 0) / totalWeight
      : 0;

    const rate = effectiveApyBps / 10_000;
    const t = horizonDays / 365;

    const calculateReturn = (r: number, days: number, freq: CompoundingFrequency): number => {
      const time = days / 365;
      switch (freq) {
        case 'daily':
          return capital * (Math.pow(1 + r / 365, 365 * time) - 1);
        case 'monthly':
          return capital * (Math.pow(1 + r / 12, 12 * time) - 1);
        case 'annually':
          return capital * (Math.pow(1 + r, time) - 1);
        case 'simple':
        default:
          return capital * r * time;
      }
    };

    const baseProjectedReturn = calculateReturn(rate, horizonDays, compoundingFrequency);
    const estimatedFees = (baseProjectedReturn * protocolFeeBps) / 10_000;
    const netReturnAfterFees = baseProjectedReturn - estimatedFees;

    const horizonList = [
      { days: 30, label: '30 Days' },
      { days: 90, label: '90 Days (Quarterly)' },
      { days: 180, label: '180 Days (Half-Year)' },
      { days: 365, label: '1 Year' },
      { days: 730, label: '2 Years' },
      { days: 1095, label: '3 Years' },
    ];

    const horizons: HorizonProjection[] = horizonList.map((h) => {
      const simpleReturn = calculateReturn(rate, h.days, 'simple');
      const compoundedReturn = calculateReturn(rate, h.days, compoundingFrequency);
      return {
        horizonDays: h.days,
        label: h.label,
        simpleReturn,
        compoundedReturn,
        projectedBalance: capital + compoundedReturn,
      };
    });

    const scenarios: ScenarioProjection[] = [
      {
        scenario: 'conservative',
        effectiveApyBps: Math.round(effectiveApyBps * 0.8),
        projectedReturn: calculateReturn(rate * 0.8, horizonDays, compoundingFrequency),
        projectedBalance: capital + calculateReturn(rate * 0.8, horizonDays, compoundingFrequency),
      },
      {
        scenario: 'base',
        effectiveApyBps: Math.round(effectiveApyBps),
        projectedReturn: baseProjectedReturn,
        projectedBalance: capital + baseProjectedReturn,
      },
      {
        scenario: 'optimistic',
        effectiveApyBps: Math.round(effectiveApyBps * 1.25),
        projectedReturn: calculateReturn(rate * 1.25, horizonDays, compoundingFrequency),
        projectedBalance: capital + calculateReturn(rate * 1.25, horizonDays, compoundingFrequency),
      },
    ];

    return {
      capital,
      horizonDays,
      compoundingFrequency,
      baseProjectedReturn,
      baseProjectedBalance: capital + baseProjectedReturn,
      effectiveAnnualApyBps: Math.round(effectiveApyBps),
      netReturnAfterFees,
      estimatedFees,
      horizons,
      scenarios,
    };
  },

  /**
   * Risk assessment integration: evaluates concentration, risk grade weights, and drawdown
   */
  assessRisk(pools: PoolAllocation[], capital: number): RiskAssessmentResult {
    const totalWeight = pools.reduce((acc, p) => acc + p.weightBps, 0);
    const normalizedPools = totalWeight > 0 ? pools : [];

    let weightedRiskSum = 0;
    let hhiSum = 0;
    const breakdown: Record<string, { amount: number; percentage: number }> = {};
    const warnings: string[] = [];

    for (const pool of normalizedPools) {
      const share = totalWeight > 0 ? pool.weightBps / totalWeight : 0;
      const poolAmount = capital * share;
      const riskScore = RISK_GRADE_SCORES[pool.riskGrade.toUpperCase()] ?? 50;

      weightedRiskSum += riskScore * share;
      hhiSum += Math.pow(share * 100, 2);

      const gradeKey = pool.riskGrade.toUpperCase();
      if (!breakdown[gradeKey]) {
        breakdown[gradeKey] = { amount: 0, percentage: 0 };
      }
      breakdown[gradeKey].amount += poolAmount;
      breakdown[gradeKey].percentage += share * 100;
    }

    const portfolioRiskScore = Math.min(100, Math.max(0, Math.round(weightedRiskSum)));

    let safetyRating: RiskAssessmentResult['safetyRating'] = 'Low';
    if (portfolioRiskScore > 70) safetyRating = 'High';
    else if (portfolioRiskScore > 45) safetyRating = 'Elevated';
    else if (portfolioRiskScore > 25) safetyRating = 'Moderate';

    // Diversification score from HHI (10000 = single pool monopoly, lower = more diversified)
    const hhi = Math.round(hhiSum);
    const diversificationScore = Math.max(0, Math.min(100, Math.round(100 - (hhi / 100))));

    if (hhi > 5000) {
      warnings.push('High concentration risk: allocation is dominated by 1-2 pools.');
    }
    if ((breakdown['C']?.percentage ?? 0) + (breakdown['D']?.percentage ?? 0) > 30) {
      warnings.push('High exposure to speculative pools (Grade C/D > 30%).');
    }

    // Stress drawdown estimate: assume high risk pools suffer 30% haircut, moderate 10%
    let stressLoss = 0;
    for (const [grade, data] of Object.entries(breakdown)) {
      const score = RISK_GRADE_SCORES[grade] ?? 50;
      const haircut = (score / 100) * 0.35;
      stressLoss += data.amount * haircut;
    }

    return {
      portfolioRiskScore,
      safetyRating,
      diversificationScore,
      hhi,
      riskGradeBreakdown: breakdown,
      maxDrawdownEstimate: Math.round(stressLoss),
      warnings,
    };
  },

  /**
   * Budget optimization solver: computes optimal allocations according to strategy
   */
  optimize(input: BudgetOptimizationInput): BudgetOptimizationResult {
    const { capital, strategy, pools, maxRiskExposureBps = {} } = input;
    if (pools.length === 0) {
      throw new Error('At least one pool is required for optimization');
    }

    const optimizedPools = pools.map((p) => ({ ...p }));
    const n = pools.length;

    let targetWeights: number[] = [];

    if (strategy === 'max_yield') {
      // Sort pools by APY descending and allocate greedily respecting capacity & risk
      const sortedIndices = pools
        .map((p, idx) => ({ idx, apy: p.apyBps, risk: p.riskGrade }))
        .sort((a, b) => b.apy - a.apy);

      let remainingBps = 10_000;
      const weights = new Array(n).fill(0);
      const gradeAllocated: Record<string, number> = {};

      for (const item of sortedIndices) {
        const pool = pools[item.idx];
        const gradeCapBps = maxRiskExposureBps[pool.riskGrade] ?? 10_000;
        const currentGradeBps = gradeAllocated[pool.riskGrade] ?? 0;
        const capacityBps = Math.min(10_000, Math.floor((pool.capacity / capital) * 10_000));

        const maxPossible = Math.min(remainingBps, capacityBps, gradeCapBps - currentGradeBps);
        const assign = Math.max(0, maxPossible);

        weights[item.idx] = assign;
        remainingBps -= assign;
        gradeAllocated[pool.riskGrade] = currentGradeBps + assign;
      }

      // If any remainder, distribute across available capacity
      if (remainingBps > 0) {
        for (let i = 0; i < n && remainingBps > 0; i++) {
          weights[i] += remainingBps;
          remainingBps = 0;
        }
      }
      targetWeights = weights;
    } else if (strategy === 'min_risk') {
      // Weight inversely proportional to risk score
      const invRisks = pools.map((p) => {
        const score = RISK_GRADE_SCORES[p.riskGrade.toUpperCase()] ?? 50;
        return 100 / Math.max(1, score);
      });
      const sumInv = invRisks.reduce((a, b) => a + b, 0);
      targetWeights = invRisks.map((inv) => Math.round((inv / sumInv) * 10_000));
    } else {
      // Balanced: Sharpe-style ratio = APY / RiskScore
      const scores = pools.map((p) => {
        const risk = RISK_GRADE_SCORES[p.riskGrade.toUpperCase()] ?? 50;
        return p.apyBps / Math.max(5, risk);
      });
      const sumScores = scores.reduce((a, b) => a + b, 0);
      targetWeights = scores.map((s) => Math.round((s / sumScores) * 10_000));
    }

    // Ensure sum equals 10,000 bps
    const sumW = targetWeights.reduce((a, b) => a + b, 0);
    const diff = 10_000 - sumW;
    if (targetWeights.length > 0) {
      targetWeights[0] += diff;
    }

    const optimizedAllocations = optimizedPools.map((p, idx) => {
      const weightBps = targetWeights[idx] ?? 0;
      const amount = (capital * weightBps) / 10_000;
      const projectedReturn = amount * (p.apyBps / 10_000);
      return {
        ...p,
        weightBps,
        amount,
        projectedReturn,
      };
    });

    const projectedReturn = optimizedAllocations.reduce((sum, p) => sum + p.projectedReturn, 0);
    const projectedApyBps = Math.round((projectedReturn / capital) * 10_000);

    const oldApyBps = pools.reduce((sum, p) => sum + (p.apyBps * p.weightBps), 0) / 10_000;
    const yieldImprovementBps = Math.round(projectedApyBps - oldApyBps);

    const riskAssessment = this.assessRisk(optimizedAllocations, capital);

    const recommendations = [
      `Strategy ${strategy.toUpperCase()} applied with expected APY of ${(projectedApyBps / 100).toFixed(2)}%.`,
      yieldImprovementBps > 0
        ? `Improves net yield by +${(yieldImprovementBps / 100).toFixed(2)}% (+${yieldImprovementBps} bps).`
        : 'Preserves balanced capital risk profile.',
      `Diversification score: ${riskAssessment.diversificationScore}/100.`,
    ];

    return {
      strategy,
      capital,
      optimizedAllocations,
      projectedReturn,
      projectedApyBps,
      riskScore: riskAssessment.portfolioRiskScore,
      yieldImprovementBps,
      recommendations,
    };
  },

  /**
   * Budget tracking: save a new budget plan
   */
  createTrackedPlan(data: {
    userAddress: string;
    name: string;
    capital: number;
    horizonDays: number;
    goalAmount?: number;
    pools: PoolAllocation[];
  }): TrackedBudgetPlan {
    const id = `plan_${Date.now()}_${Math.random().toString(36).substring(2, 7)}`;
    const built = this.build({
      capital: data.capital,
      horizonDays: data.horizonDays,
      goalAmount: data.goalAmount,
      rebalanceThresholdBps: 100,
      maxRiskExposureBps: {},
      pools: data.pools,
    });

    const plan: TrackedBudgetPlan = {
      id,
      userAddress: data.userAddress,
      name: data.name,
      capital: data.capital,
      horizonDays: data.horizonDays,
      goalAmount: data.goalAmount,
      allocations: data.pools,
      projectedReturn: built.totalProjectedReturn,
      actualReturn: 0,
      createdAt: Date.now(),
      updatedAt: Date.now(),
      status: 'on_track',
      varianceBps: 0,
    };

    trackedPlans.set(id, plan);
    return plan;
  },

  /**
   * Budget tracking: record actual performance and update variance
   */
  recordActualPerformance(planId: string, actualReturn: number): TrackedBudgetPlan {
    const plan = trackedPlans.get(planId);
    if (!plan) {
      throw new Error(`Plan with ID ${planId} not found`);
    }

    plan.actualReturn = actualReturn;
    plan.updatedAt = Date.now();

    const variance = actualReturn - plan.projectedReturn;
    plan.varianceBps = plan.projectedReturn > 0
      ? Math.round((variance / plan.projectedReturn) * 10_000)
      : 0;

    if (plan.varianceBps < -1000) {
      plan.status = 'underperforming';
    } else if (plan.varianceBps > 1000) {
      plan.status = 'outperforming';
    } else {
      plan.status = 'on_track';
    }

    trackedPlans.set(planId, plan);
    return plan;
  },

  /**
   * Budget tracking: list plans for a user
   */
  listTrackedPlans(userAddress: string): TrackedBudgetPlan[] {
    const list: TrackedBudgetPlan[] = [];
    for (const plan of trackedPlans.values()) {
      if (plan.userAddress === userAddress) {
        list.push(plan);
      }
    }
    return list;
  },

  /**
   * Budget tracking: get plan by id
   */
  getTrackedPlan(planId: string): TrackedBudgetPlan | null {
    return trackedPlans.get(planId) ?? null;
  },

  /**
   * Budget alerts: configure an alert rule
   */
  configureAlert(userAddress: string, alert: Omit<BudgetAlertConfig, 'id' | 'createdAt'>): BudgetAlertConfig {
    const id = `alert_${Date.now()}_${Math.random().toString(36).substring(2, 7)}`;
    const fullAlert: BudgetAlertConfig = {
      ...alert,
      id,
      createdAt: Date.now(),
    };

    const list = budgetAlerts.get(userAddress) ?? [];
    list.push(fullAlert);
    budgetAlerts.set(userAddress, list);
    return fullAlert;
  },

  /**
   * Budget alerts: get all alerts for user
   */
  getAlerts(userAddress: string): BudgetAlertConfig[] {
    return budgetAlerts.get(userAddress) ?? [];
  },

  /**
   * Budget alerts: evaluate active alerts against current pool conditions
   */
  evaluateAlerts(userAddress: string, currentPools: { poolId: string; currentApyBps: number; riskGrade?: string }[]): BudgetAlertTriggered[] {
    const alerts = budgetAlerts.get(userAddress) ?? [];
    const triggered: BudgetAlertTriggered[] = [];

    const poolMap = new Map(currentPools.map((p) => [p.poolId, p]));

    for (const alert of alerts) {
      if (!alert.enabled) continue;

      if (alert.type === 'yield_drop' && alert.poolId) {
        const pool = poolMap.get(alert.poolId);
        if (pool && pool.currentApyBps < alert.threshold) {
          triggered.push({
            alertId: alert.id,
            type: alert.type,
            poolId: alert.poolId,
            message: `Pool ${alert.poolId} APY dropped to ${pool.currentApyBps} bps (below threshold ${alert.threshold} bps)`,
            severity: 'warning',
            currentValue: pool.currentApyBps,
            thresholdValue: alert.threshold,
            timestamp: Date.now(),
          });
        }
      }
    }

    return triggered;
  },
};
