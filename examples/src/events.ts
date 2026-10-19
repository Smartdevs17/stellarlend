/**
 * Example: stream every event a contract emitted, with SDK auto-pagination.
 *
 * Soroban RPC's `getEvents` returns one page plus a cursor. `paginate` hides the cursor
 * bookkeeping, fetches pages lazily (break out of the loop and no further pages are
 * requested), and `maxItems` bounds the scan.
 */
import { scValToNative, type rpc } from '@stellar/stellar-sdk';
import { paginate, type PageFetcher, type PaginateOptions } from '@stellarlend/sdk';

/** The subset of `rpc.Server` this example needs (lets tests pass a stub). */
export type EventsServer = Pick<rpc.Server, 'getEvents'>;

export interface ContractEvent {
  id: string;
  ledger: number;
  txHash: string;
  /** Decoded topics; the first is usually the event name. */
  topics: unknown[];
  value: unknown;
}

/** A `PageFetcher` over `getEvents` for one contract, starting at `startLedger`. */
export function contractEventsFetcher(
  server: EventsServer,
  contractId: string,
  startLedger: number,
): PageFetcher<ContractEvent> {
  const filters: rpc.Api.EventFilter[] = [{ type: 'contract', contractIds: [contractId] }];
  return async ({ cursor, limit }) => {
    // RPC accepts either a start ledger or a cursor, never both.
    const page = await server.getEvents(cursor ? { filters, cursor, limit } : { filters, startLedger, limit });
    const items = page.events.map((event) => ({
      id: event.id,
      ledger: event.ledger,
      txHash: event.txHash,
      topics: event.topic.map((topic) => scValToNative(topic)),
      value: scValToNative(event.value),
    }));
    // RPC always returns a cursor; a short page is how it signals the end.
    return { items, nextCursor: items.length < limit ? null : page.cursor };
  };
}

export function contractEvents(
  server: EventsServer,
  contractId: string,
  startLedger: number,
  options?: PaginateOptions,
): AsyncGenerator<ContractEvent, void, undefined> {
  return paginate(contractEventsFetcher(server, contractId, startLedger), options);
}

/** Counts events by name (first topic), e.g. `{ deposit: 3, borrow: 2 }`. */
export async function countEventsByName(events: AsyncIterable<ContractEvent>): Promise<Record<string, number>> {
  const counts: Record<string, number> = {};
  for await (const event of events) {
    const name = String(event.topics[0] ?? 'unknown');
    counts[name] = (counts[name] ?? 0) + 1;
  }
  return counts;
}
