/**
 * Contract Gas Analytics Dashboard — Issue #684
 *
 * Visualizes the benchmark-driven gas report from /api/analytics/gas/contract:
 * budget utilization per operation type, per-function costs vs budget and
 * baseline, optimization recommendations, historical trend and journey gas.
 */

import React, { useEffect, useMemo, useState } from 'react';
import { AlertTriangle, Fuel, TrendingDown, TrendingUp } from 'lucide-react';

type OperationType = 'read' | 'admin' | 'user_write' | 'liquidation' | 'flash_loan' | 'batch';
type Status = 'ok' | 'near' | 'over' | 'unbudgeted';

interface FunctionRow {
  key: string;
  operationType: OperationType;
  cpuInstructions: number;
  memoryBytes: number;
  budget: number | null;
  utilizationPct: number | null;
  status: Status;
  changePct: number | null;
}

interface GasReport {
  generatedAt: string;
  source: string;
  thresholds: { regressionPct: number; nearBudgetPct: number };
  summary: {
    functions: number;
    measurements: number;
    overBudget: number;
    nearBudget: number;
    regressions: number;
    improvements: number;
  };
  functions: FunctionRow[];
  byOperationType: Record<
    OperationType,
    { measurements: number; maxCpu: number; avgCpu: number; budget: number | null; maxUtilizationPct: number | null; overBudget: number }
  >;
  recommendations: Array<{ key: string; severity: 'critical' | 'high' | 'medium' | 'low'; rule: string; message: string }>;
  trends: Array<{ timestamp: string; avgInstructions: number; maxInstructions: number }>;
  journeys: Array<{ name: string; totalCpu: number; overBudgetSteps: string[] }>;
}

const STATUS_COLOR: Record<Status, string> = {
  ok: 'bg-green-500',
  near: 'bg-amber-500',
  over: 'bg-red-500',
  unbudgeted: 'bg-gray-400',
};

const SEVERITY_STYLE: Record<string, string> = {
  critical: 'bg-red-100 text-red-800',
  high: 'bg-orange-100 text-orange-800',
  medium: 'bg-amber-100 text-amber-800',
  low: 'bg-gray-100 text-gray-700',
};

const fmt = (n: number | null) => (n === null ? '—' : n.toLocaleString());

const UtilizationBar: React.FC<{ pct: number | null; status: Status }> = ({ pct, status }) => (
  <div className="flex items-center gap-2">
    <div className="flex-1 bg-gray-100 rounded h-2 min-w-[6rem]">
      <div className={`${STATUS_COLOR[status]} h-2 rounded`} style={{ width: `${Math.min(100, pct ?? 0)}%` }} />
    </div>
    <span className="w-12 text-right tabular-nums text-xs">{pct === null ? '—' : `${pct}%`}</span>
  </div>
);

