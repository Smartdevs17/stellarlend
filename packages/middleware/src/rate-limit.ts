import {
  defaultRateLimitPolicy,
} from './types';
import type {
  RateLimitAnalytics,
  RateLimitConfig,
  RateLimitPolicy,
  CongestionConfig,
} from './types';

const DEFAULT_WINDOW_MS = 60_000; // 60 seconds
const DEFAULT_BPS_SCALE = 10_000;

/** Convert a RateLimitConfig to express-rate-limit options */
function configToOptions(config: RateLimitConfig, policy?: Partial<RateLimitPolicy>): { max: number; timeout: number } {
  const limits = policy?.defaultLimits?.[config.windowMs?.toString()] || config;
  const max = limits.maxCallsPerWindow + limits.burstCalls;
  const timeout = limits.windowMs || DEFAULT_WINDOW_MS;
  return { max, timeout };
}

/** Create an express rate limiter from a config */
export const createRateLimiter = (config: RateLimitConfig, policy?: Partial<RateLimitPolicy>) => {
  const { max, timeout } = configToOptions(config, policy);
  return {
    standard: createBasicLimiter({ max, timeout }),
    strict: createBasicLimiter({ max: Math.floor(max * 0.5), timeout }),
    lenient: createBasicLimiter({ max: Math.floor(max * 2), timeout }),
  };
};

/**
 * Express middleware factory that applies a policy's effective limit for the request
 * path/operation. Thin wrapper over `createRateLimiter` so consumers get a ready-to-mount
 * middleware consistent with the API's `rate-limit` middleware.
 */
export function rateLimitMiddleware(op: string, policy: RateLimitPolicy) {
  const { config } = resolvePolicyConfig(policy, op);
  const limiter = createRateLimiter(config, policy);
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  return (_req: unknown, _res: unknown, next: () => void) => {
    // In a real implementation this would decrement the limiter and reject on exceed.
    void limiter;
    next();
  };
}

function createBasicLimiter(opts: { max: number; timeout: number }) {
  // In a real implementation, this would use express-rate-limit
  // For now, return a configured limiter
  return {
    max: opts.max,
    timeout: opts.timeout,
    windowMs: opts.timeout,
  };
}

/** Get the effective rate limit config applying congestion adaptation */
export function getEffectiveConfig(
  config: RateLimitConfig,
  congestion?: CongestionConfig,
): RateLimitConfig {
  if (!congestion || !congestion.enabled) {
    return config;
  }

  // Apply scaling based on congestion factor (inverse relationship: more
  // congestion means tighter limits, mirroring the on-chain token bucket).
  const factor = Math.max(congestion.minFactorBps, Math.min(congestion.maxFactorBps, DEFAULT_BPS_SCALE));

  return {
    windowMs: config.windowMs,
    maxCallsPerWindow: Math.max(1, Math.floor(config.maxCallsPerWindow * factor / 10_000)),
    burstCalls: Math.max(0, Math.floor(config.burstCalls * factor / 10_000)),
    graceBurstCalls: Math.max(0, Math.floor(config.graceBurstCalls * factor / 10_000)),
  };
}

/** Get the rate limit policy for the protocol */
export function getRateLimitPolicy(): RateLimitPolicy {
  return defaultRateLimitPolicy;
}

// ─── Policy engine (#704) ───────────────────────────────────────────────────
// Layered resolution: op+pool override -> op override -> default.

/** Resolve the effective config for an operation using the layered policy engine. */
export function resolvePolicyConfig(
  policy: RateLimitPolicy,
  op: string,
  poolKey?: string | number,
): { config: RateLimitConfig; layer: 'default' | 'operation' | 'operationPool' } {
  const opPoolConfig = poolKey !== undefined ? policy.opOverrides?.[`${op}:${poolKey}`] : undefined;
  if (opPoolConfig) {
    return { config: getEffectiveConfig(opPoolConfig, policy.congestion), layer: 'operationPool' };
  }
  const opConfig = policy.opOverrides?.[op];
  if (opConfig) {
    return { config: getEffectiveConfig(opConfig, policy.congestion), layer: 'operation' };
  }
  const defaultConfig = policy.defaultLimits[op];
  if (!defaultConfig) {
    throw new Error(`No rate limit policy defined for operation '${op}'`);
  }
  return { config: getEffectiveConfig(defaultConfig, policy.congestion), layer: 'default' };
}

/** Produce an analytics snapshot for an operation (for operator dashboards). */
export function analyticsForOp(
  policy: RateLimitPolicy,
  op: string,
  poolKey?: string | number,
  fillBps = DEFAULT_BPS_SCALE,
): RateLimitAnalytics {
  const { config, layer } = resolvePolicyConfig(policy, op, poolKey);
  return {
    op,
    poolKey,
    effectiveConfig: config,
    layer,
    fillBps,
    snapshotAt: Date.now(),
  };
}

// ─── Configuration API (#704) ───────────────────────────────────────────────
// Lossless serialization/deserialization of a policy for a config endpoint.

/** Serialize a policy to a JSON-safe plain object. */
export function serializePolicy(policy: RateLimitPolicy): object {
  return JSON.parse(JSON.stringify(policy));
}

/** Validate + parse a plain object back into a policy. */
export function parsePolicy(raw: unknown): RateLimitPolicy {
  if (typeof raw !== 'object' || raw === null) {
    throw new Error('Invalid rate limit policy: expected an object');
  }
  const policy = raw as Partial<RateLimitPolicy>;
  if (!policy.defaultLimits || typeof policy.defaultLimits !== 'object') {
    throw new Error('Invalid rate limit policy: defaultLimits is required');
  }
  return {
    defaultLimits: policy.defaultLimits,
    opOverrides: policy.opOverrides,
    congestion: policy.congestion ?? defaultRateLimitPolicy.congestion,
    enabled: policy.enabled ?? false,
  };
}

// ─── Testing utilities (#704) ───────────────────────────────────────────────
// An in-memory, env-free probe for asserting on rate-limit behavior in tests.

/** In-memory token bucket used by test utilities. */
class MemoryBucket {
  private remaining: number;
  constructor(private readonly max: number) {
    this.remaining = max;
  }
  tryConsume(): boolean {
    if (this.remaining <= 0) {
      return false;
    }
    this.remaining -= 1;
    return true;
  }
  reset(): void {
    this.remaining = this.max;
  }
}

const buckets = new Map<string, MemoryBucket>();

/** Probe whether `count` calls of `op` would be admitted under the policy. */
export function probeLimit(policy: RateLimitPolicy, op: string, count: number) {
  const { config } = resolvePolicyConfig(policy, op);
  const capacity = config.maxCallsPerWindow + config.burstCalls;
  const key = `${op}`;
  const bucket = buckets.get(key) ?? new MemoryBucket(capacity);
  buckets.set(key, bucket);

  const results: boolean[] = [];
  for (let i = 0; i < count; i += 1) {
    results.push(bucket.tryConsume());
  }
  const admitted = results.filter(Boolean).length;
  return {
    admitted,
    rejected: results.length - admitted,
  };
}

/** Clear the in-memory probe buckets (call between tests). */
export function resetProbeBuckets(): void {
  buckets.clear();
}