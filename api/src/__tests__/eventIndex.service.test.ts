import { gzipSync } from 'zlib';
import { EventArchiver } from '../services/eventIndex/archive';
import { ElasticsearchEventSink } from '../services/eventIndex/elasticsearch';
import { replayEvents } from '../services/eventIndex/replay';
import { CURRENT_SCHEMA_VERSION, normalizeEvent, upcast } from '../services/eventIndex/schema';
import { EventStore } from '../services/eventIndex/store';
import type { IndexedEvent } from '../services/eventIndex/types';
import {
  account,
  BASE_TIME,
  CONTRACT,
  LEDGER_MS,
  liquidation,
  memoryIndexer,
  rawEvent,
  resetSeq,
  structured,
  typed,
  USDC,
} from '../services/eventIndex/__fixtures__/events';

const DAY_MS = 24 * 60 * 60 * 1000;
const at = (ledger: number) => BASE_TIME + ledger * LEDGER_MS;

beforeEach(() => resetSeq());

describe('event normalization', () => {
  it('normalizes a typed event: type from topic, actor, JSON-safe i128', () => {
    const user = account();
    const e = normalizeEvent(typed('deposit', 10, user, 12_345_678_901_234_567_890n));

    expect(e.type).toBe('deposit');
    expect(e.envelope).toBe('typed');
    expect(e.actor).toBe(user);
    expect(e.contract).toBe(CONTRACT);
    expect(e.asset).toBe(USDC);
    expect(e.amount).toBe('12345678901234567890');
    expect(e.data.amount).toBe('12345678901234567890');
    expect(e.timestamp).toBe(at(10));
    expect(e.accounts).toEqual([user, USDC].sort());
    expect(e.schemaVersion).toBe(1);
    expect(e.schemaStatus).toBe('current');
    expect(() => JSON.stringify(e)).not.toThrow();
  });

  it('decodes the structured envelope: module/action enums and actor from topics', () => {
    const actor = account();
    const e = normalizeEvent(structured(5, 'FlashLoan', 'FlashLoan', actor, 500n));

    expect(e.type).toBe('proto_evt');
    expect(e.envelope).toBe('structured_event_v1');
    expect(e.module).toBe('flash_loan');
    expect(e.action).toBe('flash_loan');
    expect(e.actor).toBe(actor);
    expect(e.amount).toBe('500');
  });

  it('takes liquidator as actor and borrower as counterparty', () => {
    const [liq, borrower] = [account(), account()];
    const e = normalizeEvent(liquidation(7, liq, borrower, 100n, 110n));
    expect(e.type).toBe('liquidation');
    expect(e.actor).toBe(liq);
    expect(e.counterparty).toBe(borrower);
    expect(e.amount).toBe('100');
  });

  it('keeps non-map payloads under `value`', () => {
    const e = normalizeEvent(rawEvent(1, ['ping'], 42n));
    expect(e.data).toEqual({ value: '42' });
    expect(e.type).toBe('ping');
  });
});

describe('schema versioning', () => {
  it('upcasts older payloads through the chain', () => {
    const upcasters = {
      [CURRENT_SCHEMA_VERSION - 1]: (d: Record<string, unknown>) => ({ ...d, migrated: true }),
    };
    const res = upcast({ a: 1 }, CURRENT_SCHEMA_VERSION - 1, upcasters);
    expect(res).toEqual({
      data: { a: 1, migrated: true },
      version: CURRENT_SCHEMA_VERSION,
      status: 'upcast',
    });
  });

  it('flags a gap in the upcaster chain as unknown instead of guessing', () => {
    expect(upcast({ a: 1 }, CURRENT_SCHEMA_VERSION - 1, {}).status).toBe('unknown');
  });

  it('stores events from a newer schema verbatim, flagged unknown', () => {
    const e = normalizeEvent(
      structured(1, 'Lending', 'Deposit', account(), 1n, {
        schema_version: CURRENT_SCHEMA_VERSION + 1,
        extra: 'x',
      })
    );
    expect(e.schemaVersion).toBe(CURRENT_SCHEMA_VERSION + 1);
    expect(e.schemaStatus).toBe('unknown');
    expect(e.data.extra).toBe('x');
  });
});

