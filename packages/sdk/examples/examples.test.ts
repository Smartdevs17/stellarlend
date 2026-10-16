// Tests for the SDK examples. Compiled together with the examples (so a type error in an
// example fails the run) and executed with node's built-in runner:
//
//   npm run test:examples -w @stellarlend/sdk
import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  Account,
  Address,
  Keypair,
  Networks,
  StrKey,
  TransactionBuilder,
  nativeToScVal,
  rpc,
  scValToNative,
  xdr,
} from '@stellar/stellar-sdk';

import { ProtocolError, ProtocolErrorCode, StellarLendClient, type InjectedWalletApi } from '../src';
import { FakeAdapter } from './support/fake-adapter';
import { UNIT, minimumCollateral, runBorrowLifecycle } from './borrow-lifecycle';
import { borrowWithRetry, contractErrorCode, messageForUser } from './error-handling';
import { SorobanRpcAdapter, keypairSigner, toScVal, type RpcServerLike } from './rpc-adapter';
import { borrowLimit, getUtilizationBreaker, type UtilizationBreakerStatus } from './utilization-breaker';
import { connectLending, createWalletManager } from './wallet';

const USER = Keypair.random().publicKey();
const contractId = (seed: number) => StrKey.encodeContract(Buffer.alloc(32, seed));
const CONTRACT = contractId(1);
const XLM = contractId(2);
const USDC = contractId(3);

function client(adapter: FakeAdapter) {
  return new StellarLendClient({ contractId: CONTRACT, adapter });
}

// ── borrow-lifecycle ────────────────────────────────────────────────────────

test('borrow lifecycle calls entrypoints in order with contract-shaped args', async () => {
  const adapter = new FakeAdapter()
    .on('deposit', 100n * UNIT)
    .on('get_user_position', { collateral_balance: 300n * UNIT })
    .on('get_user_debt', { borrowed_amount: 40n * UNIT })
    .on('withdraw', 75n * UNIT);

  const report = await runBorrowLifecycle(client(adapter), {
    user: USER,
    collateralAsset: XLM,
    borrowAsset: USDC,
    deposit: 100n * UNIT,
    borrow: 40n * UNIT,
  });

  assert.deepEqual(adapter.methods(), [
    'deposit',
    'borrow',
    'get_user_position',
    'get_user_debt',
    'repay',
    'get_user_debt',
    'withdraw',
  ]);
  assert.deepEqual(adapter.calls[1]!.args, [USER, USDC, 40n * UNIT, XLM, 80n * UNIT]);
  assert.deepEqual(adapter.calls[4]!.args, [USER, USDC, 20n * UNIT]);
  assert.equal(adapter.calls[2]!.readonly, true);
  assert.equal(adapter.calls[1]!.readonly, false);
  assert.equal(report.depositedBalance, 100n * UNIT);
  assert.equal(report.withdrawnBalance, 75n * UNIT);
});

test('minimumCollateral rounds up to the 150% ratio', () => {
  assert.equal(minimumCollateral(100n), 150n);
  assert.equal(minimumCollateral(1n), 2n);
});

// ── error-handling ──────────────────────────────────────────────────────────

const borrowParams = { user: USER, asset: USDC, amount: UNIT, collateralAsset: XLM, collateralAmount: 2n * UNIT };

test('non-retryable failures return a user message without retrying', async () => {
  const adapter = new FakeAdapter().on('borrow', () => {
    throw { code: ProtocolErrorCode.InsufficientCollateral };
  });
  const outcome = await borrowWithRetry(client(adapter), borrowParams, { sleep: async () => {} });
  assert.equal(outcome.ok, false);
  assert.equal(outcome.attempts, 1);
  assert.match(!outcome.ok ? outcome.userMessage : '', /150%/);
});

test('retryable failures are retried with backoff, then succeed', async () => {
  let calls = 0;
  const sleeps: number[] = [];
  const adapter = new FakeAdapter().on('borrow', () => {
    if (++calls < 3) throw { code: ProtocolErrorCode.CrossContractFailure };
  });
  const outcome = await borrowWithRetry(client(adapter), borrowParams, {
    backoffMs: 10,
    sleep: async (ms) => void sleeps.push(ms),
  });
  assert.deepEqual(outcome, { ok: true, attempts: 3 });
  assert.deepEqual(sleeps, [10, 20]);
});

