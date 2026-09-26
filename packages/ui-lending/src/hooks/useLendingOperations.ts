import { useCallback, useEffect, useRef, useState } from 'react';

export type LendingOperationStatus = 'idle' | 'pending' | 'success' | 'error';
export interface LendingOperationState<TResult> {
  status: LendingOperationStatus;
  data: TResult | null;
  error: Error | null;
  isLoading: boolean;
}
export interface LendingOperation<TResult, TParams> extends LendingOperationState<TResult> {
  execute: (params: TParams) => Promise<TResult>;
  reset: () => void;
}
export interface AssetAmountParams { user: string; asset: string; amount: bigint; }
export interface BorrowOperationParams extends AssetAmountParams {
  collateralAsset: string;
  collateralAmount: bigint;
}
export interface LiquidateOperationParams {
  liquidator: string;
  borrower: string;
  debtAsset: string;
  collateralAsset: string;
  amount: bigint;
}
export interface LendingOperationsClient {
  deposit(params: AssetAmountParams): Promise<bigint>;
  withdraw(params: AssetAmountParams): Promise<bigint>;
  borrow(params: BorrowOperationParams): Promise<void>;
  repay(params: AssetAmountParams): Promise<void>;
  liquidate(params: LiquidateOperationParams): Promise<void>;
}
export interface LendingOperationOptions<TResult> {
  onSuccess?: (result: TResult) => void;
  onError?: (error: Error) => void;
}
const initialState = <TResult>(): LendingOperationState<TResult> => ({
  status: 'idle', data: null, error: null, isLoading: false,
});
const toError = (value: unknown): Error =>
  value instanceof Error ? value : new Error(typeof value === 'string' ? value : 'Lending operation failed');

/** Runs a mutation and keeps only the latest invocation's state. */
export function useLendingOperation<TResult, TParams>(
  operation: (params: TParams) => Promise<TResult>,
  options: LendingOperationOptions<TResult> = {}
): LendingOperation<TResult, TParams> {
  const [state, setState] = useState<LendingOperationState<TResult>>(() => initialState<TResult>());
  const mounted = useRef(true);
  const sequence = useRef(0);
  const operationRef = useRef(operation);
  const optionsRef = useRef(options);
  operationRef.current = operation;
  optionsRef.current = options;

  useEffect(() => {
    mounted.current = true;
    return () => { mounted.current = false; };
  }, []);

  const execute = useCallback(async (params: TParams) => {
    const call = ++sequence.current;
    setState({ status: 'pending', data: null, error: null, isLoading: true });
    try {
      const data = await operationRef.current(params);
      if (mounted.current && call === sequence.current) {
        setState({ status: 'success', data, error: null, isLoading: false });
        optionsRef.current.onSuccess?.(data);
      }
      return data;
    } catch (value) {
      const error = toError(value);
      if (mounted.current && call === sequence.current) {
        setState({ status: 'error', data: null, error, isLoading: false });
        optionsRef.current.onError?.(error);
      }
      throw error;
    }
  }, []);

  const reset = useCallback(() => {
    sequence.current += 1;
    setState(initialState<TResult>());
  }, []);
  return { ...state, execute, reset };
}
export function useDeposit(client: Pick<LendingOperationsClient, 'deposit'>, options?: LendingOperationOptions<bigint>) {
  const operation = useLendingOperation((params: AssetAmountParams) => client.deposit(params), options);
  return { ...operation, deposit: operation.execute };
}
export function useWithdraw(client: Pick<LendingOperationsClient, 'withdraw'>, options?: LendingOperationOptions<bigint>) {
  const operation = useLendingOperation((params: AssetAmountParams) => client.withdraw(params), options);
  return { ...operation, withdraw: operation.execute };
}
export function useBorrow(client: Pick<LendingOperationsClient, 'borrow'>, options?: LendingOperationOptions<void>) {
  const operation = useLendingOperation((params: BorrowOperationParams) => client.borrow(params), options);
  return { ...operation, borrow: operation.execute };
}
export function useRepay(client: Pick<LendingOperationsClient, 'repay'>, options?: LendingOperationOptions<void>) {
  const operation = useLendingOperation((params: AssetAmountParams) => client.repay(params), options);
  return { ...operation, repay: operation.execute };
}
export function useLiquidate(client: Pick<LendingOperationsClient, 'liquidate'>, options?: LendingOperationOptions<void>) {
  const operation = useLendingOperation((params: LiquidateOperationParams) => client.liquidate(params), options);
  return { ...operation, liquidate: operation.execute };
}
