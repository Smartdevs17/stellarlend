import { normalizeProtocolError } from './errors';

export type ContractValue = string | bigint | boolean | Uint8Array | null | ContractValue[] | { [key: string]: ContractValue };

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

function requireAddress(name: string, value: string): void {
  if (!value || typeof value !== 'string') throw new TypeError(`${name} is required`);
}

function requireAmount(name: string, value: bigint): void {
  if (typeof value !== 'bigint' || value <= 0n) throw new RangeError(`${name} must be a positive bigint`);
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
  private readonly adapter: ContractAdapter;
  private readonly source?: string;
  private readonly retry: RetryPolicy;

  constructor(options: StellarLendClientOptions) {
    requireAddress('contractId', options.contractId);
    this.contractId = options.contractId;
    this.adapter = options.adapter;
    this.source = options.source;
    this.retry = resolveRetry(options.retry);
  }

  deposit(params: AssetAmount): Promise<bigint> {
    return this.mutate('deposit', assetAmountArgs(params));
  }

  withdraw(params: AssetAmount): Promise<bigint> {
    return this.mutate('withdraw', assetAmountArgs(params));
  }

  repay(params: AssetAmount): Promise<void> {
    return this.mutate('repay', assetAmountArgs(params));
  }

  borrow(params: BorrowParams): Promise<void> {
    validateAssetAmount(params);
    requireAddress('collateralAsset', params.collateralAsset);
    requireAmount('collateralAmount', params.collateralAmount);
    return this.mutate('borrow', [
      params.user, params.asset, params.amount,
      params.collateralAsset, params.collateralAmount,
    ]);
  }

  liquidate(params: LiquidateParams): Promise<void> {
    requireAddress('liquidator', params.liquidator);
    requireAddress('borrower', params.borrower);
    requireAddress('debtAsset', params.debtAsset);
    requireAddress('collateralAsset', params.collateralAsset);
    requireAmount('amount', params.amount);
    return this.mutate('liquidate', [
      params.liquidator, params.borrower, params.debtAsset,
      params.collateralAsset, params.amount,
    ]);
  }

  getUserPosition(user: string): Promise<UserPosition> {
    requireAddress('user', user);
    return this.read('get_user_position', [user]);
  }

  getUserDebt(user: string): Promise<DebtPosition> {
    requireAddress('user', user);
    return this.read('get_user_debt', [user]);
  }

  invoke<T>(method: string, args: ContractValue[] = [], readonly = false): Promise<T> {
    requireAddress('method', method);
    return this.call<T>(method, args, readonly);
  }

  private read<T>(method: string, args: ContractValue[]): Promise<T> {
    return this.call<T>(method, args, true);
  }

  private mutate<T>(method: string, args: ContractValue[]): Promise<T> {
    return this.call<T>(method, args, false);
  }

  private async call<T>(method: string, args: ContractValue[], readonly: boolean): Promise<T> {
    const maxRetries = readonly || this.retry.retryWrites ? this.retry.maxRetries : 0;
    for (let retry = 0; ; retry++) {
      try {
        return await this.adapter.invoke<T>({
          contractId: this.contractId,
          method,
          args,
          source: this.source,
          readonly,
        });
      } catch (thrown) {
        const error = normalizeProtocolError(thrown);
        if (!error.retryable || retry >= maxRetries) throw error;
        await this.retry.sleep(retryDelayMs(this.retry, retry + 1));
      }
    }
  }
}

function validateAssetAmount(params: AssetAmount): void {
  requireAddress('user', params.user);
  requireAddress('asset', params.asset);
  requireAmount('amount', params.amount);
}

function assetAmountArgs(params: AssetAmount): ContractValue[] {
  validateAssetAmount(params);
  return [params.user, params.asset, params.amount];
}