export const GasAnalyticsDashboard: React.FC<{ apiBaseUrl?: string }> = ({ apiBaseUrl = '/api' }) => {
  const [report, setReport] = useState<GasReport | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [typeFilter, setTypeFilter] = useState<OperationType | 'all'>('all');

  useEffect(() => {
    fetch(`${apiBaseUrl}/analytics/gas/contract`)
      .then((res) => {
        if (!res.ok) throw new Error('Failed to load gas report');
        return res.json();
      })
      .then(setReport)
      .catch((err) => setError(err instanceof Error ? err.message : 'Unknown error'));
  }, [apiBaseUrl]);

  const rows = useMemo(
    () =>
      (report?.functions ?? [])
        .filter((f) => typeFilter === 'all' || f.operationType === typeFilter)
        .sort((a, b) => (b.utilizationPct ?? -1) - (a.utilizationPct ?? -1)),
    [report, typeFilter]
  );

  if (error) {
    return (
      <div className="bg-red-50 border border-red-200 rounded-lg p-4">
        <p className="text-red-700">{error}</p>
      </div>
    );
  }
  if (!report) {
    return <div className="bg-white rounded-lg shadow p-6 animate-pulse h-64" />;
  }

  const s = report.summary;
  const types = (Object.keys(report.byOperationType) as OperationType[]).filter(
    (t) => report.byOperationType[t].measurements > 0
  );
  const trend = report.trends;
  const maxTrend = Math.max(1, ...trend.map((t) => t.maxInstructions));

  return (
    <div className="bg-white rounded-lg shadow-lg p-6 space-y-6">
      <div className="flex items-center gap-3">
        <Fuel className="w-6 h-6 text-blue-600" />
        <div>
          <h3 className="text-xl font-bold text-gray-900">Contract Gas Report</h3>
          <p className="text-xs text-gray-500">
            {report.source} · {new Date(report.generatedAt).toLocaleString()}
          </p>
        </div>
      </div>

      <div className="grid grid-cols-2 md:grid-cols-5 gap-4">
        {[
          { label: 'Functions', value: `${s.functions} (${s.measurements} scenarios)` },
          { label: 'Over budget', value: s.overBudget, tone: s.overBudget ? 'bg-red-50' : 'bg-gray-50' },
          { label: `Near budget (≥${report.thresholds.nearBudgetPct}%)`, value: s.nearBudget, tone: s.nearBudget ? 'bg-amber-50' : 'bg-gray-50' },
          { label: `Regressions (>${report.thresholds.regressionPct}%)`, value: s.regressions, icon: TrendingUp, tone: s.regressions ? 'bg-red-50' : 'bg-gray-50' },
          { label: 'Improvements', value: s.improvements, icon: TrendingDown },
        ].map(({ label, value, tone = 'bg-gray-50', icon: Icon }) => (
          <div key={label} className={`${tone} rounded-lg p-4`}>
            <div className="flex items-center gap-1 text-xs text-gray-500">
              {Icon && <Icon className="w-3 h-3" />} {label}
            </div>
            <div className="mt-1 text-lg font-semibold text-gray-900">{value}</div>
          </div>
        ))}
      </div>

      <section>
        <h4 className="text-sm font-semibold text-gray-700 mb-2">Budget by operation type (max utilization)</h4>
        <div className="space-y-2">
          {types.map((t) => {
            const o = report.byOperationType[t];
            const status: Status = o.maxUtilizationPct === null ? 'unbudgeted' : o.maxUtilizationPct > 100 ? 'over' : o.maxUtilizationPct >= report.thresholds.nearBudgetPct ? 'near' : 'ok';
            return (
              <button key={t} onClick={() => setTypeFilter(typeFilter === t ? 'all' : t)} className="w-full grid grid-cols-[8rem_1fr_12rem] items-center gap-3 text-sm text-left">
                <span className={`font-medium ${typeFilter === t ? 'text-blue-700' : 'text-gray-700'}`}>{t}</span>
                <UtilizationBar pct={o.maxUtilizationPct} status={status} />
                <span className="text-xs text-gray-500 tabular-nums">
                  max {fmt(o.maxCpu)} / {fmt(o.budget)}
                </span>
              </button>
            );
          })}
        </div>
      </section>

      <section>
        <div className="flex items-center justify-between mb-2">
          <h4 className="text-sm font-semibold text-gray-700">
            Functions {typeFilter !== 'all' && <span className="text-blue-700">· {typeFilter}</span>}
          </h4>
          {typeFilter !== 'all' && (
            <button onClick={() => setTypeFilter('all')} className="text-xs text-blue-700 hover:underline">
              show all
            </button>
          )}
        </div>
        <div className="max-h-96 overflow-y-auto">
          <table className="w-full text-sm">
            <thead className="sticky top-0 bg-white">
              <tr className="text-left text-gray-500 border-b">
                <th className="py-1">Function</th>
                <th className="text-right">CPU</th>
                <th className="text-right">Budget</th>
                <th className="pl-4">Utilization</th>
                <th className="text-right">Δ baseline</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((f) => (
                <tr key={f.key} className="border-b last:border-0">
                  <td className="py-1 font-mono text-xs">{f.key}</td>
                  <td className="text-right tabular-nums">{fmt(f.cpuInstructions)}</td>
                  <td className="text-right tabular-nums text-gray-500">{fmt(f.budget)}</td>
                  <td className="pl-4">
                    <UtilizationBar pct={f.utilizationPct} status={f.status} />
                  </td>
                  <td
                    className={`text-right tabular-nums ${
                      (f.changePct ?? 0) > report.thresholds.regressionPct
                        ? 'text-red-700'
                        : (f.changePct ?? 0) < 0
                          ? 'text-green-700'
                          : 'text-gray-500'
                    }`}
                  >
                    {f.changePct === null ? '—' : `${f.changePct > 0 ? '+' : ''}${f.changePct}%`}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </section>

      {report.recommendations.length > 0 && (
        <section>
          <h4 className="flex items-center gap-1 text-sm font-semibold text-gray-700 mb-2">
            <AlertTriangle className="w-4 h-4" /> Optimization recommendations
          </h4>
          <ul className="space-y-2">
            {report.recommendations.map((r) => (
              <li key={`${r.rule}:${r.key}`} className="text-sm">
                <span className={`inline-block px-2 py-0.5 rounded text-xs font-medium mr-2 ${SEVERITY_STYLE[r.severity]}`}>
                  {r.severity}
                </span>
                <code className="text-xs">{r.key}</code> — {r.message}
              </li>
            ))}
          </ul>
        </section>
      )}

      <div className="grid md:grid-cols-2 gap-6">
        {trend.length > 1 && (
          <section>
            <h4 className="text-sm font-semibold text-gray-700 mb-2">Historical trend (avg / max CPU)</h4>
            <svg viewBox={`0 0 ${(trend.length - 1) * 20} 100`} className="w-full h-28" preserveAspectRatio="none">
              {(['maxInstructions', 'avgInstructions'] as const).map((k) => (
                <polyline
                  key={k}
                  fill="none"
                  strokeWidth={2}
                  className={k === 'maxInstructions' ? 'stroke-red-400' : 'stroke-blue-500'}
                  points={trend.map((t, i) => `${i * 20},${100 - (t[k] / maxTrend) * 95}`).join(' ')}
                />
              ))}
            </svg>
          </section>
        )}
        {report.journeys.length > 0 && (
          <section>
            <h4 className="text-sm font-semibold text-gray-700 mb-2">User journeys</h4>
            <table className="w-full text-sm">
              <tbody>
                {report.journeys.map((j) => (
                  <tr key={j.name} className="border-b last:border-0">
                    <td className="py-1">{j.name}</td>
                    <td className="text-right tabular-nums">{fmt(j.totalCpu)}</td>
                    <td className="text-right text-xs text-red-700">{j.overBudgetSteps.join(', ')}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </section>
        )}
      </div>
    </div>
  );
};

export default GasAnalyticsDashboard;
