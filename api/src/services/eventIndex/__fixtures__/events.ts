/**
 * Builders for decoded Soroban contract events, shaped exactly as
 * `StellarService.fetchContractEvents` returns them (topics/values already run
 * through `scValToNative`: addresses as strings, i128/u64 as bigint, unit enum
 * variants as single-element arrays).
 */

import { Keypair } from '@stellar/stellar-sdk';
import { EventArchiver, MemoryColdStorage } from '../archive';
import { EventIndexer, EventSource } from '../indexer';
import type { RawContractEvent } from '../types';

export const CONTRACT = 'CCONTRACTAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA';
export const USDC = 'CUSDCAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA';
export const BASE_TIME = Date.parse('2026-01-01T00:00:00Z');
/** Soroban ledgers close roughly every 5 seconds. */
export const LEDGER_MS = 5_000;

export const account = (): string => Keypair.random().publicKey();

let seq = 0;

export function resetSeq(): void {
  seq = 0;
}

export function rawEvent(
  ledger: number,
  topic: unknown[],
  value: unknown,
  overrides: Partial<RawContractEvent> = {}
): RawContractEvent {
  seq += 1;
  return {
    id: `${String(ledger).padStart(12, '0')}-${String(seq).padStart(10, '0')}`,
    ledger,
    ledgerClosedAt: new Date(BASE_TIME + ledger * LEDGER_MS).toISOString(),
    contractId: CONTRACT,
    txHash: `tx${seq}`,
    inSuccessfulContractCall: true,
    topic,
    value,
    ...overrides,
  };
}

/** `DepositEvent` / `BorrowEvent` / … — `(name, user)` topics, map payload. */
export function typed(
  name: 'deposit' | 'withdrawal' | 'borrow' | 'repay',
  ledger: number,
  user: string,
  amount: bigint,
  asset: string | null = USDC
): RawContractEvent {
  return rawEvent(ledger, [`${name}_event`, user], {
    asset,
    amount,
    timestamp: BigInt(Math.floor((BASE_TIME + ledger * LEDGER_MS) / 1000)),
  });
}

export function liquidation(
  ledger: number,
  liquidator: string,
  borrower: string,
  debt: bigint,
  seized: bigint
): RawContractEvent {
  return rawEvent(ledger, ['liquidation_event', liquidator, borrower], {
    debt_asset: USDC,
    collateral_asset: USDC,
    debt_liquidated: debt,
    collateral_seized: seized,
    incentive_amount: 0n,
    timestamp: 0n,
  });
}

/** `StructuredEventV1` — `("proto_evt", module, action, actor)` topics. */
export function structured(
  ledger: number,
  module: string,
  action: string,
  actor: string,
  amount: bigint,
  extra: Record<string, unknown> = {}
): RawContractEvent {
  const snake = action.replace(/([a-z])([A-Z])/g, '$1_$2').toLowerCase();
  return rawEvent(ledger, ['proto_evt', [module], [action], actor], {
    schema_version: 1,
    action_name: snake,
    asset: USDC,
    amount,
    counterparty: null,
    metadata: [],
    timestamp: 0n,
    ...extra,
  });
}

export class FakeSource implements EventSource {
  pages: RawContractEvent[][] = [];
  requests: Array<{ startLedger?: number; cursor?: string; limit: number }> = [];
  latest = 1_000;
  fail: Error | null = null;

  async latestLedger(): Promise<number> {
    return this.latest;
  }

  async fetchEvents(request: { startLedger?: number; cursor?: string; limit: number }) {
    this.requests.push(request);
    if (this.fail) throw this.fail;
    const page = this.pages.shift() ?? [];
    return { events: page, cursor: `cursor-${this.requests.length}`, latestLedger: this.latest };
  }
}

export function memoryIndexer(
  overrides: Partial<ConstructorParameters<typeof EventIndexer>[0]> = {}
) {
  const storage = new MemoryColdStorage();
  const source = new FakeSource();
  const indexer = new EventIndexer({
    source,
    archiver: new EventArchiver(storage),
    pageSize: 3,
    ...overrides,
  });
  return { indexer, source, storage };
}