describe('EventStore queries', () => {
  const [alice, bob] = [account(), account()];
  let store: EventStore;
  let all: IndexedEvent[];

  beforeEach(() => {
    store = new EventStore();
    all = [];
    for (let ledger = 1; ledger <= 40; ledger += 1) {
      const user = ledger % 2 ? alice : bob;
      const kind = (['deposit', 'borrow', 'repay', 'withdrawal'] as const)[ledger % 4]!;
      all.push(normalizeEvent(typed(kind, ledger, user, BigInt(ledger))));
    }
    // Insert out of order to exercise sorted insertion.
    store.add([...all].reverse());
  });

  it('deduplicates by id', () => {
    expect(store.add(all)).toBe(0);
    expect(store.size).toBe(40);
  });

  it('filters by type, account and time, newest first', () => {
    const page = store.query({ type: 'borrow', account: alice, from: at(10), to: at(30) });
    const expected = all
      .filter(
        (e) =>
          e.type === 'borrow' && e.actor === alice && e.timestamp >= at(10) && e.timestamp <= at(30)
      )
      .reverse();
    expect(page.events.map((e) => e.id)).toEqual(expected.map((e) => e.id));
    expect(page.matched).toBe(expected.length);
  });

  it('filters by ledger range in ascending order', () => {
    const page = store.query({ fromLedger: 5, toLedger: 8, order: 'asc' });
    expect(page.events.map((e) => e.ledger)).toEqual([5, 6, 7, 8]);
  });

  it.each(['asc', 'desc'] as const)(
    'paginates with cursors without gaps or duplicates (%s)',
    (order) => {
      const seen: string[] = [];
      let cursor: string | undefined;
      let pages = 0;
      do {
        const page = store.query({ account: bob, order, limit: 3, cursor });
        seen.push(...page.events.map((e) => e.id));
        cursor = page.nextCursor ?? undefined;
        pages += 1;
      } while (cursor && pages < 50);

      const expected = all.filter((e) => e.actor === bob).map((e) => e.id);
      expect(seen).toEqual(order === 'asc' ? expected : [...expected].reverse());
    }
  );

  it('returns an empty page for unknown types', () => {
    expect(store.query({ type: 'nope' })).toMatchObject({
      events: [],
      matched: 0,
      nextCursor: null,
    });
  });

  it('tracks query latency', () => {
    store.query({});
    expect(store.latencyStats().samples).toBeGreaterThan(0);
  });
});

describe('EventIndexer sync', () => {
  it('pages through the source, then resumes from the RPC cursor', async () => {
    const { indexer, source } = memoryIndexer({ startLedger: 100 });
    const user = account();
    source.pages = [
      [
        typed('deposit', 100, user, 1n),
        typed('deposit', 101, user, 2n),
        typed('deposit', 102, user, 3n),
      ],
      [typed('borrow', 103, user, 1n)],
    ];

    const res = await indexer.sync();
    expect(res).toMatchObject({ fetched: 4, indexed: 4, pages: 2 });
    expect(source.requests[0]).toEqual({ startLedger: 100, limit: 3 });
    expect(source.requests[1]).toEqual({ cursor: 'cursor-1', limit: 3 });

    await indexer.sync();
    expect(source.requests[2]).toEqual({ cursor: 'cursor-2', limit: 3 });
  });

  it('starts from latest - lookback when no start ledger is configured', async () => {
    const { indexer, source } = memoryIndexer({ initialLookbackLedgers: 100 });
    source.latest = 5_000;
    await indexer.sync();
    expect(source.requests[0]).toEqual({ startLedger: 4_900, limit: 3 });
  });

  it('skips events from failed contract calls', async () => {
    const { indexer } = memoryIndexer();
    const added = await indexer.ingest([
      typed('deposit', 1, account(), 1n),
      { ...typed('deposit', 2, account(), 1n), inSuccessfulContractCall: false },
    ]);
    expect(added).toBe(1);
  });

  it('records sync errors in status without throwing away state', async () => {
    const { indexer, source } = memoryIndexer();
    source.fail = new Error('rpc down');
    await expect(indexer.sync()).rejects.toThrow('rpc down');
    expect((await indexer.status()).lastError).toBe('rpc down');
  });

  it('isolates sink failures from ingestion', async () => {
    const sink = { name: 'broken', write: jest.fn().mockRejectedValue(new Error('boom')) };
    const { indexer } = memoryIndexer({ sinks: [sink] });
    expect(await indexer.ingest([typed('deposit', 1, account(), 1n)])).toBe(1);
    expect((await indexer.status()).sinks).toEqual([{ name: 'broken', failures: 1 }]);
  });
});

