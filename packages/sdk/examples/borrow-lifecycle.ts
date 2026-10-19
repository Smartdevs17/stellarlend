/**
 * Example: the full position lifecycle — deposit, borrow, inspect, repay, withdraw.
 *
 * Works with any `ContractAdapter`: `SorobanRpcAdapter` against a real network, or a fake
 * in tests. Amounts are in the asset's smallest unit (7 decimals for Stellar assets).
 */
import { StellarLendClient, type DebtPosition, type UserPosition } from '../src';

export const UNIT = 10_000_000n; // 1 token with 7 decimals

export interface LifecycleParams {
  user: string;
  /** Asset deposited and pledged as collateral (e.g. the XLM asset contract). */
  collateralAsset: string;
  /** Asset borrowed (e.g. a USDC asset contract). */
  borrowAsset: string;
  deposit: bigint;
  borrow: bigint;
}

export interface LifecycleReport {
  depositedBalance: bigint;
  positionAfterBorrow: UserPosition;
  debtAfterBorrow: DebtPosition;
  debtAfterRepay: DebtPosition;
  withdrawnBalance: bigint;
}

/** Collateral needed for `amount` at the lending contract's 150% minimum ratio. */
export function minimumCollateral(amount: bigint): bigint {
  return (amount * 15_000n + 9_999n) / 10_000n; // round up
}

export async function runBorrowLifecycle(client: StellarLendClient, p: LifecycleParams): Promise<LifecycleReport> {
  // 1. Supply to the pool. Returns the user's new deposited balance.
  const depositedBalance = await client.deposit({ user: p.user, asset: p.collateralAsset, amount: p.deposit });

  // 2. Borrow, pledging collateral at 2x (comfortably above the 150% minimum).
  const collateral = p.borrow * 2n;
  if (collateral < minimumCollateral(p.borrow)) throw new RangeError('collateral below 150%');
  await client.borrow({
    user: p.user,
    asset: p.borrowAsset,
    amount: p.borrow,
    collateralAsset: p.collateralAsset,
    collateralAmount: collateral,
  });

  // 3. Read state back (simulated calls; free and unsigned).
  const positionAfterBorrow = await client.getUserPosition(p.user);
  const debtAfterBorrow = await client.getUserDebt(p.user);

  // 4. Repay half the principal.
  await client.repay({ user: p.user, asset: p.borrowAsset, amount: p.borrow / 2n });
  const debtAfterRepay = await client.getUserDebt(p.user);

  // 5. Withdraw part of the supplied balance.
  const withdrawnBalance = await client.withdraw({ user: p.user, asset: p.collateralAsset, amount: p.deposit / 4n });

  return { depositedBalance, positionAfterBorrow, debtAfterBorrow, debtAfterRepay, withdrawnBalance };
}
