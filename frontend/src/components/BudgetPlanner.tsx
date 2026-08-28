import React, { useState } from "react";

export interface Pool {
  poolId: string;
  apyBps: number;
  riskGrade: string;
  capacity: number;
  weightBps: number;
}

export interface BudgetPlannerProps {
  pools: Pool[];
  apiBaseUrl?: string;
}

export interface PlanStep {
  order: number;
  poolId: string;
  amount: number;
}

export interface RiskAssessment {
  portfolioRiskScore: number;
  safetyRating: "Low" | "Moderate" | "Elevated" | "High";
  diversificationScore: number;
  hhi: number;
  maxDrawdownEstimate: number;
  warnings: string[];
}

export interface BudgetPlan {
  projectedBalance: number;
  totalProjectedReturn: number;
  goalOnTrack: boolean;
  steps: PlanStep[];
  riskAssessment?: RiskAssessment;
  rebalance?: { from: string; to: string; reason: string }[];
}

export interface YieldProjectionData {
  baseProjectedReturn: number;
  baseProjectedBalance: number;
  effectiveAnnualApyBps: number;
  netReturnAfterFees: number;
  horizons: {
    horizonDays: number;
    label: string;
    simpleReturn: number;
    compoundedReturn: number;
    projectedBalance: number;
  }[];
  scenarios: {
    scenario: "conservative" | "base" | "optimistic";
    projectedReturn: number;
    projectedBalance: number;
    effectiveApyBps: number;
  }[];
}

