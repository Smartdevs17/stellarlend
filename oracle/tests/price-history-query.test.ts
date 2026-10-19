/**
 * Issue #1371 / #1372 – Historical price queries: time filters, cursor
 * pagination, bounded page size, and caching.
 */

import { describe, it, expect, beforeEach } from 'vitest';
import {
  createPriceHistoryService,
  DEFAULT_QUERY_LIMIT,
  MAX_QUERY_LIMIT,
  type PriceHistoryService,
} from '../src/services/price-history.js';

const BASE_TS = 1_735_689_600; // 2025-01-01T00:00:00Z

describe('PriceHistoryService.queryHistory()', () => {
  let service: PriceHistoryService;

  beforeEach(() => {
    service = createPriceHistoryService({ maxEntries: 50, queryCacheTtlMs: 60_000 });
    for (let i = 0; i < 20; i += 1) {
      service.addPriceEntry('XLM', BigInt(100 + i), BASE_TS + i * 60);
    }
  });

  it('returns history for an asset in time order', () => {
    const page = service.queryHistory('xlm', { limit: 5 });
    expect(page.asset).toBe('XLM');
    expect(page.entries).toHaveLength(5);
    expect(page.entries[0]!.timestamp).toBe(BASE_TS);
    expect(page.pagination.total).toBe(20);
    expect(page.pagination.hasMore).toBe(true);
  });

  it('filters by inclusive from/to bounds', () => {
    const page = service.queryHistory('XLM', {
      from: BASE_TS + 300,
      to: BASE_TS + 600,
      limit: 10,
    });
    expect(page.entries.map((e) => e.timestamp)).toEqual([
      BASE_TS + 300,
      BASE_TS + 360,
      BASE_TS + 420,
      BASE_TS + 480,
      BASE_TS + 540,
      BASE_TS + 600,
    ]);
    expect(page.pagination.total).toBe(6);
  });

  it('paginates with cursors without gaps or duplicates', () => {
    const first = service.queryHistory('XLM', { limit: 7 });
    expect(first.pagination.nextCursor).toBe(String(BASE_TS + 6 * 60));

    const second = service.queryHistory('XLM', {
      limit: 7,
      cursor: Number(first.pagination.nextCursor),
    });
    expect(second.entries[0]!.timestamp).toBe(BASE_TS + 7 * 60);
    expect(second.pagination.hasMore).toBe(true);

    const third = service.queryHistory('XLM', {
      limit: 7,
      cursor: Number(second.pagination.nextCursor),
    });
    expect(third.pagination.hasMore).toBe(false);
    expect(third.pagination.nextCursor).toBeNull();

    const timestamps = [...first.entries, ...second.entries, ...third.entries].map(
      (e) => e.timestamp
    );
    expect(new Set(timestamps).size).toBe(20);
  });

  it('caps the page size server-side', () => {
    const page = service.queryHistory('XLM', { limit: 10_000 });
    expect(page.pagination.limit).toBe(MAX_QUERY_LIMIT);
    expect(page.entries.length).toBeLessThanOrEqual(MAX_QUERY_LIMIT);
  });

  it('uses a default limit when none is provided', () => {
    expect(service.queryHistory('XLM').pagination.limit).toBe(DEFAULT_QUERY_LIMIT);
  });

  it('serves repeated queries from cache and marks them cached', () => {
    const first = service.queryHistory('XLM', { limit: 5 });
    expect(first.cached).toBe(false);

    const second = service.queryHistory('XLM', { limit: 5 });
    expect(second.cached).toBe(true);
    expect(second.entries).toEqual(first.entries);
  });

  it('invalidates cached results when a new price arrives', () => {
    service.queryHistory('XLM', { limit: 5 });
    service.addPriceEntry('XLM', 999n, BASE_TS + 10_000);

    const refreshed = service.queryHistory('XLM', { limit: 5 });
    expect(refreshed.cached).toBe(false);
    expect(refreshed.pagination.total).toBe(21);
  });

  it('returns an empty page for an unknown asset', () => {
    const page = service.queryHistory('NOPE');
    expect(page.entries).toEqual([]);
    expect(page.pagination.total).toBe(0);
    expect(page.pagination.nextCursor).toBeNull();
  });
});
