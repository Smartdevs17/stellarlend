import { StellarService } from './stellar.service';
import { redisCacheService } from './redisCache.service';
import logger from '../utils/logger';

const EVENTS_CACHE_TTL_S = 60;

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
    const events = await stellarService.readContract('get_indexed_events', filters);
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
