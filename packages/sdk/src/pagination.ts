/**
 * Auto-pagination helpers.
 *
 * Contract and indexer list endpoints return one page at a time. These helpers turn a
 * page fetcher into an async iterator, so callers write `for await (const item of …)`
 * instead of tracking offsets or cursors by hand.
 */

export interface Page<T, C = string> {
  items: T[];
  /** Cursor for the next page; `null`/`undefined` means this was the last page. */
  nextCursor?: C | null;
}

export interface PageRequest<C = string> {
  /** `undefined` for the first page. */
  cursor: C | undefined;
  limit: number;
}

export type PageFetcher<T, C = string> = (request: PageRequest<C>) => Promise<Page<T, C>>;

export interface PaginateOptions<C = string> {
  /** Items requested per page. Defaults to `DEFAULT_PAGE_SIZE`. */
  pageSize?: number;
  /** Cursor to resume from. */
  startCursor?: C;
  /** Stop after this many items in total. */
  maxItems?: number;
  /** Stop after this many pages, as a guard against endpoints that never terminate. */
  maxPages?: number;
}

export const DEFAULT_PAGE_SIZE = 50;

function requirePositiveInteger(name: string, value: number | undefined): void {
  if (value !== undefined && (!Number.isInteger(value) || value <= 0)) {
    throw new RangeError(`${name} must be a positive integer`);
  }
}

/** Iterates pages from a cursor-based fetcher until it reports no next cursor. */
export async function* paginatePages<T, C = string>(
  fetchPage: PageFetcher<T, C>,
  options: PaginateOptions<C> = {},
): AsyncGenerator<Page<T, C>, void, undefined> {
  const pageSize = options.pageSize ?? DEFAULT_PAGE_SIZE;
  requirePositiveInteger('pageSize', pageSize);
  requirePositiveInteger('maxItems', options.maxItems);
  requirePositiveInteger('maxPages', options.maxPages);

  let cursor = options.startCursor;
  let remaining = options.maxItems ?? Infinity;
  for (let pages = 0; remaining > 0 && pages < (options.maxPages ?? Infinity); pages++) {
    const page = await fetchPage({ cursor, limit: Math.min(pageSize, remaining) });
    const items = page.items.length > remaining ? page.items.slice(0, remaining) : page.items;
    remaining -= items.length;
    yield { ...page, items };
    if (page.nextCursor == null || page.items.length === 0) return;
    if (page.nextCursor === cursor) throw new Error('Page fetcher returned the same cursor twice');
    cursor = page.nextCursor;
  }
}

/** Iterates every item from a cursor-based fetcher, fetching pages lazily. */
export async function* paginate<T, C = string>(
  fetchPage: PageFetcher<T, C>,
  options: PaginateOptions<C> = {},
): AsyncGenerator<T, void, undefined> {
  for await (const page of paginatePages(fetchPage, options)) yield* page.items;
}

/**
 * Adapts an offset/limit endpoint (e.g. a contract `get_*_page(start, limit)`) to
 * `paginate`. A page shorter than `limit` is treated as the last page.
 */
export function offsetFetcher<T>(
  fetch: (offset: number, limit: number) => Promise<T[]>,
): PageFetcher<T, number> {
  return async ({ cursor = 0, limit }) => {
    const items = await fetch(cursor, limit);
    return { items, nextCursor: items.length < limit ? null : cursor + items.length };
  };
}

/** Drains an async iterable into an array. Prefer `maxItems` for unbounded sources. */
export async function collect<T>(iterable: AsyncIterable<T>): Promise<T[]> {
  const items: T[] = [];
  for await (const item of iterable) items.push(item);
  return items;
}
