import { canonicalizeSnapshot } from '../format';
import type { LendingSnapshotV2 } from '../schema';

describe('canonicalizeSnapshot', () => {
  it('normalizes envelope key order and preserves seq', () => {
    const snapshot: LendingSnapshotV2 = {
      version: 2,
      kind: 'contract-state',
      recordedAt: '2026-01-01T00:00:00.000Z',
      seq: 3,
      state: { positions: [], pools: [], governance: [] },
    };

    expect(Object.keys(canonicalizeSnapshot(snapshot))).toEqual([
      'version',
      'kind',
      'recordedAt',
      'seq',
      'state',
    ]);
  });
});