export function BudgetPlanner({
  pools: initialPools,
  apiBaseUrl = "/api",
}: BudgetPlannerProps) {
  const [capital, setCapital] = useState(10_000);
  const [horizonDays, setHorizonDays] = useState(365);
  const [goalAmount, setGoalAmount] = useState<number | undefined>(11_000);
  const [compounding, setCompounding] = useState<"simple" | "daily" | "monthly" | "annually">("monthly");
  const [pools, setPools] = useState<Pool[]>(initialPools);
  const [plan, setPlan] = useState<BudgetPlan | null>(null);
  const [projections, setProjections] = useState<YieldProjectionData | null>(null);
  const [isOptimizing, setIsOptimizing] = useState(false);
  const [activeTab, setActiveTab] = useState<"planner" | "calculator" | "tracking" | "alerts">("planner");
  const [alertThreshold, setAlertThreshold] = useState<number>(300);
  const [alertSuccess, setAlertSuccess] = useState(false);
  const [planSaved, setPlanSaved] = useState(false);

  function updateWeight(poolId: string, weightBps: number) {
    setPools((current) =>
      current.map((pool) =>
        pool.poolId === poolId ? { ...pool, weightBps } : pool,
      ),
    );
  }

  async function project() {
    try {
      const response = await fetch(`${apiBaseUrl}/planner/budget`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          capital,
          horizonDays,
          goalAmount,
          rebalanceThresholdBps: 100,
          maxRiskExposureBps: Object.fromEntries(
            pools.map((p) => [p.riskGrade, 10_000]),
          ),
          pools,
          compoundingFrequency: compounding,
        }),
      });
      const result = (await response.json()) as { data?: BudgetPlan };
      setPlan(result.data ?? null);

      // Also calculate detailed yield projections
      const projRes = await fetch(`${apiBaseUrl}/planner/yield-projections`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          capital,
          horizonDays,
          compoundingFrequency: compounding,
          pools,
        }),
      });
      const projData = (await projRes.json()) as { data?: YieldProjectionData };
      if (projData.data) {
        setProjections(projData.data);
      }
    } catch (err) {
      console.error("Failed to build budget plan:", err);
    }
  }

  async function optimize(strategy: "max_yield" | "min_risk" | "balanced") {
    setIsOptimizing(true);
    try {
      const response = await fetch(`${apiBaseUrl}/planner/optimize`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          capital,
          horizonDays,
          strategy,
          pools,
        }),
      });
      const result = await response.json();
      if (result.success && result.data?.optimizedAllocations) {
        setPools(result.data.optimizedAllocations);
        // Automatically re-project
        setTimeout(() => void project(), 50);
      }
    } catch (err) {
      console.error("Optimization failed:", err);
    } finally {
      setIsOptimizing(false);
    }
  }

  async function savePlan() {
    try {
      const response = await fetch(`${apiBaseUrl}/planner/plans`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          userAddress: "current",
          name: `Budget Plan ${new Date().toLocaleDateString()}`,
          capital,
          horizonDays,
          goalAmount,
          pools,
        }),
      });
      if (response.ok) {
        setPlanSaved(true);
        setTimeout(() => setPlanSaved(false), 3000);
      }
    } catch (err) {
      console.error("Save plan failed:", err);
    }
  }

  async function configureBudgetAlert() {
    try {
      const response = await fetch(`${apiBaseUrl}/planner/alerts`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          userAddress: "current",
          alert: {
            type: "yield_drop",
            threshold: alertThreshold,
            enabled: true,
          },
        }),
      });
      if (response.ok) {
        setAlertSuccess(true);
        setTimeout(() => setAlertSuccess(false), 3000);
      }
    } catch (err) {
      console.error("Alert configuration failed:", err);
    }
  }

  const totalWeight = pools.reduce((sum, p) => sum + p.weightBps, 0);

  return (
    <section aria-labelledby="planner-title" className="space-y-6 rounded-2xl border bg-white p-6 shadow-sm">
      <div className="flex flex-wrap items-center justify-between gap-4 border-b pb-4">
        <div>
          <h2 id="planner-title" className="text-2xl font-bold text-gray-900">
            Lending Protocol Budget Planner
          </h2>
          <p className="text-sm text-gray-500">
            Plan capital allocations, project compounded yields, assess portfolio risk, and optimize returns.
          </p>
        </div>
        <div className="flex gap-2">
          {(["planner", "calculator", "tracking", "alerts"] as const).map((tab) => (
            <button
              key={tab}
              onClick={() => setActiveTab(tab)}
              className={`rounded-lg px-3 py-1.5 text-sm font-medium capitalize transition-colors ${
                activeTab === tab
                  ? "bg-blue-600 text-white"
                  : "bg-gray-100 text-gray-700 hover:bg-gray-200"
              }`}
            >
              {tab}
            </button>
          ))}
        </div>
      </div>

      {activeTab === "planner" && (
        <div className="space-y-6">
          {/* Capital and Horizon Controls */}
          <div className="grid grid-cols-1 gap-4 md:grid-cols-3">
            <label className="block rounded-lg border p-3">
              <span className="text-xs font-semibold uppercase tracking-wider text-gray-500">Capital (USD)</span>
              <input
                type="number"
                min={1}
                value={capital}
                onChange={(e) => setCapital(Number(e.target.value))}
                className="mt-1 w-full rounded border px-3 py-2 text-lg font-semibold"
              />
            </label>
            <label className="block rounded-lg border p-3">
              <span className="text-xs font-semibold uppercase tracking-wider text-gray-500">Horizon (Days)</span>
              <select
                value={horizonDays}
                onChange={(e) => setHorizonDays(Number(e.target.value))}
                className="mt-1 w-full rounded border px-3 py-2 text-lg font-semibold"
              >
                <option value={30}>30 Days (1 Month)</option>
                <option value={90}>90 Days (1 Quarter)</option>
                <option value={180}>180 Days (Half Year)</option>
                <option value={365}>365 Days (1 Year)</option>
                <option value={730}>730 Days (2 Years)</option>
              </select>
            </label>
            <label className="block rounded-lg border p-3">
              <span className="text-xs font-semibold uppercase tracking-wider text-gray-500">Target Goal ($)</span>
              <input
                type="number"
                min={capital}
                value={goalAmount ?? ""}
                onChange={(e) => setGoalAmount(e.target.value ? Number(e.target.value) : undefined)}
                className="mt-1 w-full rounded border px-3 py-2 text-lg font-semibold"
              />
            </label>
          </div>

          {/* Budget Optimization Presets */}
          <div className="rounded-xl bg-blue-50/60 p-4 border border-blue-100">
            <div className="flex flex-wrap items-center justify-between gap-3">
              <div>
                <h4 className="font-semibold text-blue-900">Budget Optimization Engine</h4>
                <p className="text-xs text-blue-700">Auto-balance weights across pools based on your strategy</p>
              </div>
              <div className="flex gap-2">
                <button
                  type="button"
                  disabled={isOptimizing}
                  onClick={() => void optimize("max_yield")}
                  className="rounded-md bg-blue-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-blue-700"
                >
                  Max Yield
                </button>
                <button
                  type="button"
                  disabled={isOptimizing}
                  onClick={() => void optimize("balanced")}
                  className="rounded-md bg-indigo-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-indigo-700"
                >
                  Balanced (Sharpe)
                </button>
                <button
                  type="button"
                  disabled={isOptimizing}
                  onClick={() => void optimize("min_risk")}
                  className="rounded-md bg-emerald-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-emerald-700"
                >
                  Min Risk
                </button>
              </div>
            </div>
          </div>

          {/* Pool Allocations */}
          <div className="space-y-3">
            <div className="flex items-center justify-between">
              <h3 className="font-semibold text-gray-800">Pool Allocations ({pools.length})</h3>
              <span className={`text-xs font-medium ${totalWeight === 10_000 ? "text-green-600" : "text-amber-600"}`}>
                Total: {(totalWeight / 100).toFixed(0)}% / 100%
              </span>
            </div>
            {pools.map((pool) => (
              <div key={pool.poolId} className="flex flex-col gap-2 rounded-lg border p-3 hover:border-gray-300">
                <div className="flex items-center justify-between">
                  <div className="flex items-center gap-2">
                    <span className="font-medium text-gray-900">{pool.poolId}</span>
                    <span className="rounded bg-gray-100 px-2 py-0.5 text-xs font-semibold text-gray-600">
                      Grade {pool.riskGrade}
                    </span>
                    <span className="text-xs text-emerald-600 font-semibold">
                      {(pool.apyBps / 100).toFixed(2)}% APY
                    </span>
                  </div>
                  <span className="text-sm font-semibold text-gray-700">
                    {(pool.weightBps / 100).toFixed(1)}% (${((capital * pool.weightBps) / 10_000).toLocaleString()})
                  </span>
                </div>
                <input
                  aria-label={`${pool.poolId} allocation`}
                  type="range"
                  min={0}
                  max={10_000}
                  step={100}
                  value={pool.weightBps}
                  onChange={(event) => updateWeight(pool.poolId, Number(event.target.value))}
                  className="w-full cursor-pointer accent-blue-600"
                />
              </div>
            ))}
          </div>

          {/* Action Buttons */}
          <div className="flex flex-wrap gap-3">
            <button
              type="button"
              onClick={() => void project()}
              className="rounded-lg bg-blue-600 px-6 py-2.5 font-medium text-white shadow-sm hover:bg-blue-700"
            >
              Build plan
            </button>
            <button
              type="button"
              onClick={() => void savePlan()}
              className="rounded-lg border border-gray-300 px-4 py-2.5 font-medium text-gray-700 hover:bg-gray-50"
            >
              {planSaved ? "Plan Saved!" : "Save Plan"}
            </button>
          </div>

          {/* Plan Results */}
          {plan && (
            <div aria-live="polite" className="space-y-4 rounded-xl border bg-gray-50/50 p-5">
              <h4 className="font-bold text-gray-900">Projected Plan Outcomes</h4>
              <div className="grid grid-cols-1 gap-4 sm:grid-cols-3">
                <div className="rounded-lg border bg-white p-3">
                  <span className="text-xs text-gray-500">Projected Balance</span>
                  <p className="text-xl font-bold text-gray-900">${plan.projectedBalance.toFixed(2)}</p>
                </div>
                <div className="rounded-lg border bg-white p-3">
                  <span className="text-xs text-gray-500">Projected Return</span>
                  <p className="text-xl font-bold text-emerald-600">+${plan.totalProjectedReturn.toFixed(2)}</p>
                </div>
                <div className="rounded-lg border bg-white p-3">
                  <span className="text-xs text-gray-500">Goal Status</span>
                  <p className={`text-lg font-semibold ${plan.goalOnTrack ? "text-green-600" : "text-amber-600"}`}>
                    {plan.goalOnTrack ? "Goal on track" : "Goal needs adjustment"}
                  </p>
                </div>
              </div>

              {/* Risk Assessment Summary */}
              {plan.riskAssessment && (
                <div className="rounded-lg border bg-white p-4">
                  <h5 className="mb-2 text-sm font-semibold text-gray-800">Risk Assessment Overview</h5>
                  <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
                    <div>
                      <span className="text-xs text-gray-500">Portfolio Risk</span>
                      <p className="font-bold text-gray-800">{plan.riskAssessment.portfolioRiskScore} / 100</p>
                    </div>
                    <div>
                      <span className="text-xs text-gray-500">Safety Rating</span>
                      <p className="font-bold text-indigo-600">{plan.riskAssessment.safetyRating}</p>
                    </div>
                    <div>
                      <span className="text-xs text-gray-500">Diversification</span>
                      <p className="font-bold text-emerald-600">{plan.riskAssessment.diversificationScore} / 100</p>
                    </div>
                    <div>
                      <span className="text-xs text-gray-500">Stress Max Drawdown</span>
                      <p className="font-bold text-red-500">-${plan.riskAssessment.maxDrawdownEstimate}</p>
                    </div>
                  </div>
                  {plan.riskAssessment.warnings.length > 0 && (
                    <div className="mt-3 space-y-1 rounded bg-amber-50 p-2 text-xs text-amber-800">
                      {plan.riskAssessment.warnings.map((w, i) => (
                        <p key={i}>* {w}</p>
                      ))}
                    </div>
                  )}
                </div>
              )}

              {/* Execution Steps */}
              {plan.steps.length > 0 && (
                <div>
                  <h5 className="mb-2 text-sm font-semibold text-gray-800">Sequential Deployment Steps</h5>
                  <ol className="list-decimal pl-5 space-y-1 text-sm text-gray-700">
                    {plan.steps.map((step) => (
                      <li key={`${step.poolId}-${step.order}`}>
                        Deposit ${step.amount.toLocaleString()} into {step.poolId}
                      </li>
                    ))}
                  </ol>
                </div>
              )}
            </div>
          )}
        </div>
      )}

      {/* Yield Projection Calculator Tab */}
      {activeTab === "calculator" && (
        <div className="space-y-6">
          <div className="flex flex-wrap items-center justify-between gap-4">
            <div>
              <h3 className="text-lg font-bold text-gray-900">Yield Projection Calculator</h3>
              <p className="text-xs text-gray-500">Model compound interest dynamics across frequencies and market scenarios.</p>
            </div>
            <div className="flex items-center gap-2">
              <span className="text-xs font-semibold text-gray-500">Compounding:</span>
              {(["simple", "daily", "monthly", "annually"] as const).map((freq) => (
                <button
                  key={freq}
                  onClick={() => {
                    setCompounding(freq);
                    void project();
                  }}
                  className={`rounded px-2.5 py-1 text-xs font-medium capitalize ${
                    compounding === freq ? "bg-blue-600 text-white" : "bg-gray-100 text-gray-700"
                  }`}
                >
                  {freq}
                </button>
              ))}
            </div>
          </div>

          {projections && (
            <div className="space-y-6">
              {/* Scenarios */}
              <div className="grid grid-cols-1 gap-4 md:grid-cols-3">
                {projections.scenarios.map((s) => (
                  <div
                    key={s.scenario}
                    className={`rounded-xl border p-4 ${
                      s.scenario === "base"
                        ? "border-blue-200 bg-blue-50/40"
                        : "border-gray-200 bg-white"
                    }`}
                  >
                    <div className="flex items-center justify-between">
                      <span className="text-xs font-bold uppercase tracking-wider text-gray-600">
                        {s.scenario}
                      </span>
                      <span className="text-xs font-semibold text-gray-500">
                        {(s.effectiveApyBps / 100).toFixed(2)}% APY
                      </span>
                    </div>
                    <p className="mt-2 text-2xl font-bold text-gray-900">
                      ${s.projectedBalance.toFixed(2)}
                    </p>
                    <p className="text-xs text-emerald-600 font-semibold">
                      +${s.projectedReturn.toFixed(2)} yield
                    </p>
                  </div>
                ))}
              </div>

              {/* Time Horizon Curve Table */}
              <div className="overflow-x-auto rounded-xl border bg-white">
                <table className="w-full text-left text-sm text-gray-600">
                  <thead className="border-b bg-gray-50 text-xs font-semibold uppercase text-gray-500">
                    <tr>
                      <th className="px-4 py-3">Time Horizon</th>
                      <th className="px-4 py-3">Simple Return</th>
                      <th className="px-4 py-3">Compounded Return</th>
                      <th className="px-4 py-3">Projected Total</th>
                    </tr>
                  </thead>
                  <tbody className="divide-y">
                    {projections.horizons.map((h) => (
                      <tr key={h.horizonDays} className="hover:bg-gray-50/80">
                        <td className="px-4 py-3 font-medium text-gray-900">{h.label}</td>
                        <td className="px-4 py-3 text-gray-600">${h.simpleReturn.toFixed(2)}</td>
                        <td className="px-4 py-3 font-semibold text-emerald-600">${h.compoundedReturn.toFixed(2)}</td>
                        <td className="px-4 py-3 font-bold text-gray-900">${h.projectedBalance.toFixed(2)}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </div>
          )}
        </div>
      )}

      {/* Tracking Tab */}
      {activeTab === "tracking" && (
        <div className="space-y-4">
          <h3 className="text-lg font-bold text-gray-900">Budget Tracking & Variance</h3>
          <p className="text-xs text-gray-500">Compare projected lending returns with actual realized protocol yield.</p>
          <div className="rounded-xl border bg-white p-5 space-y-3">
            <div className="flex items-center justify-between border-b pb-3">
              <div>
                <p className="font-semibold text-gray-800">Active Lending Allocation</p>
                <span className="text-xs text-gray-500">Principal: ${capital.toLocaleString()}</span>
              </div>
              <span className="rounded-full bg-emerald-100 px-3 py-1 text-xs font-bold text-emerald-800">
                On Track
              </span>
            </div>
            <div className="grid grid-cols-3 gap-4 pt-2">
              <div>
                <span className="text-xs text-gray-500">Target Projected Return</span>
                <p className="font-bold text-gray-800">${(plan?.totalProjectedReturn ?? 0).toFixed(2)}</p>
              </div>
              <div>
                <span className="text-xs text-gray-500">Actual Realized</span>
                <p className="font-bold text-emerald-600">$0.00</p>
              </div>
              <div>
                <span className="text-xs text-gray-500">Variance Drift</span>
                <p className="font-bold text-gray-600">0 bps (0.0%)</p>
              </div>
            </div>
          </div>
        </div>
      )}

      {/* Alerts Tab */}
      {activeTab === "alerts" && (
        <div className="space-y-4">
          <h3 className="text-lg font-bold text-gray-900">Budget Alerts Configuration</h3>
          <p className="text-xs text-gray-500">Receive notifications when pool conditions or allocations breach your tolerances.</p>
          <div className="rounded-xl border bg-white p-5 space-y-4">
            <div>
              <label className="block text-sm font-medium text-gray-700">
                Yield Drop Notification Threshold (Basis Points)
              </label>
              <p className="text-xs text-gray-500 mb-2">Alert when any pool APY drops by more than this amount</p>
              <input
                type="number"
                value={alertThreshold}
                onChange={(e) => setAlertThreshold(Number(e.target.value))}
                className="rounded border px-3 py-2 w-48 text-sm"
              />
            </div>
            <button
              type="button"
              onClick={() => void configureBudgetAlert()}
              className="rounded-lg bg-blue-600 px-4 py-2 text-sm font-medium text-white hover:bg-blue-700"
            >
              {alertSuccess ? "Alert Configured!" : "Save Alert Rule"}
            </button>
          </div>
        </div>
      )}
    </section>
  );
}