describe('archival and replay', () => {
  const [alice, bob, liq] = [account(), account(), account()];
  // Ledgers 17_280 apart are one day apart at 5s/ledger.
  const L = 17_280;

  async function seeded() {
    const ctx = memoryIndexer();
    await ctx.indexer.ingest([
      typed('deposit', 1, alice, 1_000n),
      typed('borrow', 2, alice, 400n),
      typed('deposit', L + 1, bob, 500n),
      typed('repay', L + 2, alice, 100n),
      typed('borrow', 3 * L, bob, 200n),
      liquidation(3 * L + 1, liq, alice, 150n, 160n),
    ]);
    return ctx;
  }

  it('moves old events to day segments and keeps them queryable', async () => {
    const { indexer, storage } = await seeded();
    const now = at(3 * L + 10);
    const res = await indexer.archive(2 * DAY_MS, now);

    expect(res.archived).toBe(4);
    expect(res.segments).toHaveLength(2);
    expect(indexer.store.size).toBe(2);
    expect([...storage.objects.keys()].sort()).toEqual([
      'events/2026-01-01/segment-1.ndjson.gz',
      'events/2026-01-02/segment-1.ndjson.gz',
      'manifest.json',
    ]);

    const hotOnly = await indexer.query({ account: alice, order: 'asc' });
    expect(hotOnly.events).toHaveLength(1);

    const withArchive = await indexer.query({
      account: alice,
      order: 'asc',
      includeArchived: true,
    });
    expect(withArchive.source).toBe('hot+archive');
    expect(withArchive.events.map((e) => e.ledger)).toEqual([1, 2, L + 2, 3 * L + 1]);
  });

  it('paginates across the archive/hot boundary', async () => {
    const { indexer } = await seeded();
    await indexer.archive(2 * DAY_MS, at(3 * L + 10));

    const ids: string[] = [];
    let cursor: string | undefined;
    do {
      const page = await indexer.query({ includeArchived: true, limit: 2, cursor });
      ids.push(...page.events.map((e) => e.id));
      cursor = page.nextCursor ?? undefined;
    } while (cursor);
    expect(ids).toHaveLength(6);
    expect(new Set(ids).size).toBe(6);
  });

  it('merges re-archived days into a new segment generation', async () => {
    const { indexer } = await seeded();
    await indexer.archive(2 * DAY_MS, at(3 * L + 10));
    await indexer.ingest([typed('withdrawal', 3, alice, 10n)]);
    const res = await indexer.archive(2 * DAY_MS, at(3 * L + 10));

    expect(res.segments.map((s) => s.key)).toEqual(['events/2026-01-01/segment-2.ndjson.gz']);
    const manifest = await indexer.archiveManifest();
    expect(manifest.segments.map((s) => s.key)).toEqual([
      'events/2026-01-01/segment-2.ndjson.gz',
      'events/2026-01-02/segment-1.ndjson.gz',
    ]);
    expect(manifest.segments[0]!.count).toBe(3);
  });

  it('detects corrupted segments via checksum', async () => {
    const { indexer, storage } = await seeded();
    await indexer.archive(2 * DAY_MS, at(3 * L + 10));
    const fresh = new EventArchiver(storage);
    storage.objects.set('events/2026-01-01/segment-1.ndjson.gz', gzipSync(Buffer.from('{}\n')));
    const [segment] = await fresh.segmentsFor({});
    await expect(fresh.readSegment(segment!)).rejects.toThrow('checksum mismatch');
  });

  it('restores events to the hot store if the archive write fails', async () => {
    const { indexer, storage } = await seeded();
    jest.spyOn(storage, 'put').mockRejectedValueOnce(new Error('disk full'));
    await expect(indexer.archive(2 * DAY_MS, at(3 * L + 10))).rejects.toThrow('disk full');
    expect(indexer.store.size).toBe(6);
  });

  it('replays hot + archived events into per-account state', async () => {
    const { indexer } = await seeded();
    await indexer.archive(2 * DAY_MS, at(3 * L + 10));
    const res = await indexer.replay({});

    expect(res.eventsReplayed).toBe(6);
    expect(res.finalState[alice]).toEqual({
      collateral: { [USDC]: '840' }, // 1000 deposited - 160 seized
      debt: { [USDC]: '150' }, // 400 borrowed - 100 repaid - 150 liquidated
      eventCount: 5,
    });
    expect(res.finalState[bob]).toEqual({
      collateral: { [USDC]: '500' },
      debt: { [USDC]: '200' },
      eventCount: 2,
    });
    expect(res.anomalies).toEqual([]);
  });

  it('flags anomalies when events are missing from the replay window', async () => {
    const { indexer } = await seeded();
    // Start after alice's deposit/borrow: her repay and liquidation go negative.
    const res = await indexer.replay({ fromLedger: L, account: alice });
    expect(res.anomalies.map((a) => a.field)).toEqual(['debt', 'debt', 'collateral']);
    expect(res.anomalies[0]).toMatchObject({ account: alice, balance: '-100' });
  });

  it('can replay the structured envelope layer instead of typed events', () => {
    const actor = account();
    const events = [
      normalizeEvent(structured(1, 'Lending', 'Deposit', actor, 50n)),
      normalizeEvent(structured(2, 'Lending', 'Borrow', actor, 20n)),
      normalizeEvent(typed('deposit', 3, actor, 999n)),
    ];
    const res = replayEvents(events, { source: 'structured' });
    expect(res.finalState[actor]).toEqual({
      collateral: { [USDC]: '50' },
      debt: { [USDC]: '20' },
      eventCount: 2,
    });
  });

  it('understands the lending contract event names', () => {
    const user = account();
    const named = (name: string, ledger: number, amount: bigint) =>
      normalizeEvent(rawEvent(ledger, [`${name}_event`, user], { asset: USDC, amount }));
    const res = replayEvents([
      named('vault_deposit', 1, 1_000n),
      named('borrow_collateral_deposit', 2, 500n),
      named('withdraw', 3, 200n),
    ]);
    expect(res.finalState[user]!.collateral).toEqual({ [USDC]: '1300' });
  });

  it('truncates the transition log but still computes final state', () => {
    const actor = account();
    const events = Array.from({ length: 5 }, (_, i) =>
      normalizeEvent(typed('deposit', i + 1, actor, 1n))
    );
    const res = replayEvents(events, { maxTransitions: 2 });
    expect(res.transitions).toHaveLength(2);
    expect(res.transitionsTruncated).toBe(true);
    expect(res.finalState[actor]!.collateral[USDC]).toBe('5');
  });
});

