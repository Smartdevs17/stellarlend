import { StellarService } from './stellar.service';
import { redisCacheService } from './redisCache.service';
import logger from '../utils/logger';

const INTEREST_CACHE_TTL_S = 30;

export interface InterestRateResponse {
  borrowRateBps: number;
  supplyRateBps: number;
  utilizationBps: number;
}

export interface RateHistoryEntry {
  borrowRateBps: number;
  supplyRateBps: number;
  utilizationBps: number;
  timestamp: number;
}

export interface SimulateRateRequest {
  utilizationBps: number;
}

export interface SimulateRateResponse {
  simulatedBorrowRateBps: number;
}

const stellarService = new StellarService();

export async function getCurrentBorrowRate(): Promise<number> {
  const cacheKey = redisCacheService.buildKey('interest', 'current:borrow');
  const cached = await redisCacheService.get<number>(cacheKey);
  if (cached !== null && cached !== undefined) return cached;

  try {
    const result = await stellarService.readContract('get_current_borrow_rate');
    const value = Number(result ?? 0);
    await redisCacheService.set(cacheKey, value, INTEREST_CACHE_TTL_S);
    return value;
  } catch (error) {
    logger.warn('Failed to fetch borrow rate from contract', { error });
    return 0;
  }
}

export async function getCurrentSupplyRate(): Promise<number> {
  const cacheKey = redisCacheService.buildKey('interest', 'current:supply');
  const cached = await redisCacheService.get<number>(cacheKey);
  if (cached !== null && cached !== undefined) return cached;

  try {
    const result = await stellarService.readContract('get_current_supply_rate');
    const value = Number(result ?? 0);
    await redisCacheService.set(cacheKey, value, INTEREST_CACHE_TTL_S);
    return value;
  } catch (error) {
    logger.warn('Failed to fetch supply rate from contract', { error });
    return 0;
  }
}

export async function getCurrentUtilization(): Promise<number> {
  const cacheKey = redisCacheService.buildKey('interest', 'current:utilization');
  const cached = await redisCacheService.get<number>(cacheKey);
  if (cached !== null && cached !== undefined) return cached;

  try {
    const result = await stellarService.readContract('get_current_utilization');
    const value = Number(result ?? 0);
    await redisCacheService.set(cacheKey, value, INTEREST_CACHE_TTL_S);
    return value;
  } catch (error) {
    logger.warn('Failed to fetch utilization from contract', { error });
    return 0;
  }
}

export async function getRateHistory(): Promise<RateHistoryEntry[]> {
  const cacheKey = redisCacheService.buildKey('interest', 'history');
  const cached = await redisCacheService.get<RateHistoryEntry[]>(cacheKey);
  if (cached) return cached;

  try {
    const result = await stellarService.readContract('get_rate_history');
    const native = result ?? [];
    const history: RateHistoryEntry[] = Array.isArray(native) ? native.map((entry: any) => ({
      borrowRateBps: Number(entry.borrow_rate_bps ?? 0),
      supplyRateBps: Number(entry.supply_rate_bps ?? 0),
      utilizationBps: Number(entry.utilization_bps ?? 0),
      timestamp: Number(entry.timestamp ?? 0),
    })) : [];
    await redisCacheService.set(cacheKey, history, INTEREST_CACHE_TTL_S);
    return history;
  } catch (error) {
    logger.warn('Failed to fetch rate history from contract', { error });
    return [];
  }
}

export async function simulateRateAtUtilization(utilizationBps: number): Promise<number> {
  try {
    const result = await stellarService.readContract('simulate_rate_at_utilization', utilizationBps);
    return Number(result ?? 0);
  } catch (error) {
    logger.warn('Failed to simulate rate', { error, utilizationBps });
    return 0;
  }
}
