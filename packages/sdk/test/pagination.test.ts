// Unit tests for src/pagination.ts and StellarLendClient.paginate.
//
//   npm run test:unit -w @stellarlend/sdk
import { test } from 'node:test';
import assert from 'node:assert/strict';

import {
  DEFAULT_PAGE_SIZE,
  StellarLendClient,
  collect,
  offsetFetcher,
  paginate,
  paginatePages,
  type ContractInvocation,
  type PageRequest,
} from '../src';
import { FakeAdapter } from '../examples/support/fake-adapter';

const range = (n: number) => Array.from({ length: n }, (_, i) => i);

/** A cursor endpoint over `items` that records every request it receives. */
function cursorSource(items: number[]) {
  const requests: PageRequest[] = [];
  const fetchPage = async (request: PageRequest) => {
    requests.push(request);
    const start = request.cursor ? Number(request.cursor) : 0;
    const end = start + request.limit;
    return { items: items.slice(start, end), nextCursor: end < items.length ? String(end) : null };
  };
  return { fetchPage, requests };
}

test('paginate yields every item across pages and stops at the last page', async () => {
  const { fetchPage, requests } = cursorSource(range(25));
  assert.deepEqual(await collect(paginate(fetchPage, { pageSize: 10 })), range(25));
  assert.deepEqual(requests, [
    { cursor: undefined, limit: 10 },
    { cursor: '10', limit: 10 },
    { cursor: '20', limit: 10 },
  ]);
});

test('paginate fetches lazily: breaking early skips later pages', async () => {
  const { fetchPage, requests } = cursorSource(range(100));
  for await (const item of paginate(fetchPage, { pageSize: 10 })) {
    if (item === 4) break;
  }
  assert.equal(requests.length, 1);
});

test('maxItems caps the total and shrinks the last request', async () => {
  const { fetchPage, requests } = cursorSource(range(100));
  assert.deepEqual(await collect(paginate(fetchPage, { pageSize: 10, maxItems: 15 })), range(15));
  assert.deepEqual(requests.map((r) => r.limit), [10, 5]);
});

test('maxItems trims pages from endpoints that ignore the limit', async () => {
  const fetchPage = async () => ({ items: range(30), nextCursor: 'more' });
  assert.deepEqual(await collect(paginate(fetchPage, { maxItems: 7 })), range(7));
});

test('startCursor resumes and maxPages bounds the iteration', async () => {
  const { fetchPage, requests } = cursorSource(range(100));
  const pages = await collect(paginatePages(fetchPage, { pageSize: 10, startCursor: '50', maxPages: 2 }));
  assert.deepEqual(pages.map((p) => p.items[0]), [50, 60]);
  assert.equal(requests.length, 2);
});

test('pageSize defaults to DEFAULT_PAGE_SIZE and invalid options are rejected', async () => {
  const { fetchPage, requests } = cursorSource(range(3));
  await collect(paginate(fetchPage));
  assert.equal(requests[0]!.limit, DEFAULT_PAGE_SIZE);

  for (const options of [{ pageSize: 0 }, { pageSize: 1.5 }, { maxItems: -1 }, { maxPages: 0 }]) {
    await assert.rejects(collect(paginate(fetchPage, options)), RangeError);
  }
});

test('a fetcher that repeats its cursor is reported instead of looping forever', async () => {
  const fetchPage = async () => ({ items: [1], nextCursor: 'stuck' });
  await assert.rejects(collect(paginate(fetchPage, { startCursor: 'stuck' })), /same cursor/);
});

test('an empty page ends iteration even if a cursor is returned', async () => {
  let calls = 0;
  const fetchPage = async () => ({ items: [] as number[], nextCursor: String(++calls) });
  assert.deepEqual(await collect(paginate(fetchPage)), []);
  assert.equal(calls, 1);
});

test('offsetFetcher advances by items received and ends on a short page', async () => {
  const calls: Array<[number, number]> = [];
  const fetcher = offsetFetcher(async (offset, limit) => {
    calls.push([offset, limit]);
    return range(23).slice(offset, offset + limit);
  });
  assert.deepEqual(await collect(paginate(fetcher, { pageSize: 10 })), range(23));
  assert.deepEqual(calls, [[0, 10], [10, 10], [20, 10]]);
});

test('offsetFetcher on an exact multiple ends on the trailing empty page', async () => {
  const fetcher = offsetFetcher(async (offset, limit) => range(20).slice(offset, offset + limit));
  assert.deepEqual(await collect(paginate(fetcher, { pageSize: 10 })), range(20));
});

test('client.paginate appends offset/limit to a read-only entrypoint', async () => {
  const recipients = range(7).map((i) => `G${i}`);
  const adapter = new FakeAdapter().on('get_recipients_page', ({ args }: ContractInvocation) => {
    const [offset, limit] = args as [number, number];
    return recipients.slice(offset, offset + limit);
  });
  const client = new StellarLendClient({ contractId: 'CCONTRACT', adapter });

  assert.deepEqual(await collect(client.paginate<string>('get_recipients_page', [], { pageSize: 3 })), recipients);
  assert.deepEqual(adapter.calls.map((c) => c.args), [[0, 3], [3, 3], [6, 3]]);
  assert.ok(adapter.calls.every((c) => c.readonly));
});

test('client.paginate keeps leading arguments before offset/limit', async () => {
  const adapter = new FakeAdapter().on('get_history_page', () => []);
  const client = new StellarLendClient({ contractId: 'CCONTRACT', adapter });
  await collect(client.paginate('get_history_page', ['GUSER'], { pageSize: 5 }));
  assert.deepEqual(adapter.calls[0]!.args, ['GUSER', 0, 5]);
  assert.throws(() => client.paginate(''), TypeError);
});
