/**
 * Search performance budget for the event index (#685: "event search
 * performance < 100ms"). Loads 200k events across 2k accounts and checks the
 * p95 latency of representative filter combinations. Tune the dataset with
 * EVENT_PERF_EVENTS.
 */

import { EventStore } from '../services/eventIndex/store';
import type { EventQuery, IndexedEvent } from '../services/eventIndex/types';
import { BASE_TIME, LEDGER_MS, USDC } from '../services/eventIndex/__fixtures__/events';

const EVENTS = Number(process.env.EVENT_PERF_EVENTS ?? 200_000);
const ACCOUNTS = 2_000;
const BUDGET_MS = 100;
const RUNS = 25;
const TYPES = ['deposit', 'withdrawal', 'borrow', 'repay', 'liquidation', 'price_updated'];

function syntheticEvents(): IndexedEvent[] {
  const accounts = Array.from({ length: ACCOUNTS }, (_, i) => `G${String(i).padStart(55, 'A')}`);
  const events: IndexedEvent[] = [];
  for (let i = 0; i < EVENTS; i += 1) {
    const ledger = 1 + Math.floor(i / 4);
    const actor = accounts[(i * 7919) % ACCOUNTS]!;
    events.push({
      id: `${String(ledger).padStart(12, '0')}-${String(i).padStart(10, '0')}`,
      type: TYPES[i % TYPES.length]!,
      contract: 'C',
      topic: [],
      data: { amount: String(i) },
      timestamp: BASE_TIME + ledger * LEDGER_MS,
      ledger,
      accounts: [actor, USDC],
      actor,
      envelope: 'typed',
      asset: USDC,
      amount: String(i),
      schemaVersion: 1,
      schemaStatus: 'current',
    });
  }
  return events;
}

function p95(samples: number[]): number {
  const sorted = [...samples].sort((a, b) => a - b);
  return sorted[Math.floor(0.95 * (sorted.length - 1))] ?? 0;
}

describe(`event search performance (${EVENTS.toLocaleString()} events)`, () => {
  const store = new EventStore();
  const lastLedger = Math.floor(EVENTS / 4);
  const someAccount = `G${String(42).padStart(55, 'A')}`;

  beforeAll(() => {
    store.add(syntheticEvents());
  });

  const cases: Array<[string, EventQuery]> = [
    ['latest page, no filters', {}],
    ['by type', { type: 'borrow' }],
    ['by account', { account: someAccount }],
    ['type + account', { type: 'deposit', account: someAccount }],
    [
      'time window',
      {
        from: BASE_TIME + (lastLedger / 2) * LEDGER_MS,
        to: BASE_TIME + (lastLedger / 2 + 5_000) * LEDGER_MS,
      },
    ],
    [
      'ledger range, ascending, large page',
      { fromLedger: 1_000, toLedger: 20_000, order: 'asc', limit: 1_000 },
    ],
    ['sparse residual filter (full scan)', { module: 'governance' }],
  ];

  it.each(cases)(`%s: p95 < ${BUDGET_MS}ms`, (_name, query) => {
    const samples: number[] = [];
    for (let i = 0; i < RUNS; i += 1) samples.push(store.query(query).tookMs);
    expect(p95(samples)).toBeLessThan(BUDGET_MS);
  });

  it('pages deep into a hot account with cursors under budget', () => {
    let cursor: string | undefined;
    for (let page = 0; page < 10; page += 1) {
      const res = store.query({ account: someAccount, limit: 10, cursor });
      expect(res.tookMs).toBeLessThan(BUDGET_MS);
      cursor = res.nextCursor ?? undefined;
      if (!cursor) break;
    }
  });
});
