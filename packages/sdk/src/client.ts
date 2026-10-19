import { normalizeProtocolError } from './errors';
import { paginate, offsetFetcher, type PaginateOptions } from './pagination';
import { LendingTransactions, requireAddress, type TypedInvocation } from './transactions';

/** Contract argument/return value. `number` is a Soroban `u32` (e.g. page offsets and limits). */
export type ContractValue = string | number | bigint | boolean | Uint8Array | null | ContractValue[] | { [key: string]: ContractValue };

export interface ContractInvocation {
  contractId: string;
  method: string;
  args: ContractValue[];
  source?: string;
  readonly?: boolean;
}

export interface ContractAdapter {
  invoke<T = unknown>(invocation: ContractInvocation): Promise<T>;
}

export interface StellarLendClientOptions {
  contractId: string;
  adapter: ContractAdapter;
  source?: string;
  /** Retry policy for retryable protocol errors. Defaults to `DEFAULT_RETRY_OPTIONS`. */
  retry?: RetryOptions;
}

export interface RetryOptions {
  /** Retries after the first attempt. `0` disables retrying. */
  maxRetries?: number;
  /** Delay before the first retry, in milliseconds. */
  initialDelayMs?: number;
  /** Upper bound for any single delay, in milliseconds. */
  maxDelayMs?: number;
  /** Factor applied to the delay after each retry (exponential backoff). */
  multiplier?: number;
  /**
   * Also retry state-changing calls. Off by default: a write that failed
   * ambiguously (e.g. timed out after submission) may already have applied,
   * so only enable this when the adapter submits idempotently.
   */
  retryWrites?: boolean;
  /** Injectable delay, for tests and custom schedulers. */
  sleep?: (ms: number) => Promise<void>;
}

export const DEFAULT_RETRY_OPTIONS = {
  maxRetries: 3,
  initialDelayMs: 100,
  maxDelayMs: 5_000,
  multiplier: 2,
  retryWrites: false,
} as const;

type RetryPolicy = Required<RetryOptions>;

export interface AssetAmount {
  user: string;
  asset: string;
  amount: bigint;
}

export interface BorrowParams extends AssetAmount {
  collateralAsset: string;
  collateralAmount: bigint;
}

export interface LiquidateParams {
  liquidator: string;
  borrower: string;
  debtAsset: string;
  collateralAsset: string;
  amount: bigint;
}

export interface DebtPosition {
  principal: bigint;
  accruedInterest?: bigint;
  [key: string]: ContractValue | undefined;
}

export interface UserPosition {
  [key: string]: ContractValue | undefined;
}

function resolveRetry(options: RetryOptions = {}): RetryPolicy {
  const policy: RetryPolicy = {
    ...DEFAULT_RETRY_OPTIONS,
    sleep: (ms) => new Promise<void>((resolve) => setTimeout(resolve, ms)),
    ...Object.fromEntries(Object.entries(options).filter(([, v]) => v !== undefined)),
  };
  if (!Number.isInteger(policy.maxRetries) || policy.maxRetries < 0) {
    throw new RangeError('retry.maxRetries must be a non-negative integer');
  }
  if (!Number.isFinite(policy.initialDelayMs) || policy.initialDelayMs < 0) {
    throw new RangeError('retry.initialDelayMs must be a non-negative number');
  }
  if (!Number.isFinite(policy.maxDelayMs) || policy.maxDelayMs < policy.initialDelayMs) {
    throw new RangeError('retry.maxDelayMs must be at least retry.initialDelayMs');
  }
  if (!Number.isFinite(policy.multiplier) || policy.multiplier < 1) {
    throw new RangeError('retry.multiplier must be at least 1');
  }
  return policy;
}

/** Delay before retry number `retry` (1-based) under `policy`. */
export function retryDelayMs(policy: Omit<RetryPolicy, 'sleep' | 'retryWrites'>, retry: number): number {
  return Math.min(policy.maxDelayMs, policy.initialDelayMs * policy.multiplier ** (retry - 1));
}

/**
 * Type-safe façade over StellarLend's public lending entrypoints.
 *
 * Transaction building, simulation and signing stay in the adapter, allowing
 * the same client to work with browser wallets, server signers, or test fakes.
 */
export class StellarLendClient {
  readonly contractId: string;
  /** Typed builders for this client's contract and source account. */
  readonly tx: LendingTransactions;
  private readonly adapter: ContractAdapter;
  private readonly retry: RetryPolicy;

  constructor(options: StellarLendClientOptions) {
    this.tx = new LendingTransactions({ contractId: options.contractId, source: options.source });
    this.contractId = options.contractId;
    this.adapter = options.adapter;
    this.retry = resolveRetry(options.retry);
  }

  deposit(params: AssetAmount): Promise<bigint> {
    return this.execute(this.tx.deposit(params));
  }

  withdraw(params: AssetAmount): Promise<bigint> {
    return this.execute(this.tx.withdraw(params));
  }

  repay(params: AssetAmount): Promise<void> {
    return this.execute(this.tx.repay(params));
  }

  borrow(params: BorrowParams): Promise<void> {
    return this.execute(this.tx.borrow(params));
  }

  liquidate(params: LiquidateParams): Promise<void> {
    return this.execute(this.tx.liquidate(params));
  }

  getUserPosition(user: string): Promise<UserPosition> {
    return this.execute(this.tx.getUserPosition(user));
  }

  getUserDebt(user: string): Promise<DebtPosition> {
    return this.execute(this.tx.getUserDebt(user));
  }

  invoke<T>(method: string, args: ContractValue[] = [], readonly = false): Promise<T> {
    return this.execute(this.tx.custom<T>(method, args, readonly));
  }

  /**
   * Iterates a read-only `(…args, offset, limit)` list entrypoint page by page, e.g.
   * `client.paginate<Address>('get_recipients_page')`. Offset and limit are appended as
   * the last two arguments; a page shorter than the limit ends iteration.
   */
  paginate<T>(method: string, args: ContractValue[] = [], options: PaginateOptions<number> = {}): AsyncGenerator<T, void, undefined> {
    requireAddress('method', method);
    return paginate(
      offsetFetcher((offset, limit) => this.invoke<T[]>(method, [...args, offset, limit], true)),
      options,
    );
  }

  /**
   * Runs an invocation built with `tx` (or any `TypedInvocation`), applying the retry
   * policy. Read-only invocations are retried; writes only when `retryWrites` is set.
   */
  async execute<T>(invocation: TypedInvocation<T>): Promise<T> {
    const readonly = invocation.readonly ?? false;
    const maxRetries = readonly || this.retry.retryWrites ? this.retry.maxRetries : 0;
    for (let retry = 0; ; retry++) {
      try {
        return await this.adapter.invoke<T>({ ...invocation, readonly });
      } catch (thrown) {
        const error = normalizeProtocolError(thrown);
        if (!error.retryable || retry >= maxRetries) throw error;
        await this.retry.sleep(retryDelayMs(this.retry, retry + 1));
      }
    }
  }
}
