/**
 * Event replay for debugging.
 *
 * Re-applies an ordered event stream (hot + archived) to reconstruct each
 * account's collateral and debt per asset, recording every state transition
 * and flagging anomalies — e.g. a withdrawal or repay that drives a balance
 * negative, which means either missing events in the index or a contract
 * accounting bug. Typed events are authoritative by default; the structured
 * `proto_evt` envelope can be replayed instead to cross-check the two layers.
 */

import type { IndexedEvent } from './types';

export type ReplaySource = 'typed' | 'structured';

export interface AccountState {
  collateral: Record<string, string>;
  debt: Record<string, string>;
  eventCount: number;
}

export interface StateTransition {
  eventId: string;
  ledger: number;
  account: string;
  field: 'collateral' | 'debt';
  asset: string;
  before: string;
  after: string;
}

export interface ReplayAnomaly {
  eventId: string;
  ledger: number;
  account: string;
  asset: string;
  field: 'collateral' | 'debt';
  balance: string;
  reason: string;
}

export interface ReplayResult {
  source: ReplaySource;
  eventsReplayed: number;
  eventsApplied: number;
  fromLedger: number | null;
  toLedger: number | null;
  finalState: Record<string, AccountState>;
  transitions: StateTransition[];
  transitionsTruncated: boolean;
  anomalies: ReplayAnomaly[];
}

const NATIVE = 'native';

type Delta = { account: string; field: 'collateral' | 'debt'; asset: string; amount: bigint };

function big(value: unknown): bigint {
  if (typeof value === 'string' && /^-?\d+$/.test(value)) return BigInt(value);
  if (typeof value === 'number' && Number.isInteger(value)) return BigInt(value);
  return 0n;
}

function assetOf(value: unknown): string {
  return typeof value === 'string' && value ? value : NATIVE;
}

/** Map one event to its balance deltas under the chosen source layer. */
export function deltasFor(e: IndexedEvent, source: ReplaySource): Delta[] {
  if (source === 'typed') {
    if (e.envelope !== 'typed' || !e.actor) return [];
    const asset = assetOf(e.data.asset);
    const amount = big(e.data.amount);
    // Names cover both contracts: hello-world (`deposit`, `withdrawal`) and
    // lending (`vault_deposit`, `withdraw`, `borrow_collateral_deposit`).
    switch (e.type) {
      case 'deposit':
      case 'vault_deposit':
      case 'borrow_collateral_deposit':
        return [{ account: e.actor, field: 'collateral', asset, amount }];
      case 'withdrawal':
      case 'withdraw':
        return [{ account: e.actor, field: 'collateral', asset, amount: -amount }];
      case 'borrow':
        return [{ account: e.actor, field: 'debt', asset, amount }];
      case 'repay':
        return [{ account: e.actor, field: 'debt', asset, amount: -amount }];
      case 'liquidation': {
        const borrower = e.counterparty;
        if (!borrower) return [];
        return [
          {
            account: borrower,
            field: 'debt',
            asset: assetOf(e.data.debt_asset),
            amount: -big(e.data.debt_liquidated),
          },
          {
            account: borrower,
            field: 'collateral',
            asset: assetOf(e.data.collateral_asset),
            amount: -big(e.data.collateral_seized),
          },
        ];
      }
      default:
        return [];
    }
  }

  if (e.envelope !== 'structured_event_v1' || !e.actor) return [];
  const asset = assetOf(e.data.asset);
  const amount = big(e.data.amount);
  switch (e.action) {
    case 'deposit':
      return [{ account: e.actor, field: 'collateral', asset, amount }];
    case 'withdraw':
      return [{ account: e.actor, field: 'collateral', asset, amount: -amount }];
    case 'borrow':
      return [{ account: e.actor, field: 'debt', asset, amount }];
    case 'repay':
      return [{ account: e.actor, field: 'debt', asset, amount: -amount }];
    case 'liquidate':
      return e.counterparty
        ? [{ account: e.counterparty, field: 'debt', asset, amount: -amount }]
        : [];
    default:
      return [];
  }
}

export function replayEvents(
  events: Iterable<IndexedEvent>,
  options: { source?: ReplaySource; account?: string; maxTransitions?: number } = {}
): ReplayResult {
  const source = options.source ?? 'typed';
  const maxTransitions = options.maxTransitions ?? 1000;
  const balances = new Map<
    string,
    { collateral: Map<string, bigint>; debt: Map<string, bigint>; eventCount: number }
  >();
  const transitions: StateTransition[] = [];
  const anomalies: ReplayAnomaly[] = [];
  let replayed = 0;
  let applied = 0;
  let fromLedger: number | null = null;
  let toLedger: number | null = null;
  let truncated = false;

  for (const e of events) {
    replayed += 1;
    fromLedger ??= e.ledger;
    toLedger = e.ledger;
    const deltas = deltasFor(e, source).filter(
      (d) => !options.account || d.account === options.account
    );
    if (deltas.length) applied += 1;

    for (const d of deltas) {
      let state = balances.get(d.account);
      if (!state) {
        state = { collateral: new Map(), debt: new Map(), eventCount: 0 };
        balances.set(d.account, state);
      }
      const book = state[d.field];
      const before = book.get(d.asset) ?? 0n;
      const after = before + d.amount;
      book.set(d.asset, after);
      state.eventCount += 1;

      if (transitions.length < maxTransitions) {
        transitions.push({
          eventId: e.id,
          ledger: e.ledger,
          account: d.account,
          field: d.field,
          asset: d.asset,
          before: before.toString(),
          after: after.toString(),
        });
      } else {
        truncated = true;
      }

      if (after < 0n) {
        anomalies.push({
          eventId: e.id,
          ledger: e.ledger,
          account: d.account,
          asset: d.asset,
          field: d.field,
          balance: after.toString(),
          reason: `${d.field} balance went negative — events missing from the index before ledger ${e.ledger}, or an accounting bug`,
        });
      }
    }
  }

  const finalState: Record<string, AccountState> = {};
  for (const [account, s] of balances) {
    const render = (m: Map<string, bigint>) =>
      Object.fromEntries([...m].filter(([, v]) => v !== 0n).map(([k, v]) => [k, v.toString()]));
    finalState[account] = {
      collateral: render(s.collateral),
      debt: render(s.debt),
      eventCount: s.eventCount,
    };
  }

  return {
    source,
    eventsReplayed: replayed,
    eventsApplied: applied,
    fromLedger,
    toLedger,
    finalState,
    transitions,
    transitionsTruncated: truncated,
    anomalies,
  };
}
