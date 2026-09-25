import { test } from "node:test";
import assert from "node:assert/strict";
import {
  type ContractState,
  buildSnapshot,
  compareSnapshots,
  hashSnapshot,
  normalizeState,
} from "./snapshot.ts";

const base: ContractState = {
  positions: [
    { address: "C_ALICE", collateral: "1000", debt: "500", healthFactor: "1.5" },
    { address: "C_BOB", collateral: "2000", debt: "1000", healthFactor: "1.4" },
  ],
  pools: [
    { asset: "USDC", supplyCap: "1000000", interestRateBps: 500, collateralFactorBps: 8000 },
  ],
  governance: [{ proposalId: 1, status: "active", votesFor: "10", votesAgainst: "2", executed: false }],
};

test("hashSnapshot is deterministic and order-independent", () => {
  const shuffled: ContractState = {
    positions: [
      { address: "C_BOB", collateral: "2000", debt: "1000", healthFactor: "1.4" },
      { address: "C_ALICE", collateral: "1000", debt: "500", healthFactor: "1.5" },
    ],
    pools: [{ asset: "USDC", supplyCap: "1000000", interestRateBps: 500, collateralFactorBps: 8000 }],
    governance: [{ proposalId: 1, status: "active", votesFor: "10", votesAgainst: "2", executed: false }],
  };
  assert.equal(hashSnapshot(base), hashSnapshot(shuffled));
});

test("identical snapshots match with no changed paths", () => {
  const diff = compareSnapshots(base, JSON.parse(JSON.stringify(base)) as ContractState);
  assert.equal(diff.match, true);
  assert.deepEqual(diff.changedPaths, []);
});

test("changed field is reported at leaf path and row level", () => {
  const after: ContractState = {
    ...base,
    positions: [{ ...base.positions[0], debt: "700" }, base.positions[1]],
    pools: base.pools,
    governance: base.governance,
  };
  const diff = compareSnapshots(base, after);
  assert.equal(diff.match, false);
  assert.ok(diff.changedPaths.includes("positions/C_ALICE/debt"));
  assert.equal(diff.sections.positions.changed.length, 1);
  assert.equal(diff.sections.positions.changed[0].key, "C_ALICE");
});

test("added and removed rows are detected", () => {
  const after: ContractState = {
    positions: [base.positions[0], base.positions[1], { address: "C_CAROL", collateral: "0", debt: "0", healthFactor: "99" }],
    pools: base.pools,
    governance: base.governance,
  };
  const diff = compareSnapshots(base, after);
  assert.equal(diff.sections.positions.added.length, 1);
  assert.equal(diff.sections.positions.added[0].key, "C_CAROL");

  const removed: ContractState = { positions: [base.positions[0]], pools: base.pools, governance: base.governance };
  const diff2 = compareSnapshots(base, removed);
  assert.equal(diff2.sections.positions.removed.length, 1);
  assert.equal(diff2.sections.positions.removed[0].key, "C_BOB");
});

test("buildSnapshot normalizes state and can be re-hashed", () => {
  const snap = buildSnapshot(base);
  assert.equal(snap.kind, "contract-state");
  assert.ok(snap.recordedAt);
  assert.equal(hashSnapshot(snap.state), hashSnapshot(base));
  assert.deepEqual(normalizeState(snap.state), snap.state);
});