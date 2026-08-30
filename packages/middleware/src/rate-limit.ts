import type { RateLimitPolicy, RateLimitConfig, CongestionConfig } from './types';

const DEFAULT_WINDOW_MS = 60_000; // 60 seconds
const DEFAULT_BPS_SCALE = 10_000;

/** Convert a RateLimitConfig to express-rate-limit options */
function configToOptions(config: RateLimitConfig, policy?: Partial<RateLimitPolicy>): { max: number; timeout: number } {
  const limits = policy?.defaultLimits[config.windowMs?.toString()] || config;
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
  congestion?: CongestionConfig
): RateLimitConfig {
  if (!congestion || !congestion.enabled) {
    return config;
  }

  // Apply scaling based on congestion factor
  const factor = congestion.minFactorBps.min(congestion.maxFactorBps);

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