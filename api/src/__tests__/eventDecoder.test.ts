import { decodeCompactEvent } from '../../../backend/src/services/eventDecoder';

describe('compact contract event decoder', () => {
  const goldenEvent = Uint8Array.from([
    1, 0, 0, 0, 1, 0x61, 0, 0, 1, 0x74, 1, 2, 0, 0, 2, 0,
  ]);

  it('decodes the schema-aware v1 payload and derives its topic', () => {
    expect(decodeCompactEvent(goldenEvent)).toEqual({
      schema_version: 1,
      topic: 'lend_deposit',
      module: 'pool',
      action: 'deposit',
      severity: 'info',
      actor: 'a',
      asset: null,
      counterparty: null,
      trace_id: 't',
      timestamp: 1,
      amount: '1',
      health_factor_bps: null,
      payload: { kind: 'pool', balance_after: '1', debt_after: '0' },
    });
  });

  it('rejects unsupported, truncated, and trailing data', () => {
    expect(() => decodeCompactEvent(Uint8Array.from([2]))).toThrow(
      'Unsupported compact event schema version'
    );
    expect(() => decodeCompactEvent(goldenEvent.slice(0, -1))).toThrow('Truncated compact event');
    expect(() => decodeCompactEvent(Uint8Array.from([...goldenEvent, 0]))).toThrow(
      'Unexpected trailing bytes'
    );
  });
});
