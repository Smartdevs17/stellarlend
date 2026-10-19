/**
 * Example: build a multi-step plan with typed transaction builders, show it for review,
 * then execute it.
 *
 * `client.tx.*` (or a standalone `LendingTransactions`) validates every step up front, so
 * a bad amount fails before anything is signed. Each built step is a plain object, easy
 * to render, log or queue, and `client.execute` returns the step's typed result.
 */
import type { LendingTransactions, StellarLendClient, TypedInvocation } from '@stellarlend/sdk';

const UNIT = 10_000_000n; // 7 decimals

export interface LeveragePlan {
  user: string;
  collateralAsset: string;
  borrowAsset: string;
  collateral: bigint;
  borrow: bigint;
}

/** Deposit collateral, then borrow against it. Throws before signing if any step is invalid. */
export function buildLeveragePlan(tx: LendingTransactions, plan: LeveragePlan) {
  return [
    tx.deposit({ user: plan.user, asset: plan.collateralAsset, amount: plan.collateral }),
    tx.borrow({
      user: plan.user,
      asset: plan.borrowAsset,
      amount: plan.borrow,
      collateralAsset: plan.collateralAsset,
      collateralAmount: plan.collateral,
    }),
  ] as const;
}

/** One human-readable line per step, for a confirmation screen or an audit log. */
export function describe(invocation: TypedInvocation<unknown>): string {
  const args = invocation.args.map((arg) =>
    typeof arg === 'bigint' ? `${arg / UNIT}.${(arg % UNIT).toString().padStart(7, '0')}` : String(arg),
  );
  return `${invocation.readonly ? 'read ' : 'write'} ${invocation.method}(${args.join(', ')})`;
}

/** Runs steps in order after `confirm` approves the full list; stops at the first failure. */
export async function executePlan(
  client: StellarLendClient,
  steps: readonly TypedInvocation<unknown>[],
  confirm: (lines: string[]) => boolean | Promise<boolean> = () => true,
): Promise<{ executed: number; results: unknown[] }> {
  if (!(await confirm(steps.map(describe)))) return { executed: 0, results: [] };
  const results: unknown[] = [];
  for (const step of steps) results.push(await client.execute(step));
  return { executed: results.length, results };
}
