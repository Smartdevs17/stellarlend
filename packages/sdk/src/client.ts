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
}

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

  constructor(options: StellarLendClientOptions) {
    requireAddress('contractId', options.contractId);
    this.contractId = options.contractId;
    this.adapter = options.adapter;
    this.source = options.source;
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
    try {
      return await this.adapter.invoke<T>({
        contractId: this.contractId,
        method,
        args,
        source: this.source,
        readonly,
      });
    } catch (error) {
      throw normalizeProtocolError(error);
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
