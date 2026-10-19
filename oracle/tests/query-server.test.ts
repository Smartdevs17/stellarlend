/**
 * Issue #1371 / #1372 / #1373 – HTTP query API behaviour.
 */

import { describe, it, expect, beforeEach, afterAll } from 'vitest';
import { createQueryServer, type QueryServer } from '../src/api/query-server.js';
import { buildOpenApiDocument } from '../src/api/openapi.js';
import { createPriceHistoryService } from '../src/services/price-history.js';
import type { AggregatedPrice } from '../src/types/index.js';

const BASE_TS = 1_735_689_600;

const PRICES: Record<string, AggregatedPrice> = {
  XLM: {
    asset: 'XLM',
    price: 110_000n,
    timestamp: BASE_TS,
    confidence: 0.98,
    sources: [{ source: 'coingecko' } as any],
  },
};

describe('QueryServer', () => {
  let server: QueryServer;
  let baseUrl: string;

  beforeEach(async () => {
    const priceHistory = createPriceHistoryService({ queryCacheTtlMs: 60_000 });
    for (let i = 0; i < 5; i += 1) {
      priceHistory.addPriceEntry('XLM', BigInt(100 + i), BASE_TS + i * 60);
    }

    server = createQueryServer({
      port: 0,
      mode: 'pull',
      priceHistory,
      getPrices: async (assets) => {
        const requested = assets ?? ['XLM', 'USDC'];
        return requested.map((a) => PRICES[a.toUpperCase()]).filter(Boolean) as AggregatedPrice[];
      },
    });

    const port = await server.start();
    baseUrl = `http://127.0.0.1:${port}`;
  });

  afterAll(async () => {
    await server?.stop();
  });

  it('reports the configured mode and serializes prices', async () => {
    const res = await fetch(`${baseUrl}/prices`);
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.mode).toBe('pull');
    expect(body.data).toHaveLength(1);
    expect(body.data[0]).toMatchObject({ asset: 'XLM', price: '110000', sources: ['coingecko'] });
  });

  it('filters /prices by asset query param', async () => {
    const res = await fetch(`${baseUrl}/prices?asset=usdc`);
    const body = await res.json();
    expect(body.data).toEqual([]); // only XLM is stubbed
  });

  it('returns 404 for an unknown asset', async () => {
    const res = await fetch(`${baseUrl}/prices/NOPE`);
    expect(res.status).toBe(404);
  });

  it('paginates history with cursors', async () => {
    const first = await (await fetch(`${baseUrl}/prices/XLM/history?limit=2`)).json();
    expect(first.data.entries).toHaveLength(2);
    expect(first.data.pagination.hasMore).toBe(true);

    const cursor = first.data.pagination.nextCursor;
    const second = await (
      await fetch(`${baseUrl}/prices/XLM/history?limit=2&cursor=${cursor}`)
    ).json();
    expect(second.data.entries[0].timestamp).toBeGreaterThan(first.data.entries[1].timestamp);
  });

  it('applies from/to filters', async () => {
    const res = await fetch(
      `${baseUrl}/prices/XLM/history?from=${BASE_TS + 60}&to=${BASE_TS + 120}`
    );
    const body = await res.json();
    expect(body.data.entries.map((e: any) => e.timestamp)).toEqual([BASE_TS + 60, BASE_TS + 120]);
  });

  it('rejects a non-integer limit with 400', async () => {
    const res = await fetch(`${baseUrl}/prices/XLM/history?limit=abc`);
    expect(res.status).toBe(400);
  });

  it('serves the generated OpenAPI document', async () => {
    const res = await fetch(`${baseUrl}/openapi.json`);
    expect(res.status).toBe(200);
    expect(await res.json()).toEqual(buildOpenApiDocument());
  });

  it('responds to /health', async () => {
    expect((await fetch(`${baseUrl}/health`)).status).toBe(200);
  });

  it('rejects non-GET methods', async () => {
    const res = await fetch(`${baseUrl}/prices`, { method: 'POST' });
    expect(res.status).toBe(405);
  });
});