test('raw contract codes stay available for logging', () => {
  const err = new ProtocolError(ProtocolErrorCode.Unknown, 'x', new Error('HostError: Error(Contract, #13)'));
  assert.equal(contractErrorCode(err), 13);
  assert.match(messageForUser(new ProtocolError(ProtocolErrorCode.LimitExceeded)), /borrowing limit/);
});

// ── utilization-breaker ─────────────────────────────────────────────────────

function breaker(overrides: Partial<UtilizationBreakerStatus> & { max?: bigint; enabled?: boolean }) {
  return {
    config: {
      enabled: overrides.enabled ?? true,
      trip_utilization_bps: 9000,
      reset_utilization_bps: 8000,
      max_borrow_while_tripped: overrides.max ?? 10n * UNIT,
    },
    tripped: overrides.tripped ?? false,
    tripped_at: 0n,
    utilization_bps: overrides.utilization_bps ?? 5000,
  };
}

test('breaker status maps to a borrow limit', async () => {
  const adapter = new FakeAdapter().on('get_utilization_breaker', breaker({ tripped: true, utilization_bps: 9100 }));
  const status = await getUtilizationBreaker(client(adapter));
  assert.equal(adapter.calls[0]!.readonly, true);
  assert.deepEqual(borrowLimit(status), { kind: 'throttled', maxPerBorrow: 10n * UNIT, utilizationBps: 9100 });
  assert.deepEqual(borrowLimit(breaker({ tripped: true, max: 0n })), { kind: 'blocked', utilizationBps: 5000 });
  assert.deepEqual(borrowLimit(breaker({ tripped: false })), { kind: 'unrestricted' });
  assert.deepEqual(borrowLimit(breaker({ tripped: true, enabled: false })), { kind: 'unrestricted' });
});

// ── rpc-adapter ─────────────────────────────────────────────────────────────

test('toScVal encodes addresses, i128, structs and vectors the way the contract expects', () => {
  assert.equal(toScVal(USER).switch(), xdr.ScValType.scvAddress());
  assert.equal(toScVal(CONTRACT).switch(), xdr.ScValType.scvAddress());
  assert.equal(toScVal('hello').switch(), xdr.ScValType.scvString());
  assert.equal(toScVal(5n).switch(), xdr.ScValType.scvI128());
  assert.equal(toScVal(null).switch(), xdr.ScValType.scvVoid());
  const struct = toScVal({ enabled: true, amount: 1n });
  assert.deepEqual(
    struct.map()!.map((e) => e.key().sym().toString()),
    ['amount', 'enabled'],
  );
  assert.deepEqual(scValToNative(toScVal([USER, 1n])), [USER, 1n]);
});

function stubServer(overrides: Partial<RpcServerLike> = {}) {
  const sent: string[] = [];
  const server: RpcServerLike = {
    getAccount: async (id: string) => new Account(id, '1'),
    simulateTransaction: async () =>
      ({ result: { retval: nativeToScVal(42n, { type: 'i128' }) }, latestLedger: 1 }) as unknown as rpc.Api.SimulateTransactionResponse,
    prepareTransaction: async (tx) => tx as never,
    sendTransaction: async (tx) => {
      sent.push(tx.toXDR());
      return { status: 'PENDING', hash: 'abc' } as unknown as rpc.Api.SendTransactionResponse;
    },
    pollTransaction: async () =>
      ({
        status: rpc.Api.GetTransactionStatus.SUCCESS,
        returnValue: nativeToScVal(7n, { type: 'i128' }),
      }) as unknown as rpc.Api.GetTransactionResponse,
    ...overrides,
  };
  return { server, sent };
}

test('rpc adapter: read-only calls are simulated, never submitted', async () => {
  const { server, sent } = stubServer();
  const adapter = new SorobanRpcAdapter({
    rpcUrl: 'http://localhost:8000/rpc',
    networkPassphrase: Networks.STANDALONE,
    source: USER,
    signTransaction: async () => assert.fail('read-only call must not sign'),
    server,
  });
  const lending = new StellarLendClient({ contractId: CONTRACT, adapter });
  assert.equal(await lending.invoke<bigint>('get_debt_balance', [USER], true), 42n);
  assert.equal(sent.length, 0);
});

