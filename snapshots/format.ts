import type { CurrentLendingSnapshot, CurrentSnapshotState } from './schema';
import { CURRENT_SNAPSHOT_VERSION } from './schema';

/**
 * Canonical snapshot envelope used by tooling and contract-test consumers.
 * This keeps root snapshots in a single format while allowing legacy field
 * aliases to be normalized at read time.
 */
export interface CanonicalSnapshotEnvelope {
  version: typeof CURRENT_SNAPSHOT_VERSION;
  kind: 'contract-state';
  recordedAt: string;
  seq: number;
  state: CurrentSnapshotState;
}

export function canonicalizeSnapshot(snapshot: CurrentLendingSnapshot): CanonicalSnapshotEnvelope {
  return {
    version: CURRENT_SNAPSHOT_VERSION,
    kind: 'contract-state',
    recordedAt: snapshot.recordedAt,
    seq: snapshot.seq ?? 0,
    state: {
      positions: snapshot.state.positions.map((p) => ({
        address: p.address,
        collateral: p.collateral,
        debt: p.debt,
        healthFactor: p.healthFactor,
        ...(p.lastUpdatedLedger !== undefined ? { lastUpdatedLedger: p.lastUpdatedLedger } : {}),
      })),
      pools: snapshot.state.pools.map((p) => ({
        asset: p.asset,
        collateralFactorBps: p.collateralFactorBps,
        interestRateBps: p.interestRateBps,
        supplyCap: p.supplyCap,
        ...(p.liquidationThresholdBps !== undefined
          ? { liquidationThresholdBps: p.liquidationThresholdBps }
          : {}),
      })),
      governance: snapshot.state.governance.map((g) => ({
        proposalId: g.proposalId,
        status: g.status,
        executed: g.executed,
        votesFor: g.votesFor,
        votesAgainst: g.votesAgainst,
        ...(g.timelockAt !== undefined ? { timelockAt: g.timelockAt } : {}),
      })),
    },
  };
}

export function assertCanonicalSnapshot(snapshot: CurrentLendingSnapshot): void {
  const canonical = canonicalizeSnapshot(snapshot);
  const keys = Object.keys(canonical);
  const expected = ['version', 'kind', 'recordedAt', 'seq', 'state'];
  if (keys.length !== expected.length || keys.some((k, i) => k !== expected[i])) {
    throw new Error(`Snapshot envelope keys must be ${expected.join(', ')}`);
  }
}
