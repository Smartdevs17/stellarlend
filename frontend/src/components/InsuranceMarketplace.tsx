import React, { useState, useEffect } from "react";

interface Policy {
  id: string;
  providerId: string;
  coverageAmount: number;
  premiumBps: number;
  durationDays: number;
  terms: string;
  coveredTriggers: string[];
  exclusions: string[];
}

interface PremiumQuote {
  basePremiumBps: number;
  riskAdjustedPremiumBps: number;
  riskMultiplier: number;
  estimatedPremium: number;
}

interface InsuranceAnalytics {
  totalProviders: number;
  activePolicies: number;
  totalCoverages: number;
  totalPremiumsCollected: number;
  totalCoverageIssued: number;
  avgPremiumBps: number;
}

interface InsuranceMarketplaceProps {
  policies: Policy[];
  lender: string;
  positionId: string;
  riskScore?: number;
  apiBaseUrl?: string;
}

interface ClaimSummary {
  submitted: number;
  approved: number;
  denied: number;
  disputed: number;
}

export function InsuranceClaimsDashboard({ totals }: { totals: ClaimSummary }) {
  return (
    <section aria-labelledby="claims-title" className="rounded-xl border p-4">
      <h2 id="claims-title" className="text-xl font-semibold">
        Claims dashboard
      </h2>
      <dl className="mt-3 grid grid-cols-2 gap-3 sm:grid-cols-4">
        {Object.entries(totals).map(([status, count]) => (
          <div key={status} className="rounded-lg bg-slate-50 p-3">
            <dt className="capitalize text-slate-600">{status}</dt>
            <dd className="text-2xl font-semibold">{count}</dd>
          </div>
        ))}
      </dl>
    </section>
  );
}

export function InsuranceAnalyticsDashboard({ apiBaseUrl = "/api" }: { apiBaseUrl?: string }) {
  const [analytics, setAnalytics] = useState<InsuranceAnalytics | null>(null);

  useEffect(() => {
    fetch(`${apiBaseUrl}/insurance/analytics`)
      .then((r) => r.json())
      .then((d) => { if (d.success) setAnalytics(d.data); })
      .catch(() => {});
  }, [apiBaseUrl]);

  if (!analytics) return null;

  return (
    <section aria-labelledby="analytics-title" className="rounded-xl border p-4">
      <h2 id="analytics-title" className="text-xl font-semibold">Insurance Analytics</h2>
      <dl className="mt-3 grid grid-cols-2 gap-3 sm:grid-cols-3">
        <div className="rounded-lg bg-slate-50 p-3">
          <dt className="text-slate-600">Providers</dt>
          <dd className="text-2xl font-semibold">{analytics.totalProviders}</dd>
        </div>
        <div className="rounded-lg bg-slate-50 p-3">
          <dt className="text-slate-600">Active Policies</dt>
          <dd className="text-2xl font-semibold">{analytics.activePolicies}</dd>
        </div>
        <div className="rounded-lg bg-slate-50 p-3">
          <dt className="text-slate-600">Coverages Issued</dt>
          <dd className="text-2xl font-semibold">{analytics.totalCoverages}</dd>
        </div>
        <div className="rounded-lg bg-slate-50 p-3">
          <dt className="text-slate-600">Premiums Collected</dt>
          <dd className="text-2xl font-semibold">{analytics.totalPremiumsCollected.toLocaleString()}</dd>
        </div>
        <div className="rounded-lg bg-slate-50 p-3">
          <dt className="text-slate-600">Coverage Issued</dt>
          <dd className="text-2xl font-semibold">{analytics.totalCoverageIssued.toLocaleString()}</dd>
        </div>
        <div className="rounded-lg bg-slate-50 p-3">
          <dt className="text-slate-600">Avg Premium</dt>
          <dd className="text-2xl font-semibold">{(analytics.avgPremiumBps / 100).toFixed(2)}%</dd>
        </div>
      </dl>
    </section>
  );
}

export function InsuranceMarketplace({
  policies,
  lender,
  positionId,
  riskScore = 750,
  apiBaseUrl = "/api",
}: InsuranceMarketplaceProps) {
  const [message, setMessage] = useState("");
  const [premiums, setPremiums] = useState<Record<string, PremiumQuote>>({});

  useEffect(() => {
    for (const policy of policies) {
      fetch(`${apiBaseUrl}/insurance/premium/calculate`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ policyId: policy.id, riskScore }),
      })
        .then((r) => r.json())
        .then((d) => { if (d.success) setPremiums((prev) => ({ ...prev, [policy.id]: d.data })); })
        .catch(() => {});
    }
  }, [policies, riskScore, apiBaseUrl]);

  async function purchase(policy: Policy) {
    setMessage("Purchasing coverage…");
    const quote = premiums[policy.id];
    const response = await fetch(`${apiBaseUrl}/insurance/coverages`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        policyId: policy.id,
        lender,
        positionId,
        coverageAmount: policy.coverageAmount,
      }),
    });
    const result = await response.json();
    setMessage(
      response.ok
        ? `Coverage active. Premium: ${quote?.estimatedPremium ?? (policy.coverageAmount * policy.premiumBps) / 10_000}`
        : (result.error ?? "Purchase failed"),
    );
  }
  return (
    <section aria-labelledby="insurance-title" className="space-y-4">
      <h2 id="insurance-title" className="text-xl font-semibold">
        Deposit insurance
      </h2>
      <p aria-live="polite" className="text-sm text-slate-600">
        {message}
      </p>
      <div className="grid gap-4 md:grid-cols-2">
        {policies.map((policy) => (
          <article key={policy.id} className="rounded-xl border p-4">
            <h3 className="font-semibold">
              Coverage {policy.coverageAmount.toLocaleString()}
            </h3>
            <p>
              {policy.durationDays} days ·{" "}
              {(policy.premiumBps / 100).toFixed(2)}% base premium
            </p>
            {premiums[policy.id] && (
              <p className="text-sm font-medium text-blue-700">
                Risk-adjusted: {(premiums[policy.id]!.riskAdjustedPremiumBps / 100).toFixed(2)}%
                ({premiums[policy.id]!.riskMultiplier}x) · Est. {premiums[policy.id]!.estimatedPremium.toLocaleString()}
              </p>
            )}
            <p className="mt-2 text-sm">
              Covered: {policy.coveredTriggers.join(", ")}
            </p>
            <p className="text-sm text-slate-500">
              Excluded: {policy.exclusions.join(", ") || "None"}
            </p>
            <details className="my-3 text-sm">
              <summary>Policy terms</summary>
              {policy.terms}
            </details>
            <button
              type="button"
              onClick={() => void purchase(policy)}
              className="rounded bg-blue-600 px-4 py-2 text-white"
            >
              Protect position
            </button>
          </article>
        ))}
      </div>
    </section>
  );
}
