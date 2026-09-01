import { StellarService } from './stellar.service';
import { redisCacheService } from './redisCache.service';
import logger from '../utils/logger';

const EVENTS_CACHE_TTL_S = 60;

// --- Structured event schema exports (kept in sync with on-chain contract) ---
export const EVENT_SCHEMA_VERSION = 1;

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

export interface IndexedEvent {
  id: string;
  type: string;
  contract: string;
  topic?: string[];
  data: Record<string, any>;
  timestamp: number;
  ledger: number;
}

export interface EventStats {
  totalEvents: number;
  eventTypeCounts: Record<string, number>;
  lastUpdated: number;
}

const stellarService = new StellarService();

export async function getIndexedEvents(filters: {
  type?: string;
  address?: string;
  from?: number;
  to?: number;
  limit: number;
}): Promise<IndexedEvent[]> {
  const cacheKey = redisCacheService.buildKey(
    'events',
    `query:${filters.type ?? 'all'}:${filters.address ?? 'all'}:${filters.from ?? 0}:${filters.to ?? 0}:${filters.limit}`
  );
  const cached = await redisCacheService.get<IndexedEvent[]>(cacheKey);
  if (cached) return cached;

  try {
    const events = await (stellarService as any).readIndexedEvents(filters);
    const result = Array.isArray(events) ? events : [];
    await redisCacheService.set(cacheKey, result, EVENTS_CACHE_TTL_S);
    return result;
  } catch (error) {
    logger.warn('Failed to fetch indexed events from contract', { error, filters });
    return [];
  }
}

export function getEventTypes(): string[] {
  return [
    'deposit',
    'withdraw',
    'borrow',
    'repay',
    'liquidation',
    'flash_loan',
    'admin_action',
    'price_updated',
    'risk_params_updated',
    'pause_state_changed',
    'position_updated',
    'analytics_updated',
    'user_activity_tracked',
  ];
}

export function getEventStats(): EventStats {
  const eventTypeCounts: Record<string, number> = {};
  for (const type of getEventTypes()) {
    eventTypeCounts[type] = 0;
  }

  return {
    totalEvents: 0,
    eventTypeCounts,
    lastUpdated: Date.now(),
  };
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
    description:
      'Versioned, self-describing structured envelope emitted alongside typed events',
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
