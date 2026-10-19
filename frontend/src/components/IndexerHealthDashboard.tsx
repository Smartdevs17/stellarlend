/**
 * Indexer Health & Lag Monitoring Dashboard — Issue #1423
 *
 * Real-time monitoring of Soroban event indexer lag, sync performance,
 * component status, and automated alerting.
 */

import React, { useCallback, useEffect, useState } from 'react';
import {
  Activity,
  AlertCircle,
  AlertTriangle,
  ArrowRight,
  Bell,
  CheckCircle2,
  Clock,
  Database,
  Flame,
  Layers,
  Play,
  RefreshCw,
  Server,
  ShieldAlert,
  ShieldCheck,
  Zap,
} from 'lucide-react';

export interface IndexerAlert {
  id: string;
  rule:
    | 'lag_warning'
    | 'lag_critical'
    | 'sync_stalled'
    | 'sync_error'
    | 'sink_failure'
    | 'indexer_stopped';
  severity: 'info' | 'warning' | 'critical';
  title: string;
  message: string;
  metric: string;
  currentValue: number | string | null;
  threshold: number | string | null;
  triggeredAt: number;
  acknowledged: boolean;
  acknowledgedAt?: number;
}

export interface SyncHistoryPoint {
  timestamp: number;
  latestNetworkLedger: number;
  latestIndexedLedger: number;
  lagLedgers: number;
  durationMs: number;
  eventsFetched: number;
  eventsIndexed: number;
  status: 'ok' | 'error';
  errorMessage?: string;
}

export interface IndexerStatus {
  running: boolean;
  syncing: boolean;
  healthStatus: 'healthy' | 'degraded' | 'unhealthy';
  schemaVersion: number;
  cursor: string | null;
  lastSyncAt: number | null;
  lastSyncDurationMs: number | null;
  lastError: string | null;
  lastErrorAt: number | null;
  totalIngested: number;
  hotEvents: number;
  hotRetentionMs: number;
  latestIndexedLedger: number;
  latestNetworkLedger: number;
  lagLedgers: number | null;
  lagSeconds: number | null;
  syncSuccessCount: number;
  syncErrorCount: number;
  consecutiveErrors: number;
  archive: {
    segments: number;
    events: number;
    oldestLedger: number | null;
    newestLedger: number | null;
  };
  sinks: Array<{ name: string; failures: number }>;
  queryLatency: {
    samples: number;
    p50Ms: number;
    p95Ms: number;
    maxMs: number;
  };
  syncHistory: SyncHistoryPoint[];
  alerts: IndexerAlert[];
}

export interface IndexerHealthDashboardProps {
  apiBaseUrl?: string;
  defaultRefreshMs?: number;
  operatorToken?: string;
}

function timeAgo(epochMs: number | null): string {
  if (!epochMs) return 'Never';
  const diffSec = Math.floor((Date.now() - epochMs) / 1000);
  if (diffSec < 2) return 'Just now';
  if (diffSec < 60) return `${diffSec}s ago`;
  const diffMin = Math.floor(diffSec / 60);
  if (diffMin < 60) return `${diffMin}m ago`;
  const diffHours = Math.floor(diffMin / 60);
  return `${diffHours}h ago`;
}

