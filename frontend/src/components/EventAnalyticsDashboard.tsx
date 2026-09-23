/**
 * Event Analytics Dashboard — Issue #685
 *
 * Indexer health, event volume over time, type breakdown, top accounts and a
 * live event search backed by /api/events (showing server-side query time
 * against the 100ms search budget).
 */

import React, { useCallback, useEffect, useState } from 'react';
import { Activity, Archive, Database, Search, Timer } from 'lucide-react';

interface Analytics {
  totalEvents: number;
  byType: Record<string, number>;
  byModule: Record<string, number>;
  bySchemaStatus: Record<string, number>;
  timeseries: Array<{ bucketStart: number; total: number; counts: Record<string, number> }>;
  topAccounts: Array<{ account: string; events: number }>;
  bucket: 'hour' | 'day';
  queryLatency: { samples: number; p50Ms: number; p95Ms: number; maxMs: number };
}

interface IndexerStatus {
  running: boolean;
  lastSyncAt: number | null;
  lastError: string | null;
  hotEvents: number;
  latestIndexedLedger: number;
  lagLedgers: number | null;
  archive: { segments: number; events: number };
  sinks: Array<{ name: string; failures: number }>;
}

interface IndexedEvent {
  id: string;
  type: string;
  ledger: number;
  timestamp: number;
  actor?: string;
  amount?: string;
  asset?: string;
}

interface EventPage {
  events: IndexedEvent[];
  nextCursor: string | null;
  matched: number;
  tookMs: number;
}

interface EventAnalyticsDashboardProps {
  apiBaseUrl?: string;
  refreshMs?: number;
}

const SEARCH_BUDGET_MS = 100;
const short = (a: string) => (a.length > 12 ? `${a.slice(0, 6)}…${a.slice(-4)}` : a);

