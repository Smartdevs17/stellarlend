import { getEventIndexer } from './eventIndex';
import { CURRENT_SCHEMA_VERSION } from './eventIndex/schema';
import type { EventPage, EventQuery } from './eventIndex/types';

// --- Structured event schema exports (kept in sync with on-chain contract) ---
export const EVENT_SCHEMA_VERSION = CURRENT_SCHEMA_VERSION;

export const EVENT_MODULES = Object.freeze([
  'lending',
  'collateral',
  'liquidation',
  'oracle',
  'governance',
  'treasury',
  'risk',
  'flash_loan',
  'admin',
  'emergency',
]);

export const EVENT_ACTIONS = Object.freeze([
  'deposit',
  'withdraw',
  'borrow',
  'repay',
  'liquidate',
  'price_update',
  'params_update',
  'pause',
  'unpause',
  'proposal_created',
  'vote_cast',
  'execute',
  'claim',
  'flash_loan',
  'other',
]);

export type { IndexedEvent } from './eventIndex/types';

export interface EventStats {
  totalEvents: number;
  eventTypeCounts: Record<string, number>;
  lastUpdated: number;
}

/**
 * Event names as published on-chain: typed events use their snake_case struct
 * name minus `_event` (e.g. `WithdrawalEvent` → `withdrawal`), and structured
 * envelopes use the `proto_evt` topic prefix. The lending contract's names
 * are pinned by `stellar-lend/contracts/lending/tests/event_topics.rs`.
 */
const KNOWN_EVENT_TYPES = [
  'deposit',
  'vault_deposit',
  'borrow_collateral_deposit',
  'withdrawal',
  'withdraw',
  'borrow',
  'repay',
  'liquidation',
  'flash_loan_initiated',
  'flash_loan_repaid',
  'admin_action',
  'price_updated',
  'risk_params_updated',
  'pause_state_changed',
  'position_updated',
  'analytics_updated',
  'user_activity_tracked',
  'proto_evt',
];

/** Query the event index (hot store, optionally including archived segments). */
export async function getIndexedEvents(query: EventQuery): Promise<EventPage> {
  return getEventIndexer().query(query);
}

/** Known event types plus any other type the indexer has seen. */
export function getEventTypes(): string[] {
  const seen = getEventIndexer().store.types();
  return [...KNOWN_EVENT_TYPES, ...seen.filter((t) => !KNOWN_EVENT_TYPES.includes(t))];
}

export function getEventStats(): EventStats {
  const { totalEvents, byType } = getEventIndexer().analytics();
  const eventTypeCounts: Record<string, number> = {};
  for (const type of getEventTypes()) eventTypeCounts[type] = byType[type] ?? 0;
  return { totalEvents, eventTypeCounts, lastUpdated: Date.now() };
}

/**
 * Return a machine-readable catalog of the event schema and known typed
 * events. This is used by off-chain consumers to discover envelopes and
 * the topic layout.
 */
export function getEventSchemaCatalog() {
  const envelope = {
    name: 'structured_event_v1',
    module: 'lending',
    action: null,
    envelope: true,
    topicPrefix: 'proto_evt',
    description: 'Versioned, self-describing structured envelope emitted alongside typed events',
    fields: [
      { name: 'module', type: 'EventModule', topic: true },
      { name: 'action', type: 'EventAction', topic: true },
      { name: 'actor', type: 'Address', topic: true },
      { name: 'schema_version', type: 'u32', topic: false },
      { name: 'action_name', type: 'Symbol', topic: false },
      { name: 'asset', type: 'Option<Address>', topic: false },
      { name: 'amount', type: 'i128', topic: false },
      { name: 'counterparty', type: 'Option<Address>', topic: false },
      { name: 'metadata', type: 'Vec<StructuredEventField>', topic: false },
      { name: 'timestamp', type: 'u64', topic: false },
    ],
  };

  // Minimal typed event catalog — expand as needed. Tests expect a
  // `liquidation` entry with the two topic fields below.
  const typedEvents = [
    {
      name: 'liquidation',
      module: 'liquidation',
      action: 'liquidate',
      topicPrefix: 'liquidation',
      description: 'Borrower liquidation occurred',
      fields: [
        { name: 'liquidator', type: 'Address', topic: true },
        { name: 'borrower', type: 'Address', topic: true },
        { name: 'debt_asset', type: 'Address', topic: false },
        { name: 'debt_amount', type: 'i128', topic: false },
        { name: 'collateral_seized', type: 'i128', topic: false },
        { name: 'timestamp', type: 'u64', topic: false },
      ],
    },
    // A few common events to make the catalog useful to clients
    {
      name: 'deposit',
      module: 'lending',
      action: 'deposit',
      topicPrefix: 'deposit',
      fields: [
        { name: 'user', type: 'Address', topic: true },
        { name: 'asset', type: 'Address', topic: false },
        { name: 'amount', type: 'i128', topic: false },
        { name: 'timestamp', type: 'u64', topic: false },
      ],
    },
  ];

  return {
    schemaVersion: EVENT_SCHEMA_VERSION,
    envelope: 'structured_event_v1',
    modules: EVENT_MODULES,
    actions: EVENT_ACTIONS,
    events: [envelope, ...typedEvents],
  };
}

export function getEventSchemaByName(name: string) {
  const catalog = getEventSchemaCatalog();
  const found = catalog.events.find((e: any) => e.name === name);
  return found ?? null;
}

export function getEventSchemaVersion() {
  return { schemaVersion: EVENT_SCHEMA_VERSION };
}

export function getEventModules() {
  return { modules: [...EVENT_MODULES] };
}

export function getEventActions() {
  return { actions: [...EVENT_ACTIONS] };
}