export const IndexerHealthDashboard: React.FC<IndexerHealthDashboardProps> = ({
  apiBaseUrl = '/api',
  defaultRefreshMs = 10_000,
  operatorToken,
}) => {
  const [status, setStatus] = useState<IndexerStatus | null>(null);
  const [loading, setLoading] = useState<boolean>(true);
  const [syncingNow, setSyncingNow] = useState<boolean>(false);
  const [error, setError] = useState<string | null>(null);
  const [refreshInterval, setRefreshInterval] = useState<number>(defaultRefreshMs);
  const [ackInProgress, setAckInProgress] = useState<Record<string, boolean>>({});

  const fetchStatus = useCallback(async () => {
    try {
      const res = await fetch(`${apiBaseUrl}/events/indexer/status`);
      if (!res.ok) {
        throw new Error(`Failed to load indexer status (HTTP ${res.status})`);
      }
      const data: IndexerStatus = await res.json();
      setStatus(data);
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to fetch indexer health');
    } finally {
      setLoading(false);
    }
  }, [apiBaseUrl]);

  useEffect(() => {
    fetchStatus();
    if (refreshInterval <= 0) return;
    const timer = setInterval(fetchStatus, refreshInterval);
    return () => clearInterval(timer);
  }, [fetchStatus, refreshInterval]);

  const handleManualSync = async () => {
    setSyncingNow(true);
    try {
      const headers: Record<string, string> = {
        'Content-Type': 'application/json',
      };
      if (operatorToken) {
        headers['Authorization'] = `Bearer ${operatorToken}`;
      }
      const res = await fetch(`${apiBaseUrl}/events/indexer/sync`, {
        method: 'POST',
        headers,
      });
      if (!res.ok) {
        const body = await res.json().catch(() => ({}));
        throw new Error(body?.error?.message ?? `Sync failed (HTTP ${res.status})`);
      }
      await fetchStatus();
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Manual sync failed');
    } finally {
      setSyncingNow(false);
    }
  };

  const handleAcknowledgeAlert = async (alertId: string) => {
    setAckInProgress((prev) => ({ ...prev, [alertId]: true }));
    try {
      const res = await fetch(`${apiBaseUrl}/events/indexer/alerts/${alertId}/ack`, {
        method: 'POST',
      });
      if (res.ok) {
        await fetchStatus();
      }
    } catch (err) {
      console.error('Failed to acknowledge alert', err);
    } finally {
      setAckInProgress((prev) => ({ ...prev, [alertId]: false }));
    }
  };

  if (loading && !status) {
    return (
      <div className="bg-white rounded-xl shadow-sm border border-gray-100 p-8 animate-pulse space-y-6">
        <div className="flex justify-between items-center">
          <div className="h-7 bg-gray-200 rounded w-1/4" />
          <div className="h-9 bg-gray-200 rounded w-32" />
        </div>
        <div className="grid grid-cols-1 md:grid-cols-4 gap-4">
          {[...Array(4)].map((_, i) => (
            <div key={i} className="h-28 bg-gray-100 rounded-lg" />
          ))}
        </div>
        <div className="h-64 bg-gray-100 rounded-lg" />
      </div>
    );
  }

  const lag = status?.lagLedgers ?? 0;
  const isLagCritical = lag >= 100;
  const isLagWarning = lag >= 20 && lag < 100;
  const healthBadge =
    status?.healthStatus === 'healthy'
      ? { label: 'HEALTHY', bg: 'bg-emerald-50', text: 'text-emerald-700', border: 'border-emerald-200', dot: 'bg-emerald-500' }
      : status?.healthStatus === 'degraded'
      ? { label: 'DEGRADED', bg: 'bg-amber-50', text: 'text-amber-700', border: 'border-amber-200', dot: 'bg-amber-500' }
      : { label: 'CRITICAL', bg: 'bg-rose-50', text: 'text-rose-700', border: 'border-rose-200', dot: 'bg-rose-500' };

  const history = status?.syncHistory ?? [];
  const maxLagInHistory = Math.max(10, ...(history.map((p) => p.lagLedgers) || [10]));

  return (
    <div className="bg-white rounded-xl shadow-sm border border-gray-200 p-6 space-y-6">
      {/* ── Header & Action Controls ────────────────────────────────────── */}
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4 pb-4 border-b border-gray-100">
        <div className="flex items-center gap-3">
          <div className="p-2 bg-indigo-50 text-indigo-600 rounded-lg">
            <Activity className="w-6 h-6" />
          </div>
          <div>
            <div className="flex items-center gap-2">
              <h3 className="text-xl font-bold text-gray-900">Indexer Health &amp; Lag</h3>
              <span
                className={`inline-flex items-center gap-1.5 px-2.5 py-0.5 rounded-full text-xs font-semibold border ${healthBadge.bg} ${healthBadge.text} ${healthBadge.border}`}
              >
                <span className={`w-2 h-2 rounded-full ${healthBadge.dot} animate-pulse`} />
                {healthBadge.label}
              </span>
              {status?.syncing && (
                <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-xs font-medium bg-blue-50 text-blue-700 border border-blue-200">
                  <RefreshCw className="w-3 h-3 animate-spin" />
                  Syncing RPC
                </span>
              )}
            </div>
            <p className="text-xs text-gray-500 mt-0.5">
              Live ingest tracking: Soroban RPC ➔ Hot EventStore ➔ Cold Archive
            </p>
          </div>
        </div>

        <div className="flex flex-wrap items-center gap-2">
          {/* Refresh interval selector */}
          <div className="flex items-center bg-gray-50 rounded-lg p-1 border border-gray-200 text-xs">
            <Clock className="w-3.5 h-3.5 text-gray-400 ml-1 mr-1.5" />
            {[
              { label: '5s', ms: 5000 },
              { label: '10s', ms: 10000 },
              { label: '30s', ms: 30000 },
              { label: 'Off', ms: 0 },
            ].map(({ label, ms }) => (
              <button
                key={label}
                onClick={() => setRefreshInterval(ms)}
                className={`px-2 py-1 rounded font-medium transition-colors ${
                  refreshInterval === ms
                    ? 'bg-white text-gray-900 shadow-sm'
                    : 'text-gray-600 hover:text-gray-900'
                }`}
              >
                {label}
              </button>
            ))}
          </div>

          {/* Refresh button */}
          <button
            onClick={fetchStatus}
            disabled={loading}
            className="flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium rounded-lg border border-gray-300 text-gray-700 bg-white hover:bg-gray-50 disabled:opacity-50"
            title="Refresh metrics now"
          >
            <RefreshCw className={`w-3.5 h-3.5 ${loading ? 'animate-spin' : ''}`} />
            Refresh
          </button>

          {/* Operator manual sync */}
          <button
            onClick={handleManualSync}
            disabled={syncingNow || status?.syncing}
            className="flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium rounded-lg bg-indigo-600 text-white hover:bg-indigo-700 disabled:opacity-50 shadow-sm"
            title="Trigger immediate sync from Soroban RPC"
          >
            <Zap className={`w-3.5 h-3.5 ${syncingNow ? 'animate-bounce' : ''}`} />
            {syncingNow ? 'Syncing...' : 'Sync Now'}
          </button>
        </div>
      </div>

      {error && (
        <div className="bg-rose-50 border border-rose-200 rounded-lg p-4 flex items-start gap-3">
          <AlertCircle className="w-5 h-5 text-rose-600 flex-shrink-0 mt-0.5" />
          <div className="text-sm text-rose-800">
            <span className="font-semibold">Error: </span>
            {error}
          </div>
        </div>
      )}

      {/* ── KPI Cards Grid ──────────────────────────────────────────────── */}
      <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
        {/* Card 1: Indexer Lag */}
        <div
          className={`p-4 rounded-xl border transition-all ${
            isLagCritical
              ? 'bg-rose-50/60 border-rose-200'
              : isLagWarning
              ? 'bg-amber-50/60 border-amber-200'
              : 'bg-gray-50 border-gray-200'
          }`}
        >
          <div className="flex items-center justify-between text-xs text-gray-500 mb-1">
            <span className="font-medium flex items-center gap-1.5">
              <Activity className="w-4 h-4 text-indigo-500" />
              Indexer Lag
            </span>
            <span
              className={`px-2 py-0.5 rounded text-[11px] font-semibold ${
                isLagCritical
                  ? 'bg-rose-100 text-rose-800'
                  : isLagWarning
                  ? 'bg-amber-100 text-amber-800'
                  : 'bg-emerald-100 text-emerald-800'
              }`}
            >
              {isLagCritical ? 'CRITICAL LAG' : isLagWarning ? 'LAG ELEVATED' : 'IN SYNC'}
            </span>
          </div>
          <div className="text-2xl font-bold text-gray-900">
            {status?.lagLedgers === null || status?.lagLedgers === undefined
              ? '—'
              : `${status.lagLedgers.toLocaleString()} ledgers`}
          </div>
          <p className="text-xs text-gray-500 mt-1">
            {status?.lagSeconds !== null && status?.lagSeconds !== undefined
              ? `~${status.lagSeconds}s behind network tip`
              : 'Awaiting sync confirmation'}
          </p>
        </div>

        {/* Card 2: Ledger Head Comparison */}
        <div className="p-4 rounded-xl border border-gray-200 bg-gray-50">
          <div className="flex items-center justify-between text-xs text-gray-500 mb-1">
            <span className="font-medium flex items-center gap-1.5">
              <Layers className="w-4 h-4 text-blue-500" />
              Ledger Tip
            </span>
            <span className="font-mono text-[11px] text-gray-500">
              v{status?.schemaVersion ?? 1}
            </span>
          </div>
          <div className="text-2xl font-bold text-gray-900 font-mono">
            {status?.latestIndexedLedger.toLocaleString() ?? '0'}
          </div>
          <div className="flex items-center gap-1 text-xs text-gray-500 mt-1 font-mono">
            <span>Net: {status?.latestNetworkLedger.toLocaleString() ?? '0'}</span>
            <ArrowRight className="w-3 h-3 text-gray-400" />
            <span className="text-emerald-700 font-medium">
              {status?.latestNetworkLedger
                ? `${((status.latestIndexedLedger / status.latestNetworkLedger) * 100).toFixed(2)}%`
                : '100%'}
            </span>
          </div>
        </div>

        {/* Card 3: Sync Duration & Cadence */}
        <div className="p-4 rounded-xl border border-gray-200 bg-gray-50">
          <div className="flex items-center justify-between text-xs text-gray-500 mb-1">
            <span className="font-medium flex items-center gap-1.5">
              <Clock className="w-4 h-4 text-emerald-500" />
              Last Sync
            </span>
            <span className="text-[11px] text-gray-500">
              {timeAgo(status?.lastSyncAt ?? null)}
            </span>
          </div>
          <div className="text-2xl font-bold text-gray-900">
            {status?.lastSyncDurationMs !== null && status?.lastSyncDurationMs !== undefined
              ? `${status.lastSyncDurationMs} ms`
              : '—'}
          </div>
          <p className="text-xs text-gray-500 mt-1">
            {status?.syncSuccessCount ?? 0} syncs succeeded
            {status?.consecutiveErrors ? ` (${status.consecutiveErrors} fail)` : ''}
          </p>
        </div>

        {/* Card 4: Ingestion & Store Volume */}
        <div className="p-4 rounded-xl border border-gray-200 bg-gray-50">
          <div className="flex items-center justify-between text-xs text-gray-500 mb-1">
            <span className="font-medium flex items-center gap-1.5">
              <Database className="w-4 h-4 text-purple-500" />
              Ingested Events
            </span>
            <span className="text-[11px] text-gray-500">
              Query p95: {status?.queryLatency?.p95Ms?.toFixed(1) ?? '—'}ms
            </span>
          </div>
          <div className="text-2xl font-bold text-gray-900">
            {status?.totalIngested.toLocaleString() ?? '0'}
          </div>
          <p className="text-xs text-gray-500 mt-1">
            {status?.hotEvents.toLocaleString() ?? '0'} hot in-memory ·{' '}
            {status?.archive.events.toLocaleString() ?? '0'} archived
          </p>
        </div>
      </div>

      {/* ── Active Alerts Banner / List ─────────────────────────────────── */}
      <div className="space-y-3">
        <div className="flex items-center justify-between">
          <h4 className="text-sm font-semibold text-gray-900 flex items-center gap-2">
            <Bell className="w-4 h-4 text-gray-700" />
            Active Health Alerts
            {status?.alerts && status.alerts.length > 0 && (
              <span className="px-2 py-0.5 rounded-full text-xs font-bold bg-rose-100 text-rose-800">
                {status.alerts.length}
              </span>
            )}
          </h4>
          <span className="text-xs text-gray-500">
            Warning at &gt;20 ledgers · Critical at &gt;100 ledgers or stalled &gt;300s
          </span>
        </div>

        {(!status?.alerts || status.alerts.length === 0) ? (
          <div className="p-4 rounded-lg bg-emerald-50/70 border border-emerald-200 flex items-center gap-3">
            <ShieldCheck className="w-5 h-5 text-emerald-600 flex-shrink-0" />
            <div className="text-sm text-emerald-900">
              <span className="font-semibold">All indexer monitors normal.</span> No lag or sync
              exceptions detected.
            </div>
          </div>
        ) : (
          <div className="space-y-2">
            {status.alerts.map((alert) => {
              const isCrit = alert.severity === 'critical';
              const isAcked = alert.acknowledged;
              return (
                <div
                  key={alert.id}
                  className={`p-3.5 rounded-lg border flex flex-col sm:flex-row sm:items-center justify-between gap-3 ${
                    isAcked
                      ? 'bg-gray-50 border-gray-200 opacity-75'
                      : isCrit
                      ? 'bg-rose-50 border-rose-200'
                      : 'bg-amber-50 border-amber-200'
                  }`}
                >
                  <div className="flex items-start gap-3">
                    {isCrit ? (
                      <Flame className="w-5 h-5 text-rose-600 flex-shrink-0 mt-0.5" />
                    ) : (
                      <AlertTriangle className="w-5 h-5 text-amber-600 flex-shrink-0 mt-0.5" />
                    )}
                    <div>
                      <div className="flex items-center gap-2">
                        <span
                          className={`px-2 py-0.5 rounded text-[10px] font-bold uppercase tracking-wider ${
                            isCrit
                              ? 'bg-rose-200 text-rose-900'
                              : 'bg-amber-200 text-amber-900'
                          }`}
                        >
                          {alert.severity}
                        </span>
                        <span className="text-sm font-semibold text-gray-900">
                          {alert.title}
                        </span>
                        <span className="text-xs text-gray-500">
                          {timeAgo(alert.triggeredAt)}
                        </span>
                      </div>
                      <p className="text-xs text-gray-700 mt-1">{alert.message}</p>
                    </div>
                  </div>

                  <div className="flex items-center gap-2 sm:self-center">
                    {isAcked ? (
                      <span className="text-xs text-gray-500 flex items-center gap-1 font-medium">
                        <CheckCircle2 className="w-4 h-4 text-emerald-600" />
                        Acknowledged
                      </span>
                    ) : (
                      <button
                        onClick={() => handleAcknowledgeAlert(alert.id)}
                        disabled={ackInProgress[alert.id]}
                        className="px-3 py-1 text-xs font-semibold rounded border border-gray-300 bg-white hover:bg-gray-50 text-gray-700 shadow-sm disabled:opacity-50"
                      >
                        {ackInProgress[alert.id] ? 'Saving...' : 'Acknowledge'}
                      </button>
                    )}
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </div>

      {/* ── Lag History SVG Timeline Chart ──────────────────────────────── */}
      <div className="space-y-2">
        <div className="flex items-center justify-between">
          <h4 className="text-sm font-semibold text-gray-900">Lag History Across Sync Cycles</h4>
          <span className="text-xs text-gray-500">
            Last {history.length} sync runs (ledgers behind tip)
          </span>
        </div>

        {history.length === 0 ? (
          <div className="p-8 text-center text-sm text-gray-400 bg-gray-50 rounded-lg border border-dashed border-gray-200">
            No historical sync points captured yet. Sync events will populate this chart.
          </div>
        ) : (
          <div className="bg-gray-50 rounded-xl p-4 border border-gray-200">
            <svg
              viewBox={`0 0 ${Math.max(100, history.length * 20)} 90`}
              className="w-full h-28"
              preserveAspectRatio="none"
            >
              {/* Threshold line: warning at 20 */}
              <line
                x1="0"
                y1={90 - (20 / maxLagInHistory) * 80}
                x2={history.length * 20}
                y2={90 - (20 / maxLagInHistory) * 80}
                stroke="#f59e0b"
                strokeDasharray="3 3"
                strokeWidth="1"
              />

              {/* Threshold line: critical at 100 if in scale */}
              {maxLagInHistory >= 100 && (
                <line
                  x1="0"
                  y1={90 - (100 / maxLagInHistory) * 80}
                  x2={history.length * 20}
                  y2={90 - (100 / maxLagInHistory) * 80}
                  stroke="#ef4444"
                  strokeDasharray="3 3"
                  strokeWidth="1"
                />
              )}

              {/* Data points */}
              {history.map((point, i) => {
                const barHeight = Math.max(3, (point.lagLedgers / maxLagInHistory) * 80);
                const x = i * 20 + 4;
                const y = 90 - barHeight;
                const isErr = point.status === 'error';
                const color = isErr
                  ? '#ef4444'
                  : point.lagLedgers >= 100
                  ? '#dc2626'
                  : point.lagLedgers >= 20
                  ? '#f59e0b'
                  : '#10b981';

                return (
                  <g key={point.timestamp + i}>
                    <rect
                      x={x}
                      y={y}
                      width={12}
                      height={barHeight}
                      rx={2}
                      fill={color}
                      className="transition-all hover:opacity-80"
                    >
                      <title>
                        {new Date(point.timestamp).toLocaleTimeString()}:{' '}
                        {point.lagLedgers} ledgers lag ({point.durationMs}ms) [
                        {point.status}]
                      </title>
                    </rect>
                  </g>
                );
              })}
            </svg>
            <div className="flex items-center justify-between text-[11px] text-gray-500 mt-2 px-1">
              <div className="flex items-center gap-3">
                <span className="flex items-center gap-1">
                  <span className="w-2.5 h-2.5 rounded-sm bg-emerald-500" /> &lt;20 ledgers
                </span>
                <span className="flex items-center gap-1">
                  <span className="w-2.5 h-2.5 rounded-sm bg-amber-500" /> 20-99 ledgers
                </span>
                <span className="flex items-center gap-1">
                  <span className="w-2.5 h-2.5 rounded-sm bg-rose-500" /> &gt;=100 ledgers
                </span>
              </div>
              <span>Oldest ➔ Newest</span>
            </div>
          </div>
        )}
      </div>

      {/* ── Component Health & Downstream Sinks ─────────────────────────── */}
      <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
        {/* Component Health Diagnostics */}
        <div className="border border-gray-200 rounded-lg p-4 space-y-3">
          <h4 className="text-sm font-semibold text-gray-900 flex items-center gap-1.5">
            <Server className="w-4 h-4 text-gray-600" />
            Component Diagnostics
          </h4>
          <table className="w-full text-xs">
            <tbody className="divide-y divide-gray-100">
              <tr>
                <td className="py-1.5 text-gray-500">Polling Loop</td>
                <td className="py-1.5 text-right font-medium">
                  {status?.running ? (
                    <span className="text-emerald-700">Active</span>
                  ) : (
                    <span className="text-gray-500">Idle / Manual</span>
                  )}
                </td>
              </tr>
              <tr>
                <td className="py-1.5 text-gray-500">Cursor State</td>
                <td className="py-1.5 text-right font-mono text-gray-700 truncate max-w-[12rem]">
                  {status?.cursor ? status.cursor.slice(0, 16) + '…' : 'Initial'}
                </td>
              </tr>
              <tr>
                <td className="py-1.5 text-gray-500">Hot Retention Window</td>
                <td className="py-1.5 text-right font-medium text-gray-700">
                  {status?.hotRetentionMs
                    ? `${status.hotRetentionMs / (24 * 60 * 60 * 1000)} days`
                    : '30 days'}
                </td>
              </tr>
              <tr>
                <td className="py-1.5 text-gray-500">Cold Storage Segments</td>
                <td className="py-1.5 text-right font-medium text-gray-700">
                  {status?.archive.segments ?? 0} segments (
                  {status?.archive.events.toLocaleString() ?? 0} events)
                </td>
              </tr>
            </tbody>
          </table>
        </div>

        {/* Downstream Sinks Status */}
        <div className="border border-gray-200 rounded-lg p-4 space-y-3">
          <h4 className="text-sm font-semibold text-gray-900 flex items-center gap-1.5">
            <Database className="w-4 h-4 text-gray-600" />
            Downstream Event Sinks
          </h4>
          {(!status?.sinks || status.sinks.length === 0) ? (
            <p className="text-xs text-gray-500 py-3">
              No downstream sinks configured (using primary in-memory store).
            </p>
          ) : (
            <table className="w-full text-xs">
              <thead>
                <tr className="text-left text-gray-500 border-b">
                  <th className="py-1">Sink Name</th>
                  <th className="py-1 text-center">Status</th>
                  <th className="py-1 text-right">Failures</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-gray-100">
                {status.sinks.map((sink) => (
                  <tr key={sink.name}>
                    <td className="py-1.5 font-medium text-gray-900">{sink.name}</td>
                    <td className="py-1.5 text-center">
                      {sink.failures === 0 ? (
                        <span className="px-1.5 py-0.5 rounded bg-emerald-50 text-emerald-700 font-semibold">
                          Healthy
                        </span>
                      ) : (
                        <span className="px-1.5 py-0.5 rounded bg-rose-50 text-rose-700 font-semibold">
                          Failing
                        </span>
                      )}
                    </td>
                    <td className="py-1.5 text-right font-mono text-gray-700">
                      {sink.failures.toLocaleString()}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </div>
      </div>
    </div>
  );
};

export default IndexerHealthDashboard;
