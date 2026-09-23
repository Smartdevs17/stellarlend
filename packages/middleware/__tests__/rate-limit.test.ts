import {
  defaultRateLimitPolicy,
  getRateLimitPolicy,
  resolvePolicyConfig,
  analyticsForOp,
  serializePolicy,
  parsePolicy,
  probeLimit,
  resetProbeBuckets,
} from '../src/rate-limit';
import type { RateLimitPolicy } from '../src/types';

describe('rate-limit policy engine (#704)', () => {
  const policy: RateLimitPolicy = {
    ...defaultRateLimitPolicy,
    opOverrides: {
      withdraw: {
        windowMs: 60_000,
        maxCallsPerWindow: 20,
        burstCalls: 5,
        graceBurstCalls: 0,
      },
    },
  };

  beforeEach(() => resetProbeBuckets());

  it('resolves the default layer for an unoverridden op', () => {
    const { config, layer } = resolvePolicyConfig(policy, 'borrow');
    expect(layer).toBe('default');
    expect(config.maxCallsPerWindow).toBe(5);
  });

  it('resolves the operation layer for an overridden op', () => {
    const { config, layer } = resolvePolicyConfig(policy, 'withdraw');
    expect(layer).toBe('operation');
    expect(config.maxCallsPerWindow).toBe(20);
  });

  it('throws for an unknown operation', () => {
    expect(() => resolvePolicyConfig(policy, 'nope')).toThrow();
  });

  it('produces an analytics snapshot', () => {
    const snap = analyticsForOp(policy, 'borrow', 7, 5_000);
    expect(snap.op).toBe('borrow');
    expect(snap.poolKey).toBe(7);
    expect(snap.fillBps).toBe(5_000);
    expect(snap.layer).toBe('default');
  });

  it('serialize/parse round-trips', () => {
    const serialized = serializePolicy(policy);
    const parsed = parsePolicy(serialized);
    expect(parsed.defaultLimits).toEqual(policy.defaultLimits);
  });

  it('parse rejects non-objects', () => {
    expect(() => parsePolicy(null)).toThrow();
  });

  it('probe admits up to capacity then rejects', () => {
    const result = probeLimit(policy, 'borrow', 12);
    // borrow capacity = max(5) + burst(3) = 8
    expect(result.admitted).toBe(8);
    expect(result.rejected).toBe(4);
  });
});
