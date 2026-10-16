import { Request } from 'express';

export interface AuthConfig {
  jwtSecret?: string;
  apiKeyHeader?: string;
  validateApiKey?: (key: string) => Promise<boolean>;
}

export interface JWTPayload {
  userId: string;
  email?: string;
  role?: string;
  [key: string]: any;
}

export interface RateLimitConfig {
  windowMs: number;
  maxCallsPerWindow: number;
  burstCalls: number;
  graceBurstCalls: number;
}

export interface CongestionConfig {
  enabled: boolean;
  baselineSecsPerLedger: number;
  reportTtlSeconds: number;
  minFactorBps: number;
  maxFactorBps: number;
}

export interface RateLimitPolicy {
  /** Default rate limit per operation type */
  defaultLimits: Record<string, RateLimitConfig>;
  /** Optional per-operation override map (operation -> config). */
  opOverrides?: Record<string, RateLimitConfig>;
  /** Congestion adaptation config */
  congestion: CongestionConfig;
  /** Whether congestion adaptation is enabled */
  enabled: boolean;
}

/** Which layer of the policy produced the effective limit for an operation. */
export type PolicyLayer = 'default' | 'operation' | 'operationPool';

/** Read-only analytics snapshot for an operation + pool. */
export interface RateLimitAnalytics {
  op: string;
  poolKey?: string | number;
  effectiveConfig: RateLimitConfig;
  layer: PolicyLayer;
  /** Fill fraction of the budget in basis points (0..10_000). */
  fillBps: number;
  snapshotAt: number;
}

/** Rate-limit testing utility result. */
export interface LimitProbe {
  allowed: boolean;
  remaining: number;
  retryAfterMs?: number;
}

/** Default rate limit policy for the protocol */
export const defaultRateLimitPolicy: RateLimitPolicy = {
  defaultLimits: {
    borrow: {
      windowMs: 60_000,
      maxCallsPerWindow: 5,
      burstCalls: 3,
      graceBurstCalls: 10,
    },
    liquidate: {
      windowMs: 60_000,
      maxCallsPerWindow: 10,
      burstCalls: 5,
      graceBurstCalls: 20,
    },
    deposit: {
      windowMs: 60_000,
      maxCallsPerWindow: 30,
      burstCalls: 10,
      graceBurstCalls: 0,
    },
    repay: {
      windowMs: 60_000,
      maxCallsPerWindow: 30,
      burstCalls: 10,
      graceBurstCalls: 0,
    },
  },
  congestion: {
    enabled: false,
    baselineSecsPerLedger: 5,
    reportTtlSeconds: 300,
    minFactorBps: 2_500,
    maxFactorBps: 10_000,
  },
  enabled: false,
};