export const EventAnalyticsDashboard: React.FC<EventAnalyticsDashboardProps> = ({
  apiBaseUrl = '/api',
  refreshMs = 30_000,
}) => {
  const [bucket, setBucket] = useState<'hour' | 'day'>('hour');
  const [analytics, setAnalytics] = useState<Analytics | null>(null);
  const [status, setStatus] = useState<IndexerStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [filters, setFilters] = useState({ type: '', account: '', includeArchived: false });
  const [results, setResults] = useState<EventPage | null>(null);
  const [searching, setSearching] = useState(false);

  const load = useCallback(async () => {
    try {
      const [a, s] = await Promise.all([
        fetch(`${apiBaseUrl}/events/analytics?bucket=${bucket}`),
        fetch(`${apiBaseUrl}/events/indexer/status`),
      ]);
      if (!a.ok || !s.ok) throw new Error('Failed to load event analytics');
      setAnalytics(await a.json());
      setStatus(await s.json());
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Unknown error');
    }
  }, [apiBaseUrl, bucket]);

  useEffect(() => {
    load();
    const timer = setInterval(load, refreshMs);
    return () => clearInterval(timer);
  }, [load, refreshMs]);

  const search = async (cursor?: string) => {
    setSearching(true);
    try {
      const params = new URLSearchParams({ limit: '20' });
      if (filters.type) params.set('type', filters.type);
      if (filters.account) params.set('account', filters.account.trim());
      if (filters.includeArchived) params.set('includeArchived', 'true');
      if (cursor) params.set('cursor', cursor);
      const res = await fetch(`${apiBaseUrl}/events?${params}`);
      const body = await res.json();
      if (!res.ok) throw new Error(body?.error?.message ?? 'Search failed');
      setResults((prev) =>
        cursor && prev ? { ...body, events: [...prev.events, ...body.events] } : body
      );
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Search failed');
    } finally {
      setSearching(false);
    }
  };

  if (error && !analytics) {
    return (
      <div className="bg-red-50 border border-red-200 rounded-lg p-4">
        <p className="text-red-700">{error}</p>
      </div>
    );
  }

  if (!analytics || !status) {
    return (
      <div className="bg-white rounded-lg shadow p-6 animate-pulse">
        <div className="h-6 bg-gray-200 rounded w-1/3 mb-4" />
        <div className="h-48 bg-gray-200 rounded" />
      </div>
    );
  }

  const types = Object.entries(analytics.byType).sort((a, b) => b[1] - a[1]);
  const maxType = Math.max(1, ...types.map(([, n]) => n));
  const series = analytics.timeseries.slice(-48);
  const maxBucket = Math.max(1, ...series.map((p) => p.total));
  const p95 = analytics.queryLatency.p95Ms;

  const tiles = [
    { icon: Database, label: 'Indexed events', value: analytics.totalEvents.toLocaleString() },
    {
      icon: Activity,
      label: 'Indexer lag',
      value: status.lagLedgers === null ? '—' : `${status.lagLedgers} ledgers`,
      warn: (status.lagLedgers ?? 0) > 100 || !!status.lastError,
    },
    {
      icon: Archive,
      label: 'Archived',
      value: `${status.archive.events.toLocaleString()} in ${status.archive.segments} segments`,
    },
    {
      icon: Timer,
      label: 'Query p95',
      value: `${p95.toFixed(1)} ms`,
      warn: p95 >= SEARCH_BUDGET_MS,
    },
  ];

  return (
    <div className="bg-white rounded-lg shadow-lg p-6 space-y-6">
      <div className="flex items-center justify-between">
        <h3 className="text-xl font-bold text-gray-900">Contract Event Analytics</h3>
        <div className="flex gap-2">
          {(['hour', 'day'] as const).map((b) => (
            <button
              key={b}
              onClick={() => setBucket(b)}
              className={`px-3 py-1 text-sm rounded-lg font-medium ${
                bucket === b ? 'bg-blue-600 text-white' : 'bg-gray-100 text-gray-700 hover:bg-gray-200'
              }`}
            >
              {b === 'hour' ? 'Hourly' : 'Daily'}
            </button>
          ))}
        </div>
      </div>

      {status.lastError && (
        <div className="bg-amber-50 border border-amber-200 rounded-lg p-3 text-sm text-amber-800">
          Last sync failed: {status.lastError}
        </div>
      )}

      <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
        {tiles.map(({ icon: Icon, label, value, warn }) => (
          <div key={label} className={`rounded-lg p-4 ${warn ? 'bg-amber-50' : 'bg-gray-50'}`}>
            <div className="flex items-center gap-2 text-sm text-gray-500">
              <Icon className="w-4 h-4" /> {label}
            </div>
            <div className="mt-1 text-lg font-semibold text-gray-900">{value}</div>
          </div>
        ))}
      </div>

      <section>
        <h4 className="text-sm font-semibold text-gray-700 mb-2">Events per {bucket}</h4>
        {series.length === 0 ? (
          <p className="text-sm text-gray-500">No events indexed yet.</p>
        ) : (
          <svg viewBox={`0 0 ${series.length * 12} 100`} className="w-full h-32" preserveAspectRatio="none">
            {series.map((p, i) => {
              const h = (p.total / maxBucket) * 96;
              return (
                <rect key={p.bucketStart} x={i * 12 + 1} y={100 - h} width={10} height={h} className="fill-blue-500">
                  <title>
                    {new Date(p.bucketStart).toLocaleString()}: {p.total} events
                  </title>
                </rect>
              );
            })}
          </svg>
        )}
      </section>

      <div className="grid md:grid-cols-2 gap-6">
        <section>
          <h4 className="text-sm font-semibold text-gray-700 mb-2">By type</h4>
          <div className="space-y-1">
            {types.map(([type, n]) => (
              <button
                key={type}
                onClick={() => setFilters((f) => ({ ...f, type }))}
                className="w-full flex items-center gap-2 text-left text-sm hover:bg-gray-50 rounded"
              >
                <span className="w-40 truncate text-gray-700">{type}</span>
                <span className="flex-1 bg-gray-100 rounded h-3">
                  <span className="block bg-indigo-500 h-3 rounded" style={{ width: `${(n / maxType) * 100}%` }} />
                </span>
                <span className="w-14 text-right tabular-nums text-gray-900">{n.toLocaleString()}</span>
              </button>
            ))}
          </div>
          {Object.keys(analytics.bySchemaStatus).some((s) => s !== 'current') && (
            <p className="mt-3 text-xs text-amber-700">
              Schema status: {JSON.stringify(analytics.bySchemaStatus)} — some events need an upcaster.
            </p>
          )}
        </section>

        <section>
          <h4 className="text-sm font-semibold text-gray-700 mb-2">Most active accounts</h4>
          <table className="w-full text-sm">
            <tbody>
              {analytics.topAccounts.map(({ account, events }) => (
                <tr key={account} className="border-b last:border-0">
                  <td className="py-1">
                    <button
                      className="font-mono text-blue-700 hover:underline"
                      onClick={() => setFilters((f) => ({ ...f, account }))}
                      title={account}
                    >
                      {short(account)}
                    </button>
                  </td>
                  <td className="py-1 text-right tabular-nums">{events.toLocaleString()}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </section>
      </div>

      <section>
        <h4 className="text-sm font-semibold text-gray-700 mb-2">Search events</h4>
        <form
          className="flex flex-wrap gap-2 items-center"
          onSubmit={(e) => {
            e.preventDefault();
            search();
          }}
        >
          <input
            value={filters.type}
            onChange={(e) => setFilters((f) => ({ ...f, type: e.target.value }))}
            placeholder="type (e.g. borrow)"
            className="border rounded px-2 py-1 text-sm"
          />
          <input
            value={filters.account}
            onChange={(e) => setFilters((f) => ({ ...f, account: e.target.value }))}
            placeholder="account G… / C…"
            className="border rounded px-2 py-1 text-sm font-mono flex-1 min-w-[16rem]"
          />
          <label className="flex items-center gap-1 text-sm text-gray-600">
            <input
              type="checkbox"
              checked={filters.includeArchived}
              onChange={(e) => setFilters((f) => ({ ...f, includeArchived: e.target.checked }))}
            />
            include archive
          </label>
          <button
            type="submit"
            disabled={searching}
            className="flex items-center gap-1 px-3 py-1 text-sm rounded-lg bg-blue-600 text-white disabled:opacity-50"
          >
            <Search className="w-4 h-4" /> Search
          </button>
        </form>

        {results && (
          <div className="mt-3">
            <p className="text-xs text-gray-500 mb-1">
              {results.matched.toLocaleString()} hot matches ·{' '}
              <span className={results.tookMs < SEARCH_BUDGET_MS ? 'text-green-700' : 'text-red-700'}>
                {results.tookMs.toFixed(2)} ms
              </span>{' '}
              server time (budget {SEARCH_BUDGET_MS} ms)
            </p>
            <table className="w-full text-sm">
              <thead>
                <tr className="text-left text-gray-500 border-b">
                  <th className="py-1">Ledger</th>
                  <th>Time</th>
                  <th>Type</th>
                  <th>Actor</th>
                  <th className="text-right">Amount</th>
                </tr>
              </thead>
              <tbody>
                {results.events.map((e) => (
                  <tr key={e.id} className="border-b last:border-0">
                    <td className="py-1 tabular-nums">{e.ledger}</td>
                    <td>{new Date(e.timestamp).toLocaleString()}</td>
                    <td>{e.type}</td>
                    <td className="font-mono" title={e.actor}>
                      {e.actor ? short(e.actor) : '—'}
                    </td>
                    <td className="text-right tabular-nums">{e.amount ?? '—'}</td>
                  </tr>
                ))}
              </tbody>
            </table>
            {results.nextCursor && (
              <button
                onClick={() => search(results.nextCursor ?? undefined)}
                disabled={searching}
                className="mt-2 text-sm text-blue-700 hover:underline"
              >
                Load more
              </button>
            )}
          </div>
        )}
      </section>
    </div>
  );
};

export default EventAnalyticsDashboard;
