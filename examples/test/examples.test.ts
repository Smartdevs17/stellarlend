// Tests for the examples, run against stubs so no network is needed:
//
//   npm test -w @stellarlend/examples
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { Keypair, StrKey, nativeToScVal, type rpc } from '@stellar/stellar-sdk';
import {
  LendingTransactions,
  ProtocolErrorCode,
  StellarLendClient,
  collect,
  type ContractAdapter,
  type ContractInvocation,
} from '@stellarlend/sdk';

import { devnetStatePath, loadDevnetState, parseDevnetState } from '../src/devnet';
import { contractEvents, countEventsByName, type EventsServer } from '../src/events';
import { buildLeveragePlan, describe, executePlan } from '../src/review-queue';

const contractId = (seed: number) => StrKey.encodeContract(Buffer.alloc(32, seed));
const LENDING = contractId(1);
const XLM = contractId(2);
const USDC = contractId(3);
const USER = Keypair.random().publicKey();
const UNIT = 10_000_000n;

// ── devnet ──────────────────────────────────────────────────────────────────

const STATE = {
  network: { rpcUrl: 'http://localhost:8000/rpc', networkPassphrase: 'Standalone Network ; February 2017' },
  contracts: { lending: LENDING, xlm: XLM, usdc: USDC },
  accounts: { alice: { identity: 'sl-alice', address: USER } },
};

test('devnet state loads from DEVNET_STATE_DIR and is validated', () => {
  const dir = mkdtempSync(join(tmpdir(), 'examples-'));
  writeFileSync(join(dir, 'state.json'), JSON.stringify(STATE));
  const path = devnetStatePath({ DEVNET_STATE_DIR: dir });
  assert.equal(loadDevnetState(path).contracts.lending, LENDING);

  assert.throws(() => loadDevnetState(join(dir, 'missing.json')), /npm run devnet/);
  assert.throws(() => parseDevnetState(JSON.stringify({ ...STATE, contracts: { lending: LENDING } })), /contracts\.xlm/);
  assert.throws(() => parseDevnetState(JSON.stringify({ ...STATE, network: {} })), /rpcUrl/);
});

// ── events (auto-pagination over getEvents) ─────────────────────────────────

/** A getEvents stub over `names`, recording each request. */
function eventsServer(names: string[]) {
  const requests: rpc.Api.GetEventsRequest[] = [];
  const server: EventsServer = {
    getEvents: async (request) => {
      requests.push(request);
      const start = request.cursor ? Number(request.cursor) : 0;
      const limit = request.limit ?? 100;
      const events = names.slice(start, start + limit).map((name, i) => ({
        id: String(start + i),
        ledger: 100 + start + i,
        txHash: `tx${start + i}`,
        topic: [nativeToScVal(name, { type: 'symbol' })],
        value: nativeToScVal(1n, { type: 'i128' }),
      }));
      return { events, cursor: String(start + events.length) } as unknown as rpc.Api.GetEventsResponse;
    },
  };
  return { server, requests };
}

test('contractEvents walks every page, starting by ledger then following the cursor', async () => {
  const names = ['deposit', 'borrow', 'deposit', 'repay', 'deposit'];
  const { server, requests } = eventsServer(names);
  const counts = await countEventsByName(contractEvents(server, LENDING, 42, { pageSize: 2 }));

  assert.deepEqual(counts, { deposit: 3, borrow: 1, repay: 1 });
  assert.equal(requests.length, 3);
  assert.equal(requests[0]!.startLedger, 42);
  assert.equal(requests[0]!.cursor, undefined);
  assert.deepEqual(requests.slice(1).map((r) => [r.cursor, r.startLedger]), [['2', undefined], ['4', undefined]]);
  assert.deepEqual(requests[0]!.filters, [{ type: 'contract', contractIds: [LENDING] }]);
});

test('contractEvents decodes events and respects maxItems without over-fetching', async () => {
  const { server, requests } = eventsServer(Array(500).fill('deposit'));
  const events = await collect(contractEvents(server, LENDING, 1, { pageSize: 100, maxItems: 3 }));
  assert.equal(events.length, 3);
  assert.deepEqual(events[0], { id: '0', ledger: 100, txHash: 'tx0', topics: ['deposit'], value: 1n });
  assert.equal(requests.length, 1);
  assert.equal(requests[0]!.limit, 3);
});

// ── review-queue (typed transaction builders) ───────────────────────────────

class RecordingAdapter implements ContractAdapter {
  readonly calls: ContractInvocation[] = [];
  constructor(private readonly respond: (call: ContractInvocation) => unknown = () => undefined) {}
  async invoke<T>(call: ContractInvocation): Promise<T> {
    this.calls.push(call);
    return this.respond(call) as T;
  }
}

const plan = { user: USER, collateralAsset: XLM, borrowAsset: USDC, collateral: 150n * UNIT, borrow: 100n * UNIT };

test('buildLeveragePlan yields typed deposit + borrow steps and describes them', () => {
  const steps = buildLeveragePlan(new LendingTransactions({ contractId: LENDING }), plan);
  assert.deepEqual(steps.map((s) => s.method), ['deposit', 'borrow']);
  assert.equal(describe(steps[0]), `write deposit(${USER}, ${XLM}, 150.0000000)`);
  assert.equal(describe(new LendingTransactions({ contractId: LENDING }).getUserDebt(USER)), `read  get_user_debt(${USER})`);
});

test('buildLeveragePlan rejects invalid amounts before anything is signed', () => {
  assert.throws(() => buildLeveragePlan(new LendingTransactions({ contractId: LENDING }), { ...plan, borrow: 0n }), RangeError);
});

test('executePlan runs steps after confirmation and returns typed results', async () => {
  const adapter = new RecordingAdapter((call) => (call.method === 'deposit' ? 150n * UNIT : undefined));
  const client = new StellarLendClient({ contractId: LENDING, adapter });
  let shown: string[] = [];
  const outcome = await executePlan(client, buildLeveragePlan(client.tx, plan), (lines) => {
    shown = lines;
    return true;
  });
  assert.equal(shown.length, 2);
  assert.deepEqual(outcome, { executed: 2, results: [150n * UNIT, undefined] });
  assert.deepEqual(adapter.calls.map((c) => c.method), ['deposit', 'borrow']);
});

test('executePlan sends nothing when the review is declined, and stops at the first failure', async () => {
  const declined = new RecordingAdapter();
  const outcome = await executePlan(
    new StellarLendClient({ contractId: LENDING, adapter: declined }),
    buildLeveragePlan(new LendingTransactions({ contractId: LENDING }), plan),
    () => false,
  );
  assert.deepEqual(outcome, { executed: 0, results: [] });
  assert.equal(declined.calls.length, 0);

  const failing = new RecordingAdapter((call) => {
    if (call.method === 'deposit') throw { code: ProtocolErrorCode.InsufficientBalance };
  });
  const client = new StellarLendClient({ contractId: LENDING, adapter: failing });
  await assert.rejects(executePlan(client, buildLeveragePlan(client.tx, plan)), { code: ProtocolErrorCode.InsufficientBalance });
  assert.deepEqual(failing.calls.map((c) => c.method), ['deposit']);
});
