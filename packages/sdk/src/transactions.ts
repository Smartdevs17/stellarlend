/**
 * Typed transaction builders.
 *
 * Each builder validates its parameters and returns a `TypedInvocation<R>`: a plain
 * `ContractInvocation` that also carries the entrypoint's result type. Build calls up
 * front (for review screens, batching or queuing), then run them with
 * `StellarLendClient.execute`, which infers `R`.
 */
import type {
  AssetAmount,
  BorrowParams,
  ContractInvocation,
  ContractValue,
  DebtPosition,
  LiquidateParams,
  UserPosition,
} from './client';

declare const resultType: unique symbol;

/** A contract invocation whose successful result has type `R`. */
export interface TypedInvocation<R> extends ContractInvocation {
  /** Type-level only; never present at runtime. */
  readonly [resultType]?: R;
}

/** Result type of a `TypedInvocation`. */
export type InvocationResult<I> = I extends TypedInvocation<infer R> ? R : never;

export interface LendingTransactionsOptions {
  contractId: string;
  /** Default source account for built invocations. */
  source?: string;
}

export function requireAddress(name: string, value: string): void {
  if (!value || typeof value !== 'string') throw new TypeError(`${name} is required`);
}

export function requireAmount(name: string, value: bigint): void {
  if (typeof value !== 'bigint' || value <= 0n) throw new RangeError(`${name} must be a positive bigint`);
}

function validateAssetAmount(params: AssetAmount): void {
  requireAddress('user', params.user);
  requireAddress('asset', params.asset);
  requireAmount('amount', params.amount);
}

/** Builds validated, typed invocations for the lending contract's public entrypoints. */
export class LendingTransactions {
  readonly contractId: string;
  readonly source?: string;

  constructor(options: LendingTransactionsOptions) {
    requireAddress('contractId', options.contractId);
    this.contractId = options.contractId;
    this.source = options.source;
  }

  /** Builder with a different default source account. */
  withSource(source: string): LendingTransactions {
    requireAddress('source', source);
    return new LendingTransactions({ contractId: this.contractId, source });
  }

  deposit(params: AssetAmount): TypedInvocation<bigint> {
    validateAssetAmount(params);
    return this.write('deposit', [params.user, params.asset, params.amount]);
  }

  withdraw(params: AssetAmount): TypedInvocation<bigint> {
    validateAssetAmount(params);
    return this.write('withdraw', [params.user, params.asset, params.amount]);
  }

  repay(params: AssetAmount): TypedInvocation<void> {
    validateAssetAmount(params);
    return this.write('repay', [params.user, params.asset, params.amount]);
  }

  borrow(params: BorrowParams): TypedInvocation<void> {
    validateAssetAmount(params);
    requireAddress('collateralAsset', params.collateralAsset);
    requireAmount('collateralAmount', params.collateralAmount);
    return this.write('borrow', [
      params.user, params.asset, params.amount,
      params.collateralAsset, params.collateralAmount,
    ]);
  }

  liquidate(params: LiquidateParams): TypedInvocation<void> {
    requireAddress('liquidator', params.liquidator);
    requireAddress('borrower', params.borrower);
    requireAddress('debtAsset', params.debtAsset);
    requireAddress('collateralAsset', params.collateralAsset);
    requireAmount('amount', params.amount);
    return this.write('liquidate', [
      params.liquidator, params.borrower, params.debtAsset,
      params.collateralAsset, params.amount,
    ]);
  }

  getUserPosition(user: string): TypedInvocation<UserPosition> {
    requireAddress('user', user);
    return this.read('get_user_position', [user]);
  }

  getUserDebt(user: string): TypedInvocation<DebtPosition> {
    requireAddress('user', user);
    return this.read('get_user_debt', [user]);
  }

  /** Escape hatch for entrypoints without a dedicated builder. */
  custom<R>(method: string, args: ContractValue[] = [], readonly = false): TypedInvocation<R> {
    requireAddress('method', method);
    return this.build(method, args, readonly);
  }

  private read<R>(method: string, args: ContractValue[]): TypedInvocation<R> {
    return this.build(method, args, true);
  }

  private write<R>(method: string, args: ContractValue[]): TypedInvocation<R> {
    return this.build(method, args, false);
  }

  private build<R>(method: string, args: ContractValue[], readonly: boolean): TypedInvocation<R> {
    return { contractId: this.contractId, method, args, source: this.source, readonly };
  }
}
