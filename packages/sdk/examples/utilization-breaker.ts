/**
 * Example: reading the utilization circuit breaker to gate a borrow form.
 *
 * The lending contract throttles new borrows once utilization passes the configured trip
 * threshold. Reading the breaker first lets a UI cap the input instead of failing the
 * transaction. Endpoints without a typed helper are reached through `client.invoke`.
 */
import type { StellarLendClient } from '../src';

/** Shape returned by the contract's `get_utilization_breaker` view. */
export interface UtilizationBreakerStatus {
  config: {
    enabled: boolean;
    trip_utilization_bps: number;
    reset_utilization_bps: number;
    max_borrow_while_tripped: bigint;
  };
  tripped: boolean;
  tripped_at: bigint;
  utilization_bps: number;
}

export type BorrowLimit =
  | { kind: 'unrestricted' }
  | { kind: 'throttled'; maxPerBorrow: bigint; utilizationBps: number }
  | { kind: 'blocked'; utilizationBps: number };

export function getUtilizationBreaker(client: StellarLendClient): Promise<UtilizationBreakerStatus> {
  return client.invoke<UtilizationBreakerStatus>('get_utilization_breaker', [], true);
}

/**
 * Largest borrow the breaker will accept right now. Mirrors the contract: throttling
 * applies while tripped, or when the borrow itself would reach the trip threshold — so
 * an armed breaker close to the line is reported as `unrestricted` and the caller should
 * still expect `LimitExceeded` for borrows that cross it.
 */
export function borrowLimit(status: UtilizationBreakerStatus): BorrowLimit {
  if (!status.config.enabled || !status.tripped) return { kind: 'unrestricted' };
  const max = BigInt(status.config.max_borrow_while_tripped);
  return max === 0n
    ? { kind: 'blocked', utilizationBps: status.utilization_bps }
    : { kind: 'throttled', maxPerBorrow: max, utilizationBps: status.utilization_bps };
}
