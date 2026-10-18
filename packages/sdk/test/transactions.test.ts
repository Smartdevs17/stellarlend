// Unit tests for src/transactions.ts and StellarLendClient.execute.
//
//   npm run test:unit -w @stellarlend/sdk
import { test } from 'node:test';
import assert from 'node:assert/strict';

import {
  LendingTransactions,
  ProtocolErrorCode,
  StellarLendClient,
  type DebtPosition,
  type InvocationResult,
  type TypedInvocation,
} from '../src';
import { FakeAdapter } from '../examples/support/fake-adapter';

const CONTRACT = 'CLENDING';
const USER = 'GUSER';
const XLM = 'CXLM';
const USDC = 'CUSDC';

const tx = new LendingTransactions({ contractId: CONTRACT, source: USER });

// Compile-time checks: these lines fail `tsc` if a builder's result type drifts.
type Equals<A, B> = (<T>() => T extends A ? 1 : 2) extends (<T>() => T extends B ? 1 : 2) ? true : false;
const typeChecks: [
  Equals<InvocationResult<ReturnType<LendingTransactions['deposit']>>, bigint>,
  Equals<InvocationResult<ReturnType<LendingTransactions['withdraw']>>, bigint>,
  Equals<InvocationResult<ReturnType<LendingTransactions['borrow']>>, void>,
  Equals<InvocationResult<ReturnType<LendingTransactions['getUserDebt']>>, DebtPosition>,
] = [true, true, true, true];
void typeChecks;

test('builders produce contract-shaped invocations', () => {
  assert.deepEqual(tx.deposit({ user: USER, asset: XLM, amount: 10n }), {
    contractId: CONTRACT, method: 'deposit', args: [USER, XLM, 10n], source: USER, readonly: false,
  });
  assert.deepEqual(
    tx.borrow({ user: USER, asset: USDC, amount: 2n, collateralAsset: XLM, collateralAmount: 3n }).args,
    [USER, USDC, 2n, XLM, 3n],
  );
  assert.deepEqual(
    tx.liquidate({ liquidator: 'GLIQ', borrower: USER, debtAsset: USDC, collateralAsset: XLM, amount: 1n }).args,
    ['GLIQ', USER, USDC, XLM, 1n],
  );
  assert.equal(tx.withdraw({ user: USER, asset: XLM, amount: 1n }).method, 'withdraw');
  assert.equal(tx.repay({ user: USER, asset: USDC, amount: 1n }).method, 'repay');
});

test('read builders are marked readonly; writes are not', () => {
  assert.equal(tx.getUserPosition(USER).readonly, true);
  assert.deepEqual(tx.getUserDebt(USER).args, [USER]);
  assert.equal(tx.repay({ user: USER, asset: USDC, amount: 1n }).readonly, false);
  assert.equal(tx.custom('get_utilization_breaker', [], true).readonly, true);
});

test('builders validate parameters before anything is sent', () => {
  assert.throws(() => tx.deposit({ user: '', asset: XLM, amount: 1n }), TypeError);
  assert.throws(() => tx.deposit({ user: USER, asset: XLM, amount: 0n }), RangeError);
  assert.throws(
    () => tx.borrow({ user: USER, asset: USDC, amount: 1n, collateralAsset: XLM, collateralAmount: -1n }),
    RangeError,
  );
  assert.throws(() => tx.custom(''), TypeError);
  assert.throws(() => new LendingTransactions({ contractId: '' }), TypeError);
});

test('withSource returns a builder for another account without mutating the original', () => {
  const asBob = tx.withSource('GBOB');
  assert.equal(asBob.repay({ user: 'GBOB', asset: USDC, amount: 1n }).source, 'GBOB');
  assert.equal(tx.source, USER);
  assert.throws(() => tx.withSource(''), TypeError);
});

test('client.execute runs a built invocation and returns its typed result', async () => {
  const adapter = new FakeAdapter().on('deposit', 42n);
  const client = new StellarLendClient({ contractId: CONTRACT, adapter, source: USER });
  const result: bigint = await client.execute(client.tx.deposit({ user: USER, asset: XLM, amount: 1n }));
  assert.equal(result, 42n);
  assert.deepEqual(adapter.calls[0], {
    contractId: CONTRACT, method: 'deposit', args: [USER, XLM, 1n], source: USER, readonly: false,
  });
});

test('client.execute applies the retry policy by invocation kind', async () => {
  let reads = 0;
  let writes = 0;
  const adapter = new FakeAdapter()
    .on('get_user_debt', () => {
      if (++reads < 2) throw { code: ProtocolErrorCode.CrossContractFailure };
      return { principal: 1n };
    })
    .on('repay', () => {
      ++writes;
      throw { code: ProtocolErrorCode.CrossContractFailure };
    });
  const client = new StellarLendClient({ contractId: CONTRACT, adapter, retry: { sleep: async () => {} } });

  assert.deepEqual(await client.execute(tx.getUserDebt(USER)), { principal: 1n });
  assert.equal(reads, 2);
  await assert.rejects(client.execute(tx.repay({ user: USER, asset: USDC, amount: 1n })));
  assert.equal(writes, 1);
});

test('invocations built ahead of time can be queued and executed later', async () => {
  const adapter = new FakeAdapter();
  const client = new StellarLendClient({ contractId: CONTRACT, adapter });
  const queue: TypedInvocation<unknown>[] = [
    client.tx.deposit({ user: USER, asset: XLM, amount: 3n }),
    client.tx.borrow({ user: USER, asset: USDC, amount: 1n, collateralAsset: XLM, collateralAmount: 2n }),
  ];
  for (const invocation of queue) await client.execute(invocation);
  assert.deepEqual(adapter.methods(), ['deposit', 'borrow']);
});