describe('analytics', () => {
  it('aggregates counts, series, top accounts and volume', async () => {
    const { indexer } = memoryIndexer();
    const [alice, bob] = [account(), account()];
    await indexer.ingest([
      typed('deposit', 1, alice, 100n),
      typed('deposit', 2, alice, 50n),
      typed('borrow', 3, bob, 30n),
      structured(4, 'Lending', 'Borrow', bob, 30n),
    ]);
    const a = indexer.analytics({ bucket: 'day' });

    expect(a.totalEvents).toBe(4);
    expect(a.byType).toEqual({ deposit: 2, borrow: 1, proto_evt: 1 });
    expect(a.byModule).toEqual({ lending: 1 });
    expect(a.bySchemaStatus).toEqual({ current: 4 });
    expect(a.timeseries).toEqual([{ bucketStart: BASE_TIME, total: 4, counts: a.byType }]);
    // Ties (both have 2 events) are broken by address, so compare as a set.
    expect(a.topAccounts).toHaveLength(2);
    expect(a.topAccounts).toEqual(
      expect.arrayContaining([
        { account: alice, events: 2 },
        { account: bob, events: 2 },
      ])
    );
    // Structured envelopes are excluded from volume to avoid double counting.
    expect(a.volumeByAsset).toEqual({ deposit: { [USDC]: '150' }, borrow: { [USDC]: '30' } });
  });
});

describe('ElasticsearchEventSink', () => {
  function sinkWith(data: unknown) {
    const post = jest.fn().mockResolvedValue({ data });
    return {
      post,
      sink: new ElasticsearchEventSink({ url: 'http://es', http: { post } as never }),
    };
  }

  it('bulk-creates documents keyed by event id', async () => {
    const { post, sink } = sinkWith({ errors: false, items: [] });
    const e = normalizeEvent(typed('deposit', 1, account(), 7n));
    await sink.write([e]);

    const [url, body] = post.mock.calls[0]!;
    expect(url).toBe('/_bulk');
    const [action, doc] = (body as string)
      .trim()
      .split('\n')
      .map((l) => JSON.parse(l));
    expect(action).toEqual({ create: { _index: 'stellarlend-events', _id: e.id } });
    expect(doc).toMatchObject({ event_id: e.id, type: 'deposit', amount: '7', amount_numeric: 7 });
    expect(doc['@timestamp']).toBe(new Date(e.timestamp).toISOString());
  });

  it('treats 409 conflicts as already-indexed, but surfaces real failures', async () => {
    const e = normalizeEvent(typed('deposit', 1, account(), 7n));
    await expect(
      sinkWith({ errors: true, items: [{ create: { status: 409 } }] }).sink.write([e])
    ).resolves.toBeUndefined();
    await expect(
      sinkWith({
        errors: true,
        items: [
          { create: { status: 400, error: { type: 'mapper_parsing_exception', reason: 'bad' } } },
        ],
      }).sink.write([e])
    ).rejects.toThrow('mapper_parsing_exception');
  });
});
