import express from 'express';
import request from 'supertest';
import eventsRoutes from '../routes/events';
import { errorHandler } from '../middleware/errorHandler';
import { setEventIndexer } from '../services/eventIndex';
import type { EventIndexer } from '../services/eventIndex/indexer';
import {
  account,
  BASE_TIME,
  LEDGER_MS,
  memoryIndexer,
  resetSeq,
  structured,
  typed,
  USDC,
} from '../services/eventIndex/__fixtures__/events';

// Mount the router with the real error handler rather than importing the
// full app, so these tests don't depend on unrelated route modules loading.
const app = express();
app.use(express.json());
app.use('/api/events', eventsRoutes);
app.use(errorHandler);

const OPERATOR = { 'x-user-role': 'operator', 'x-user-address': 'GOPERATOR' };

describe('Event indexing routes (/api/events)', () => {
  const [alice, bob] = [account(), account()];
  let indexer: EventIndexer;

  beforeEach(async () => {
    resetSeq();
    ({ indexer } = memoryIndexer());
    await indexer.ingest([
      typed('deposit', 1, alice, 1_000n),
      typed('borrow', 2, alice, 300n),
      typed('deposit', 3, bob, 500n),
      structured(4, 'Lending', 'Borrow', bob, 100n),
    ]);
    setEventIndexer(indexer);
  });

  afterAll(() => setEventIndexer(null));

  describe('GET /api/events', () => {
    it('returns a paginated page, newest first', async () => {
      const res = await request(app).get('/api/events');
      expect(res.status).toBe(200);
      expect(res.body.events.map((e: { ledger: number }) => e.ledger)).toEqual([4, 3, 2, 1]);
      expect(res.body).toMatchObject({ matched: 4, nextCursor: null, source: 'hot' });
      expect(typeof res.body.tookMs).toBe('number');
    });

    it('filters by type, account (and legacy `address`) and time', async () => {
      const byType = await request(app).get('/api/events').query({ type: 'deposit' });
      expect(byType.body.events).toHaveLength(2);

      const byAccount = await request(app).get('/api/events').query({ account: alice });
      expect(byAccount.body.events.every((e: { actor: string }) => e.actor === alice)).toBe(true);

      const legacy = await request(app).get('/api/events').query({ address: alice });
      expect(legacy.body.events).toEqual(byAccount.body.events);

      const window = await request(app)
        .get('/api/events')
        .query({
          from: new Date(BASE_TIME + 2 * LEDGER_MS).toISOString(),
          to: BASE_TIME + 3 * LEDGER_MS,
        });
      expect(window.body.events.map((e: { ledger: number }) => e.ledger)).toEqual([3, 2]);
    });

    it('filters structured envelopes by module and action', async () => {
      const res = await request(app)
        .get('/api/events')
        .query({ module: 'lending', action: 'borrow' });
      expect(res.body.events).toHaveLength(1);
      expect(res.body.events[0]).toMatchObject({ type: 'proto_evt', actor: bob, amount: '100' });
    });

    it('follows nextCursor across pages', async () => {
      const first = await request(app).get('/api/events').query({ limit: 3, order: 'asc' });
      expect(first.body.events).toHaveLength(3);
      const second = await request(app)
        .get('/api/events')
        .query({ limit: 3, order: 'asc', cursor: first.body.nextCursor });
      expect(second.body.events.map((e: { ledger: number }) => e.ledger)).toEqual([4]);
      expect(second.body.nextCursor).toBeNull();
    });

    it.each([
      [{ limit: '0' }, 'limit'],
      [{ order: 'sideways' }, 'order'],
      [{ from: 'not-a-date' }, 'from'],
      [{ from: '10', to: '5' }, 'from must be <= to'],
      [{ includeArchived: 'maybe' }, 'includeArchived'],
    ])('rejects invalid filter %o with 400', async (query, message) => {
      const res = await request(app).get('/api/events').query(query);
      expect(res.status).toBe(400);
      expect(JSON.stringify(res.body)).toContain(message);
    });
  });

  it('GET /api/events/:id returns one event or 404', async () => {
    const list = await request(app).get('/api/events').query({ limit: 1 });
    const id = list.body.events[0].id;
    const found = await request(app).get(`/api/events/${id}`);
    expect(found.status).toBe(200);
    expect(found.body.id).toBe(id);
    expect((await request(app).get('/api/events/000-missing')).status).toBe(404);
  });

  it('GET /api/events/types and /stats reflect indexed data', async () => {
    const types = await request(app).get('/api/events/types');
    expect(types.body).toEqual(
      expect.arrayContaining(['deposit', 'withdrawal', 'borrow', 'proto_evt'])
    );

    const stats = await request(app).get('/api/events/stats');
    expect(stats.body.totalEvents).toBe(4);
    expect(stats.body.eventTypeCounts).toMatchObject({
      deposit: 2,
      borrow: 1,
      proto_evt: 1,
      repay: 0,
    });
  });

  it('GET /api/events/analytics returns dashboard aggregates', async () => {
    const res = await request(app).get('/api/events/analytics').query({ bucket: 'day' });
    expect(res.status).toBe(200);
    expect(res.body).toMatchObject({
      totalEvents: 4,
      bucket: 'day',
      volumeByAsset: { deposit: { [USDC]: '1500' }, borrow: { [USDC]: '300' } },
    });
    expect(res.body.queryLatency).toHaveProperty('p95Ms');
  });

  it('GET /api/events/indexer/status reports ingestion and archive state', async () => {
    const res = await request(app).get('/api/events/indexer/status');
    expect(res.status).toBe(200);
    expect(res.body).toMatchObject({
      hotEvents: 4,
      totalIngested: 4,
      latestIndexedLedger: 4,
      schemaVersion: 1,
    });
    expect(res.body.archive).toMatchObject({ segments: 0, events: 0 });
  });

  describe('operator endpoints', () => {
    it('require the operator role', async () => {
      for (const path of [
        '/api/events/archive',
        '/api/events/replay',
        '/api/events/indexer/sync',
      ]) {
        const res = await request(app).post(path).set('x-user-role', 'viewer').send({});
        expect(res.status).toBe(401);
      }
    });

    it('POST /api/events/archive moves old events to cold storage', async () => {
      // Every seeded event is from 2026-01-01, far older than one day.
      const res = await request(app)
        .post('/api/events/archive')
        .set(OPERATOR)
        .send({ olderThanDays: 1 });
      expect(res.status).toBe(200);
      expect(res.body.archived).toBe(4);

      const hot = await request(app).get('/api/events');
      expect(hot.body.events).toHaveLength(0);
      const all = await request(app).get('/api/events').query({ includeArchived: 'true' });
      expect(all.body.events).toHaveLength(4);

      const manifest = await request(app).get('/api/events/archive');
      expect(manifest.body.segments).toHaveLength(1);
    });

    it('POST /api/events/replay rebuilds account state', async () => {
      const res = await request(app)
        .post('/api/events/replay')
        .set(OPERATOR)
        .send({ account: alice });
      expect(res.status).toBe(200);
      expect(res.body.finalState[alice]).toEqual({
        collateral: { [USDC]: '1000' },
        debt: { [USDC]: '300' },
        eventCount: 2,
      });
      expect(res.body.anomalies).toEqual([]);
    });

    it('POST /api/events/indexer/sync pulls from the source', async () => {
      const res = await request(app).post('/api/events/indexer/sync').set(OPERATOR).send();
      expect(res.status).toBe(200);
      expect(res.body).toMatchObject({ pages: 1, fetched: 0 });
    });
  });
});