test('rpc adapter: writes are signed by the signer, submitted and decoded', async () => {
  const signer = Keypair.random();
  const { server, sent } = stubServer();
  const adapter = new SorobanRpcAdapter({
    rpcUrl: 'http://localhost:8000/rpc',
    networkPassphrase: Networks.STANDALONE,
    source: signer.publicKey(),
    signTransaction: keypairSigner(signer.secret()),
    server,
  });
  const lending = new StellarLendClient({ contractId: CONTRACT, adapter });
  assert.equal(await lending.deposit({ user: signer.publicKey(), asset: XLM, amount: UNIT }), 7n);

  assert.equal(sent.length, 1);
  const tx = TransactionBuilder.fromXDR(sent[0]!, Networks.STANDALONE);
  assert.equal(tx.signatures.length, 1);
  const op = (tx as unknown as { operations: Array<{ func: xdr.HostFunction }> }).operations[0]!;
  const call = op.func.invokeContract();
  assert.equal(call.functionName().toString(), 'deposit');
  assert.equal(Address.fromScAddress(call.contractAddress()).toString(), CONTRACT);
});

test('rpc adapter: simulation failures surface as ProtocolError via the client', async () => {
  const { server } = stubServer({
    simulateTransaction: async () =>
      ({ error: 'HostError: Error(Contract, #1008)', latestLedger: 1 }) as unknown as rpc.Api.SimulateTransactionResponse,
  });
  const adapter = new SorobanRpcAdapter({
    rpcUrl: 'http://localhost:8000/rpc',
    networkPassphrase: Networks.STANDALONE,
    source: USER,
    signTransaction: async (x) => x,
    server,
  });
  const lending = new StellarLendClient({ contractId: CONTRACT, adapter });
  await assert.rejects(lending.getUserDebt(USER), (e: unknown) => {
    assert.ok(e instanceof ProtocolError);
    assert.equal(e.code, ProtocolErrorCode.InsufficientCollateral);
    return true;
  });
});

test('rpc adapter: failed transactions reject', async () => {
  const { server } = stubServer({
    pollTransaction: async () =>
      ({ status: rpc.Api.GetTransactionStatus.FAILED }) as unknown as rpc.Api.GetTransactionResponse,
  });
  const signer = Keypair.random();
  const adapter = new SorobanRpcAdapter({
    rpcUrl: 'http://localhost:8000/rpc',
    networkPassphrase: Networks.STANDALONE,
    source: signer.publicKey(),
    signTransaction: keypairSigner(signer.secret()),
    server,
  });
  await assert.rejects(
    adapter.invoke({ contractId: CONTRACT, method: 'repay', args: [USER, USDC, 1n] }),
    /FAILED/,
  );
});

// ── wallet ──────────────────────────────────────────────────────────────────

test('wallet example connects, reports state, and signs through the wallet', async () => {
  const signedBy: string[] = [];
  const api: InjectedWalletApi = {
    connect: async () => ({ address: USER, network: 'STANDALONE' }),
    signTransaction: async (txXdr, opts) => {
      signedBy.push(opts?.accountToSign ?? '');
      return { signedTxXdr: txXdr };
    },
  };
  const wallets = createWalletManager(() => api);
  const states: string[] = [];
  const { client: lending, address, unsubscribe } = await connectLending(
    wallets,
    { rpcUrl: 'http://localhost:8000/rpc', networkPassphrase: Networks.STANDALONE, lendingContractId: CONTRACT },
    (s) => states.push(s.status),
  );
  unsubscribe();
  assert.equal(address, USER);
  assert.deepEqual(states, ['connecting', 'connected']);
  assert.equal(lending.contractId, CONTRACT);

  // The adapter's signer is the wallet: sign a dummy envelope through it.
  assert.equal(await wallets.signTransaction('AAAA', { accountToSign: address }), 'AAAA');
  assert.deepEqual(signedBy, [USER]);
});

test('wallet example fails cleanly when the extension is missing', async () => {
  const wallets = createWalletManager(() => undefined);
  await assert.rejects(
    connectLending(wallets, { rpcUrl: 'http://x', networkPassphrase: Networks.STANDALONE, lendingContractId: CONTRACT }),
    /not available/,
  );
});
